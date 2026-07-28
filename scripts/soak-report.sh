#!/bin/sh
# Read-only operational summary for qxfx0-soak-24h.sh artifacts.
set -eu

if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 STATUS_FILE LOG_FILE [FINAL_REPORT_FILE] [DIAGNOSTICS_JSONL]" >&2
    exit 2
fi

status_file=$1
log_file=$2
report_file=${3:-}
diagnostics_file=${4:-}
slow_turn_threshold_ms=${QXFX0_SLOW_TURN_MS:-2000}

case "$slow_turn_threshold_ms" in
    ''|*[!0-9]*)
        echo "QXFX0_SLOW_TURN_MS must be a non-negative integer" >&2
        exit 2
        ;;
esac

value() {
    sed -n "s/^$1=//p" "$status_file" | tail -n 1
}

percentile_values() {
    percentile=$1
    values=$2
    count=$(printf '%s\n' "$values" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$count" -eq 0 ]; then
        printf 'n/a'
        return
    fi
    rank=$(( (count * percentile + 99) / 100 ))
    printf '%s\n' "$values" | sort -n | sed -n "${rank}p"
}

log_percentile() {
    percentile=$1
    latencies=$(sed -n 's/.* latency_ms=\([0-9][0-9]*\).*/\1/p' "$log_file")
    percentile_values "$percentile" "$latencies"
}

diagnostic_percentile() {
    query=$1
    percentile=$2
    values=$(jq -r "$query // empty" "$diagnostics_file")
    percentile_values "$percentile" "$values"
}

printf 'state=%s\n' "$(value state)"
printf 'session_id=%s\n' "$(value session_id)"
printf 'turns=%s\n' "$(value turns)"
printf 'turn_failures=%s\n' "$(value turn_failures)"
printf 'slow_turns=%s\n' "$(value slow_turns)"
printf 'health_failures=%s\n' "$(value health_failures)"
printf 'database_bytes=%s\n' "$(value database_bytes)"
printf 'latency_p50_ms=%s\n' "$(log_percentile 50)"
printf 'latency_p95_ms=%s\n' "$(log_percentile 95)"
printf 'latency_p99_ms=%s\n' "$(log_percentile 99)"

if [ -n "$report_file" ] && [ -f "$report_file" ]; then
    printf 'final_doctor_ok=%s\n' "$(sed -n 's/^final_doctor_ok=//p' "$report_file" | tail -n 1)"
    printf 'final_metrics_ok=%s\n' "$(sed -n 's/^final_metrics_ok=//p' "$report_file" | tail -n 1)"
fi

if [ -n "$diagnostics_file" ]; then
    if [ ! -f "$diagnostics_file" ]; then
        printf 'diagnostics_error=missing file: %s\n' "$diagnostics_file"
    elif ! command -v jq >/dev/null 2>&1; then
        printf 'diagnostics_error=jq is required to summarize JSONL diagnostics\n'
    else
        diagnostic_records=$(jq -c 'select(type == "object")' "$diagnostics_file" | wc -l | tr -d ' ')
        printf 'diagnostics_records=%s\n' "$diagnostic_records"
        printf 'diagnostics_schema=%s\n' "$(jq -r 'select(type == "object") | .schema // empty' "$diagnostics_file" | head -n 1)"
        printf 'diagnostics_host_os=%s\n' "$(jq -r 'select(type == "object") | .host.os // empty' "$diagnostics_file" | head -n 1)"
        printf 'diagnostics_host_architecture=%s\n' "$(jq -r 'select(type == "object") | .host.architecture // empty' "$diagnostics_file" | head -n 1)"
        printf 'diagnostics_host_parallelism=%s\n' "$(jq -r 'select(type == "object") | .host.available_parallelism // empty' "$diagnostics_file" | head -n 1)"
        printf 'diagnostics_host_hostname=%s\n' "$(jq -r 'select(type == "object") | .host.hostname // empty' "$diagnostics_file" | head -n 1)"
        printf 'slow_turn_threshold_ms=%s\n' "$slow_turn_threshold_ms"

        for metric in \
            db_open_ms \
            db_load_ms \
            pipeline.semantic_selection_ms \
            pipeline.plan_render_ms \
            pipeline.guard_ms \
            db_save.total_ms \
            db_save.sqlite_write_lock_ms \
            db_save.sqlite_commit_checkpoint_ms \
            total_ms
        do
            metric_name=$(printf '%s' "$metric" | tr '.' '_')
            for percentile in 50 95 99; do
                printf 'diagnostic_%s_p%s_ms=%s\n' \
                    "$metric_name" \
                    "$percentile" \
                    "$(diagnostic_percentile ".$metric" "$percentile")"
            done
        done

        sed -n 's/.* SOAK_TURN n=\([0-9][0-9]*\).*latency_ms=\([0-9][0-9]*\).*/\1 \2/p' "$log_file" \
            | while read -r turn latency; do
                if [ "$latency" -gt "$slow_turn_threshold_ms" ]; then
                    jq -r --argjson turn "$turn" '
                        select(.turn == $turn)
                        | "slow_turn_stage_evidence=turn=\(.turn),total_ms=\(.total_ms),db_load_ms=\(.db_load_ms),semantic_selection_ms=\(.pipeline.semantic_selection_ms),plan_render_ms=\(.pipeline.plan_render_ms),guard_ms=\(.pipeline.guard_ms),db_save_ms=\(.db_save.total_ms),sqlite_write_lock_ms=\(.db_save.sqlite_write_lock_ms),sqlite_commit_checkpoint_ms=\(.db_save.sqlite_commit_checkpoint_ms)"
                    ' "$diagnostics_file"
                fi
            done
    fi
fi
