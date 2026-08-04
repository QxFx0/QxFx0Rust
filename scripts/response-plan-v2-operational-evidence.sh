#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/release/qxfx0}"
output_dir="${2:-target/response-plan-v2-operational-evidence}"
manifest="${3:-data/gates/response-plan-v2/operational-formulations-v1.json}"

test ! -e "$output_dir"
mkdir -p "$output_dir/positive" "$output_dir/negative"
manifest_digest="$(sha256sum "$manifest" | cut -d' ' -f1)"
build_sha="$(git rev-parse HEAD)"

while IFS= read -r case; do
  case_id="$(jq -r .case_id <<<"$case")"
  input_class="$(jq -r .input_class <<<"$case")"
  expected_result="$(jq -r .expected_result <<<"$case")"
  expected_guard="$(jq -r .expected_guard <<<"$case")"
  utterance="$(jq -r .utterance <<<"$case")"
  group=positive
  [[ "$expected_result" == "compositional" ]] || group=negative
  "$binary" \
    --db "$output_dir/operational.db" \
    --session-id "operational-$case_id" \
    turn "$utterance" \
    --response-plan-v2-authority \
    --response-plan-v2-trace-jsonl "$output_dir/$group/$case_id.jsonl" \
    --authority-case-id "$case_id" \
    --authority-input-class "$input_class" \
    --authority-expected-result "$expected_result" \
    --authority-expected-guard "$expected_guard" >/dev/null
done < <(jq -c '.cases[]' "$manifest")

positive=("$output_dir"/positive/*.jsonl)
negative=("$output_dir"/negative/*.jsonl)
for trace in "${positive[@]}"; do
  "$binary" verify-authority-trace "$trace" >/dev/null
done
"$binary" authority-report --scope positive "${positive[@]}" > "$output_dir/positive-report.json"
"$binary" authority-report --scope negative "${negative[@]}" > "$output_dir/negative-report.json"

positive_expected="$(jq '[.cases[] | select(.expected_result == "compositional")] | length' "$manifest")"
negative_expected="$(jq '[.cases[] | select(.expected_result != "compositional")] | length' "$manifest")"
jq -e --argjson expected "$positive_expected" '
  .turns == $expected and .expectation_failures == 0 and
  .realization_downgrade == 0 and .replay_failures == 0 and
  .guard_blocks == 0 and .unexpected_denials == 0 and
  .unexpected_rollbacks == 0
' "$output_dir/positive-report.json" >/dev/null
jq -e --argjson expected "$negative_expected" '
  .turns == $expected and .expectation_failures == 0 and
  .expected_denials == $expected and .unexpected_denials == 0 and
  .expected_rollbacks == $expected and .unexpected_rollbacks == 0
' "$output_dir/negative-report.json" >/dev/null

jq -n \
  --arg schema "qxfx0.response-plan-v2.operational-evidence.v1" \
  --arg build_sha "$build_sha" \
  --arg manifest_digest "$manifest_digest" \
  --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson positive "$positive_expected" \
  --argjson negative "$negative_expected" \
  '{schema:$schema,build_sha:$build_sha,manifest_digest:$manifest_digest,generated_at:$generated_at,positive_cases:$positive,negative_cases:$negative,raw_user_logs:false}' \
  > "$output_dir/metadata.json"
