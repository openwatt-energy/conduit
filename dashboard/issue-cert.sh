#!/bin/bash
# Issue wildcard TLS cert for mqtt.openwatt.eu + mqtt.openwatt.be.
# Requires certbot and DNS access to add _acme-challenge TXT records.
#
# Usage:
#   1. Run: ./dashboard/issue-cert.sh
#   2. Follow prompts to add DNS TXT records
#   3. Cert lands in /etc/letsencrypt/live/openwatt/
#
# After issuance, reload nginx: docker restart conduit-tls
#
set -e

DOMAINS=("*.openwatt.eu" "*.openwatt.be")
CERT_NAME="openwatt"

echo "Requesting wildcard cert for: ${DOMAINS[*]}"

sudo certbot certonly \
  --manual \
  --preferred-challenges dns \
  --agree-tos \
  --email support@openwatt.be \
  --cert-name "$CERT_NAME" \
  -d "${DOMAINS[0]}" -d "${DOMAINS[1]}"

echo ""
echo "✓ Cert issued. Copying to conduit TLS dir..."

CERT_DIR="/etc/letsencrypt/live/$CERT_NAME"
TARGET_DIR="/etc/conduit/certs"

sudo mkdir -p "$TARGET_DIR"
sudo cp "$CERT_DIR/fullchain.pem" "$TARGET_DIR/cert.pem"
sudo cp "$CERT_DIR/privkey.pem" "$TARGET_DIR/key.pem"
sudo chmod 644 "$TARGET_DIR/cert.pem"
sudo chmod 600 "$TARGET_DIR/key.pem"

echo "✓ Certs copied to $TARGET_DIR"
echo "Reload nginx: docker restart conduit-tls"
echo ""
echo "DNS must point mqtt.openwatt.eu and mqtt.openwatt.be to this VM's IP."