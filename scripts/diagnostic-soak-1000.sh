#!/bin/sh
# Create a new cadence-preserving diagnostic pilot without reusing artifacts.
set -eu

qxfx0_bin=${QXFX0_BIN:?QXFX0_BIN is required}
diagnostic_dir=${QXFX0_DIAGNOSTIC_DIR:?QXFX0_DIAGNOSTIC_DIR is required and must not exist}
turn_limit=${QXFX0_DIAGNOSTIC_TURNS:-1000}
interval_seconds=${QXFX0_DIAGNOSTIC_INTERVAL_SECONDS:-60}
max_response_ms=${QXFX0_MAX_RESPONSE_MS:-2000}
session_id=${QXFX0_DIAGNOSTIC_SESSION_ID:-diagnostic-1000}

case "$turn_limit:$interval_seconds:$max_response_ms" in
    *[!0-9:]*|:*|*::*|*:)
        echo "turn count, interval, and response limit must be non-negative integers" >&2
        exit 2
        ;;
esac

if [ "$turn_limit" -eq 0 ] || [ ! -x "$qxfx0_bin" ] || [ -e "$diagnostic_dir" ]; then
    echo "diagnostic directory must be new, turn count positive, and QXFX0_BIN executable" >&2
    exit 2
fi

mkdir "$diagnostic_dir"
pilot_db="$diagnostic_dir/pilot.db"
status_file="$diagnostic_dir/pilot.status"
log_file="$diagnostic_dir/pilot.log"
report_file="$diagnostic_dir/pilot.report"
diagnostics_file="$diagnostic_dir/turns.jsonl"
turns=0
turn_failures=0
slow_turns=0

emit() { printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" >>"$log_file"; }
write_status() {
    printf 'state=%s\nsession_id=%s\nturns=%s\nturn_failures=%s\nslow_turns=%s\nhealth_failures=%s\ndatabase_bytes=%s\n' \
        "$1" "$session_id" "$turns" "$turn_failures" "$slow_turns" 0 \
        "$(stat -c %s "$pilot_db" 2>/dev/null || printf 0)" >"$status_file"
}

emit "SOAK_START session=$session_id turns=$turn_limit interval_seconds=$interval_seconds db=$pilot_db"
write_status running
while [ "$turns" -lt "$turn_limit" ]; do
    case $((turns % 12)) in
        0) prompt='Что такое истина?' ;; 1) prompt='Как связаны свобода и ответственность?' ;;
        2) prompt='Что означает человеческое достоинство?' ;; 3) prompt='Как память влияет на личность?' ;;
        4) prompt='В чём различие знания и убеждения?' ;; 5) prompt='Как надежда связана с действием?' ;;
        6) prompt='Что делает решение справедливым?' ;; 7) prompt='Как язык формирует понимание?' ;;
        8) prompt='Почему доверие требует ответственности?' ;; 9) prompt='Как связаны причина и следствие?' ;;
        10) prompt='Что означает сохранять внутреннюю целостность?' ;; *) prompt='Как опыт меняет представление о будущем?' ;;
    esac
    started_ms=$(date +%s%3N)
    if "$qxfx0_bin" --db "$pilot_db" --session-id "$session_id" turn --diagnostics-jsonl "$diagnostics_file" "$prompt" >/dev/null 2>&1; then result=ok; else result=failed; turn_failures=$((turn_failures + 1)); fi
    latency_ms=$(( $(date +%s%3N) - started_ms ))
    turns=$((turns + 1))
    [ "$latency_ms" -gt "$max_response_ms" ] && slow_turns=$((slow_turns + 1))
    emit "SOAK_TURN n=$turns result=$result latency_ms=$latency_ms"
    write_status running
    [ "$turns" -lt "$turn_limit" ] && sleep "$interval_seconds"
done

doctor_ok=0; metrics_ok=0
"$qxfx0_bin" --db "$pilot_db" --session-id "$session_id" doctor --json >/dev/null 2>&1 && doctor_ok=1
"$qxfx0_bin" --db "$pilot_db" --session-id "$session_id" metrics --json --max-response-ms "$max_response_ms" >/dev/null 2>&1 && metrics_ok=1
printf 'final_doctor_ok=%s\nfinal_metrics_ok=%s\n' "$doctor_ok" "$metrics_ok" >"$report_file"
write_status completed
emit "SOAK_FINISH state=completed turns=$turns turn_failures=$turn_failures slow_turns=$slow_turns"
"$(dirname "$0")/soak-report.sh" "$status_file" "$log_file" "$report_file" "$diagnostics_file" >"$diagnostic_dir/summary.report"
[ "$turn_failures" -eq 0 ] && [ "$slow_turns" -eq 0 ] && [ "$doctor_ok" -eq 1 ] && [ "$metrics_ok" -eq 1 ]
