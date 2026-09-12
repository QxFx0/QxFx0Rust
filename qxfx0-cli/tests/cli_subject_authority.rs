//! Subject-authority acceptance (ADR-0044 M1): `--subject-authority-v2`
//! turns record their authority per journal entry, and both diary and
//! FELT verification replay each entry under its recorded authority.

mod common;

use common::{assert_success, run, TestDir};

#[test]
fn v2_authority_turns_export_and_verify() {
    let dir = TestDir::new("subject-authority");
    let db = dir.join("authority.db");
    let session = "m1";
    for text in ["что такое свобода?", "что такое память?"] {
        let output = run(
            &db,
            &[
                "--session-id",
                session,
                "turn",
                "--subject-authority-v2",
                text,
            ],
        );
        assert_success(&output);
    }
    let diary = dir.join("diary.md");
    assert_success(&run(
        &db,
        &[
            "--session-id",
            session,
            "export",
            "--out",
            diary.to_str().unwrap(),
        ],
    ));
    let verify = run(&db, &["verify-diary", diary.to_str().unwrap()]);
    assert_success(&verify);

    let felt = dir.join("felt.md");
    assert_success(&run(
        &db,
        &[
            "--session-id",
            session,
            "felt-export",
            "--out",
            felt.to_str().unwrap(),
        ],
    ));
    let verify = run(&db, &["felt-verify", felt.to_str().unwrap()]);
    assert_success(&verify);
    assert!(
        String::from_utf8_lossy(&verify.stdout).contains("не пройдены")
            || String::from_utf8_lossy(&verify.stdout).contains("пройдены"),
        "verify reports the gate outcome"
    );
}
