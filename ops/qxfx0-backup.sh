#!/bin/sh
set -eu

qxfx0_bin=${QXFX0_BIN:-/usr/local/bin/qxfx0}
qxfx0_db=${QXFX0_DB:-/var/lib/qxfx0/qxfx0.db}
backup_dir=${QXFX0_BACKUP_DIR:-/var/backups/qxfx0}
retention_days=${QXFX0_BACKUP_RETENTION_DAYS:-14}

case "$retention_days" in
    ''|*[!0-9]*)
        echo "QXFX0_BACKUP_RETENTION_DAYS must be a non-negative integer" >&2
        exit 2
        ;;
esac

if [ "$backup_dir" = "/" ]; then
    echo "Refusing to use / as QXFX0_BACKUP_DIR" >&2
    exit 2
fi

install -d -m 0700 "$backup_dir"
timestamp=$(date -u +%Y%m%dT%H%M%SZ)
destination="$backup_dir/qxfx0-$timestamp.db"

"$qxfx0_bin" --db "$qxfx0_db" backup "$destination"
find "$backup_dir" -maxdepth 1 -type f -name 'qxfx0-*.db' \
    -mtime "+$retention_days" -delete

echo "Backup rotation complete: $destination"
