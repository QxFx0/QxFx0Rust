#!/bin/sh
# Read-only operational summary for qxfx0-soak-24h.sh artifacts.
set -eu

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    echo "usage: $0 STATUS_FILE LOG_FILE [FINAL_REPORT_FILE]" >&2
    exit 2
fi

status_file=$1
log_file=$2
report_file=${3:-}

value() {
    sed -n "s/^$1=//p" "$status_file" | tail -n 1
}

percentile() {
    percentile=$1
    latencies=$(sed -n 's/.* latency_ms=\([0-9][0-9]*\).*/\1/p' "$log_file")
    count=$(printf '%s\n' "$latencies" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$count" -eq 0 ]; then
        printf 'n/a'
        return
    fi
    rank=$(( (count * percentile + 99) / 100 ))
    printf '%s\n' "$latencies" | sort -n | sed -n "${rank}p"
}

printf 'state=%s\n' "$(value state)"
printf 'session_id=%s\n' "$(value session_id)"
printf 'turns=%s\n' "$(value turns)"
printf 'turn_failures=%s\n' "$(value turn_failures)"
printf 'slow_turns=%s\n' "$(value slow_turns)"
printf 'health_failures=%s\n' "$(value health_failures)"
printf 'database_bytes=%s\n' "$(value database_bytes)"
printf 'latency_p50_ms=%s\n' "$(percentile 50)"
printf 'latency_p95_ms=%s\n' "$(percentile 95)"
printf 'latency_p99_ms=%s\n' "$(percentile 99)"

if [ -n "$report_file" ] && [ -f "$report_file" ]; then
    printf 'final_doctor_ok=%s\n' "$(sed -n 's/^final_doctor_ok=//p' "$report_file" | tail -n 1)"
    printf 'final_metrics_ok=%s\n' "$(sed -n 's/^final_metrics_ok=//p' "$report_file" | tail -n 1)"
fi
