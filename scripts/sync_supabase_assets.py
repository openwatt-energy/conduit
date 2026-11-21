#!/usr/bin/env python3
"""Sync P1 dongles from production Supabase → v2 Cloud SQL assets table.

Usage:
    python3 scripts/sync_supabase_assets.py [--apply]

Requires env vars (set via .env or export):
    SUPABASE_URL     https://<ref>.supabase.co
    SUPABASE_KEY     service_role key from GCP secret supabase-prod
    DATABASE_URL     postgresql+asyncpg://openwatt:<pw>@<cloud-sql-ip>:5432/openwatt

The raw Cloud SQL password is available from:
    gcloud secrets versions access latest --secret openwatt-v2-db-url
    (extract the password between 'openwatt:' and '@')
"""

import os
import sys

import psycopg2
try:
    import psycopg2.extras
except ImportError:
    pass
from supabase import create_client


def main():
    apply = '--apply' in sys.argv

    supa_url = os.environ['SUPABASE_URL']
    supa_key = os.environ['SUPABASE_KEY']
    dsn = os.environ['DATABASE_URL'].replace('+asyncpg', '')

    print(f"Supabase: {supa_url}")
    print(f"Cloud SQL: {dsn[:50]}...")
    print(f"Mode: {'APPLY' if apply else 'DRY-RUN'}")
    print()

    supa = create_client(supa_url, supa_key)
    conn = psycopg2.connect(dsn, connect_timeout=10)
    cur = conn.cursor()

    # Count
    cnt = supa.table('p1_dongles').select('ow_id', count='exact').limit(1).execute()
    total = cnt.count if hasattr(cnt, 'count') else 0
    print(f"Supabase p1_dongles: {total}")

    stats = {'total': 0, 'insert': 0, 'update': 0}
    offset = 0
    limit = 500

    while True:
        resp = supa.table('p1_dongles').select('ow_id').range(offset, offset + limit - 1).execute()
        dongles = resp.data if hasattr(resp, 'data') else []
        if not dongles:
            break

        for d in dongles:
            ow_id = d.get('ow_id', '')
            if not ow_id:
                continue
            stats['total'] += 1

            if apply:
                cur.execute('SELECT id FROM assets WHERE external_id = %s', (ow_id,))
                row = cur.fetchone()
                if row:
                    cur.execute(
                        "UPDATE assets SET type = 'p1_dongle', status = 'online', updated_at = now() WHERE id = %s",
                        (row[0],)
                    )
                    stats['update'] += 1
                else:
                    cur.execute(
                        "INSERT INTO assets (id, type, vendor, external_id, status, config, created_at, updated_at) "
                        "VALUES (gen_random_uuid(), 'p1_dongle', 'openwatt', %s, 'online', '{}', now(), now())",
                        (ow_id,)
                    )
                    stats['insert'] += 1

        if apply:
            conn.commit()
        offset += limit
        print(f"  [{offset}/{total}] inserted={stats['insert']} updated={stats['update']}")

    if apply:
        conn.commit()
        print(f"\nDone. Inserted: {stats['insert']}, Updated: {stats['update']}")
    else:
        conn.rollback()
        print(f"\nDry-run. Would insert {stats['insert']}, update {stats['update']}")

    cur.close()
    conn.close()


if __name__ == '__main__':
    main()