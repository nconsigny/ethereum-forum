// Type definitions for the forum API responses
export interface TopicSummary {
    id: number;
    title: string;
    posts_count: number;
    created_at: string;
    last_posted_at: string;
    views: number;
    like_count: number;
    participants?: Array<{ id: number; username: string; avatar_template?: string }>;
}

export interface Post {
    id: number;
    topic_id: number;
    post_number: number;
    raw?: string;
    cooked: string;
    created_at: string;
    username: string;
    name?: string;
    avatar_template?: string;
    like_count?: number;
    reply_count?: number;
}

export interface SearchEntity {
    entity_type: 'topic' | 'post';
    topic_id: number | null;
    post_id: number | null;
    post_number: number | null;
    user_id: number | null;
    username: string | null;
    title: string | null;
    slug: string | null;
    pm_issue: number | null;
    cooked: string | null;
    entity_id: string;
}

export interface SearchResult {
    topics?: TopicSummary[];
    posts?: Post[];
    hits?: number;
}

export interface UserProfile {
    id: number;
    username: string;
    name?: string;
    avatar_template?: string;
    bio_raw?: string;
    location?: string;
    website_name?: string;
    created_at: string;
    last_posted_at?: string;
    last_seen_at?: string;
    post_count: number;
    topic_count: number;
    likes_given: number;
    likes_received: number;
    trust_level: number;
}

export interface ToolResultDisplayProps {
    toolName: string;
    result: string;
    isExpanded: boolean;
    onExpand?: () => void;
}
