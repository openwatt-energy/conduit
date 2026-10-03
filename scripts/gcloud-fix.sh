#!/bin/bash
# One-command gcloud auth fix — run whenever gcloud SSH breaks.
# Usage:  ./gcloud-fix.sh
set -e

echo "=== gcloud auth fix ==="

# Check if already working
if gcloud compute ssh conduit --zone europe-west1-b --command 'echo ok' 2>/dev/null | grep -q ok; then
    echo "✅ Already authenticated and working."
    exit 0
fi

# Check which project we need
echo "Checking projects..."
PROJECTS=$(gcloud projects list --format="value(projectId)" 2>/dev/null | head -5)
if echo "$PROJECTS" | grep -q powertrader; then
    gcloud config set project powertrader-399210
else
    echo "⚠  powertrader-399210 not in your project list."
    echo "   Run: gcloud auth login"
    exit 1
fi

# Try available accounts
for ACCT in $(gcloud auth list --format="value(account)" 2>/dev/null); do
    echo "Trying: $ACCT"
    gcloud config set account "$ACCT" 2>/dev/null
    if gcloud compute ssh conduit --zone europe-west1-b --command 'echo ok' 2>/dev/null | grep -q ok; then
        echo "✅ $ACCT works! Saved as default."
        exit 0
    fi
done

# Nothing works — prompt user
echo ""
echo "❌ No account works. Run:"
echo "   gcloud auth login"
echo "   gcloud config set project powertrader-399210"
echo ""
echo "Then re-run: ./gcloud-fix.sh"