#!/bin/bash
# Fast incremental build — pulls previous image for layer caching.
# First build: ~4 min (deps) + ~2 min (compile).  Subsequent: ~30s.
set -e

IMAGE="europe-west1-docker.pkg.dev/powertrader-399210/conduit/conduitd:latest"

echo "Pulling previous image for layer cache..."
docker pull "$IMAGE" 2>/dev/null || echo "(no previous image, full build)"

echo "Building..."
docker build \
    -f docker/Dockerfile \
    --cache-from "$IMAGE" \
    -t "$IMAGE" \
    .

if [ "${1:-}" = "push" ]; then
    echo "Pushing..."
    docker push "$IMAGE"
fi

echo "Done. Image: $IMAGE"