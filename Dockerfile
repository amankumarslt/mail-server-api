# Build Stage
FROM rust:slim-bookworm as builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev
COPY . .
RUN cargo build --release

# Runtime Stage
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies + tools to download cloudflared
RUN apt-get update && apt-get install -y libssl-dev ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Install Cloudflare Tunnel via Package Manager (Handles Arch automatically)
RUN mkdir -p --mode=0755 /usr/share/keyrings && \
    curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null && \
    echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared bookworm main' | tee /etc/apt/sources.list.d/cloudflared.list && \
    apt-get update && apt-get install -y cloudflared

COPY --from=builder /app/target/release/mail-server /app/mail-server
# Don't copy .env.example - use --env-file at runtime instead
COPY start.sh .

# Fix "exec format error" by stripping Windows line endings (CRLF -> LF)
RUN sed -i 's/\r$//' start.sh && chmod +x start.sh

EXPOSE 8080 2525
# Run with explicit bash to avoid shebang issues
CMD ["bash", "start.sh"]
