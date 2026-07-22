FROM rust:1.91-slim-bookworm AS builder

ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown

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
ENV RUST_LOG=warn,csml_server=info,csml_engine=info,csml_interpreter=warn,hyper=off,h2=off,reqwest=off,tokio=off,rustls=off
ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown
ENV SERVICE_VERSION=${GIT_SHA}
ENV BUILD_TIME=${BUILD_TIME}

RUN apt update && apt install -y ca-certificates libssl3 libpq5 libsqlite3-0 \
    && apt clean \
    && update-ca-certificates

WORKDIR /app

COPY --from=builder /app/target/release/csml_server server
COPY --from=builder /app/csml_server/static static
RUN chmod 755 server
EXPOSE 5000
CMD ./server
