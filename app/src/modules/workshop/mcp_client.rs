use async_openai::types::{ChatCompletionTool, ChatCompletionRequestMessage, ChatCompletionToolType, FunctionObject};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use tracing;
use std::time::{Duration, Instant};
use reqwest::Client;
use async_std::task::sleep;

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Initialization failed: {0}")]
    Initialization(String),
    #[error("Other error: {0}")]
    Other(String),
}

/// Tool response from MCP server
#[derive(Debug, Deserialize, Serialize)]
pub struct McpToolResponse {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

/// Content item from MCP responses
#[derive(Debug, Deserialize, Serialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub data: Option<String>,
}

/// MCP Tool definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

/// MCP Client Manager using direct HTTP requests
pub struct McpClientManager {
    client: Client,
    server_url: String,
    session_id: Option<String>,
    server_info: Option<Value>,
    tools_cache: Option<Vec<McpTool>>,
    cache_duration: Duration,
    last_update: Option<Instant>,
}

impl McpClientManager {
    pub fn new() -> Self {
        let server_url = std::env::var("MCP_SERVER_URL")
            .unwrap_or_else(|_| "https://ethereum.forum/mcp".to_string());

        tracing::info!("🔧 MCP Client initialized with URL: {}", server_url);
        tracing::debug!("🔧 MCP_SERVER_URL environment variable: {:?}", std::env::var("MCP_SERVER_URL"));

        Self {
            client: Client::builder().use_rustls_tls().build().unwrap(),
            server_url,
            session_id: None,
            server_info: None,
            tools_cache: None,
            cache_duration: Duration::from_secs(300),
            last_update: None,
        }
    }

    /// Check if an error is retryable
    fn is_retryable_error(error: &McpError) -> bool {
        match error {
            McpError::Http(req_error) => {
                // Retry on connection errors, timeouts, etc.
                req_error.is_connect() || req_error.is_timeout() || req_error.is_request()
            }
            McpError::Connection(_) => true,
            McpError::Protocol(msg) => {
                // Retry on 404, 503, 502, 500 errors
                msg.contains("404") || msg.contains("503") || msg.contains("502") || msg.contains("500")
            }
            _ => false,
        }
    }

    /// Retry logic with exponential backoff
    async fn retry_with_backoff<F, Fut, T>(
        &self,
        operation: F,
        operation_name: &str,
    ) -> Result<T, McpError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, McpError>>,
    {
        const MAX_RETRIES: u32 = 3;
        const BASE_DELAY_MS: u64 = 1000;

        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    if !Self::is_retryable_error(&error) {
                        tracing::warn!("❌ {} failed with non-retryable error: {}", operation_name, error);
                        return Err(error);
                    }

                    last_error = Some(error);

                    if attempt < MAX_RETRIES - 1 {
                        let delay = Duration::from_millis(BASE_DELAY_MS * 2_u64.pow(attempt));
                        tracing::warn!(
                            "⚠️ {} failed (attempt {}/{}), retrying in {:?}...", 
                            operation_name, 
                            attempt + 1, 
                            MAX_RETRIES, 
                            delay
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| McpError::Other("All retries exhausted".to_string())))
    }

    /// Initialize the MCP connection with the server (with retries)
    async fn initialize_connection(&mut self) -> Result<(), McpError> {
        tracing::info!("🔗 Initializing MCP connection to {}", self.server_url);

        let server_url = self.server_url.clone();
        let client = self.client.clone();

        let result = self.retry_with_backoff(
            || async {
                tracing::debug!("🌐 Attempting MCP initialization to URL: {}", &server_url);
                
                let init_request = json!({
                    "jsonrpc": "2.0",
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": {
                            "roots": {"listChanged": true},
                            "sampling": {}
                        },
                        "clientInfo": {
                            "name": "ethereum-forum-workshop",
                            "version": "0.1.0"
                        }
                    },
                    "id": 1
                });

                tracing::debug!("📤 Sending initialization request: {}", serde_json::to_string_pretty(&init_request).unwrap_or_default());

                let response = client
                    .post(&server_url)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json, text/event-stream")
                    .header("User-Agent", "ethereum-forum-workshop/0.1.0")
                    .json(&init_request)
                    .send()
                    .await?;

                let status = response.status();
                let headers = response.headers().clone();
                
                tracing::debug!("📥 Response status: {}", status);
                tracing::debug!("📥 Response headers: {:?}", headers);

                if !status.is_success() {
                    let response_text = response.text().await.unwrap_or_default();
                    tracing::error!("❌ Initialization failed - Status: {}, Body: {}", status, response_text);
                    return Err(McpError::Protocol(format!(
                        "Initialization failed with status: {} - Response: {}", 
                        status, response_text
                    )));
                }

                Ok(response)
            },
            "MCP initialization"
        ).await?;

        // Extract session ID from headers
        self.session_id = result
            .headers()
            .get("Mcp-Session-Id")
            .or_else(|| result.headers().get("mcp-session-id"))
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let response_text = result.text().await?;
        tracing::debug!("📥 Initialization response body: {}", response_text);
        
        let response_json: Value = serde_json::from_str(&response_text)?;

        if let Some(result) = response_json.get("result") {
            self.server_info = Some(result.clone());
            
            if let (Some(name), Some(version)) = (
                result["serverInfo"]["name"].as_str(),
                result["serverInfo"]["version"].as_str()
            ) {
                tracing::info!("✅ Connected to {} v{}", name, version);
            }
            
            if let Some(protocol) = result["protocolVersion"].as_str() {
                tracing::info!("📋 Protocol version: {}", protocol);
            }
        }

        if let Some(ref session_id) = self.session_id {
            tracing::info!("🔑 Session ID: {}", session_id);
        }

        tracing::info!("✅ MCP initialization successful!");
        Ok(())
    }

    /// Initialize the MCP client with direct HTTP transport
    pub async fn init(&mut self) -> Result<(), McpError> {
        tracing::info!("🔧 Testing MCP connection...");
        
        // Run connectivity test first
        if let Err(e) = self.test_connectivity().await {
            tracing::warn!("⚠️ Connectivity test failed, but continuing: {}", e);
        }
        
        // Initialize the connection
        self.initialize_connection().await?;
        
        tracing::info!("✅ MCP client test successful");
        Ok(())
    }

    /// Initialize with a specific base URL
    pub async fn init_default_client(&mut self, base_url: String) -> Result<(), McpError> {
        self.server_url = base_url;
        self.init().await
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        !self.server_url.is_empty()
    }

    /// Get tools using direct HTTP requests (with retries)
    pub async fn get_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        tracing::info!("🔧 get_tools called, server_url: {}", self.server_url);
        
        // Check cache
        if let (Some(cached_tools), Some(last_update)) = (&self.tools_cache, self.last_update) {
            if last_update.elapsed() < self.cache_duration {
                tracing::debug!("📋 Returning cached MCP tools ({} tools)", cached_tools.len());
                return Ok(cached_tools.clone());
            }
        }

        tracing::info!("🔄 Fetching fresh MCP tools from server");
        
        // Always ensure we have a fresh connection for tools requests
        // This fixes issues with stale session IDs
        tracing::info!("🔄 Ensuring fresh MCP session for tools request...");
        self.reset_connection().await;
        self.initialize_connection().await?;

        // Store session info for debugging
        let session_id_for_debug = self.session_id.clone();
        tracing::info!("🔑 About to use session ID for tools request: {:?}", session_id_for_debug);

        let server_url = self.server_url.clone();
        let client = self.client.clone();
        let session_id = self.session_id.clone();

        let response_json = self.retry_with_backoff(
            || async {
                tracing::debug!("🌐 Attempting tools fetch to URL: {}", &server_url);
                
                let tools_request = json!({
                    "jsonrpc": "2.0",
                    "method": "tools/list",
                    "params": {},
                    "id": 2
                });

                tracing::debug!("📤 Sending tools request: {}", serde_json::to_string_pretty(&tools_request).unwrap_or_default());

                let mut request_builder = client
                    .post(&server_url)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json, text/event-stream")
                    .header("User-Agent", "ethereum-forum-workshop/0.1.0");

                if let Some(ref session_id) = session_id {
                    request_builder = request_builder.header("Mcp-Session-Id", session_id);
                    tracing::debug!("🔑 Using session ID: {}", session_id);
                } else {
                    tracing::warn!("⚠️ No session ID available for tools request");
                }

                let response = request_builder
                    .json(&tools_request)
                    .send()
                    .await?;

                let status = response.status();
                let headers = response.headers().clone();
                
                tracing::debug!("📥 Tools response status: {}", status);
                tracing::debug!("📥 Tools response headers: {:?}", headers);

                if !status.is_success() {
                    let response_text = response.text().await.unwrap_or_default();
                    tracing::error!("❌ Tools fetch failed - Status: {}, Body: {}", status, response_text);
                    
                    // Enhanced debugging for 404 errors in production
                    if status == 404 {
                        tracing::error!("🚨 404 Error Details:");
                        tracing::error!("  - Session ID used: {:?}", session_id);
                        tracing::error!("  - Server URL: {}", server_url);
                        tracing::error!("  - Request headers sent: Content-Type: application/json, Accept: application/json, text/event-stream");
                        tracing::error!("  - Response headers: {:?}", headers);
                        tracing::error!("  - Response body: {}", response_text);
                        
                        // Return a specific 404 error that can be handled differently
                        return Err(McpError::Protocol(format!(
                            "Session invalid (404) - Tools list request failed with status: {}\nResponse: {}", 
                            status, 
                            response_text
                        )));
                    }
                    
                    return Err(McpError::Protocol(format!(
                        "Tools list request failed with status: {}\nResponse: {}", 
                        status, 
                        response_text
                    )));
                }

                // Handle both JSON and SSE responses
                let content_type = response.headers()
                    .get("content-type")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("");

                tracing::debug!("📥 Content-Type: {}", content_type);

                let response_json: Value = if content_type.starts_with("text/event-stream") {
                    // Handle Server-Sent Events response
                    let response_text = response.text().await?;
                    tracing::debug!("📥 SSE response body: {}", response_text);
                    
                    // Parse SSE events to extract JSON-RPC response
                    let mut json_response = None;
                    for line in response_text.lines() {
                        if line.starts_with("data: ") {
                            let json_data = &line[6..]; // Remove "data: " prefix
                            if let Ok(parsed) = serde_json::from_str::<Value>(json_data) {
                                json_response = Some(parsed);
                                break;
                            }
                        }
                    }
                    
                    json_response.ok_or_else(|| McpError::Protocol("No valid JSON found in SSE response".to_string()))?
                } else {
                    // Handle regular JSON response
                    let response_text = response.text().await?;
                    tracing::debug!("📥 JSON response body: {}", response_text);
                    serde_json::from_str(&response_text)?
                };

                Ok(response_json)
            },
            "MCP tools fetch"
        ).await;

        // Handle the result with automatic session refresh on 404
        let response_json = match response_json {
            Ok(json) => json,
            Err(McpError::Protocol(ref msg)) if msg.contains("Session invalid (404)") => {
                tracing::warn!("🔄 Got session invalid error, attempting session refresh and retry...");
                
                // Force a complete reset and re-initialization
                self.reset_connection().await;
                
                // Add a small delay to ensure any server-side cleanup is complete
                sleep(Duration::from_millis(500)).await;
                
                // Re-initialize with fresh session
                self.initialize_connection().await?;
                
                let fresh_session_id = self.session_id.clone();
                tracing::info!("🆕 Retrying with fresh session ID: {:?}", fresh_session_id);
                
                // Retry the tools request with fresh session
                let server_url = self.server_url.clone();
                let client = self.client.clone();
                
                self.retry_with_backoff(
                    || async {
                        let tools_request = json!({
                            "jsonrpc": "2.0",
                            "method": "tools/list",
                            "params": {},
                            "id": 2
                        });

                        let mut request_builder = client
                            .post(&server_url)
                            .header("Content-Type", "application/json")
                            .header("Accept", "application/json, text/event-stream")
                            .header("User-Agent", "ethereum-forum-workshop/0.1.0");

                        if let Some(ref session_id) = fresh_session_id {
                            request_builder = request_builder.header("Mcp-Session-Id", session_id);
                            tracing::debug!("🔑 Retry using fresh session ID: {}", session_id);
                        }

                        let response = request_builder
                            .json(&tools_request)
                            .send()
                            .await?;

                        let status = response.status();
                        
                        if !status.is_success() {
                            let response_text = response.text().await.unwrap_or_default();
                            tracing::error!("❌ Tools fetch retry failed - Status: {}, Body: {}", status, response_text);
                            return Err(McpError::Protocol(format!(
                                "Tools list retry failed with status: {}\nResponse: {}", 
                                status, 
                                response_text
                            )));
                        }

                        let content_type = response.headers()
                            .get("content-type")
                            .and_then(|h| h.to_str().ok())
                            .unwrap_or("");

                        let response_json: Value = if content_type.starts_with("text/event-stream") {
                            let response_text = response.text().await?;
                            let mut json_response = None;
                            for line in response_text.lines() {
                                if line.starts_with("data: ") {
                                    let json_data = &line[6..];
                                    if let Ok(parsed) = serde_json::from_str::<Value>(json_data) {
                                        json_response = Some(parsed);
                                        break;
                                    }
                                }
                            }
                            json_response.ok_or_else(|| McpError::Protocol("No valid JSON found in SSE response".to_string()))?
                        } else {
                            let response_text = response.text().await?;
                            serde_json::from_str(&response_text)?
                        };

                        Ok(response_json)
                    },
                    "MCP tools fetch retry"
                ).await?
            }
            Err(e) => return Err(e),
        };

        // Handle the response - it might be an array or a single object
        let tools_result = if response_json.is_array() {
            // If response is an array, take the first element
            response_json[0]["result"]["tools"].as_array()
        } else {
            // If response is a single object
            response_json["result"]["tools"].as_array()
        };

        let tools: Vec<McpTool> = if let Some(tools_array) = tools_result {
            tools_array.iter().map(|tool| {
                McpTool {
                    name: tool["name"].as_str().unwrap_or("").to_string(),
                    description: tool["description"].as_str().map(|s| s.to_string()),
                    input_schema: tool.get("inputSchema").cloned(),
                }
            }).collect()
        } else {
            Vec::new()
        };
        
        tracing::info!("✅ Retrieved {} tools from MCP server", tools.len());

        // Update cache
        self.tools_cache = Some(tools.clone());
        self.last_update = Some(Instant::now());

        Ok(tools)
    }

    /// Convert MCP tools to OpenAI function format
    pub async fn get_openai_tools(&mut self) -> Result<Vec<ChatCompletionTool>, McpError> {
        tracing::info!("🔧 get_openai_tools called, getting MCP tools...");
        let tools = self.get_tools().await?;
        tracing::info!("📋 Retrieved {} MCP tools from get_tools()", tools.len());
        
        let openai_tools: Vec<ChatCompletionTool> = tools.into_iter().map(|tool| {
            ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.input_schema,
                    strict: None,
                },
            }
        }).collect();

        tracing::info!("🔧 Converted {} MCP tools to OpenAI format", openai_tools.len());
        Ok(openai_tools)
    }

    /// Call a tool using direct HTTP requests (with retries)
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<McpToolResponse, McpError> {
        tracing::info!("🔧 Calling MCP tool: {} with arguments: {}", name, arguments);
        
        // Initialize connection if not already done
        if self.session_id.is_none() {
            self.initialize_connection().await?;
        }

        let tool_name = name.to_string();
        let server_url = self.server_url.clone();
        let client = self.client.clone();
        let session_id = self.session_id.clone();

        let response_json = self.retry_with_backoff(
            || async {
                let tool_request = json!({
                    "jsonrpc": "2.0",
                    "method": "tools/call",
                    "params": {
                        "name": &tool_name,
                        "arguments": &arguments
                    },
                    "id": 3
                });

                let mut request_builder = client
                    .post(&server_url)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json, text/event-stream");

                if let Some(ref session_id) = session_id {
                    request_builder = request_builder.header("Mcp-Session-Id", session_id);
                }

                let response = request_builder
                    .json(&tool_request)
                    .send()
                    .await?;

                let status = response.status();
                if !status.is_success() {
                    let response_text = response.text().await?;
                    return Err(McpError::Protocol(format!(
                        "Tool call failed with status: {}\nResponse: {}", 
                        status, 
                        response_text
                    )));
                }

                // Handle both JSON and SSE responses
                let content_type = response.headers()
                    .get("content-type")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("");

                let response_json: Value = if content_type.starts_with("text/event-stream") {
                    // Handle Server-Sent Events response
                    let response_text = response.text().await?;
                    
                    // Parse SSE events to extract JSON-RPC response
                    let mut json_response = None;
                    for line in response_text.lines() {
                        if line.starts_with("data: ") {
                            let json_data = &line[6..]; // Remove "data: " prefix
                            if let Ok(parsed) = serde_json::from_str::<Value>(json_data) {
                                json_response = Some(parsed);
                                break;
                            }
                        }
                    }
                    
                    json_response.ok_or_else(|| McpError::Protocol("No valid JSON found in SSE response".to_string()))?
                } else {
                    // Handle regular JSON response
                    let response_text = response.text().await?;
                    serde_json::from_str(&response_text)?
                };

                Ok(response_json)
            },
            "MCP tool call"
        ).await?;

        // Handle the response - it might be an array or a single object
        let (response_obj, result) = if response_json.is_array() {
            // If response is an array, take the first element
            let first_response = &response_json[0];
            (first_response, first_response.get("result"))
        } else {
            // If response is a single object
            (&response_json, response_json.get("result"))
        };

        if let Some(error) = response_obj.get("error") {
            return Err(McpError::Protocol(format!("Tool call error: {}", error)));
        }

        if let Some(result) = result {
            // Convert the result to our McpToolResponse format
            let tool_response = McpToolResponse {
                content: if let Some(content_array) = result.get("content").and_then(|c| c.as_array()) {
                    content_array.iter().map(|content| {
                        McpContent {
                            content_type: content.get("type").and_then(|t| t.as_str()).unwrap_or("text").to_string(),
                            text: content.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()),
                            data: content.get("data").and_then(|d| d.as_str()).map(|s| s.to_string()),
                        }
                    }).collect()
                } else {
                    vec![]
                },
                is_error: result.get("isError").and_then(|e| e.as_bool()),
            };
            
            if tool_response.is_error == Some(true) {
                let error_msg = tool_response.content.iter()
                    .find(|c| c.content_type == "text")
                    .and_then(|c| c.text.as_ref())
                    .unwrap_or(&"Unknown error".to_string())
                    .clone();
                
                return Err(McpError::Protocol(format!("Tool execution error: {}", error_msg)));
            }
            
            tracing::info!("✅ MCP tool call completed successfully");
            Ok(tool_response)
        } else {
            Err(McpError::Protocol(format!(
                "Unexpected response format: {}", 
                serde_json::to_string_pretty(&response_json).unwrap_or_default()
            )))
        }
    }

    /// Reset the connection
    pub async fn reset_connection(&mut self) {
        tracing::info!("🔄 Resetting MCP connection");
        self.session_id = None;
        self.server_info = None;
        self.tools_cache = None;
        self.last_update = None;
    }

    /// Check if MCP client is enabled
    pub fn is_enabled(&self) -> bool {
        !self.server_url.is_empty()
    }

    /// Get the base URL
    pub fn get_base_url(&self) -> String {
        self.server_url.clone()
    }

    /// List servers
    pub async fn list_servers(&self) -> Result<Vec<String>, McpError> {
        Ok(vec![self.server_url.clone()])
    }

    /// List all available tools
    pub async fn list_all_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        self.get_tools().await
    }

    /// Test connectivity to the MCP server
    pub async fn test_connectivity(&self) -> Result<(), McpError> {
        tracing::info!("🔍 Testing MCP server connectivity to: {}", self.server_url);
        
        // First, test if the URL is reachable at all with a simple GET
        let get_response = self.client
            .get(&self.server_url)
            .header("User-Agent", "ethereum-forum-workshop/0.1.0")
            .send()
            .await;

        match get_response {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                tracing::info!("🌐 GET request to MCP URL succeeded - Status: {}", status);
                tracing::debug!("🌐 GET response headers: {:?}", headers);
                
                // Read response body
                let body = response.text().await.unwrap_or_default();
                if body.len() > 500 {
                    tracing::debug!("🌐 GET response body (truncated): {}...", &body[..500]);
                } else {
                    tracing::debug!("🌐 GET response body: {}", body);
                }
            }
            Err(e) => {
                tracing::error!("❌ GET request to MCP URL failed: {}", e);
                return Err(McpError::Connection(format!("Basic connectivity test failed: {}", e)));
            }
        }

        // Test OPTIONS request to check CORS/method support
        let options_response = self.client
            .request(reqwest::Method::OPTIONS, &self.server_url)
            .header("User-Agent", "ethereum-forum-workshop/0.1.0")
            .send()
            .await;

        match options_response {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                tracing::info!("🔍 OPTIONS request to MCP URL - Status: {}", status);
                tracing::debug!("🔍 OPTIONS response headers: {:?}", headers);
            }
            Err(e) => {
                tracing::warn!("⚠️ OPTIONS request to MCP URL failed: {}", e);
            }
        }

        // Test basic POST to see what happens
        let test_post = self.client
            .post(&self.server_url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "ethereum-forum-workshop/0.1.0")
            .json(&json!({"test": "connectivity"}))
            .send()
            .await;

        match test_post {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                tracing::info!("📤 Test POST to MCP URL - Status: {}", status);
                tracing::debug!("📤 Test POST response headers: {:?}", headers);
                
                let body = response.text().await.unwrap_or_default();
                if body.len() > 200 {
                    tracing::debug!("📤 Test POST response body (truncated): {}...", &body[..200]);
                } else {
                    tracing::debug!("📤 Test POST response body: {}", body);
                }
            }
            Err(e) => {
                tracing::error!("❌ Test POST to MCP URL failed: {}", e);
                return Err(McpError::Connection(format!("POST connectivity test failed: {}", e)));
            }
        }

        tracing::info!("✅ MCP server connectivity tests completed");
        Ok(())
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for processing tool calls in OpenAI chat completions
pub struct ToolCallHelper;

impl ToolCallHelper {
    /// Process tool calls from OpenAI chat completion messages
    pub async fn process_tool_calls(
        mcp_client: &mut McpClientManager,
        messages: &[ChatCompletionRequestMessage],
    ) -> Result<Vec<(String, String)>, McpError> {
        // Find the last assistant message with tool calls
        let tool_calls = messages
            .iter()
            .rev()
            .find_map(|msg| match msg {
                ChatCompletionRequestMessage::Assistant(assistant_msg) => {
                    assistant_msg.tool_calls.as_ref()
                }
                _ => None,
            });

        let mut results = Vec::new();

        if let Some(tool_calls) = tool_calls {
            for tool_call in tool_calls {
                let function = &tool_call.function;
                let arguments: Value = serde_json::from_str(&function.arguments)
                    .unwrap_or(Value::Object(serde_json::Map::new()));
                
                match mcp_client.call_tool(&function.name, arguments).await {
                    Ok(response) => {
                        // Format the response content
                        let content = response.content
                            .into_iter()
                            .filter_map(|c| c.text)
                            .collect::<Vec<_>>()
                            .join("\n");
                        
                        results.push((tool_call.id.clone(), content));
                    }
                    Err(e) => {
                        let error_msg = format!("Tool call failed: {}", e);
                        tracing::error!("{}", error_msg);
                        results.push((tool_call.id.clone(), error_msg));
                    }
                }
            }
        }

        Ok(results)
    }

    /// Format tool call results for chat completion
    pub fn format_tool_results(tool_results: &[(String, String)]) -> String {
        if tool_results.is_empty() {
            return String::new();
        }
        
        tool_results
            .iter()
            .map(|(id, result)| format!("Tool {} result: {}", id, result))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
} 