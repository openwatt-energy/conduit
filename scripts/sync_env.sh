#!/bin/bash
# Set up env vars for sync_supabase_assets.py from GCP secrets.
# Source this:  source scripts/sync_env.sh
# Then run:     python3 scripts/sync_supabase_assets.py --apply
#
# Or combine:   source scripts/sync_env.sh && pip install -q psycopg2-binary supabase && python3 scripts/sync_supabase_assets.py --apply

export SUPABASE_URL="https://eohagurgmhwipdmqwcfw.supabase.co"

# Get Supabase service key from GCP
SUPABASE_KEY=$(gcloud secrets versions access latest --secret supabase-prod 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['service_role_key'])")
export SUPABASE_KEY="$SUPABASE_KEY"

# Get Cloud SQL password and construct URL
SQL_PWD=$(gcloud secrets versions access latest --secret openwatt-v2-db-url 2>/dev/null | grep -oP 'openwatt:\K[^@]+')
SQL_IP="35.205.255.76"  # gcloud sql instances describe openwatt-v2-db --format="value(ipAddresses[0].ipAddress)"
export DATABASE_URL="postgresql+asyncpg://openwatt:${SQL_PWD}@${SQL_IP}:5432/openwatt"

echo "SUPABASE_URL=$SUPABASE_URL"
echo "DATABASE_URL=postgresql+asyncpg://openwatt:***@${SQL_IP}:5432/openwatt"
echo ""
echo "Credentials loaded. Run:"
echo "  pip install -q psycopg2-binary supabase"
echo "  python3 scripts/sync_supabase_assets.py --apply"