# Conduit — Deployment

## Architecture

```
dongle ──MQTT (:1883)──┐
dongle ──MQTT (:8883)──┤
                        ▼
     ┌──────────────────────────────┐
     │  GCE e2-micro (conduitd)     │
     │  ┌────────────────────────┐  │
     │  │ auth + parse           │──┼──► InfluxDB v2 (REDACTED_IP:8086)
     │  │ command downlink       │  │    org=OpenWatt, bucket=openwatt_test
     │  └────────────────────────┘  │
     │  API (:9090)                 │◄── v2.openwatt.eu (device registry)
     └──────────────────────────────┘
              ▲
              │
        conduit.openwatt.eu
```

## Prod config

| Config | Value |
|---|---|
| InfluxDB URL | `https://REDACTED_IP:8086` (internal VPC) |
| InfluxDB org | `OpenWatt` |
| InfluxDB bucket | `openwatt_test` |
| Registry | `https://v2.openwatt.eu/api/devices/authorized` |
| DNS | `conduit.openwatt.eu` |
| VM | e2-micro (2 vCPU, 1 GB), europe-west1-b |

## Estimated monthly cost

| Item | ~$/mo |
|---|---|
| e2-micro VM | $8 |
| 10 GB boot disk | $1 |
| Static IP | free |
| **Total** | **~$9** |

## Deploy (GCE VM)

### Prerequisites

```bash
gcloud config set project openwatt-prod
gcloud services enable compute.googleapis.com artifactregistry.googleapis.com
gcloud auth configure-docker europe-west1-docker.pkg.dev
```

### 1. Build and push the Docker image

```bash
cd /path/to/conduit
docker build -f docker/Dockerfile -t europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest .
docker push europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest
```

### 2. Reserve a static IP

```bash
gcloud compute addresses create conduit-mqtt \
  --region europe-west1

gcloud compute addresses describe conduit-mqtt \
  --region europe-west1 --format='value(address)'
# → 34.xx.xx.xx
```

### 3. Create firewall rule

```bash
gcloud compute firewall-rules create conduit-mqtt \
  --allow tcp:1883,tcp:8883,tcp:9090 \
  --target-tags conduit \
  --source-ranges 0.0.0.0/0
```

### 4. Create and start the VM

```bash
# Replace secrets with real values
CONDUIT_AUTH_SALT="..."
CONDUIT_INFLUX_TOKEN="..."
CONDUIT_API_ADMIN_TOKEN="..."
CONDUIT_REGISTRY_API_KEY="..."

gcloud compute instances create-with-container conduit \
  --zone europe-west1-b \
  --machine-type e2-micro \
  --container-image europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest \
  --container-restart-policy always \
  --container-env RUST_LOG=conduitd=info \
  --container-env CONDUIT_PLAIN_ENABLED=1 \
  --container-env CONDUIT_PLAIN_ADDR=0.0.0.0:1883 \
  --container-env CONDUIT_TLS_ADDR=0.0.0.0:8883 \
  --container-env CONDUIT_API_ADDR=0.0.0.0:9090 \
  --container-env CONDUIT_INFLUX_URL=https://REDACTED_IP:8086 \
  --container-env CONDUIT_INFLUX_ORG=OpenWatt \
  --container-env CONDUIT_INFLUX_BUCKET=openwatt_test \
  --container-env CONDUIT_REGISTRY_URL=https://v2.openwatt.eu/api/devices/authorized \
  --container-env CONDUIT_REGISTRY_RELOAD_SECS=300 \
  --container-env CONDUIT_AUTH_SALT="${CONDUIT_AUTH_SALT}" \
  --container-env CONDUIT_INFLUX_TOKEN="${CONDUIT_INFLUX_TOKEN}" \
  --container-env CONDUIT_API_ADMIN_TOKEN="${CONDUIT_API_ADMIN_TOKEN}" \
  --container-env CONDUIT_REGISTRY_API_KEY="${CONDUIT_REGISTRY_API_KEY}" \
  --address conduit-mqtt \
  --tags conduit \
  --boot-disk-size 10GB \
  --boot-disk-type pd-standard \
  --scopes cloud-platform
```

### 5. Verify

```bash
# Check the container is running
gcloud compute ssh conduit -- docker ps

# Health check
curl http://conduit.openwatt.eu:9090/health

# Status
curl http://conduit.openwatt.eu:9090/status | jq

# View logs
gcloud compute ssh conduit -- docker logs "$(gcloud compute ssh conduit -- docker ps -q)"
```

## DNS — conduit.openwatt.eu

Create an A record pointing at the reserved static IP:

```
conduit.openwatt.eu.  A  34.xx.xx.xx
```

Verify:

```bash
dig conduit.openwatt.eu
curl http://conduit.openwatt.eu:9090/health
```

## TLS

Once DNS resolves, get a Let's Encrypt cert and use it with conduit on port 8883:

```bash
# On the VM:
certbot certonly --standalone -d conduit.openwatt.eu

# Update the running container with TLS cert paths:
gcloud compute instances update-container conduit \
  --zone europe-west1-b \
  --container-env CONDUIT_TLS_CERT_CHAIN=/etc/letsencrypt/live/conduit.openwatt.eu/fullchain.pem \
  --container-env CONDUIT_TLS_KEY_FILE=/etc/letsencrypt/live/conduit.openwatt.eu/privkey.pem
```

For mTLS, also set `CONDUIT_TLS_CA_BUNDLE` and `CONDUIT_TLS_REQUIRE_CLIENT_CERT=1`.

## Update the container

```bash
docker build -f docker/Dockerfile -t europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest .
docker push europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest

gcloud compute instances update-container conduit \
  --zone europe-west1-b \
  --container-image europe-west1-docker.pkg.dev/openwatt-prod/conduit/conduitd:latest
```

## Rollback

```bash
gcloud compute instances reset conduit --zone europe-west1-b
```

## Logs

```bash
# Last 100 lines
gcloud compute ssh conduit -- docker logs --tail 100 "$(gcloud compute ssh conduit -- docker ps -q)"

# Follow
gcloud compute ssh conduit -- docker logs -f "$(gcloud compute ssh conduit -- docker ps -q)"

# Or via Cloud Logging (if the VM has the logging agent)
gcloud logging read "resource.type=gce_instance AND resource.labels.instance_id=$(gcloud compute instances describe conduit --zone europe-west1-b --format='value(id)')"
```

## Alternative: GKE (for reference)

If you switch to GKE later, the manifests are in `deploy/gke/`. Estimated ~$31/mo
with Autopilot. See the GKE section of the git history for details.