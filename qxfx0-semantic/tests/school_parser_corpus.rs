//! Parser drift gate over the school-parser research corpus (400 rows).
//!
//! The corpus originates from the Haskell QxFx0 project (see
//! `data/eval/school-parser-400/manifest.json` for provenance). It was
//! authored for parser-layer regression testing and was never wired to a
//! test on the Haskell side; this harness is its first executable consumer.
//!
//! Contract: for every corpus row the `PropositionParser` must produce a
//! stable (mode, subject) pair. The expected values are pinned in
//! `parser-snapshot.csv` next to the corpus. The snapshot file is written
//! on first run (bless) and compared byte-for-byte afterwards — any parser
//! change that shifts mode or subject for a row must update the snapshot
//! consciously.

use qxfx0_semantic::PropositionParser;
use std::path::PathBuf;

const CORPUS_ROWS: usize = 400;

fn eval_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/eval/school-parser-400")
}

/// Minimal quoted-CSV field splitter for the corpus format.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for character in line.chars() {
        match character {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    fields.push(current);
    fields
}

fn corpus_rows() -> Vec<(String, String)> {
    let raw = std::fs::read_to_string(eval_dir().join("corpus.csv")).expect("corpus readable");
    let raw = raw.trim_start_matches('\u{feff}');
    let mut rows = Vec::new();
    for line in raw.lines().skip(1) {
        let fields = split_csv_line(line);
        // Columns: id, category, raw_text, ...
        assert!(fields.len() >= 3, "corpus row must have id and raw_text");
        rows.push((fields[0].clone(), fields[2].clone()));
    }
    rows
}

#[test]
fn school_parser_corpus_modes_and_subjects_are_pinned() {
    let rows = corpus_rows();
    assert_eq!(rows.len(), CORPUS_ROWS, "corpus row count is fixed");

    let mut actual = String::new();
    for (id, raw_text) in &rows {
        let proposition = PropositionParser::parse(raw_text);
        actual.push_str(&format!(
            "{},{}\n",
            id,
            serde_json::to_string(&(format!("{:?}", proposition.mode), proposition.subject))
                .expect("snapshot row serializes")
        ));
    }

    let snapshot_path = eval_dir().join("parser-snapshot.csv");
    match std::fs::read_to_string(&snapshot_path) {
        Ok(pinned) => {
            let pinned_lines: Vec<&str> = pinned.lines().collect();
            let actual_lines: Vec<&str> = actual.lines().collect();
            let mut first_divergence = None;
            for (index, (pinned_line, actual_line)) in
                pinned_lines.iter().zip(actual_lines.iter()).enumerate()
            {
                if pinned_line != actual_line {
                    first_divergence = Some((index, pinned_line, actual_line));
                    break;
                }
            }
            if let Some((index, pinned_line, actual_line)) = first_divergence {
                panic!(
                    "parser drift at row {index}:\n  pinned: {pinned_line}\n  actual: {actual_line}\n\
                     if this change is intentional, update parser-snapshot.csv"
                );
            }
            assert_eq!(
                pinned_lines.len(),
                actual_lines.len(),
                "snapshot row count drifted (corpus changed?)"
            );
        }
        Err(_) => {
            std::fs::write(&snapshot_path, &actual).expect("bless parser snapshot");
            panic!(
                "parser snapshot absent; wrote a fresh one to {} — review and re-run",
                snapshot_path.display()
            );
        }
    }
}
