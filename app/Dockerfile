FROM debian:trixie-slim

# Set the working directory inside the container
WORKDIR /app

# Copy the built binary from the builder stage
COPY ./target/release/ethereum-forum .
COPY ./www ./www

# Expose port 3000
EXPOSE 3000

# Set the entrypoint command to run your application
ENTRYPOINT ["./ethereum-forum"]
