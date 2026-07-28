# Soak pilot report gate

Run `scripts/soak-report.sh STATUS LOG [REPORT] [DIAGNOSTICS_JSONL]` during or
after a pilot. It is read-only: it does not signal a service or open the
database. The optional diagnostics JSONL is produced by opt-in
`qxfx0 turn --diagnostics-jsonl PATH`; it adds stage percentiles and timing
evidence for slow turns without writing telemetry into the session database.

The final release gate requires `state=completed`, zero turn and health
failures, zero slow turns, `final_doctor_ok=1`, `final_metrics_ok=1`, and a
recorded p50/p95/p99 latency and database growth. A pilot that is interrupted
or whose launcher disappears is reported separately and is not evidence of
stability.
