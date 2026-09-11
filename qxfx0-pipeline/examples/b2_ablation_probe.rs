//! B2 ablation probe (ADR-0044 verdict procedure): run both arms over the
//! full admitted corpus in fresh sessions and over a long single-topic
//! challenged session, then aggregate the shadow trajectories. Re-run
//! after any v2 tuning to re-evaluate the unification verdict. Not
//! executed by CI (examples are built, not run).
//!
//! Thin printer over `qxfx0_pipeline::b2_report::run_b2_report` — the
//! numbers below are the same ones a `flip-draft` proposal embeds, so the
//! human re-run and the machine proposal can never diverge.
use qxfx0_pipeline::b2_report::{parse_b2_prompts, run_b2_report, B2ArmCorpus, B2ArmLong};

fn print_corpus(arm_name: &str, summary: &B2ArmCorpus) {
    println!(
        "{arm_name}: turns={} blocked={} advances={} committed={} sessions_with_commit={} suppressed={} violations={} mean_angst={:.4}",
        summary.prompts,
        summary.blocked,
        summary.advances,
        summary.committed,
        summary.sessions_with_commit,
        summary.suppressed,
        summary.violations,
        summary.mean_angst,
    );
}

fn print_long(arm_name: &str, summary: &B2ArmLong) {
    println!(
        "long-{arm_name}: turns={} v1_committed={} v2_committed={} suppressed={} violations={} max_run={} releases={} mean_angst={:.4}",
        summary.turns,
        summary.v1_committed,
        summary.v2_committed,
        summary.suppressed,
        summary.violations,
        summary.max_run,
        summary.releases,
        summary.mean_angst,
    );
}

fn main() {
    let tsv = include_str!("../../qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv");
    let prompts = parse_b2_prompts(tsv);
    println!("prompts: {}", prompts.len());
    let report = run_b2_report(&prompts);
    print_corpus("enabled", &report.enabled_corpus);
    print_corpus("ablated", &report.ablated_corpus);

    // Leg 3: long single-topic session with challenges — the regime where
    // commitment dynamics actually live. Compares v1 essence commitment
    // against the v2 trajectory in both arms.
    print_long("enabled", &report.enabled_long);
    print_long("ablated", &report.ablated_long);
}
