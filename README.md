<p align="center">
  <img src="logo.svg" alt="Conduit logo" width="100">
  &nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://openwatt.be">
    <img src="openwatt-logo.svg" alt="OpenWatt" width="240">
  </a>
</p>

<h1 align="center">conduit</h1>
<p align="center"><strong>A Rust MQTT broker purpose-built for smart meter telemetry.</strong><br>
Authenticates, parses, and writes directly to InfluxDB — no intermediate APIs, no message queues, no overhead.</p>

<p align="center">
  <img alt="GitHub CI" src="https://github.com/openwatt-energy/conduit/actions/workflows/ci.yml/badge.svg">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
  <img alt="Rust" src="https://img.shields.io/badge/rust-1.98+-orange">
</p>

---

## Why Conduit?

### &#9889; Dynamic credential loading
Device registry fetched from your backend API on startup and reloaded every 5 minutes. **Add a dongle and it authenticates automatically — no restarts, no credential file edits.** Supports multiple auth schemes: SHA256-salted (v1.0.47+), plain-password (1st-gen), and configurable fallback for unknown devices.

### &#128640; Direct InfluxDB writes
Telemetry is parsed and written straight to InfluxDB with **no intermediate REST API**. The legacy pattern — MQTT → bridge → API → InfluxDB — is collapsed into a single process. **Zero network hops between parse and write.**

### &#9881; Config-driven field mapping
Field mappings live in `configs/p1m5.json` and `configs/smartstuff.json`. **Add a new meter field without touching Rust code** — just add the OBIS code, target name, type, and optional scale factor. Compiled into the binary via `include_str!` for zero runtime overhead.

### &#9961; Multi-protocol parser
Handles **three telemetry formats simultaneously**: raw DSMR text, JSON from v1.0.47+ firmware, and SmartStuff/BEPR JSON. Auto-detection by topic prefix and payload type. All parsed through a unified pipeline into a consistent InfluxDB schema.

### &#128274; Layered authentication
Static device auth (1st-gen, plain password), then SHA256-based auth (P1M5 firmware), then configurable fallback for unregistered devices. **Dongles never get locked out** while the registry sync catches up.

### &#128221; Built-in observability
Live HTML dashboard with real-time metrics, auth failure tracking, and event log. Optional Prometheus metrics and GCP Cloud Monitoring integration. Docker log rotation built-in.

---

## Conduit vs Mosquitto + mqtt2api

| | Legacy (Mosquitto + mqtt2api) | Conduit |
|---|---|---|
| Language | Python (mqtt2api) + C (Mosquitto) | **Rust** |
| Binary size | ~500 MB (Docker image) | **11 MB** binary + 90 MB image |
| Memory (idle) | ~200 MB | **~30 MB** |
| Data path | MQTT → mqtt2api → API → InfluxDB | **MQTT → InfluxDB** |
| Parse latency | ~50 ms per message | **<1 ms** per message |
| Credential changes | Edit password file, restart Mosquitto | **Auto-reload from API every 5 min** |
| New fields | Edit Python code, redeploy | **Edit JSON config, rebuild (~20s)** |

---

## Quick start

```bash
cp .env.example prod.env    # edit with your credentials
./deploy.sh                 # builds from source (~30s incremental)
```

First build takes ~4 min (downloading Rust crates). Subsequent builds are ~20-30s courtesy of Docker layer caching.

---

## Architecture

| Port | Protocol | Service | Purpose |
|---|---|---|---|
| 1883 | MQTT plain | conduit | Dongle connections |
| 8883 | MQTT TLS | nginx → conduit:8884 | TLS-terminated connections |
| 9090 | HTTP | nginx → conduit:9091 | Dashboard + API proxy |

---

## API

| Endpoint | Auth | Description |
|---|---|---|
| `GET /health` | None | Liveness probe |
| `GET /status` | Bearer | Full MQTT stats (JSON) |
| `GET /metrics` | Bearer | Prometheus metrics |
| `POST /admin/reload` | Bearer | Reload device registry |
| `GET /admin/clients` | Bearer | All authorized devices |

```bash
curl -H "Authorization: Bearer <token>" http://localhost:9090/status
```

---

## Configuration

Key env vars (see `.env.example` for the full list):

| Variable | Purpose |
|---|---|
| `CONDUIT_AUTH_SALT` | SHA256 password salt (must match firmware) |
| `CONDUIT_INFLUX_BUCKET` | InfluxDB bucket |
| `CONDUIT_INFLUX_TOKEN` | InfluxDB write token |
| `CONDUIT_API_ADMIN_TOKEN` | Bearer token for API endpoints |
| `CONDUIT_REGISTRY_URL` | Backend API for device registry |
| `CONDUIT_REGISTRY_API_KEY` | Registry API authentication key |

---

## Files of interest

| Path | Purpose |
|---|---|
| `configs/p1m5.json` | P1M5 field mappings |
| `configs/smartstuff.json` | SmartStuff field mappings |
| `configs/be-devices.json` | 1st-gen device auth |
| `src/auth/` | Authentication providers |
| `src/parser/` | Telemetry parsers |
| `dashboard/docker-compose.yml` | Full stack for local dev |
| `dashboard/landing.html` | Commercial page |

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `auth failed` | Dongle not in registry | Wait for registry sync |
| `parse error: unknown data topic` | Unhandled topic pattern | Add to config JSON |
| `ACL denied PUBLISH` | Topic not authorized | Expected for `homeassistant/` |
| `influx_failed > 0` | InfluxDB unreachable or type conflict | Check connectivity + field types |
| `connections=0` | Broker or proxy down | `docker restart conduit conduit-tls` |

---

MIT License &middot; Built by <a href="https://openwatt.be">OpenWatt srl</a>