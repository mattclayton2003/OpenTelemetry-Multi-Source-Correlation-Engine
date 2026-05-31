FROM lukemathwalker/cargo-chef:latest-rust-1.88 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p accounts

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/accounts /usr/local/bin/accounts
EXPOSE 8002
ENTRYPOINT ["/usr/local/bin/accounts"]
