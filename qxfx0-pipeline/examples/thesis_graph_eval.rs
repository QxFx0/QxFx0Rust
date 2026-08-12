use qxfx0_pipeline::{
    fact_grounded::ThesisProjectionRollout, process_turn_with_options_and_trace, RendererAuthority,
    TurnInput, TurnOptions,
};
use qxfx0_types::SystemState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
};

#[derive(Deserialize)]
struct Corpus {
    schema_version: u32,
    corpus_id: String,
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    id: String,
    pack: String,
    category: String,
    adversarial: bool,
    turns: Vec<String>,
    expected: Expected,
}
#[derive(Deserialize)]
struct Expected {
    projected_thesis_id: String,
    projected_digest: String,
    user_text_must_not_be_authority: bool,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Disabled,
    Shadow,
}
#[derive(Serialize, Clone)]
struct RunBundle {
    responses: Vec<String>,
    blocked: Vec<bool>,
    traces: Vec<Value>,
    thesis_state: Value,
    final_state_digest: String,
}
#[derive(Serialize)]
struct Record {
    scenario_id: String,
    pack: String,
    category: String,
    adversarial: bool,
    mode: Mode,
    replay_equal: bool,
    structural_checks: usize,
    structural_violations: usize,
    explainability_chain_valid: bool,
    false_authority: bool,
    expected_thesis_present: bool,
    bundle: RunBundle,
}
#[derive(Default, Serialize)]
struct Aggregate {
    scenarios: usize,
    turns: usize,
    replay_passes: usize,
    structural_checks: usize,
    structural_violations: usize,
    explainability_chains: usize,
    false_authority: usize,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
fn mode_rollout(mode: Mode) -> ThesisProjectionRollout {
    match mode {
        Mode::Disabled => ThesisProjectionRollout::Disabled,
        Mode::Shadow => ThesisProjectionRollout::Shadow,
    }
}
fn run(s: &Scenario, mode: Mode) -> RunBundle {
    let mut state = SystemState {
        session_id: format!("eval:{}:{mode:?}", s.id),
        ..SystemState::default()
    };
    let options = TurnOptions::new()
        .with_renderer(RendererAuthority::AuditedPlan)
        .with_thesis_projection(mode_rollout(mode));
    let mut responses = vec![];
    let mut blocked = vec![];
    let mut traces = vec![];
    for text in &s.turns {
        let input = TurnInput {
            session_id: state.session_id.clone(),
            raw_text: text.clone(),
        };
        let (out, trace) = process_turn_with_options_and_trace(&input, &mut state, options);
        responses.push(out.response);
        blocked.push(out.blocked);
        traces.push(serde_json::to_value(trace).unwrap());
    }
    let state_bytes = serde_json::to_vec(&state).unwrap();
    RunBundle {
        responses,
        blocked,
        traces,
        thesis_state: serde_json::to_value(&state.semantic.thesis_state).unwrap(),
        final_state_digest: digest(&state_bytes),
    }
}
fn ids_and_digests(v: &Value) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut ids = BTreeSet::new();
    let mut digests = BTreeSet::new();
    if let Some(o) = v.get("lifecycles").and_then(Value::as_object) {
        ids.extend(o.keys().cloned());
    }
    if let Some(a) = v.get("projected_digests").and_then(Value::as_array) {
        for d in a {
            if let Some(x) = d.as_str() {
                digests.insert(x.into());
            }
        }
    }
    (ids, digests)
}
fn pack_chain(root: &Path, s: &Scenario) -> bool {
    let dir = root.join("data/packs").join(&s.pack);
    let arr =
        |name: &str| -> Vec<Value> { serde_json::from_slice(&read(&dir.join(name))).unwrap() };
    let theses = arr("theses.json");
    let relations = arr("relations.json");
    let links = arr("evidence-links.json");
    let evidence = arr("evidence.json");
    let assessments = arr("assessments.json");
    let thesis = theses.iter().any(|v| {
        v["thesis_id"] == s.expected.projected_thesis_id
            && v["thesis_digest"] == s.expected.projected_digest
    });
    let relation = relations.iter().any(|v| {
        v["from"] == s.expected.projected_digest || v["to"] == s.expected.projected_digest
    });
    let linked: Vec<&str> = links
        .iter()
        .filter(|v| v["thesis_id"] == s.expected.projected_thesis_id)
        .filter_map(|v| v["evidence_id"].as_str())
        .collect();
    let evidence_ok = linked.iter().any(|id| {
        evidence.iter().any(|v| {
            v["record"]["id"] == *id
                && matches!(
                    v["record"]["trust_class"].as_str(),
                    Some("curated_embedded" | "verified_signed_external")
                )
        })
    });
    let assessment = assessments.iter().any(|v| {
        v["thesis_id"] == s.expected.projected_thesis_id
            && v["thesis_digest"] == s.expected.projected_digest
            && v["confidence_basis_points"]
                .as_u64()
                .is_some_and(|x| x <= 10000)
            && v["authority_evidence_count"]
                .as_u64()
                .is_some_and(|x| x > 0)
    });
    thesis && relation && evidence_ok && assessment
}
fn approved_ids(root: &Path, corpus: &Corpus) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for p in corpus
        .scenarios
        .iter()
        .map(|s| &s.pack)
        .collect::<BTreeSet<_>>()
    {
        let v: Vec<Value> =
            serde_json::from_slice(&read(&root.join("data/packs").join(p).join("theses.json")))
                .unwrap();
        for x in v {
            if let Some(id) = x["thesis_id"].as_str() {
                set.insert(id.into());
            }
        }
    }
    set
}
fn main() {
    let root = env::args().nth(1).unwrap_or_else(|| ".".into());
    let root = Path::new(&root);
    let corpus_path = root.join("data/eval/thesis-graph-v1/corpus.json");
    let config_path = root.join("data/eval/thesis-graph-v1/preregistered-config.json");
    let corpus_bytes = read(&corpus_path);
    let config_bytes = read(&config_path);
    let corpus: Corpus = serde_json::from_slice(&corpus_bytes).expect("valid corpus schema");
    assert_eq!(corpus.schema_version, 1);
    assert_eq!(corpus.corpus_id, "thesis-graph-v1");
    assert_eq!(corpus.scenarios.len(), 18);
    let approved = approved_ids(root, &corpus);
    let mut records = vec![];
    let mut by_mode: BTreeMap<Mode, Aggregate> = BTreeMap::new();
    let mut surfaces: BTreeMap<String, BTreeMap<Mode, Vec<String>>> = BTreeMap::new();
    for s in &corpus.scenarios {
        assert_eq!(s.turns.len(), 3);
        for mode in [Mode::Disabled, Mode::Shadow] {
            let a = run(s, mode);
            let b = run(s, mode);
            let replay_equal = serde_json::to_vec(&a).unwrap() == serde_json::to_vec(&b).unwrap();
            let (ids, digests) = ids_and_digests(&a.thesis_state);
            let expected_present = pack_chain(root, s);
            let checks = 2;
            let violations = usize::from(!ids.is_empty()) + usize::from(!digests.is_empty());
            let chain = expected_present;
            let false_authority = s.expected.user_text_must_not_be_authority
                && ids.iter().any(|id| !approved.contains(id));
            let ag = by_mode.entry(mode).or_default();
            ag.scenarios += 1;
            ag.turns += s.turns.len();
            ag.replay_passes += usize::from(replay_equal);
            ag.structural_checks += checks;
            ag.structural_violations += violations;
            ag.explainability_chains += usize::from(chain);
            ag.false_authority += usize::from(false_authority);
            surfaces
                .entry(s.id.clone())
                .or_default()
                .insert(mode, a.responses.clone());
            records.push(Record {
                scenario_id: s.id.clone(),
                pack: s.pack.clone(),
                category: s.category.clone(),
                adversarial: s.adversarial,
                mode,
                replay_equal,
                structural_checks: checks,
                structural_violations: violations,
                explainability_chain_valid: chain,
                false_authority,
                expected_thesis_present: expected_present,
                bundle: a,
            });
        }
    }
    let shadow_differences = surfaces
        .values()
        .filter(|m| m[&Mode::Disabled] != m[&Mode::Shadow])
        .count();
    let all_replay = by_mode.values().all(|a| a.replay_passes == a.scenarios);
    let safety = by_mode.values().all(|a| a.false_authority == 0);
    let structural = by_mode.values().all(|a| a.structural_violations == 0);
    let explain = by_mode
        .values()
        .all(|a| a.explainability_chains == a.scenarios);
    let verdict = if structural && explain && all_replay && safety {
        "Observation-only"
    } else {
        "Not proven"
    };
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let report = json!({"schema_version":1,"evaluation_id":"thesis-graph-v1","commit":commit,"config_sha256":digest(&config_bytes),"corpus_sha256":digest(&corpus_bytes),"modes":by_mode,"surface":{"shadow_vs_disabled_differences":shadow_differences,"scenario_count":corpus.scenarios.len()},"hypotheses":{"H1":{"pass":structural,"scope":"no thesis state mutation in Disabled/Shadow"},"H2":{"pass":explain,"scope":"static catalog evidence chain"},"H3":{"pass":all_replay},"H4":{"pass":safety},"surface_effect":{"pass":shadow_differences==0}},"verdict":verdict,"limitations":["No active thesis lifecycle persistence is evaluated or enabled.","Explainability is structural traceability, not a subjective rating of prose.","Corpus uses embedded deterministic packs and synchronous public pipeline API; no external model variability is measured."]});
    let out = root.join("data/eval/results");
    fs::create_dir_all(&out).unwrap();
    let raw = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(out.join("thesis-graph-v1.raw.jsonl"), raw).unwrap();
    fs::write(
        out.join("thesis-graph-v1.report.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
