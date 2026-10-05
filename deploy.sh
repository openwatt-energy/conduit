#!/bin/bash
# Conduit deploy — builds binary incrementally (~10s), restarts container.
#   ./deploy.sh       # prod (default)
#   ./deploy.sh test  # test instance
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="/home/conduit-target"
ENV=${1:-prod}

ENV_FILE="${ENV}.env"
[ -f "$ENV_FILE" ] || { echo "ERROR: env file not found: $ENV_FILE"; exit 1; }
set -a; source "$ENV_FILE"; set +a

NAME="conduit$( [ "$ENV" != "prod" ] && echo "-$ENV" )"
BINARY="$TARGET_DIR/release/conduitd"

echo "=== $NAME ($ENV) — $CONDUIT_INFLUX_BUCKET ==="

# ── Build ───────────────────────────────────────────────────────
echo "[1/3] Building..."
sudo mkdir -p "$TARGET_DIR"
sudo docker run --rm \
    -v "${ROOT_DIR}:/app" -v "${TARGET_DIR}:/app/target" \
    -e CARGO_BUILD_JOBS=2 \
    rust:1.98-slim-bookworm bash -c '
        apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev
        rm -rf /var/lib/apt/lists/*
        cd /app
        cargo build --release 2>&1 | tail -1
    '
echo "  ✓ $(du -h "$BINARY" | cut -f1)"

# ── Docker image from binary ────────────────────────────────────
echo "[2/3] Packaging..."
sudo docker build -t "conduitd:${ENV}" --build-arg BINARY="$BINARY" -f - "${ROOT_DIR}" <<'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && useradd --no-create-home --shell /bin/false conduit
COPY /target/release/conduitd /usr/local/bin/conduitd
USER conduit
EXPOSE 1883 8883 9090
CMD ["conduitd"]
DOCKERFILE

# ── Restart ─────────────────────────────────────────────────────
echo "[3/3] Restarting..."
sudo docker rm -f "$NAME" 2>/dev/null || true
sudo docker run -d \
    --name "$NAME" --restart unless-stopped \
    --log-opt max-size=10m --log-opt max-file=3 \
    -p "${MQTT_PORT}:1883" -p "127.0.0.1:${MQTT_PROXY_PORT}:8883" -p "127.0.0.1:${API_PORT}:9090" \
    -e RUST_LOG="$RUST_LOG" -e CONDUIT_PLAIN_ENABLED="$CONDUIT_PLAIN_ENABLED" \
    -e CONDUIT_PLAIN_ADDR="$CONDUIT_PLAIN_ADDR" -e CONDUIT_TLS_ADDR="$CONDUIT_TLS_ADDR" \
    -e CONDUIT_API_ADDR="$CONDUIT_API_ADDR" -e CONDUIT_API_ADMIN_TOKEN="$CONDUIT_API_ADMIN_TOKEN" \
    -e CONDUIT_INFLUX_URL="$CONDUIT_INFLUX_URL" -e CONDUIT_INFLUX_ORG="$CONDUIT_INFLUX_ORG" \
    -e CONDUIT_INFLUX_BUCKET="$CONDUIT_INFLUX_BUCKET" -e CONDUIT_INFLUX_TOKEN="$CONDUIT_INFLUX_TOKEN" \
    -e CONDUIT_REGISTRY_URL="$CONDUIT_REGISTRY_URL" -e CONDUIT_REGISTRY_RELOAD_SECS="$CONDUIT_REGISTRY_RELOAD_SECS" \
    -e CONDUIT_REGISTRY_API_KEY="$CONDUIT_REGISTRY_API_KEY" -e CONDUIT_AUTH_SALT="$CONDUIT_AUTH_SALT" \
    "conduitd:${ENV}"
sudo docker restart conduit-tls 2>/dev/null || true

sleep 3
curl -s --max-time 3 "http://127.0.0.1:${API_PORT}/health" && echo "  ✓ running" || echo "  ⚠  check logs"
echo ""