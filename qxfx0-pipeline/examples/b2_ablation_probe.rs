//! B2 ablation probe (ADR-0044 verdict procedure): run both arms over the
//! full admitted corpus in fresh sessions and over a long single-topic
//! challenged session, then aggregate the shadow trajectories. Re-run
//! after any v2 tuning to re-evaluate the unification verdict. Not
//! executed by CI (examples are built, not run).
use qxfx0_pipeline::{
    process_turn_with_options_and_trace, EssenceAblation, TurnInput, TurnOptions,
};
use qxfx0_types::system_state::SystemState;

fn main() {
    let tsv = include_str!("../../qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv");
    let prompts: Vec<&str> = tsv
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    println!("prompts: {}", prompts.len());
    for (arm_name, ablation) in [
        ("enabled", EssenceAblation::Enabled),
        ("ablated", EssenceAblation::CommitDisabled),
    ] {
        let options = TurnOptions::new().with_essence_v2_ablation(ablation);
        let mut advances = 0usize;
        let mut committed = 0usize;
        let mut suppressed = 0usize;
        let mut violations = 0usize;
        let mut angst_sum = 0.0f64;
        let mut blocked = 0usize;
        let mut sessions_with_commit = 0usize;
        for (i, prompt) in prompts.iter().enumerate() {
            let session_id = format!("b2-{arm_name}-{i}");
            let mut state = SystemState {
                session_id: session_id.clone(),
                ..SystemState::default()
            };
            let input = TurnInput {
                session_id,
                raw_text: prompt.to_string(),
            };
            let (output, trace) = process_turn_with_options_and_trace(&input, &mut state, options);
            if output.blocked {
                blocked += 1;
            }
            if let Some(adv) = trace.essence_advance {
                advances += 1;
                angst_sum += adv.angst_level;
                if adv.committed.is_some() {
                    committed += 1;
                    sessions_with_commit += 1;
                }
                if adv.ablated_commit_suppressed {
                    suppressed += 1;
                }
                if adv.violation.is_some() {
                    violations += 1;
                }
            }
        }
        println!(
            "{arm_name}: turns={} blocked={blocked} advances={advances} committed={committed} sessions_with_commit={sessions_with_commit} suppressed={suppressed} violations={violations} mean_angst={:.4}",
            prompts.len(),
            angst_sum / advances.max(1) as f64,
        );
    }

    // Leg 3: long single-topic session with challenges — the regime where
    // commitment dynamics actually live. Compares v1 essence commitment
    // against the v2 trajectory in both arms.
    let script = [
        "что такое свобода?",
        "свобода это просто вседозволенность",
        "я не согласен: свобода требует осознанности",
        "свобода — это отсутствие ограничений, разве нет?",
        "я считаю, что свобода без ответственности невозможна",
        "но ведь произвол — тоже свобода?",
        "свобода для меня — это прежде всего выбор",
        "ты противоречишь себе: определись",
    ];
    for (arm_name, ablation) in [
        ("enabled", EssenceAblation::Enabled),
        ("ablated", EssenceAblation::CommitDisabled),
    ] {
        let options = TurnOptions::new().with_essence_v2_ablation(ablation);
        let session_id = format!("b2-long-{arm_name}");
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        let mut committed = 0usize;
        let mut suppressed = 0usize;
        let mut violations = 0usize;
        let mut angst_sum = 0.0f64;
        let mut n = 0usize;
        for _ in 0..8 {
            for prompt in &script {
                let input = TurnInput {
                    session_id: session_id.clone(),
                    raw_text: prompt.to_string(),
                };
                let (_, trace) = process_turn_with_options_and_trace(&input, &mut state, options);
                if let Some(adv) = trace.essence_advance {
                    n += 1;
                    angst_sum += adv.angst_level;
                    if adv.committed.is_some() {
                        committed += 1;
                    }
                    if adv.ablated_commit_suppressed {
                        suppressed += 1;
                    }
                    if adv.violation.is_some() {
                        violations += 1;
                    }
                }
            }
        }
        let v1 = state.semantic.essence.commitment.is_some();
        println!(
            "long-{arm_name}: turns={n} v1_committed={v1} v2_committed={committed} suppressed={suppressed} violations={violations} mean_angst={:.4}",
            angst_sum / n.max(1) as f64,
        );
    }
}
