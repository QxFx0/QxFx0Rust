#!/bin/sh
# Latency comparison across renderer authorities.
#
# Runs a cadence-preserving turn pilot and aggregates the per-stage timing
# diagnostics emitted by `--diagnostics-jsonl`, so the legacy_shadow vs
# audited_plan renderers can be compared against the incident latency gate
# (docs/operations/incidents/2026-07-27-legacy-latency.md):
#   slow_turns = 0 at the configured threshold
#   p95 <= 500 ms, p99 <= 1000 ms
#
# Usage:
#   scripts/latency-authority-pilot.sh \
#     --authority legacy|audited|both \
#     --turns 200 \
#     --interval 0 \
#     --max-response-ms 2000 \
#     --out <dir>
#
# Emits: <dir>/<authority>.summary with percentile tables and slow-turn count.
# Read-only wrt existing databases: creates a fresh pilot.<authority>.db each run.
set -eu

qxfx0_bin=${QXFX0_BIN:-target/release/qxfx0}
authority=all
turns=200
interval=0
max_response_ms=2000
out=

while [ "$#" -gt 0 ]; do
    case "$1" in
        --authority) authority=$2; shift 2;;
        --turns) turns=$2; shift 2;;
        --interval) interval=2; shift 2;;
        --max-response-ms) max_response_ms=$2; shift 2;;
        --out) out=$2; shift 2;;
        *) echo "unknown arg: $1" >&2; exit 2;;
    esac
done

case "$authority" in legacy|audited|all) :;; *) echo "bad --authority" >&2; exit 2;; esac

if [ -z "$out" ]; then
    out=/tmp/opencode/latency-pilot
fi
mkdir -p "$out"

prompts='Что такое истина?|Как связаны свобода и ответственность?|Что означает человеческое достоинство?|Как память влияет на личность?|В чём различие знания и убеждения?|Как надежда связана с действием?|Что делает решение справедливым?|Как язык формирует понимание?|Почему доверие требует ответственности?|Как связаны причина и следствие?|Что означает сохранять внутреннюю целостность?|Как опыт меняет представление о будущем?'
prompt_for() { printf '%s' "$prompts" | cut -d'|' -f$(( 1 + ($1 % 12) )); }

# Field path → human label. Most timing fields are top-level in each JSONL
# record; pipeline stages nest under diagnostics.pipeline.* and save timings
# under diagnostics.db_save.* .
top_fields="cli_process_ms db_open_ms db_load_ms total_ms"
nested_fields="pipeline.input_normalization_ms pipeline.prepare_ms pipeline.route_ms \
pipeline.semantic_selection_ms pipeline.plan_render_ms pipeline.finalize_ms \
pipeline.guard_ms pipeline.persist_ms \
db_save.serialization_ms db_save.sqlite_transaction_begin_ms \
db_save.sqlite_write_lock_ms db_save.sqlite_remaining_writes_ms \
db_save.sqlite_commit_checkpoint_ms db_save.total_ms"

run_authority() {
    auth=$1
    case "$auth" in legacy) flags=" " ;; audited) flags=" --render-audited-plan" ;; esac

    pilot_db="$out/$auth.db"
    diagnostics_file="$out/$auth.turns.jsonl"
    log_file="$out/$auth.log"
    summary="$out/$auth.summary"

    [ -e "$pilot_db" ] && rm -f "$pilot_db" "$pilot_db"-wal "$pilot_db"-shm
    : > "$diagnostics_file"
    : > "$log_file"

    # Warm the OS page cache and the lazy morphology/seed-graph asset load so
    # the cold-first-turn cost does not dominate the latency percentiles. The
    # warmup turn is discarded: it writes to a throwaway diagnostics sink, so the
    # measured diagnostics_file and log_file contain only measured turns.
    "$qxfx0_bin" --db "$pilot_db" --session-id "latency-$auth" \
        turn $flags --diagnostics-jsonl "$out/$auth.warmup.jsonl" "Что такое истина?" >/dev/null 2>&1 || true

    turns_done=0
    turn_failures=0
    slow_turns=0
    emit() { printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" >>"$log_file"; }

    emit "PILOT_START authority=$auth turns=$turns max_response_ms=$max_response_ms"
    while [ "$turns_done" -lt "$turns" ]; do
        prompt=$(prompt_for "$turns_done")
        started_ms=$(date +%s%3N)
        if "$qxfx0_bin" --db "$pilot_db" --session-id "latency-$auth" \
                turn $flags --diagnostics-jsonl "$diagnostics_file" "$prompt" >/dev/null 2>&1
        then
            result=ok
        else
            result=failed
            turn_failures=$((turn_failures + 1))
        fi
        latency_ms=$(( $(date +%s%3N) - started_ms ))
        turns_done=$((turns_done + 1))
        if [ "$latency_ms" -gt "$max_response_ms" ]; then
            slow_turns=$((slow_turns + 1))
        fi
        emit "PILOT_TURN n=$turns_done result=$result latency_ms=$latency_ms"
        [ "$turns_done" -lt "$turns" ] && [ "$interval" -gt 0 ] && sleep "$interval"
    done

    # Aggregated percentiles over the diagnostics JSONL.
    {
        echo "authority=$auth"
        echo "turns=$turns_done"
        echo "turn_failures=$turn_failures"
        echo "slow_turns=$slow_turns"
        echo "max_response_ms=$max_response_ms"
        echo "---- log latency (date, ms) ----"
        echo "log_latency_p50_ms=$(percentile 50 <(sed -n 's/.* latency_ms=\([0-9]*\).*/\1/p' "$log_file"))"
        echo "log_latency_p95_ms=$(percentile 95 <(sed -n 's/.* latency_ms=\([0-9]*\).*/\1/p' "$log_file"))"
        echo "log_latency_p99_ms=$(percentile 99 <(sed -n 's/.* latency_ms=\([0-9]*\).*/\1/p' "$log_file"))"
        echo "log_latency_max_ms=$(sed -n 's/.* latency_ms=\([0-9]*\).*/\1/p' "$log_file" | sort -n | tail -1)"
        echo "---- diagnostics percentiles (top-level, ms) ----"
        for field in $top_fields; do
            p50=$(percentile 50 <(jq -r ".${field} // empty" "$diagnostics_file"))
            p95=$(percentile 95 <(jq -r ".${field} // empty" "$diagnostics_file"))
            p99=$(percentile 99 <(jq -r ".${field} // empty" "$diagnostics_file"))
            echo "diag_${field}_p50_ms=$p50"
            echo "diag_${field}_p95_ms=$p95"
            echo "diag_${field}_p99_ms=$p99"
        done
        echo "---- diagnostics percentiles (nested, ms) ----"
        for field in $nested_fields; do
            p50=$(percentile 50 <(jq -r ".${field} // empty" "$diagnostics_file"))
            p95=$(percentile 95 <(jq -r ".${field} // empty" "$diagnostics_file"))
            p99=$(percentile 99 <(jq -r ".${field} // empty" "$diagnostics_file"))
            echo "diag_${field}_p50_ms=$p50"
            echo "diag_${field}_p95_ms=$p95"
            echo "diag_${field}_p99_ms=$p99"
        done
    } > "$summary"

    "$qxfx0_bin" --db "$pilot_db" doctor --json >/dev/null 2>&1 && echo "doctor=ok" >>"$summary" || echo "doctor=fail" >>"$summary"
    emit "PILOT_FINISH authority=$auth turns=$turns_done failures=$turn_failures slow_turns=$slow_turns"
    cat "$summary"
}

percentile() {
    p=$1; src=${2:-/dev/stdin}
    vals=$(sed '/^$/d; /null/d' "$src" | sort -n)
    count=$(printf '%s\n' "$vals" | grep -c . || true)
    if [ "$count" -eq 0 ]; then printf '0'; return; fi
    rank=$(( (count * p + 99) / 100 ))
    printf '%s\n' "$vals" | sed -n "${rank}p"
}

case "$authority" in
    legacy) run_authority legacy ;;
    audited) run_authority audited ;;
    all) run_authority legacy; echo "========"; run_authority audited;;
esac
