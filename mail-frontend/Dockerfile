# Build Stage
# Bun official image is based on Debian (glibc)
FROM oven/bun:1 as builder
WORKDIR /app
# If you use bun.lock (new) instead of bun.lockb (old binary), use this:
COPY package.json bun.lock ./
RUN bun install
COPY . .
RUN bun run build

# Runtime Stage (Debian-based Nginx)
FROM nginx:bookworm
# Copy build output to Nginx html folder
COPY --from=builder /app/dist /usr/share/nginx/html
# Expose port 80
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
