#!/usr/bin/env python3
"""Push conduit status metrics to Cloud Monitoring every 60s."""
import json
import os
import time
import urllib.request

TOKEN = os.environ.get("CONDUIT_API_TOKEN", "")
STATUS_URL = os.environ.get("CONDUIT_STATUS_URL", "http://127.0.0.1:9091/status")
INTERVAL = int(os.environ.get("METRICS_INTERVAL", "60"))
GCP_PROJECT_ID = os.environ.get("GCP_PROJECT_ID", "your-project-id")
GCP_LOCATION = os.environ.get("GCP_LOCATION", "europe-west1-b")

METRICS = {
    "connected_devices": {"name": "connections", "type": "gauge"},
    "messages_processed": {"name": "messages_total", "type": "cumulative"},
    "auth_failures": {"name": "auth_failures_total", "type": "cumulative"},
    "parse_errors": {"name": "parse_errors_total", "type": "cumulative"},
    "influx_flushed": {"name": "influx_flushed_total", "type": "cumulative"},
    "influx_failed": {"name": "influx_failed_total", "type": "cumulative"},
    "registry_entries": {"name": "registry_entries", "type": "gauge"},
}


def get_status():
    req = urllib.request.Request(STATUS_URL, headers={"Authorization": f"Bearer {TOKEN}"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read())


def get_token():
    try:
        req = urllib.request.Request(
            "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token?scopes=https://www.googleapis.com/auth/cloud-platform",
            headers={"Metadata-Flavor": "Google"},
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read())["access_token"]
    except Exception:
        return ""


def push_metrics(data, gcp_token):
    if not gcp_token:
        return

    now = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    start = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - INTERVAL))
    series = []

    for status_key, cfg in METRICS.items():
        if status_key not in data:
            continue
        value = data[status_key]
        is_cumulative = cfg["type"] == "cumulative"
        point_interval = {"startTime": start, "endTime": now} if is_cumulative else {"endTime": now}

        series.append({
            "metric": {
                "type": f"custom.googleapis.com/openwatt/conduit/{cfg['name']}",
                "labels": {"instance": "conduit-prod"},
            },
            "resource": {
                "type": "generic_task",
                "labels": {
                    "project_id": GCP_PROJECT_ID,
                    "location": GCP_LOCATION,
                    "namespace": "openwatt",
                    "job": "conduit",
                    "task_id": "conduit-vm",
                },
            },
            "metricKind": "GAUGE" if cfg["type"] == "gauge" else "CUMULATIVE",
            "valueType": "INT64",
            "points": [{
                "interval": point_interval,
                "value": {"int64Value": int(value)},
            }],
        })

    body = json.dumps({"timeSeries": series}).encode()
    req = urllib.request.Request(
        f"https://monitoring.googleapis.com/v3/projects/{GCP_PROJECT_ID}/timeSeries",
        data=body, method="POST",
        headers={
            "Authorization": f"Bearer {gcp_token}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            if resp.status == 200:
                print(f"[{now}] pushed {len(series)} metrics")
            else:
                print(f"[{now}] error: {resp.status} {resp.read()[:200]}")
    except Exception as e:
        print(f"[{now}] push failed: {e}")


def main():
    print("conduit-metrics sidecar started")
    while True:
        try:
            data = get_status()
            token = get_token()
            push_metrics(data, token)
        except Exception as e:
            print(f"[{time.strftime('%H:%M:%S')}] scrape error: {e}")
        time.sleep(INTERVAL)


if __name__ == "__main__":
    main()