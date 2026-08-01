//! QxFx0 cli crate — generated from Haskell specification.
//!
//! Exposes the same entry point used by `main.rs` so integration tests can
//! drive the turn / chat flow without spawning a subprocess.

mod measurement;

pub use measurement::{
    run_renderer_diversity_audit, run_runtime_benchmark, LatencyDistributionMicros,
    RendererDiversityReport, RepeatedText, RuntimeBenchmarkReport, TopicRenderMeasurement,
};

use qxfx0_code::{build_full_registry, CodeOrchestrator};
use qxfx0_pipeline::{process_turn, TurnInput};
use qxfx0_semantic::{argued_topic_registry, seed_graph};
use qxfx0_types::system_state::{SemanticState, SystemState};
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationalMetrics {
    pub doctor_healthy: bool,
    pub database_bytes: u64,
    pub doctor_duration_ms: u64,
    pub response_probe_ms: u64,
    pub response_probe_healthy: bool,
}

impl OperationalMetrics {
    pub fn threshold_violations(
        &self,
        max_database_bytes: u64,
        max_response_ms: u64,
    ) -> Vec<String> {
        let mut violations = Vec::new();
        if !self.doctor_healthy {
            violations.push("doctor reported an unhealthy subsystem".into());
        }
        if self.database_bytes > max_database_bytes {
            violations.push(format!(
                "database storage is {} bytes, limit is {} bytes",
                self.database_bytes, max_database_bytes
            ));
        }
        if !self.response_probe_healthy {
            violations.push("response probe returned an invalid result".into());
        }
        if self.response_probe_ms > max_response_ms {
            violations.push(format!(
                "response probe took {} ms, limit is {} ms",
                self.response_probe_ms, max_response_ms
            ));
        }
        violations
    }

    pub fn to_prometheus(&self) -> String {
        format!(
            concat!(
                "# TYPE qxfx0_doctor_healthy gauge\n",
                "qxfx0_doctor_healthy {}\n",
                "# TYPE qxfx0_database_bytes gauge\n",
                "qxfx0_database_bytes {}\n",
                "# TYPE qxfx0_doctor_duration_seconds gauge\n",
                "qxfx0_doctor_duration_seconds {:.6}\n",
                "# TYPE qxfx0_response_probe_duration_seconds gauge\n",
                "qxfx0_response_probe_duration_seconds {:.6}\n",
                "# TYPE qxfx0_response_probe_healthy gauge\n",
                "qxfx0_response_probe_healthy {}\n"
            ),
            u8::from(self.doctor_healthy),
            self.database_bytes,
            self.doctor_duration_ms as f64 / 1_000.0,
            self.response_probe_ms as f64 / 1_000.0,
            u8::from(self.response_probe_healthy),
        )
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn database_storage_bytes(db_path: &str) -> u64 {
    [
        db_path.to_string(),
        format!("{db_path}-wal"),
        format!("{db_path}-shm"),
    ]
    .iter()
    .filter_map(|path| std::fs::metadata(path).ok())
    .map(|metadata| metadata.len())
    .sum()
}

/// Collect machine-readable health, storage and synthetic response metrics.
/// The response probe runs entirely in memory and never changes the monitored
/// database.
pub fn run_operational_metrics(db_path: &str) -> OperationalMetrics {
    let doctor_started = Instant::now();
    let doctor_report = run_doctor(db_path);
    let doctor_duration_ms = elapsed_millis(doctor_started);

    let mut probe_state = fresh_state("__operational_probe__");
    let probe_input = TurnInput {
        raw_text: "что такое свобода?".into(),
        session_id: probe_state.session_id.clone(),
    };
    let response_started = Instant::now();
    let probe_output = process_turn(&probe_input, &mut probe_state);
    let response_probe_ms = elapsed_millis(response_started);
    let response_probe_healthy =
        !probe_output.response.trim().is_empty() && probe_state.validate().is_empty();

    OperationalMetrics {
        doctor_healthy: doctor_report.is_healthy(),
        database_bytes: database_storage_bytes(db_path),
        doctor_duration_ms,
        response_probe_ms,
        response_probe_healthy,
    }
}

impl DoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

/// Execute production health checks without mutating session state. Opening
/// the database may apply the normal idempotent schema migration.
pub fn run_doctor(db_path: &str) -> DoctorReport {
    let mut report = DoctorReport::default();

    match qxfx0_persistence::Persistence::open(db_path) {
        Ok(db) => match db.health_check() {
            Ok(violations) => report.checks.push(DoctorCheck {
                name: "SQLite",
                passed: violations.is_empty(),
                details: if violations.is_empty() {
                    format!(
                        "schema v{}, quick_check/foreign keys/session states valid",
                        db.schema_version().unwrap_or_default()
                    )
                } else {
                    violations.join("; ")
                },
            }),
            Err(error) => report.checks.push(DoctorCheck {
                name: "SQLite",
                passed: false,
                details: error.to_string(),
            }),
        },
        Err(error) => report.checks.push(DoctorCheck {
            name: "SQLite",
            passed: false,
            details: error.to_string(),
        }),
    }

    let graph = seed_graph();
    let mut graph_violations = graph.validate().err().unwrap_or_default();
    for topic in qxfx0_semantic::COVERED_TOPICS {
        if !graph
            .atoms
            .contains_key(&qxfx0_types::atom::AtomId::new(*topic))
        {
            graph_violations.push(format!("covered topic '{topic}' is absent from seed graph"));
        }
    }
    report.checks.push(DoctorCheck {
        name: "Seed graph",
        passed: graph_violations.is_empty(),
        details: if graph_violations.is_empty() {
            format!(
                "{} atoms, {} relations, {} covered topics",
                graph.atoms.len(),
                graph.edges.len(),
                qxfx0_semantic::COVERED_TOPICS.len()
            )
        } else {
            graph_violations.join("; ")
        },
    });

    let concepts = qxfx0_semantic::get_resolver();
    let concept_violations = concepts
        .records()
        .filter(|record| {
            !graph.atoms.contains_key(&qxfx0_types::atom::AtomId::new(
                record.graph_atom_id.clone(),
            ))
        })
        .map(|record| format!("dangling graph atom for {}", record.concept_id.0))
        .collect::<Vec<_>>();
    let missing_topic_concepts = qxfx0_semantic::COVERED_TOPICS
        .iter()
        .filter(|topic| {
            !matches!(
                concepts.resolve(topic),
                qxfx0_semantic::ResolutionOutcome::Resolved(ref entry)
                    if entry.atom_id.as_str() == **topic
            )
        })
        .copied()
        .collect::<Vec<_>>();
    report.checks.push(DoctorCheck {
        name: "Concept registry",
        passed: concept_violations.is_empty() && missing_topic_concepts.is_empty(),
        details: if concept_violations.is_empty() && missing_topic_concepts.is_empty() {
            format!(
                "{} concepts, {} covered topics, {} ambiguous aliases, fingerprint={}",
                concepts.concept_count(),
                qxfx0_semantic::COVERED_TOPICS.len(),
                concepts.ambiguous_alias_count(),
                concepts.fingerprint(),
            )
        } else {
            concept_violations
                .into_iter()
                .chain(
                    missing_topic_concepts
                        .into_iter()
                        .map(|topic| format!("missing primary concept for {topic}")),
                )
                .collect::<Vec<_>>()
                .join("; ")
        },
    });

    let packs = qxfx0_semantic::active_pack_set();
    let active_pack_details = packs
        .summaries()
        .iter()
        .map(|pack| {
            format!(
                "{}@{}(schema={}, concepts={}, facts={}, relations={})",
                pack.pack_id,
                pack.pack_version,
                pack.schema_version,
                pack.concept_count,
                pack.fact_count,
                pack.relation_count
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    report.checks.push(DoctorCheck {
        name: "Knowledge packs",
        passed: !packs.summaries().is_empty() && packs.fact_conflict_count() == 0,
        details: format!(
            concat!(
                "active_packs=[{}], manifest_valid=true, ambiguous_aliases={}, ",
                "fact_conflicts={}, fingerprint={}"
            ),
            active_pack_details,
            packs.ambiguous_alias_count(),
            packs.fact_conflict_count(),
            packs.fingerprint(),
        ),
    });

    match qxfx0_semantic::corpus_import_report() {
        Ok(import) => report.checks.push(DoctorCheck {
            name: "Corpus import pilot",
            passed: import.metrics.status == "audit_only"
                && !import.metrics.promotion_enabled
                && import.metrics.already_active + import.metrics.quarantined
                    == import.metrics.pilot_unique_topics,
            details: format!(
                concat!(
                    "import_id={}, source_rows={}, raw_unique_topics={}, ",
                    "normalized_unique_topics={}, predicates={}, ontology_records={}, ",
                    "pilot_topics={}, already_active={}, quarantine={}, ",
                    "source_worktree_dirty={}, promotion_enabled={}, fingerprint={}"
                ),
                import.metrics.import_id,
                import.metrics.source_rows,
                import.metrics.source_raw_unique_topics,
                import.metrics.source_normalized_unique_topics,
                import.metrics.source_predicates,
                import.metrics.ontology_records,
                import.metrics.pilot_unique_topics,
                import.metrics.already_active,
                import.metrics.quarantined,
                import.metrics.source_worktree_dirty,
                import.metrics.promotion_enabled,
                import.fingerprint,
            ),
        }),
        Err(error) => report.checks.push(DoctorCheck {
            name: "Corpus import pilot",
            passed: false,
            details: error.into(),
        }),
    }

    match argued_topic_registry() {
        Ok(registry) => {
            let metrics = registry.metrics();
            let missing_topics = registry
                .topics()
                .filter(|topic| !graph.atoms.contains_key(topic.topic()))
                .map(|topic| topic.topic().as_str())
                .collect::<Vec<_>>();
            report.checks.push(DoctorCheck {
                name: "Content plan assets",
                passed: missing_topics.is_empty(),
                details: if missing_topics.is_empty() {
                    format!(
                        concat!(
                            "recognition_topics_total={}, content_predicates_total={}, ",
                            "argued_topics_admitted={}, argued_predicates_admitted={}, ",
                            "profile_enabled={}"
                        ),
                        metrics.recognition_topics_total,
                        metrics.content_predicates_total,
                        metrics.argued_topics_admitted,
                        metrics.argued_predicates_admitted,
                        metrics.profile_enabled,
                    )
                } else {
                    format!(
                        "admitted topics absent from seed graph: {}",
                        missing_topics.join(", ")
                    )
                },
            });
            let facts = registry.facts();
            let self_referential_facts = facts
                .records()
                .filter(|fact| fact.subject == fact.object)
                .count();
            let semantic_object_concepts = concepts
                .records()
                .filter(|record| record.ontology_kind == "semantic_object")
                .count();
            report.checks.push(DoctorCheck {
                name: "Fact model",
                passed: facts.len() == metrics.content_predicates_total
                    && facts.count_by_status(qxfx0_semantic::FactStatus::Curated) == facts.len()
                    && self_referential_facts == 0
                    && semantic_object_concepts == 30,
                details: format!(
                    concat!(
                        "facts={}, curated={}, deprecated={}, retracted={}, draft={}, ",
                        "semantic_object_concepts={}, self_referential_facts={}"
                    ),
                    facts.len(),
                    facts.count_by_status(qxfx0_semantic::FactStatus::Curated),
                    facts.count_by_status(qxfx0_semantic::FactStatus::Deprecated),
                    facts.count_by_status(qxfx0_semantic::FactStatus::Retracted),
                    facts.count_by_status(qxfx0_semantic::FactStatus::Draft),
                    semantic_object_concepts,
                    self_referential_facts,
                ),
            });
        }
        Err(error) => report.checks.push(DoctorCheck {
            name: "Content plan assets",
            passed: false,
            details: error.into(),
        }),
    }

    let perspective_facts = qxfx0_semantic::active_pack_set().facts();
    let perspective_counterpoints = perspective_facts
        .records()
        .filter(|fact| {
            fact.conditions
                .iter()
                .any(|condition| matches!(condition, qxfx0_semantic::FactCondition::Counters(_)))
        })
        .count();
    let perspective_consequences = perspective_facts
        .records()
        .filter(|fact| {
            fact.conditions
                .iter()
                .any(|condition| matches!(condition, qxfx0_semantic::FactCondition::FollowsFrom(_)))
        })
        .count();
    let perspective_grounding_valid = perspective_facts
        .records()
        .all(|fact| perspective_facts.select(&fact.id).is_ok());
    report.checks.push(DoctorCheck {
        name: "Perspective model",
        passed: perspective_grounding_valid && perspective_counterpoints >= 30,
        details: format!(
            concat!(
                "fact_id_grounded={}, counterpoint_links={}, consequence_links={}, ",
                "max_opinions={}, max_episodes={}"
            ),
            perspective_grounding_valid,
            perspective_counterpoints,
            perspective_consequences,
            qxfx0_types::MAX_PERSPECTIVE_OPINIONS,
            qxfx0_types::MAX_PERSPECTIVE_EPISODES,
        ),
    });

    let templates = qxfx0_semantic::TemplateRegistry::load();
    let template_violations = templates.validate();
    let used_relation_types = graph
        .edges
        .iter()
        .map(|relation| relation.rel_type)
        .collect::<std::collections::BTreeSet<_>>();
    let covered_relation_types = used_relation_types
        .iter()
        .filter(|relation_type| !templates.get(**relation_type).is_empty())
        .count();
    report.checks.push(DoctorCheck {
        name: "Templates",
        passed: template_violations.is_empty(),
        details: if template_violations.is_empty() {
            format!(
                "{} templates for {} types; direct coverage {}/{} used relation types",
                templates.template_count(),
                templates.relation_type_count(),
                covered_relation_types,
                used_relation_types.len()
            )
        } else {
            template_violations.join("; ")
        },
    });

    let morphology = qxfx0_morphology::get_runtime();
    let morphology_stats = morphology.stats();
    let morphology_manifest = morphology.manifest.as_ref();
    let morphology_version_valid =
        morphology_manifest.is_some_and(|manifest| manifest.bundle_version == 1);
    let morphology_commit_valid = morphology_manifest.is_some_and(|manifest| {
        (manifest.source_commit.len() == 40 || manifest.source_commit.len() == 64)
            && manifest
                .source_commit
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    });
    let morphology_passed = morphology_manifest.is_some()
        && morphology_version_valid
        && morphology_commit_valid
        && morphology.manifest_hash_valid()
        && morphology_stats.total_lexemes > 0
        && matches!(
            morphology.lemmatize("свободы"),
            qxfx0_morphology::MorphologyLookup::Resolved(_)
        )
        && morphology.inflect(
            "дом",
            qxfx0_morphology::Case::Prepositional,
            qxfx0_morphology::Number::Singular,
        ) == Some("доме".into());
    report.checks.push(DoctorCheck {
        name: "Morphology bundle",
        passed: morphology_passed,
        details: if morphology_passed {
            format!(
                concat!(
                    "version={}, source_commit={}, sha256={}, lexemes={}, ",
                    "source_tier_counts={{curated:{}, reviewed:{}, auto_verified:{}, auto_coverage:{}}}, ",
                    "ambiguous_surfaces={}"
                ),
                morphology_manifest.map_or(0, |manifest| manifest.bundle_version),
                morphology_manifest.map_or("missing", |manifest| manifest.source_commit.as_str()),
                morphology.lexemes_sha256(),
                morphology_stats.total_lexemes,
                morphology_stats.curated_count,
                morphology_stats.reviewed_count,
                morphology_stats.auto_verified_count,
                morphology_stats.auto_coverage_count,
                morphology.ambiguous_surface_count(),
            )
        } else {
            format!(
                "invalid morphology bundle: version_valid={}, source_commit_valid={}, hash_valid={}, lexemes={}",
                morphology_version_valid,
                morphology_commit_valid,
                morphology.manifest_hash_valid(),
                morphology_stats.total_lexemes,
            )
        },
    });

    let code_graph = build_full_registry();
    let mut code_violations = code_graph.validate();
    let type_edges = code_graph
        .edges
        .iter()
        .filter(|edge| edge.rel_type == qxfx0_code::CodeRelationType::RelComposes)
        .count();
    if code_graph.atoms.len() < 80 {
        code_violations.push("production registry contains fewer than 80 real atoms".into());
    }
    if type_edges == 0 {
        code_violations.push("production registry contains no type-directed edges".into());
    }
    report.checks.push(DoctorCheck {
        name: "Code registry",
        passed: code_violations.is_empty(),
        details: if code_violations.is_empty() {
            format!(
                "{} typed atoms, {} relations, {} RelComposes edges",
                code_graph.atoms.len(),
                code_graph.edges.len(),
                type_edges
            )
        } else {
            code_violations.join("; ")
        },
    });

    report
}

/// Build a freshly seeded `SystemState` for a given session id.
pub fn fresh_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.to_string(),
        semantic: SemanticState {
            runtime_graph: seed_graph(),
            pack_set_fingerprint: qxfx0_semantic::active_pack_set().fingerprint().into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Load existing state for the session, or create a fresh one seeded with the
/// knowledge graph. Any persistence error other than "no such session row"
/// (which `Persistence::load_state` already maps to `Ok(None)`) is propagated
/// via `?` so the caller can surface it to the user.
pub fn load_or_create_state(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
) -> anyhow::Result<SystemState> {
    match db.load_state(session_id) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(fresh_state(session_id)),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// DialogueSession encapsulates the state and tools needed for a conversation.
pub struct DialogueSession {
    pub state: SystemState,
    pub db: qxfx0_persistence::Persistence,
    pub orchestrator: CodeOrchestrator,
}

impl DialogueSession {
    pub fn new(db: qxfx0_persistence::Persistence, session_id: &str) -> anyhow::Result<Self> {
        let state = load_or_create_state(&db, session_id)?;
        let graph = build_full_registry();
        let orchestrator = CodeOrchestrator::new(graph);
        Ok(Self {
            state,
            db,
            orchestrator,
        })
    }

    /// Process a turn, integrating both the semantic pipeline and code orchestration.
    pub fn process_turn(&mut self, text: &str) -> anyhow::Result<String> {
        let input = TurnInput {
            raw_text: text.to_string(),
            session_id: self.state.session_id.clone(),
        };

        // 1. Try the standard semantic pipeline
        let output = process_turn(&input, &mut self.state);

        // 2. If the response is empty or looks like a request for code/action,
        // we can integrate the CodeOrchestrator here.
        // For now, we prioritize the pipeline but allow the orchestrator to supplement
        // if the pipeline output is a specific trigger or empty.
        let final_response = if output.response.is_empty() || output.response.contains("код") {
            match self.orchestrator.orchestrate(text) {
                Ok(res) => format!(
                    "{} \n\n[Code Orchestration]: {}\n",
                    output.response, res.rendered
                ),
                Err(_) => output.response,
            }
        } else {
            output.response
        };

        self.db.save_state(&self.state.session_id, &self.state)?;
        Ok(final_response)
    }
}

/// Run a single turn through the pipeline (mirrors the `Turn` CLI branch).
/// Persists before returning the response text — see H4 in the audit.
pub fn run_turn(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
) -> anyhow::Result<String> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let output = process_turn(&input, &mut state);
    db.save_state(session_id, &state)?;
    Ok(output.response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Stdio;

    /// M7.1 — smoke test: run a `Turn` against an in-memory DB and assert the
    /// pipeline returns a non-empty response. Mirrors the `Turn` CLI branch.
    #[test]
    fn test_turn_smoke() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open in-memory db");
        let response =
            run_turn(&db, "smoke-session", "что такое свобода?").expect("turn should succeed");
        assert!(
            !response.is_empty(),
            "pipeline produced empty response for seeded topic"
        );

        // State should have been persisted by run_turn.
        let loaded = db
            .load_state("smoke-session")
            .expect("load after save")
            .expect("session row must exist");
        assert_eq!(loaded.session_id, "smoke-session");
        assert!(loaded.dialogue.turn_count >= 1);
    }

    /// M7.2 — chat-mode EOF must still persist state. We spawn the actual
    /// binary with piped stdin that ends in EOF (no `:quit`) so the chat
    /// loop reaches the unconditional save at the end.
    #[test]
    fn test_chat_eof_saves() {
        let tmp = std::env::temp_dir().join(format!("qxfx0-cli-eof-{}.db", std::process::id()));
        let db_path = tmp.to_string_lossy().to_string();
        let session_id = format!("eof-session-{}", std::process::id());

        let stdin_payload = "что такое истина?\n".to_string();

        // Resolve the binary path. `CARGO_BIN_EXE_qxfx0` is set when this
        // crate's own integration tests build (or when running via
        // `cargo test --bin qxfx0`); fall back to the conventional
        // `target/debug/qxfx0` path for plain `cargo test --workspace`.
        let bin = match std::env::var("CARGO_BIN_EXE_qxfx0") {
            Ok(p) => p,
            Err(_) => {
                let target_dir =
                    std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
                let workspace = std::env::var("CARGO_MANIFEST_DIR")
                    .map(|m| {
                        std::path::PathBuf::from(m)
                            .join(&target_dir)
                            .join("debug/qxfx0")
                    })
                    .unwrap_or_else(|_| std::path::PathBuf::from(&target_dir).join("debug/qxfx0"));
                workspace.to_string_lossy().to_string()
            }
        };

        if !std::path::Path::new(&bin).exists() {
            eprintln!(
                "skipping test_chat_eof_saves: qxfx0 binary not found at {}",
                bin
            );
            return;
        }

        let mut child = std::process::Command::new(&bin)
            .args(["chat", "--session-id", &session_id, "--db", &db_path])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn qxfx0 chat");

        {
            let mut stdin = child.stdin.take().expect("stdin pipe");
            stdin
                .write_all(stdin_payload.as_bytes())
                .expect("write stdin");
            // Dropping `stdin` here closes the pipe → EOF on the child side.
        }

        let output = child.wait_with_output().expect("wait for qxfx0");
        assert!(
            output.status.success(),
            "qxfx0 chat exited non-zero: {:?}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let db =
            qxfx0_persistence::Persistence::open(&db_path).expect("reopen db written by child");
        let loaded = db
            .load_state(&session_id)
            .expect("load after child exit")
            .expect("session row must exist after EOF exit");
        assert_eq!(loaded.session_id, session_id);
        assert!(
            loaded.dialogue.turn_count >= 1,
            "expected at least one processed turn before EOF"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    /// M7.3 — a corrupted session row must surface as an `Err` rather than
    /// silently being treated as "no state, create a fresh one".
    #[test]
    fn test_corrupted_db() {
        let tmp = std::env::temp_dir().join(format!("qxfx0-cli-corrupt-{}.db", std::process::id()));
        let db_path = tmp.to_string_lossy().to_string();

        // Create the schema by opening the DB once.
        {
            let db = qxfx0_persistence::Persistence::open(&db_path).expect("open");
            // Use the persistence API to insert a valid row first (so the
            // table exists), then overwrite the JSON column with garbage
            // through a side-channel `rusqlite::Connection`.
            db.save_state("corrupt-session", &fresh_state("corrupt-session"))
                .expect("initial save");
        }

        // Corrupt the JSON column directly.
        {
            use rusqlite::{params, Connection};
            let conn = Connection::open(&db_path).expect("open conn");
            conn.execute(
                "UPDATE runtime_sessions SET state_json = ?1 WHERE id = ?2",
                params!["{not valid json", "corrupt-session"],
            )
            .expect("corrupt row");
        }

        // Re-open via the typed Persistence and try load_or_create_state.
        let db = qxfx0_persistence::Persistence::open(&db_path).expect("reopen");
        let result = load_or_create_state(&db, "corrupt-session");
        assert!(
            result.is_err(),
            "load_or_create_state must propagate serialization errors, got Ok"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_doctor_checks_real_subsystems() {
        let path = std::env::temp_dir().join(format!("qxfx0-doctor-{}.db", std::process::id()));
        let report = run_doctor(path.to_str().unwrap());
        assert!(
            report.is_healthy(),
            "doctor failures: {:?}",
            report
                .checks
                .iter()
                .filter(|check| !check.passed)
                .collect::<Vec<_>>()
        );
        assert_eq!(report.checks.len(), 11);
        let concepts = report
            .checks
            .iter()
            .find(|check| check.name == "Concept registry")
            .expect("concept registry check");
        assert!(concepts.details.contains("137 concepts"));
        assert!(concepts.details.contains("107 covered topics"));
        let morphology = report
            .checks
            .iter()
            .find(|check| check.name == "Morphology bundle")
            .expect("morphology bundle check");
        assert!(morphology.details.contains("source_tier_counts="));
        assert!(morphology.details.contains("ambiguous_surfaces="));
        let facts = report
            .checks
            .iter()
            .find(|check| check.name == "Fact model")
            .expect("fact model check");
        assert!(facts.details.contains("facts=69"));
        assert!(facts.details.contains("semantic_object_concepts=30"));
        assert!(facts.details.contains("self_referential_facts=0"));
        let perspective = report
            .checks
            .iter()
            .find(|check| check.name == "Perspective model")
            .expect("perspective model check");
        assert!(perspective.details.contains("fact_id_grounded=true"));
        assert!(perspective.details.contains("counterpoint_links=30"));
        let content_assets = report
            .checks
            .iter()
            .find(|check| check.name == "Content plan assets")
            .expect("content plan assets check");
        assert!(content_assets.details.contains("argued_topics_admitted=30"));
        assert!(content_assets
            .details
            .contains("content_predicates_total=69"));
        let packs = report
            .checks
            .iter()
            .find(|check| check.name == "Knowledge packs")
            .expect("knowledge packs check");
        assert!(packs.details.contains("philosophy-core-v1@1"));
        assert!(packs.details.contains("fact_conflicts=0"));
        let import = report
            .checks
            .iter()
            .find(|check| check.name == "Corpus import pilot")
            .expect("corpus import pilot check");
        assert!(import.details.contains("pilot_topics=300"));
        assert!(import.details.contains("quarantine=295"));
        assert!(import.details.contains("promotion_enabled=false"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_operational_metrics_cover_health_storage_and_latency() {
        let path = std::env::temp_dir().join(format!("qxfx0-metrics-{}.db", std::process::id()));
        let metrics = run_operational_metrics(path.to_str().unwrap());
        assert!(metrics.doctor_healthy);
        assert!(metrics.database_bytes > 0);
        assert!(metrics.response_probe_healthy);
        assert!(metrics.threshold_violations(u64::MAX, u64::MAX).is_empty());
        assert!(metrics.to_prometheus().contains("qxfx0_database_bytes"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
