#!/bin/bash
# gcloud authentication helper — tries SA key first, then user accounts.
# Run:  ./scripts/auth.sh   (or source it:  source scripts/auth.sh)
#
# One-time setup (requires your user account with IAM access):
#   gcloud auth login
#   gcloud projects add-iam-policy-binding powertrader-399210 \
#     --member=serviceAccount:secret-cli@powertrader-399210.iam.gserviceaccount.com \
#     --role=roles/compute.instanceAdmin.v1
#
# After that, this script works permanently — SA keys never expire.

set -e

PROJECT="powertrader-399210"
ZONE="europe-west1-b"
VM="conduit"
SA_KEY="${HOME}/src/gitlab/openwatt/secrets/service_accounts/powertrader-399210-29d43501ae69.json"
SA_EMAIL="secret-cli@${PROJECT}.iam.gserviceaccount.com"

gcloud config set project "$PROJECT" --quiet 2>/dev/null

# 1. Try service account key (permanent, never expires)
if [ -f "$SA_KEY" ]; then
    gcloud auth activate-service-account "$SA_EMAIL" --key-file="$SA_KEY" 2>/dev/null && {
        gcloud config set project "$PROJECT" --quiet
        echo "✅ Auth: $SA_EMAIL (SA key — permanent)"
        return 0 2>/dev/null || exit 0
    }
fi

# 2. Try existing user accounts
for ACCT in $(gcloud auth list --format="value(account)" 2>/dev/null); do
    gcloud config set account "$ACCT" --quiet 2>/dev/null
    gcloud config set project "$PROJECT" --quiet 2>/dev/null
    if gcloud compute ssh "$VM" --zone "$ZONE" --command 'echo ok' 2>/dev/null | grep -q ok; then
        echo "✅ Auth: $ACCT (user account)"
        return 0 2>/dev/null || exit 0
    fi
done

# 3. Nothing works — need interactive login
echo ""
echo "❌ No working account found."
echo ""
echo "Service account — needs one-time IAM grant (run with your user account):"
echo "  gcloud auth login"
echo "  gcloud projects add-iam-policy-binding ${PROJECT} \\"
echo "    --member=serviceAccount:${SA_EMAIL} \\"
echo "    --role=roles/compute.instanceAdmin.v1"
echo ""
echo "Or login interactively:"
echo "  gcloud auth login --no-launch-browser"
return 1 2>/dev/null || exit 1