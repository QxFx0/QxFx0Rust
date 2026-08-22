//! Seed knowledge graph — an embedded, digest-pinned data asset.
//!
//! The graph data (207 atoms, 346 relations) lives in
//! `assets/seed_graph.json` and is pinned by SHA-256 at load, matching the
//! digest-pinning policy of the morphology bundle and knowledge packs.
//! Regenerate the asset with:
//!
//! ```bash
//! cargo run -p qxfx0-semantic --example gen_seed_asset \
//!   > qxfx0-semantic/assets/seed_graph.json
//! ```
//!
//! then copy the printed digest into `SEED_GRAPH_SHA256` below. Topic
//! recognition vocabulary stays in source: it is the parser's API surface,
//! not graph payload.

use qxfx0_types::atom::{Atom, AtomGraph, PathProof, Relation};
use qxfx0_types::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SEED_GRAPH_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/seed_graph.json"
));

/// SHA-256 of `assets/seed_graph.json`, printed by `gen_seed_asset`.
const SEED_GRAPH_SHA256: &str = "f35ce67674c8182a6f6176d687551063f9247aaa03140c1b34a348af11946b3d";

const SEED_GRAPH_SCHEMA: &str = "qxfx0:seed-graph:v1";

/// 141 covered philosophical topics.
pub const COVERED_TOPICS: &[&str] = &[
    "свобода",
    "произвол",
    "ответственность",
    "истина",
    "мнение",
    "память",
    "воспоминание",
    "помнить",
    "сознание",
    "самосознание",
    "вера",
    "красота",
    "долг",
    "доверие",
    "страх",
    "надежда",
    "справедливость",
    "время",
    "разум",
    "бытие",
    "история",
    "язык",
    "воля",
    "смерть",
    "одиночество",
    "любовь",
    "труд",
    "покой",
    "власть",
    "правда",
    "молчание",
    "знание",
    "понимание",
    "сомнение",
    "интуиция",
    "добро",
    "зло",
    "совесть",
    "поступок",
    "сущность",
    "существование",
    "необходимость",
    "мышление",
    "воображение",
    "желание",
    "значение",
    "интерпретация",
    "коммуникация",
    "культура",
    "творчество",
    "мудрость",
    "прогресс",
    "искусство",
    "гармония",
    "человек",
    "жизнь",
    "смысл",
    "счастье",
    "страдание",
    "природа",
    "игра",
    "работа",
    "деньги",
    "здоровье",
    "дружба",
    "семья",
    "образование",
    "музыка",
    "наука",
    "технология",
    "дом",
    "путешествие",
    "личность",
    "мотивация",
    "стресс",
    "развитие",
    "государство",
    "демократия",
    "равенство",
    "права",
    "конфликт",
    "ресурс",
    "ценность",
    "обмен",
    "отношения",
    "уважение",
    "ревность",
    "привязанность",
    "успех",
    "талант",
    "дисциплина",
    "призвание",
    "информация",
    "внимание",
    "скорость",
    "радость",
    "грусть",
    "гнев",
    "спокойствие",
    "мораль",
    "этика",
    "система",
    "метод",
    "процесс",
    "результат",
    "диалог",
    "спор",
    "аксиома",
    "беспристрастность",
    "благодарность",
    "верность",
    "граница",
    "длительность",
    "долженствование",
    "дух",
    "идентичность",
    "когерентность",
    "алгоритм",
    "объективность",
    "поэзия",
    "право",
    "присутствие",
    "психика",
    "решимость",
    "самоопределение",
    "самооценка",
    "свидетельство",
    "слушание",
    "собственность",
    "становление",
    "воспроизводимость",
    "выбор",
    "договор",
    "доказательство",
    "закон",
    "инстинкт",
    "нация",
    "революция",
    "ремонт",
    "рынок",
    "цифра",
];

/// The serialized seed asset. Indexes are rebuilt after loading.
#[derive(Deserialize)]
struct SeedAsset {
    schema: String,
    atoms: BTreeMap<AtomId, Atom>,
    edges: Vec<Relation>,
}

/// Build the seed graph from the embedded, digest-pinned asset.
///
/// The parsed graph is cached for the process lifetime; callers receive a
/// clone and own their mutations.
pub fn seed_graph() -> AtomGraph {
    static SEED: OnceLock<AtomGraph> = OnceLock::new();
    SEED.get_or_init(load_seed_graph).clone()
}

fn load_seed_graph() -> AtomGraph {
    let digest = format!("{:x}", Sha256::digest(SEED_GRAPH_JSON.as_bytes()));
    assert_eq!(
        digest, SEED_GRAPH_SHA256,
        "embedded seed graph asset digest mismatch: regenerate assets/seed_graph.json          with gen_seed_asset and update SEED_GRAPH_SHA256"
    );
    let asset: SeedAsset =
        serde_json::from_str(SEED_GRAPH_JSON).expect("embedded seed graph asset is valid JSON");
    assert_eq!(
        asset.schema, SEED_GRAPH_SCHEMA,
        "embedded seed graph asset schema drifted"
    );
    let mut graph = AtomGraph {
        atoms: asset.atoms,
        edges: asset.edges,
        edges_from: BTreeMap::new(),
        edges_to: BTreeMap::new(),
    };
    graph.rebuild_indices();
    graph
}

/// Verbalize a relation into Russian surface text.
/// Uses `ru_original` (hand-written grammatically correct sentence) when available.
/// Falls back to morphological assembly from parts for runtime-generated relations.
pub fn verbalize_relation(rel: &Relation) -> String {
    if !rel.ru_original.trim().is_empty() {
        return rel.ru_original.clone();
    }
    if let Some(verb) = &rel.verb_override {
        format!("{} {} {}", rel.from.as_str(), verb, rel.object_text)
    } else {
        format!(
            "{} {} {}",
            rel.from.as_str(),
            rel.rel_type.verb_ru(),
            rel.object_text
        )
    }
}

/// Verbalize a path proof into text.
pub fn verbalize_path(proof: &PathProof) -> String {
    proof
        .edges
        .iter()
        .map(verbalize_relation)
        .collect::<Vec<_>>()
        .join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_graph_counts_match_the_documented_census() {
        let graph = seed_graph();
        assert_eq!(graph.atoms.len(), 207, "seed atom census");
        assert_eq!(graph.edges.len(), 346, "seed edge census");
        assert_eq!(COVERED_TOPICS.len(), 141, "topic census");
    }

    #[test]
    fn seed_graph_passes_full_validation() {
        let graph = seed_graph();
        if let Err(violations) = graph.validate() {
            panic!("seed graph must satisfy its own integrity checks: {violations:?}");
        }
    }

    #[test]
    fn every_topic_has_a_topic_atom() {
        let graph = seed_graph();
        for topic in COVERED_TOPICS {
            let atom = graph
                .atoms
                .get(&AtomId::new(*topic))
                .unwrap_or_else(|| panic!("topic '{topic}' has no atom"));
            assert!(matches!(
                atom.category,
                qxfx0_types::atom::AtomCategory::CatTopic
            ));
        }
    }
}
