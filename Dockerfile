FROM rust:alpine3.23 AS builder

RUN apk add --no-cache \
    build-base \
    musl-dev \
    binutils \
    nodejs \
    npm \
    && npm install -g pnpm \
    && rustup target add x86_64-unknown-linux-musl

WORKDIR /build
COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl \
    && strip /build/target/x86_64-unknown-linux-musl/release/ysm_upload

FROM alpine:3.23 AS runtime

RUN apk add --no-cache ca-certificates

WORKDIR /data
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/ysm_upload /usr/local/bin/ysm_upload

VOLUME ["/data"]
EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/ysm_upload"]