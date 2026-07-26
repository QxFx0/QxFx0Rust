# Soak pilot report gate

Run `scripts/soak-report.sh STATUS LOG [REPORT]` during or after a pilot. It
is read-only: it does not signal a service or open the database.

The final release gate requires `state=completed`, zero turn and health
failures, zero slow turns, `final_doctor_ok=1`, `final_metrics_ok=1`, and a
recorded p50/p95/p99 latency and database growth. A pilot that is interrupted
or whose launcher disappears is reported separately and is not evidence of
stability.
