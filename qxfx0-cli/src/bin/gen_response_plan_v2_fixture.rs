use qxfx0_semantic::response_plan_v2::*;

fn main() {
    let binary_digest = std::env::args()
        .nth(1)
        .expect("usage: gen_response_plan_v2_fixture <target-binary-sha256>");
    assert!(
        binary_digest.len() == 64 && binary_digest.chars().all(|value| value.is_ascii_hexdigit()),
        "expected a 64-character target binary SHA-256 argument"
    );
    let policy = SelectionPolicy {
        response_plan_v2_mode: ResponsePlanV2Mode::Shadow,
        ..SelectionPolicy::default()
    };
    let budgets = V2BudgetPolicy::default();
    let contract = TurnContractSnapshot::new(
        AuthoritySnapshot::new(
            qxfx0_semantic::active_pack_set().fingerprint(),
            AssertionPolicy::v1().digest(),
        ),
        PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
        RealizationSnapshot::new(
            valency_lexicon().fingerprint(),
            "clause-grammar-v1",
            qxfx0_morphology::get_runtime().lexemes_sha256(),
            preposition_allomorphs().fingerprint(),
        ),
        SelectionPolicySnapshot::new(policy),
    );
    let execution = execute_audited_topic_at(
        "свобода",
        EvidenceEvaluationContext::new(42, None),
        &budgets,
        &contract,
        SelfSelectionContext::quantize(0.0, 0.0, 0.0),
        policy,
        valency_lexicon(),
        qxfx0_morphology::get_runtime(),
    );
    let selection = execution.selection.expect("selection");
    let replay = execution.exact_replay.expect("exact replay");
    let record = TurnRecord::new(contract, selection, binary_digest, replay);
    let mut json = serde_json::to_string_pretty(&record).expect("json");
    json.push('\n');
    let path = std::path::Path::new("data/gates/response-plan-v2/turn-record-v2.json");
    std::fs::write(path, json).expect("write replay fixture");
    println!("wrote {}", path.display());
}
