//! «Кодекс» product acceptance: reflect never touches the database, report
//! is read-only, fail-closed on missing artifacts, and never overwrites a
//! journal export.

mod common;

use common::{assert_success, run, turn, TestDir};

#[test]
fn reflect_daily_topic_never_creates_a_database() {
    let dir = TestDir::new("codex-reflect");
    let db = dir.join("missing.db");
    let output = run(&db, &["reflect"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("тема дня"),
        "daily card must render: {stdout}"
    );
    assert!(stdout.contains("Контрпункт"));
    assert!(
        stdout.contains("первая запись") && stdout.contains("Ответь одним предложением"),
        "first contact must explain the ritual: {stdout}"
    );
    assert!(
        !db.exists(),
        "reflect must not create the database on a mistyped path"
    );
}

#[test]
fn reflect_explicit_topic_works_and_unaudited_topic_fails_closed() {
    let dir = TestDir::new("codex-reflect-topic");
    let db = dir.join("unused.db");
    let output = run(&db, &["reflect", "свобода"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("свобода"));

    // Explicit CLI arguments are echoed back in usage errors: they are not
    // secrets, and the message must tell the user which topic was rejected.
    let output = run(&db, &["reflect", "нетакойтемы"]);
    assert!(!output.status.success(), "unaudited topic must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("нетакойтемы"), "got: {stderr}");
    assert!(stderr.contains("аудированн"), "got: {stderr}");
}

#[test]
fn report_requires_an_existing_database_and_session() {
    let dir = TestDir::new("codex-report-missing");
    let missing_db = dir.join("absent.db");
    let output = run(&missing_db, &["report"]);
    assert!(
        !output.status.success(),
        "missing database must fail closed"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).trim().is_empty(),
        "failure must explain itself on stderr"
    );
    assert!(
        !missing_db.exists(),
        "report must not create a database on a mistyped path"
    );

    let db = dir.join("real.db");
    assert_success(&run(&db, &["sessions"])); // creates an empty database
    let output = run(&db, &["--session-id", "ghost", "report"]);
    assert!(!output.status.success(), "missing session must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ghost"), "got: {stderr}");
}

#[test]
fn report_shows_the_belief_protocol_after_real_turns() {
    let dir = TestDir::new("codex-report-flow");
    let db = dir.join("diary.db");
    assert_success(&turn(&db, "diary", "что такое свобода?"));
    assert_success(&turn(&db, "diary", "что ты думаешь об ответственности?"));

    let output = run(&db, &["--session-id", "diary", "report"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("протокол размышлений"), "got: {stdout}");
    assert!(stdout.contains("Сессия: diary"));
    assert!(stdout.contains("ходов: 2"));
    assert!(
        stdout.contains("свобода"),
        "topic counts must appear: {stdout}"
    );
    assert!(
        stdout.contains("дней практики: 1"),
        "practice metric must be visible: {stdout}"
    );

    let markdown = run(&db, &["--session-id", "diary", "report", "--markdown"]);
    assert_success(&markdown);
    let markdown = String::from_utf8_lossy(&markdown.stdout);
    assert!(markdown.contains("# Кодекс"));
    assert!(markdown.contains("## Убеждения"));
    assert!(markdown.contains("## Governance"));
}

#[test]
fn report_export_never_overwrites_an_existing_file() {
    let dir = TestDir::new("codex-report-export");
    let db = dir.join("diary.db");
    assert_success(&turn(&db, "diary", "что такое свобода?"));

    let export = dir.join("week-1.md");
    let output = run(
        &db,
        &[
            "--session-id",
            "diary",
            "report",
            "--markdown",
            "--out",
            export.to_str().unwrap(),
        ],
    );
    assert_success(&output);
    let written = std::fs::read_to_string(&export).expect("export written");
    assert!(written.contains("# Кодекс"));

    let second = run(
        &db,
        &[
            "--session-id",
            "diary",
            "report",
            "--markdown",
            "--out",
            export.to_str().unwrap(),
        ],
    );
    assert!(
        !second.status.success(),
        "an existing export must never be overwritten"
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "failure must explain itself on stderr"
    );
    assert_eq!(
        std::fs::read_to_string(&export).expect("original export intact"),
        written,
        "the refused overwrite must not touch the existing file"
    );
}
