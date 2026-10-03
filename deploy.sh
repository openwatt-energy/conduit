#!/bin/bash
# Conduit deploy script — pulls latest image and (re)starts a container.
#
# Usage:
#   ./deploy.sh            # deploy prod (default)
#   ./deploy.sh test       # deploy test instance
#
# Requires Docker auth to Artifact Registry:
#   TOKEN=$(curl -s -H "Metadata-Flavor: Google" \
#     "http://metadata.google.internal/.../token?scopes=https://www.googleapis.com/auth/cloud-platform" \
#     | python3 -c "import sys,json; print(json.load(sys.stdin)['access_token'])")
#   echo "{\"auths\":{\"europe-west1-docker.pkg.dev\":{\"auth\":\"$(echo -n oauth2accesstoken:$TOKEN | base64 -w0)\"}}}" \
#     | sudo tee /tmp/.docker/config.json > /dev/null

set -e

# Auto-auth Docker to Artifact Registry (GCE metadata server)
docker_auth() {
    if [ -f /tmp/.docker/config.json ]; then return 0; fi
    TOKEN=$(curl -sf -H "Metadata-Flavor: Google" \
        "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token?scopes=https://www.googleapis.com/auth/cloud-platform" \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['access_token'])" 2>/dev/null) || true
    if [ -n "$TOKEN" ]; then
        AUTH=$(echo -n "oauth2accesstoken:${TOKEN}" | base64 -w0)
        sudo mkdir -p /tmp/.docker
        echo "{\"auths\":{\"europe-west1-docker.pkg.dev\":{\"auth\":\"${AUTH}\"}}}" | sudo tee /tmp/.docker/config.json > /dev/null
    fi
}
docker_auth

ENV=${1:-prod}
ENV_FILE="${ENV}.env"

if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: env file not found: $ENV_FILE"
    exit 1
fi

set -a; source "$ENV_FILE"; set +a
NAME="conduit${ENV:+-$ENV}"
NAME="${NAME/prod/conduit}"  # "conduit-prod" → "conduit"

echo "=== Deploying $NAME ($ENV) ==="
echo "  image:  $IMAGE"
echo "  bucket: $CONDUIT_INFLUX_BUCKET"
echo "  mqtt:   $MQTT_PORT (plain) / $TLS_PORT (tls)"
echo ""

# Pull
echo "[1/3] Pulling image..."
if [ -f /tmp/.docker/config.json ]; then
    sudo DOCKER_CONFIG=/tmp/.docker docker pull "$IMAGE"
else
    sudo docker pull "$IMAGE"
fi

# (Re)create
echo "[2/3] Recreating container..."
sudo docker rm -f "$NAME" 2>/dev/null || true

sudo docker run -d \
    --name "$NAME" \
    --restart unless-stopped \
    -p "${MQTT_PORT}:1883" \
    -p "127.0.0.1:${MQTT_PROXY_PORT}:8883" \
    -p "127.0.0.1:${API_PORT}:9090" \
    -e RUST_LOG="$RUST_LOG" \
    -e CONDUIT_PLAIN_ENABLED="$CONDUIT_PLAIN_ENABLED" \
    -e CONDUIT_PLAIN_ADDR="$CONDUIT_PLAIN_ADDR" \
    -e CONDUIT_TLS_ADDR="$CONDUIT_TLS_ADDR" \
    -e CONDUIT_API_ADDR="$CONDUIT_API_ADDR" \
    -e CONDUIT_API_ADMIN_TOKEN="$CONDUIT_API_ADMIN_TOKEN" \
    -e CONDUIT_INFLUX_URL="$CONDUIT_INFLUX_URL" \
    -e CONDUIT_INFLUX_ORG="$CONDUIT_INFLUX_ORG" \
    -e CONDUIT_INFLUX_BUCKET="$CONDUIT_INFLUX_BUCKET" \
    -e CONDUIT_INFLUX_TOKEN="$CONDUIT_INFLUX_TOKEN" \
    -e CONDUIT_REGISTRY_URL="$CONDUIT_REGISTRY_URL" \
    -e CONDUIT_REGISTRY_RELOAD_SECS="$CONDUIT_REGISTRY_RELOAD_SECS" \
    -e CONDUIT_REGISTRY_API_KEY="$CONDUIT_REGISTRY_API_KEY" \
    -e CONDUIT_AUTH_SALT="$CONDUIT_AUTH_SALT" \
    "$IMAGE"

# Verify
echo "[3/3] Verifying..."
sleep 3
HEALTH=$(curl -s --max-time 3 "http://127.0.0.1:${API_PORT}/health" 2>/dev/null || echo "FAIL")
if [ "$HEALTH" = '{"status":"ok"}' ]; then
    echo "  ✓ $NAME is healthy"
else
    echo "  ⚠  health check failed: $HEALTH"
    sudo docker logs "$NAME" --tail 5
fi
echo ""
echo "Done. Dashboard: http://$(hostname -I | awk '{print $1}'):9090"