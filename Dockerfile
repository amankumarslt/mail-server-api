# Build Stage
FROM rust:1.75-alpine as builder
WORKDIR /app
# Install build dependencies for Alpine (musl, openssl)
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
COPY . .
# Build release binary
RUN cargo build --release

# Runtime Stage
FROM alpine:latest
WORKDIR /app
# Install runtime dependencies
RUN apk add --no-cache libgcc openssl ca-certificates
COPY --from=builder /app/target/release/mail-server /app/mail-server
# Copy .env if needed (user might mount it instead, but copying for simplicity)
COPY .env.example .env

EXPOSE 8080 2525
CMD ["./mail-server"]
