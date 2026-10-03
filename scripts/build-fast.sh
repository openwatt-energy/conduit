#!/bin/bash
# Fast conduit build — compiles Rust in a container with persistent target/ cache.
# First run: ~4 min.  Subsequent runs: ~10-30s.
# No host-level Rust installation needed.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="/home/conduit-target"

echo "Building conduitd (incremental, target dir: $TARGET_DIR)..."

sudo mkdir -p "$TARGET_DIR"

# Build inside a Rust container — target/ directory persists across runs
sudo docker run --rm \
    -v "${ROOT_DIR}:/app" \
    -v "${TARGET_DIR}:/app/target" \
    -e CARGO_BUILD_JOBS=2 \
    rust:1.98-slim-bookworm \
    bash -c '
        apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
        cd /app
        cargo build --release 2>&1 | tail -3
    '

sudo cp "$TARGET_DIR/release/conduitd" /usr/local/bin/conduitd
sudo chmod 755 /usr/local/bin/conduitd
echo "✓ conduitd built ($(du -h /usr/local/bin/conduitd | cut -f1))"