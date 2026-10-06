FROM rust:1-alpine AS core-policy
WORKDIR /build
RUN apk add --no-cache build-base cmake pkgconf openssl-dev
COPY Cargo.toml Cargo.lock ./
COPY sorrel-core ./sorrel-core
COPY sorrel-cli ./sorrel-cli
COPY sorrel-runners ./sorrel-runners
COPY sorrel-sdk-rust ./sorrel-sdk-rust
RUN cargo build --locked --release -p sorrel-core --bin sorrel-core-policy

FROM node:22-alpine

WORKDIR /app

LABEL org.opencontainers.image.title="Sorrel Hub API" \
      org.opencontainers.image.description="Sorrel collaboration API and sync server" \
      org.opencontainers.image.source="https://github.com/MGRAFF2006/sorrel" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"

COPY --chown=node:node sorrel-hub/package.json ./
COPY --chown=node:node sorrel-hub/package-lock.json ./
COPY --chown=node:node sorrel-hub/src ./src
COPY --from=core-policy /build/target/release/sorrel-core-policy /usr/local/bin/sorrel-core-policy
RUN apk add --no-cache libgcc libstdc++ libssl3 libcrypto3

RUN mkdir -p /app/data && chown node:node /app/data

ENV SORREL_HUB_CORE_POLICY_BIN=/usr/local/bin/sorrel-core-policy
ENV NODE_ENV=production
ENV PORT=3000
ENV SORREL_HUB_DATA_DIR=/app/data/sync
ENV SORREL_HUB_METADATA_DIR=/app/data/metadata

EXPOSE 3000

VOLUME ["/app/data"]

USER node

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:' + (process.env.PORT ?? '3000') + '/healthz').then((response) => { if (!response.ok) process.exit(1); }).catch(() => process.exit(1))"]

CMD ["node", "src/server.js"]
