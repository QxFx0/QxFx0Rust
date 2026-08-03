#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/release/qxfx0}"
output_dir="${2:-target/response-plan-v2-behavioral-canary}"
rm -rf "$output_dir"
mkdir -p "$output_dir/positive" "$output_dir/negative"

run_case() {
  local group="$1" case_id="$2" input_class="$3" expected_result="$4"
  local expected_guard="$5" session="$6" input="$7"
  "$binary" --db "$output_dir/canary.db" --session-id "$session" turn "$input" \
    --response-plan-v2-authority \
    --response-plan-v2-trace-jsonl "$output_dir/$group/$case_id.jsonl" \
    --authority-case-id "$case_id" \
    --authority-input-class "$input_class" \
    --authority-expected-result "$expected_result" \
    --authority-expected-guard "$expected_guard" >/dev/null
}

for entry in \
  "truth|правда" \
  "arbitrariness|произвол" \
  "freedom|свобода"; do
  IFS='|' read -r id topic <<<"$entry"
  session="behavioral-$id"
  run_case positive "$id-define" definition compositional v2_successfully_emitted "$session" "что такое $topic?"
  run_case positive "$id-paraphrase" definition_paraphrase compositional v2_successfully_emitted "$session" "что есть $topic?"
  run_case negative "$id-challenge" challenge authority_denied authority_denied_before_render "$session" "$topic это просто мнение"
  run_case positive "$id-clarify" definition_clarification compositional v2_successfully_emitted "$session" "уточни, что такое $topic?"
  run_case positive "$id-repeat" same_session_repeat compositional v2_successfully_emitted "$session" "что такое $topic?"
done

run_case negative outside-allowlist known_outside_allowlist authority_denied authority_denied_before_render negative-outside "что такое истина?"
run_case negative unknown-topic unknown_topic authority_denied authority_denied_before_render negative-unknown "что такое кванточайник?"
run_case negative unsupported-intent assertion authority_denied authority_denied_before_render negative-assertion "свобода существует"
run_case negative guard-rejected empty_input authority_denied authority_denied_before_render negative-empty ""

default_output="$($binary --db "$output_dir/default.db" --session-id negative-default turn "что такое свобода?")"
authority_output="$($binary --db "$output_dir/rollback.db" --session-id negative-rollback turn "что такое свобода?" --response-plan-v2-authority)"
rollback_output="$($binary --db "$output_dir/rollback.db" --session-id negative-rollback turn "что такое свобода?")"
test "$default_output" != ""
test "$rollback_output" != ""
test "$default_output" != "$authority_output"
test "$rollback_output" != "$authority_output"
printf '%s\n' '{"case_id":"default-v1","input_class":"authority_disabled","expected_result":"v1_default","expected_guard":"not_recorded"}' > "$output_dir/negative/default-v1.json"
printf '%s\n' '{"case_id":"explicit-rollback","input_class":"authority_then_disabled","expected_result":"v1_default","expected_guard":"not_recorded"}' > "$output_dir/negative/explicit-rollback.json"

positive=("$output_dir"/positive/*.jsonl)
negative=("$output_dir"/negative/*.jsonl)
for trace in "${positive[@]}"; do
  "$binary" verify-authority-trace "$trace" >/dev/null
done
"$binary" authority-report --scope positive "${positive[@]}" > "$output_dir/positive-report.json"
"$binary" authority-report --scope negative "${negative[@]}" > "$output_dir/negative-report.json"
jq -e '
  .turns == 12 and .positive_turns == 12 and .expectation_failures == 0 and
  .typed_non_declarative == 0 and .realization_downgrade == 0 and
  .replay_failures == 0 and .guard_blocks == 0 and .rollback_activations == 0
' "$output_dir/positive-report.json" >/dev/null
jq -e '
  .turns == 7 and .negative_turns == 7 and .expectation_failures == 0 and
  .compositional == 0 and .audited_verbatim == 0 and .guard_blocks == 0 and
  .rollback_activations == 7
' "$output_dir/negative-report.json" >/dev/null
