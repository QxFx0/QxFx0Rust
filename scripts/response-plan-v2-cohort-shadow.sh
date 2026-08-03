#!/usr/bin/env bash
set -euo pipefail

binary="${1:-target/release/qxfx0}"
output="${2:-target/response-plan-v2-cohort-shadow.jsonl}"
manifest="data/gates/response-plan-v2/audited-corpus-manifest.json"
cohort="data/gates/response-plan-v2/cohort-2026-08.json"
test ! -e "$output"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

manifest_digest="$(sha256sum "$manifest" | cut -d' ' -f1)"
cohort_digest="$(sha256sum "$cohort" | cut -d' ' -f1)"
for topic in время справедливость ответственность; do
  claims="$(jq -c --arg topic "$topic" '.topics[$topic].claims | to_entries | map({claim_id:.key,path:.value.canonical_path,fact_id:.value.fact_id,strategy:.value.realization_strategy,validation:.value.surface_validation,surface_digest:.value.approved_surface_sha256})' "$manifest")"
  trace="$workdir/$topic.jsonl"
  "$binary" \
    --db "$workdir/cohort.db" \
    --session-id "cohort-shadow-$topic" \
    turn "что такое $topic?" \
    --response-plan-v2-shadow-trace-jsonl "$trace" >/dev/null
  jq -ce \
    --arg schema "qxfx0.response-plan-v2.cohort-shadow-evidence.v1" \
    --arg topic "$topic" \
    --arg manifest_digest "$manifest_digest" \
    --arg cohort_digest "$cohort_digest" \
    --argjson claims "$claims" \
    '. as $record | .trace.steps[] | select(.stage == "response_plan_v2") as $step |
     select(
       $step.metadata.requested_mode == "Shadow" and
       $step.metadata.effective_mode == "Shadow" and
       $step.metadata.attempted == "true" and
       $step.metadata.completed == "true" and
       $step.metadata.downgrade_count == "0" and
       $step.metadata.semantic_parity == "true" and
       $step.metadata.authority_parity == "true" and
       $step.metadata.realization_parity == "true" and
       $step.metadata.replay_parity == "true" and
       $step.metadata.v1_authoritative == "true" and
       $step.metadata.v1_fallback_used == "false"
     ) |
     {schema:$schema,topic:$topic,manifest_digest:$manifest_digest,cohort_digest:$cohort_digest,claims:$claims,shadow_step:$step,authority_receipt:$record.trace.authority_receipt}' \
    "$trace" \
    >> "$output"
done

test "$(wc -l < "$output")" -eq 3
jq -e -s '
  length == 3 and
  (map(.topic) == ["время", "справедливость", "ответственность"]) and
  all(.[];
    .schema == "qxfx0.response-plan-v2.cohort-shadow-evidence.v1" and
    .authority_receipt.requested_mode == "shadow" and
    .authority_receipt.effective_mode == "shadow" and
    .authority_receipt.authority == "Disabled"
  )
' "$output" >/dev/null
