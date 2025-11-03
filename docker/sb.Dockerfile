FROM rust:1.91-slim-bookworm AS builder

WORKDIR /app

RUN apt update && apt install -y \
    build-essential \
    libssl-dev \
    libpq-dev \
    libsqlite3-dev \
    ca-certificates \
    && apt clean \
    && update-ca-certificates

COPY . .
RUN cd csml_server && cargo build --release --features csml_engine/mongo,csml_engine/dynamo,csml_engine/postgresql,csml_engine/sqlite

FROM debian:bookworm-slim AS final

ENV RUST_VERSION=1.91

RUN apt update && apt install -y ca-certificates libssl3 libpq5 libsqlite3-0 \
    && apt clean \
    && update-ca-certificates

WORKDIR /app

COPY --from=builder /app/target/release/csml_server server
COPY --from=builder /app/csml_server/static static
RUN chmod 755 server
EXPOSE 5000
CMD ./server
