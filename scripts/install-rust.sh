#!/bin/bash
# One-time Rust toolchain install for Conduit VM.
# Run once, then use scripts/build-fast.sh for fast incremental builds.
set -e

echo "Installing Rust + build deps..."

# Build dependencies (C linker, SSL)
sudo apt-get update -qq
sudo apt-get install -y -qq pkg-config libssl-dev build-essential

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"

# Verify
rustc --version
cargo --version

echo ""
echo "✓ Rust installed. Next: ./scripts/build-fast.sh"