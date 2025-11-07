# -------------------------------
# 1. BUILD STAGE
# -------------------------------
FROM rust:1.82 as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true

COPY . .
RUN cargo build --release

# -------------------------------
# 2. RUNTIME STAGE
# -------------------------------
FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /app/target/release/okey-optimizer /app/okey-optimizer

RUN apt-get update && apt-get install -y ca-certificates && update-ca-certificates && rm -rf /var/lib/apt/lists/*

ENV PORT=10000
EXPOSE 10000

CMD ["/app/okey-optimizer"]
