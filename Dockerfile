# ---- Stage 1: Builder ----
FROM rust:1-slim-bookworm AS builder

# Install build dependencies (pkg-config, libssl for reqwest/TLS, cmake for some native deps)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifest and lockfile first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy all workspace member Cargo.toml files (for dependency resolution)
COPY mclaw/Cargo.toml mclaw/Cargo.toml
COPY mclaw-gateway/Cargo.toml mclaw-gateway/Cargo.toml
COPY mclaw-agent/Cargo.toml mclaw-agent/Cargo.toml
COPY mclaw-skills/Cargo.toml mclaw-skills/Cargo.toml
COPY mclaw-channels/Cargo.toml mclaw-channels/Cargo.toml
COPY mclaw-memory/Cargo.toml mclaw-memory/Cargo.toml
COPY mclaw-permissions/Cargo.toml mclaw-permissions/Cargo.toml
COPY mclaw-wasm/Cargo.toml mclaw-wasm/Cargo.toml
COPY mclaw-mcp/Cargo.toml mclaw-mcp/Cargo.toml

# Create dummy source files so cargo can resolve dependencies and cache them
RUN mkdir -p mclaw/src && echo "fn main() {}" > mclaw/src/main.rs && \
    mkdir -p mclaw-gateway/src && echo "" > mclaw-gateway/src/lib.rs && \
    mkdir -p mclaw-agent/src && echo "" > mclaw-agent/src/lib.rs && \
    mkdir -p mclaw-skills/src && echo "" > mclaw-skills/src/lib.rs && \
    mkdir -p mclaw-channels/src && echo "" > mclaw-channels/src/lib.rs && \
    mkdir -p mclaw-memory/src && echo "" > mclaw-memory/src/lib.rs && \
    mkdir -p mclaw-permissions/src && echo "" > mclaw-permissions/src/lib.rs && \
    mkdir -p mclaw-wasm/src && echo "" > mclaw-wasm/src/lib.rs && \
    mkdir -p mclaw-mcp/src && echo "" > mclaw-mcp/src/lib.rs

# Pre-build dependencies (this layer is cached unless Cargo.toml/Cargo.lock change)
RUN cargo build --release --workspace 2>/dev/null || true

# Now copy actual source code
COPY mclaw/src mclaw/src
COPY mclaw-gateway/src mclaw-gateway/src
COPY mclaw-agent/src mclaw-agent/src
COPY mclaw-skills/src mclaw-skills/src
COPY mclaw-channels/src mclaw-channels/src
COPY mclaw-memory/src mclaw-memory/src
COPY mclaw-permissions/src mclaw-permissions/src
COPY mclaw-wasm/src mclaw-wasm/src
COPY mclaw-mcp/src mclaw-mcp/src

# Copy skill definitions and config
COPY skills/ skills/
COPY config/ config/

# Touch source files to invalidate the dummy build artifacts (not the dependencies)
RUN find . -path ./target -prune -o -name '*.rs' -print | xargs touch

# Build the release binary with LTO for small binary size
ENV CARGO_PROFILE_RELEASE_LTO=true
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL="z"
ENV CARGO_PROFILE_RELEASE_STRIP="symbols"

RUN cargo build --release --bin mclaw

# Verify the binary exists
RUN test -f /build/target/release/mclaw

# ---- Stage 2: Runtime ----
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy the built binary
COPY --from=builder /build/target/release/mclaw ./mclaw

# Copy skill definitions and default config
COPY --from=builder /build/skills/ ./skills/
COPY --from=builder /build/config/ ./config/

EXPOSE 18789

ENTRYPOINT ["./mclaw", "run"]
