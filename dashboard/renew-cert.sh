#!/bin/bash
# Run as root via cron (weekly):
#   0 3 * * 0 /opt/conduit/dashboard/renew-cert.sh
set -e

CERT_NAME="openwatt"
CERT_DIR="/etc/letsencrypt/live/$CERT_NAME"
TARGET_DIR="/etc/conduit/certs"

echo "[$(date)] Running certbot renew..."
certbot renew --quiet --no-self-upgrade

if [ -f "$CERT_DIR/fullchain.pem" ]; then
    cp "$CERT_DIR/fullchain.pem" "$TARGET_DIR/cert.pem"
    cp "$CERT_DIR/privkey.pem" "$TARGET_DIR/key.pem"
    chmod 644 "$TARGET_DIR/cert.pem"
    chmod 600 "$TARGET_DIR/key.pem"
    docker restart conduit-tls
    echo "[$(date)] Certs renewed, nginx reloaded."
else
    echo "[$(date)] No renewal needed."
fi