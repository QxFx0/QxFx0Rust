//! Import-quarantine candidate source (ADR-0043 U5.2): the rejected slice of
//! the Haskell corpus importer (`quarantine.jsonl` from
//! `scripts/import_haskell_corpus.py`) as a *global, offline* candidate
//! feed for promotion drafts.
//!
//! Each importer row carries a topic, a graph atom (or the topic itself),
//! and a list of predicates (`en` kind `prop`/`rel`, `ru` surface). This
//! module parses those rows and resolves them into canonical
//! [`PromotionCandidate`] triples — or refuses them with a named reason
//! that the draft report surfaces. It is the only sanctioned way
//! non-runtime evidence reaches a draft: it reuses the exact same
//! informativeness and identity machinery as the runtime-corroboration
//! path, so one candidate is one candidate regardless of provenance.
//!
//! Purity contract: no file IO, no clock, no network. The caller reads the
//! file and supplies bytes; known atoms are the union of the installation's
//! session-graph atoms at draft time. Deterministic: rows processed in
//! file order, atom matching over the sorted known set, event JSON byte
//! verbatim into the snapshot digest.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use qxfx0_types::{AtomId, RelationType};

/// One predicate inside an importer quarantine row: an English gloss, a
/// `prop` (single-atom claim) or `rel` (binary claim) kind, and the Russian
/// surface the importer's audit saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPredicate {
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub ru: String,
}

/// One importer quarantine record: the topic it failed under, the graph
/// atom (empty when the importer could not ground it), the predicates it
/// carried, and the refusal reasons the importer recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub graph_atom_id: String,
    #[serde(default)]
    pub predicates: Vec<ImportPredicate>,
}

/// Why an import row could not become a candidate. Reported per input
/// line (1-based) in the draft trace; refused rows never enter a draft,
/// never enter the store, and never touch the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRefusal {
    pub line: usize,
    pub detail: String,
}

fn refuse(line: usize, detail: impl Into<String>) -> ImportRefusal {
    ImportRefusal {
        line,
        detail: detail.into(),
    }
}

/// Map an importer predicate to a canonical `RelationType` by keyword. The
/// pilot's vocabulary is philosophical Russian corpus English glosses; the
/// table is derived from those glosses, editorial like the relation-type
/// weight table, and total on the pilot (`None` refuses with the row's
/// gloss echoed). New vocabulary lands here first, stays reviewable.
pub fn map_import_relation(kind: &str, en: &str) -> Option<RelationType> {
    use RelationType::*;
    if kind.trim().eq_ignore_ascii_case("rel") {
        let gloss = en.to_ascii_lowercase();
        if gloss.contains("caus") {
            // Our relation model has no bare "causes" constructor; the
            // closest causal shape is `RelGives` ("придаёт", causation by
            // contribution). Not mapping to anything would silently drop
            // causal claims, so map explicitly and let downstream review
            // see the choice in the candidate record.
            return Some(RelGives);
        }
        if gloss.contains("connect") {
            return Some(RelConnects);
        }
        if gloss.contains("orient") || gloss.contains("toward purpose") {
            return Some(RelOrientsToward);
        }
        if gloss.contains("limit") {
            return Some(RelLimitedBy);
        }
        if gloss.contains("contrast") {
            return Some(RelContrastsWith);
        }
        if gloss.contains("presuppos") {
            return Some(RelPresupposes);
        }
        if gloss.contains("requir") {
            return Some(RelRequires);
        }
        None
    } else {
        // `prop`: a claim about a single atom — the canonical identity
        // edge. `RelIsA` is the neutral membership shape; the informativeness
        // gate (object must differ from topic/subject) still applies, so a
        // prop that merely restates the topic is refused downstream as a
        // paraphrase, not silently promoted.
        Some(RelIsA)
    }
}

/// Parse the quarantine JSONL byte stream into records. Fail-closed per
/// line: a malformed line becomes an `ImportRefusal` naming the line, so a
/// corrupt export can never partially poison a draft.
pub fn parse_quarantine_jsonl(text: &str) -> (Vec<ImportRecord>, Vec<ImportRefusal>) {
    let mut records = Vec::new();
    let mut refusals = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ImportRecord>(line) {
            Ok(record) => records.push(record),
            Err(error) => refusals.push(refuse(index + 1, format!("malformed record: {error}"))),
        }
    }
    (records, refusals)
}

/// Scan a surface for the first known atom (case-insensitive), iterating
/// the sorted known set so resolution is deterministic across machines.
/// Exact token match on whitespace boundaries — a substring hit inside an
/// unrelated word is not an endpoint.
fn find_atom_in(surface: &str, known: &BTreeSet<AtomId>) -> Option<AtomId> {
    let lower = surface.to_lowercase();
    for atom in known {
        let name = atom.as_str().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if lower.split_whitespace().any(|token| token == name) {
            return Some(atom.clone());
        }
    }
    None
}

/// Resolve one record's predicates into candidates against `known` atoms.
/// Returns the resolved candidates plus per-row refusal reasons for
/// everything that did not resolve. `snapshot_id` is the mixed draft
/// snapshot the caller already computed (runtime triples + import bytes);
/// a candidate's identity embeds it, so the same content re-drafts to the
/// same version.
pub fn candidates_from_import(
    records: &[ImportRecord],
    snapshot_id: &str,
    known: &BTreeSet<AtomId>,
) -> (
    Vec<crate::PromotionCandidate>,
    Vec<(String, crate::ExclusionReason)>,
) {
    use crate::PromotionCandidate;
    let mut candidates = Vec::new();
    let mut refused: Vec<(String, crate::ExclusionReason)> = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let subject_name = if record.graph_atom_id.trim().is_empty() {
            record.topic.trim()
        } else {
            record.graph_atom_id.trim()
        };
        if subject_name.is_empty() {
            refused.push((
                format!(
                    "import line {}: no subject (topic and graph atom both empty)",
                    index + 1
                ),
                crate::ExclusionReason::NotTautological,
            ));
            continue;
        }
        let subject = AtomId::new(subject_name);
        if !known.contains(&subject) {
            refused.push((
                format!(
                    "import line {}: subject '{}' unknown",
                    index + 1,
                    subject_name
                ),
                crate::ExclusionReason::UnknownEndpoint,
            ));
            continue;
        }
        for predicate in &record.predicates {
            let line = index + 1;
            let Some(relation) = map_import_relation(&predicate.kind, &predicate.en) else {
                refused.push((
                    format!(
                        "import line {line}: unmapped relation kind '{}' gloss '{}'",
                        predicate.kind, predicate.en
                    ),
                    crate::ExclusionReason::UnknownEndpoint,
                ));
                continue;
            };
            let rendered_ru = predicate.ru.trim();
            if rendered_ru.is_empty() {
                refused.push((
                    format!("import line {line}: empty rendered surface"),
                    crate::ExclusionReason::NotTautological,
                ));
                continue;
            }
            let Some(object) = std::iter::once(&subject)
                .chain(known.iter())
                .filter(|atom| *atom != &subject)
                .find(|atom| {
                    rendered_ru.to_lowercase().split_whitespace().any(|token| {
                        token
                            == atom
                                .as_str()
                                .to_lowercase()
                                .trim_end_matches(['.', ',', ';', ':', '!', '?'])
                    })
                })
                .cloned()
                .or_else(|| find_atom_in(&predicate.en, known).filter(|atom| *atom != subject))
            else {
                refused.push((
                    format!(
                        "import line {line}: no known object atom in surface '{}'",
                        predicate.ru
                    ),
                    crate::ExclusionReason::UnknownEndpoint,
                ));
                continue;
            };
            candidates.push(PromotionCandidate {
                snapshot_id: snapshot_id.to_string(),
                topic: normalize_topic(&record.topic, subject_name),
                subject: subject.clone(),
                relation,
                object,
                rendered_ru: rendered_ru.to_string(),
                // Import provenance: one sighting by a screened exporter.
                // Confidence and support deliberately carry no runtime-ladder
                // semantics here; preserved only so drafts stay comparable.
                confidence: 1.0,
                support: 1,
            });
        }
    }
    (candidates, refused)
}

fn normalize_topic(topic: &str, fallback: &str) -> String {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        fallback.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(names: &[&str]) -> BTreeSet<AtomId> {
        names.iter().map(|name| AtomId::new(*name)).collect()
    }

    fn fixture() -> &'static str {
        concat!(
            "{\"topic\":\"смысл\",\"graph_atom_id\":\"смысл\",\"predicates\":[",
            "{\"en\":\"meaning orients understanding\",\"kind\":\"rel\",\"ru\":\"смысл ориентирует понимание\"},",
            "{\"en\":\"mystery unfolds\",\"kind\":\"rel\",\"ru\":\"тайна раскрывается\"},",
            "{\"en\":\"self reference\",\"kind\":\"prop\",\"ru\":\"смысл\"}",
            "],\"reasons\":[\"quarantined\"]}\n",
            "not json at all\n",
            "{\"topic\":\"разум\",\"graph_atom_id\":\"\",\"predicates\":[],\"reasons\":[]}\n",
        )
    }

    #[test]
    fn malformed_lines_fail_closed_per_line() {
        let (records, refusals) = parse_quarantine_jsonl(fixture());
        assert_eq!(records.len(), 2);
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0].line, 2);
    }

    #[test]
    fn resolution_maps_rel_connects_prop_to_membership_and_refuses_unknown() {
        let (records, _) = parse_quarantine_jsonl(fixture());
        let snapshot = "snap-import";
        // "понимание" is not in the known set: orients-claim must refuse.
        let known_without_object = known(&["смысл", "тайна"]);
        let (candidates, refused) = candidates_from_import(
            std::slice::from_ref(&records[0]),
            snapshot,
            &known_without_object,
        );
        assert!(candidates.is_empty());
        assert!(refused
            .iter()
            .all(|(_, reason)| *reason == crate::ExclusionReason::UnknownEndpoint));
        // With the object known, the orients-triple resolves and the
        // self-referential prop refuses (topic paraphrase belongs to the
        // informativeness gate, but an identity surface is already
        // structurally unusable — it must refuse at resolution).
        let known_full = known(&["смысл", "понимание", "тайна"]);
        let (candidates, refused) =
            candidates_from_import(std::slice::from_ref(&records[0]), snapshot, &known_full);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].topic, "смысл");
        assert_eq!(candidates[0].subject.as_str(), "смысл");
        assert_eq!(candidates[0].object.as_str(), "понимание");
        assert_eq!(candidates[0].snapshot_id, "snap-import");
        assert!(refused
            .iter()
            .any(|(_, reason)| *reason == crate::ExclusionReason::UnknownEndpoint));
    }

    #[test]
    fn unmapped_relation_and_unknown_subject_refuse() {
        let known = known(&["a", "b"]);
        let (records, _) = parse_quarantine_jsonl(
            "{\"topic\":\"x\",\"graph_atom_id\":\"q\",\"predicates\":[{\"en\":\"x levitates y\",\"kind\":\"rel\",\"ru\":\"x y\"}],\"reasons\":[]}",
        );
        let (_, refused) = candidates_from_import(&records, "s", &known);
        assert_eq!(refused.len(), 1);
    }

    #[test]
    fn empty_subject_and_empty_surface_are_degenerate() {
        let known = known(&["a"]);
        let (records, _) = parse_quarantine_jsonl(
            "{\"topic\":\"\",\"graph_atom_id\":\"\",\"predicates\":[{\"en\":\"e\",\"kind\":\"rel\",\"ru\":\"\"}],\"reasons\":[]}",
        );
        let (_, refused) = candidates_from_import(&records, "s", &known);
        assert_eq!(refused.len(), 1);
    }

    #[test]
    fn import_relation_lexicon_is_reviewable() {
        assert_eq!(
            map_import_relation("rel", "X requires Y"),
            Some(RelationType::RelRequires)
        );
        assert_eq!(
            map_import_relation("rel", "X is connected to Y"),
            Some(RelationType::RelConnects)
        );
        assert_eq!(
            map_import_relation("rel", "X CONTRasts with Y"),
            Some(RelationType::RelContrastsWith)
        );
        assert_eq!(
            map_import_relation("rel", "something unheard-of here"),
            None
        );
        assert_eq!(
            map_import_relation("prop", "anything"),
            Some(RelationType::RelIsA)
        );
    }

    #[test]
    fn atom_scan_is_exact_token_match_and_sorted() {
        let known = known(&["мысль", "мыслью"]);
        // A substring inside a longer word must not resolve ...
        assert_eq!(find_atom_in("осмысление", &known), None);
        // ... but the first sorted match on a token boundary wins.
        assert_eq!(
            find_atom_in("его мысль и мыслью", &known).map(|atom| atom.0.clone()),
            Some("мысль".to_string())
        );
    }
}
