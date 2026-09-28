#!/bin/bash
# Deploy Conduit to GKE.
# Needs: gcloud, kubectl, envsubst, a secret.yaml with real tokens

set -euo pipefail

NAMESPACE="${K8S_NAMESPACE:-conduit}"
PROJECT_ID="${PROJECT_ID:-openwatt-prod}"
CLUSTER="${GKE_CLUSTER:-openwatt-prod}"
ZONE="${GKE_ZONE:-europe-west1-b}"
IMAGE="REDACTED_REGISTRY/${PROJECT_ID}/conduit/conduitd"

# ── InfluxDB v2 (prod instance) ──────────────────────────────────
export CONDUIT_INFLUX_URL="https://REDACTED_IP:8086"
export CONDUIT_INFLUX_ORG="OpenWatt"
export CONDUIT_INFLUX_BUCKET="openwatt_test"

# ── Registry (v2.openwatt.eu) ─────────────────────────────────────
export CONDUIT_REGISTRY_URL="https://v2.openwatt.eu/api/devices/authorized"

# ── GKE context ───────────────────────────────────────────────────
gcloud container clusters get-credentials "${CLUSTER}" --zone "${ZONE}"

# ── Create namespace if needed ────────────────────────────────────
kubectl get ns "${NAMESPACE}" >/dev/null 2>&1 || kubectl create ns "${NAMESPACE}"

# ── Apply secrets (from file) ─────────────────────────────────────
echo "→ applying secrets"
kubectl apply -f deploy/gke/secret.yaml -n "${NAMESPACE}"

# ── Apply deployment ──────────────────────────────────────────────
echo "→ applying deployment (bucket=${CONDUIT_INFLUX_BUCKET})"
envsubst < deploy/gke/deployment.yaml | kubectl apply -f - -n "${NAMESPACE}"

# ── Apply service ─────────────────────────────────────────────────
echo "→ applying service"
kubectl apply -f deploy/gke/service.yaml -n "${NAMESPACE}"

echo ""
echo "Waiting for external IP..."
kubectl get svc conduit -n "${NAMESPACE}" -w