FROM docker.io/rust:1-slim-bookworm AS build
ARG pkg=niobium
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    cargo build --release; \
    objcopy --compress-debug-sections target/release/$pkg ./niobium

FROM docker.io/debian:bookworm-slim
RUN apt-get update && apt-get install -y curl
WORKDIR /app
COPY --from=build /build/niobium ./
COPY static ./static
COPY templates ./templates
COPY niobium.config.sample ./
COPY niobium_collections.config.sample ./
COPY --chmod=0755 docker-entrypoint.sh /

EXPOSE 8000

ENTRYPOINT ["/docker-entrypoint.sh"]

CMD ["./niobium"]