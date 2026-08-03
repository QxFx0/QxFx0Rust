#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/release/qxfx0}"
repetitions="${2:-10}"
window_id="${3:-response-plan-v2-canary-2026-08}"
output_dir="${4:-target/response-plan-v2-observation/$window_id}"
expected_sha="${5:-}"
[[ "$repetitions" =~ ^[1-9][0-9]*$ ]] || { printf 'repetitions must be positive\n' >&2; exit 2; }

rm -rf "$output_dir"
mkdir -p "$output_dir/positive" "$output_dir/negative"

manifest_digest="$(sha256sum data/gates/response-plan-v2/behavioral-canary-manifest.json | cut -d' ' -f1)"
allowlist_digest="$(printf '%s\n' правда произвол свобода | sha256sum | cut -d' ' -f1)"
build_sha="$(git rev-parse HEAD)"
version="$($binary version)"
if [[ -n "$expected_sha" && "$build_sha" != "$expected_sha" ]]; then
  printf 'build SHA mismatch: expected %s, got %s\n' "$expected_sha" "$build_sha" >&2
  exit 1
fi

mkdir -p "$output_dir/gates"
for gate in \
  response-plan-v2-phase-a \
  response-plan-v2-phase-b \
  response-plan-v2-phase-c \
  response-plan-v2-replay \
  response-plan-v2-zero-downgrade \
  response-plan-v2-canary-report; do
  "$binary" doctor --gate "$gate" --json > "$output_dir/gates/$gate.json"
done

run_case() {
  local group="$1" case_id="$2" input_class="$3" expected_result="$4"
  local expected_guard="$5" session="$6" input="$7" trace="$output_dir/$group/$case_id.jsonl"
  "$binary" --db "$output_dir/canary.db" --session-id "$session" turn "$input" \
    --response-plan-v2-authority --response-plan-v2-trace-jsonl "$trace" \
    --authority-case-id "$case_id" --authority-input-class "$input_class" \
    --authority-expected-result "$expected_result" \
    --authority-expected-guard "$expected_guard" >/dev/null
}

for topic_entry in "truth|правда" "arbitrariness|произвол" "freedom|свобода"; do
  IFS='|' read -r topic_id topic <<<"$topic_entry"
  for repeat in $(seq 1 "$repetitions"); do
    session="observation-$topic_id-$repeat"
    run_case positive "$topic_id-$repeat-define" definition compositional v2_successfully_emitted "$session" "что такое $topic?"
    run_case positive "$topic_id-$repeat-paraphrase" definition_paraphrase compositional v2_successfully_emitted "$session" "что есть $topic?"
    run_case positive "$topic_id-$repeat-clarify" definition_clarification compositional v2_successfully_emitted "$session" "уточни, что такое $topic?"
    run_case positive "$topic_id-$repeat-repeat" same_session_repeat compositional v2_successfully_emitted "$session" "что такое $topic?"
  done
done

run_case negative outside-allowlist known_outside_allowlist authority_denied authority_denied_before_render negative-outside "что такое истина?"
run_case negative unknown-topic unknown_topic authority_denied authority_denied_before_render negative-unknown "что такое кванточайник?"
run_case negative unsupported-intent assertion authority_denied authority_denied_before_render negative-assertion "свобода существует"
run_case negative guard-rejected empty_input authority_denied authority_denied_before_render negative-empty ""

positive=("$output_dir"/positive/*.jsonl)
negative=("$output_dir"/negative/*.jsonl)
for trace in "${positive[@]}" "${negative[@]}"; do
  "$binary" verify-authority-trace "$trace" >/dev/null || {
    # Negative controls are expected denials; report them without requiring
    # the release-eligibility verifier to accept them.
    "$binary" authority-report --scope negative "$trace" >/dev/null
  }
done

"$binary" authority-report --scope positive "${positive[@]}" > "$output_dir/positive-report.json"
"$binary" authority-report --scope negative "${negative[@]}" > "$output_dir/negative-report.json"
jq -n \
  --arg schema "qxfx0.response-plan-v2.observation.v1" \
  --arg window "$window_id" \
  --arg build "$build_sha" \
  --arg version "$version" \
  --arg manifest "$manifest_digest" \
  --arg allowlist "$allowlist_digest" \
  --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson repetitions "$repetitions" \
  '{schema:$schema,window_id:$window,build_sha:$build,binary_version:$version,manifest_digest:$manifest,allowlist_digest:$allowlist,repetitions:$repetitions,generated_at:$generated_at}' \
  > "$output_dir/observation-metadata.json"
jq -e --argjson expected "$((repetitions * 12))" '
  .turns == $expected and .expectation_failures == 0 and
  .realization_downgrade == 0 and .replay_failures == 0 and
  .guard_blocks == 0 and .rollback_activations == 0 and
  .unexpected_denials == 0 and .unexpected_rollbacks == 0
' "$output_dir/positive-report.json" >/dev/null
jq -e '
  .turns == 4 and .expected_denials == 4 and .unexpected_denials == 0 and
  .expected_rollbacks == 4 and .unexpected_rollbacks == 0 and
  .expectation_failures == 0 and .guard_blocks == 0
' "$output_dir/negative-report.json" >/dev/null
