#!/bin/sh
set -eu

qxfx0_bin=${QXFX0_BIN:-/usr/local/bin/qxfx0}
qxfx0_db=${QXFX0_DB:-/var/lib/qxfx0/qxfx0.db}
max_db_bytes=${QXFX0_MAX_DB_BYTES:-1073741824}
max_response_ms=${QXFX0_MAX_RESPONSE_MS:-2000}

case "$max_db_bytes:$max_response_ms" in
    *[!0-9:]*|:*|*:)
        echo "QXFX0_MAX_DB_BYTES and QXFX0_MAX_RESPONSE_MS must be integers" >&2
        exit 2
        ;;
esac

exec "$qxfx0_bin" --db "$qxfx0_db" metrics \
    --max-db-bytes "$max_db_bytes" \
    --max-response-ms "$max_response_ms"
