<p align="center">
  <img src="logo.svg" alt="Conduit logo" width="120" height="120">
</p>

# conduit — OpenWatt MQTT broker

Rust MQTT broker replacing Mosquitto for the OpenWatt EMS. Authenticates
P1M5, SmartStuff, and 1st-gen dongles, parses JSON and DSMR telemetry,
and writes directly to InfluxDB.

Runs on a lightweight VM (e.g. GCE COS) or any container host.

## Quick deploy

```bash
./scripts/auth.sh       # one-time gcloud auth (uses SA key — never expires)
./deploy.sh             # prod (builds from source, ~30s incremental)
./deploy.sh test        # test instance (openwatt_test bucket, port 1884)
```

First build takes ~4 min (downloading Rust crates). Subsequent builds are
~20-30s thanks to Docker layer caching.

## Restart

```bash
sudo docker restart conduit          # no rebuild
sudo docker restart conduit-tls      # nginx TLS proxy
sudo docker restart conduit-metrics  # monitoring sidecar
```

## View logs

```bash
sudo docker logs -f conduit          # follow logs
sudo docker logs conduit --tail 50   # last 50 lines
sudo docker logs conduit | grep "auth failed"
sudo docker logs conduit | grep "unregistered device"  # P1M5 fallback auth
sudo docker logs conduit | grep "parse error"           # parser issues
```

## Architecture

| Port | Protocol | Service | Purpose |
|---|---|---|---|
| 1883 | MQTT plain | conduit | Dongle connections (no TLS) |
| 8883 | MQTT TLS | nginx → conduit:8884 | TLS-terminated dongle connections |
| 9090 | HTTP | nginx → conduit:9091 | Dashboard + API proxy |
| 9091 | HTTP | conduit (internal) | API health endpoint |

### Containers

| Container | Image | Role |
|---|---|---|
| `conduit` | `conduitd:latest` | MQTT broker |
| `conduit-tls` | `nginx:alpine` | TLS termination + dashboard proxy |
| `conduit-metrics` | `conduit-metrics:latest` | Cloud Monitoring sidecar |

## Configuration

Key env vars (see `prod.env` / `test.env` for full list):

| Variable | Purpose |
|---|---|
| `CONDUIT_AUTH_SALT` | SHA256 password salt (must match firmware `SALT_STRING`) |
| `CONDUIT_INFLUX_BUCKET` | InfluxDB bucket (`openwatt` / `openwatt_test`) |
| `CONDUIT_INFLUX_TOKEN` | InfluxDB write token |
| `CONDUIT_API_ADMIN_TOKEN` | Bearer token for API endpoints |
| `CONDUIT_REGISTRY_URL` | Backend API for device registry |
| `CONDUIT_REGISTRY_API_KEY` | Backend API key |

## Health dashboard

```
http://localhost:9090/
```

Shows real-time metrics (8 cards), recent auth failures table, and prod/test
tabs switching between instances.

## API endpoints

| Endpoint | Auth | Description |
|---|---|---|
| `GET /health` | None | Liveness probe |
| `GET /status` | Bearer | Full MQTT stats (JSON) |
| `GET /metrics` | Bearer | Prometheus metrics |
| `POST /admin/reload` | Bearer | Reload device registry from backend |
| `GET /admin/clients` | Bearer | All authorized devices |

Token: set via `CONDUIT_API_ADMIN_TOKEN` env var (see `.env.example`).

```bash
curl -H "Authorization: Bearer <token>" http://localhost:9090/status
```

## Files of interest

| Path | Purpose |
|---|---|
| `configs/p1m5.json` | P1M5 field mappings (OBIS → InfluxDB) |
| `configs/smartstuff.json` | SmartStuff field mappings |
| `configs/be-devices.json` | 1st-gen device MACs with hardcoded credentials |
| `src/auth/` | Authentication providers (PasswordAuth, StaticDeviceAuth) |
| `src/parser/` | Telemetry parsers (DSMR, JSON, SmartStuff) |
| `scripts/conduit-metrics.py` | Cloud Monitoring sidecar |
| `dashboard/health.html` | HTML dashboard |
| `dashboard/nginx.conf` | nginx proxy config |
| `dashboard/docker-compose.yml` | Full stack for local development |
| `prod.env` / `test.env` | Environment variables |

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `auth failed: user="P1..."` | Dongle not in registry | Wait for registry sync, or see fallback below |
| `unregistered device accepted via fallback` | P1M5 dongle accepted without registry entry | Normal — device exists in Supabase but not v2 assets |
| `parse error: unknown data topic` | Unhandled topic pattern | Add to config JSON or parser dispatch |
| `ACL denied PUBLISH` | Topic not authorized for device | Expected for `homeassistant/`; unexpected for `BEPR/` |
| `influx_failed > 0` | InfluxDB unreachable or type conflict | Check InfluxDB connectivity and field types |
| `connections=0` | Conduit or nginx down | `sudo docker restart conduit conduit-tls` |
| Dashboard 502 | `conduit` stopped but `conduit-tls` running | `sudo docker start conduit` |

## Alerting

Cloud Monitoring alerts (GCP):

| Alert | Severity | Condition |
|---|---|---|
| No connected dongles | CRITICAL | `connections < 100` for 5 min |
| Auth failures | WARNING | `rate(auth_failures) > 50/min` |
| InfluxDB failures | CRITICAL | `rate(influx_failed) > 0/min` |
| Parse errors | WARNING | `rate(parse_errors) > 50/min` |

Metrics are pushed every 60s by `conduit-metrics` to
`custom.googleapis.com/openwatt/conduit/*`.