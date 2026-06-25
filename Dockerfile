# Multi-stage build for the LakeCat Iceberg REST catalog service.
#
# Default features only: memory store, allow-all governance, deferred Sail,
# no-op graph sink. Enough to serve the Iceberg REST surface and accept
# commits. Pass --build-arg FEATURES="turso-local" for a durable local store.
FROM rust:1.92-bookworm AS builder

ARG FEATURES=""
WORKDIR /build

# Copy the whole workspace (sibling repos are not required for the default
# feature set; sail-local/typesec-local/grust-local are intentionally off).
COPY . .

# The default-feature service does not need the sibling Sail/Grust source trees:
#  - lakecat-sail and lakecat-cli reference Sail by relative path -> drop them as
#    workspace members.
#  - grust-graph / grust-turso are published on crates.io (0.10.0) -> drop the
#    local `path =` so they resolve from the registry.
#  - lakecat-service's Sail dev-dependencies are test-only -> drop them.
RUN sed -i '/crates\/lakecat-sail",/d; /crates\/lakecat-cli",/d' Cargo.toml \
 && sed -i 's| path = "../grust/crates/grust",||; s| path = "../grust/crates/grust-turso",||' Cargo.toml \
 && sed -i '/^sail-catalog-iceberg = { path = /d; /^sail-iceberg = { path = /d' crates/lakecat-service/Cargo.toml

RUN if [ -n "$FEATURES" ]; then \
        cargo build --release -p lakecat-service --features "$FEATURES"; \
    else \
        cargo build --release -p lakecat-service; \
    fi

FROM debian:bookworm-slim AS runtime

RUN useradd --system --uid 10001 --create-home lakecat
COPY --from=builder /build/target/release/lakecat-service /usr/local/bin/lakecat-service

# Containers must listen on all interfaces to be reachable; the binary default
# is 127.0.0.1:8181 which is unreachable from outside the container.
ENV LAKECAT_BIND_ADDR=0.0.0.0:8181 \
    LAKECAT_WAREHOUSE=local

USER lakecat
EXPOSE 8181
ENTRYPOINT ["/usr/local/bin/lakecat-service"]
