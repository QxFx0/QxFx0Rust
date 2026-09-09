# QxFx0 operations

The supported operational checks are built into the `qxfx0` binary:

```bash
qxfx0 --db /var/lib/qxfx0/qxfx0.db doctor --json
qxfx0 --db /var/lib/qxfx0/qxfx0.db metrics
qxfx0 --db /var/lib/qxfx0/qxfx0.db backup /var/backups/qxfx0/manual.db
```

`backup` uses SQLite's online backup API. It opens the source read-only, writes
to a private partial file, runs `PRAGMA quick_check`, and atomically renames the
verified result. Existing destinations are never overwritten.

<!-- qxfx0-current-schema-version: 12 -->
## SQLite writer model

Deploy exactly one QxFx0 writer process per database. A process may serve many
sessions, but multiple processes must not concurrently use the same database as
an active write target. SQLite WAL permits readers alongside that writer; it
does not turn the deployment into a multi-writer service.

Connections use a five-second SQLite busy timeout. An accidental second writer
may therefore succeed if the current writer releases the lock within that
window; otherwise the operation fails with a diagnosable SQLite `database is
busy`/`database is locked` error. Treat repeated lock errors as a deployment
configuration fault: stop the extra writer rather than adding retries or an
async writer service. Before backup recovery or migration, stop every writer.

`metrics` emits Prometheus text and exits non-zero when `doctor` fails, total
DB/WAL/SHM storage exceeds the configured limit, or the in-memory response
probe is invalid or too slow. Use `metrics --json` for JSON output.

## systemd installation

1. Install the release binary as `/usr/local/bin/qxfx0` and copy `ops/` to
   `/opt/qxfx0/ops/`.
2. Create a locked-down `qxfx0` user, `/var/lib/qxfx0`, and
   `/var/backups/qxfx0` with ownership `qxfx0:qxfx0`.
3. Copy `ops/qxfx0.env.example` to `/etc/qxfx0/qxfx0.env` and adjust paths and
   thresholds.
4. Copy the units from `ops/systemd/` to `/etc/systemd/system/`, then run:

```bash
systemctl daemon-reload
systemctl enable --now qxfx0-backup.timer qxfx0-monitor.timer
systemctl list-timers 'qxfx0-*'
```

Timer output and failures are retained by journald:

```bash
journalctl -u qxfx0-backup.service -u qxfx0-monitor.service
```

Journald performs its own rotation. If application logs are redirected to
`/var/log/qxfx0/*.log`, install `ops/logrotate/qxfx0` under
`/etc/logrotate.d/qxfx0` and test it with `logrotate --debug`.

## Recovery drill

Stop every writer, preserve the failed file for diagnosis, copy a verified
backup into place, and run `doctor` before restarting traffic. Never restore
only a live database's main file without its WAL state; the built-in backup
command avoids this problem.

### Thesis projection rollout and schema v10

SQLite schema v10 additively reserves nullable `session_semantic.thesis_state_json`; v9 rows are not rewritten and NULL loads as an empty bounded projection. `ThesisProjectionRollout` is explicit and default-off: `Disabled` preserves the production path, while `Shadow` validates catalog-bound receipts without state mutation. There is deliberately no pipeline or CLI write mode: thesis lifecycle persistence requires a separate policy, retention/export/delete design, and evidence window. User or generated text never creates thesis authority.
