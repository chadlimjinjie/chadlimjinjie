FROM rust:1-slim AS builder
WORKDIR /app
# Pre-build dependencies against a stub to cache the layer.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release || true
# migrations/ and templates/ are compiled into the binary
COPY . .
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/auth-service .
EXPOSE 4112
CMD ["./auth-service"]
