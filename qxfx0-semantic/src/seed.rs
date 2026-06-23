use qxfx0_types::*;
use qxfx0_types::atom::PathProof;

/// 30 covered philosophical topics.
pub const COVERED_TOPICS: &[&str] = &[
    "свобода", "произвол", "ответственность", "истина", "мнение",
    "память", "воспоминание", "сознание", "самосознание", "вера",
    "красота", "долг", "доверие", "страх", "надежда",
    "справедливость", "время", "разум", "бытие", "история",
    "язык", "воля", "смерть", "одиночество", "любовь",
    "труд", "покой", "власть", "правда", "молчание",
];

/// Seed the AtomGraph with core philosophical relations.
pub fn seed_graph() -> AtomGraph {
    let mut graph = AtomGraph::new();

    for topic in COVERED_TOPICS {
        let id = AtomId::new(topic.to_string());
        graph.atoms.insert(id.clone(), Atom {
            id: id.clone(),
            display: topic.to_string(),
            category: AtomCategory::CatTopic,
        });
    }

    let rel = |from: &str, to: &str, rt: RelationType, case: ObjectCase, obj: &str, ru: &str, topic: &str,
               rationale: Option<&str>, counter: Option<&str>, synthesis: Option<&str>| {
        Relation {
            from: AtomId::new(from.to_string()),
            to: AtomId::new(to.to_string()),
            rel_type: rt,
            object_case: case,
            object_text: obj.to_string(),
            verb_override: None,
            ru_original: ru.to_string(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: topic.to_string(),
            rationale: rationale.map(String::from),
            counter: counter.map(String::from),
            synthesis: synthesis.map(String::from),
        }
    };

    graph.add_relation(rel(
        "свобода", "выбор", RelationType::RelPresupposes, ObjectCase::CaseAccusative,
        "возможность выбора", "свобода предполагает возможность выбора", "свобода",
        Some("без выбора действие не отличается от рефлекса"),
        Some("не любой выбор свободен: выбор под принуждением не делает действие свободным"),
        Some("свобода требует не только возможности, но и осознанности выбора"),
    ));

    graph.add_relation(rel(
        "свобода", "ответственность", RelationType::RelLimitedBy, ObjectCase::CaseInstrumental,
        "ответственностью", "свобода ограничена ответственностью", "свобода",
        Some("без ответственности свобода превращается в произвол"),
        Some("не любое ограничение убивает свободу — только произвольное"),
        Some("ответственность не враг свободы, а условие её осмысленности"),
    ));

    graph.add_relation(rel(
        "свобода", "принуждение", RelationType::RelDetermines, ObjectCase::CaseAccusative,
        "отсутствие принуждения", "свобода определяет отсутствие принуждения", "свобода",
        None, None, None,
    ));

    graph.add_relation(rel(
        "свобода", "сознание", RelationType::RelRequires, ObjectCase::CaseAccusative,
        "сознание", "свобода требует сознание", "свобода",
        None, None, None,
    ));

    graph.add_relation(rel(
        "свобода", "истина", RelationType::RelContrastsWith, ObjectCase::CaseAccusative,
        "истина", "свобода контрастирует с истина", "свобода",
        None, None, None,
    ));

    graph.add_relation(rel(
        "истина", "воспроизводимость", RelationType::RelVerifiedBy, ObjectCase::CaseInstrumental,
        "воспроизводимостью", "истина проверяется через воспроизводимость", "истина",
        Some("единичное совпадение может быть случайностью, а повторяемое — закономерностью"),
        Some("воспроизводимость не гарантирует истину — она лишь отсеивает то, что точно ею не является"),
        None,
    ));

    graph.add_relation(rel(
        "истина", "реальность", RelationType::RelClaims, ObjectCase::CaseAccusative,
        "соответствие реальности", "истина претендует на соответствие реальности", "истина",
        None, None, None,
    ));

    graph.add_relation(rel(
        "сознание", "самоотчёт", RelationType::RelIncludes, ObjectCase::CaseAccusative,
        "способность к самоотчёту", "сознание включает способность к самоотчёту", "сознание",
        Some("существо, не способное сказать «я чувствую это», может реагировать — но не осознавать"),
        None, None,
    ));

    graph.add_relation(rel(
        "сознание", "разум", RelationType::RelContrastsWith, ObjectCase::CaseAccusative,
        "разум", "сознание контрастирует с разум", "сознание",
        None, None, None,
    ));

    graph.add_relation(rel(
        "ответственность", "долг", RelationType::RelRequires, ObjectCase::CaseAccusative,
        "долг", "ответственность требует долг", "ответственность",
        None, None, None,
    ));

    graph.add_relation(rel(
        "ответственность", "последствия", RelationType::RelRequires, ObjectCase::CaseAccusative,
        "осознания последствий", "ответственность требует осознания последствий", "ответственность",
        Some("нельзя отвечать за то, чего не понимаешь: ответственность без осознания — имитация"),
        Some("осознание последствий не гарантирует правильного выбора — оно лишь исключает неведение как оправдание"),
        None,
    ));

    graph
}

/// Verbalize a relation into Russian surface text.
pub fn verbalize_relation(rel: &Relation) -> String {
    if let Some(verb) = &rel.verb_override {
        format!("{} {} {}", rel.from.as_str(), verb, rel.object_text)
    } else {
        format!("{} {} {}", rel.from.as_str(), rel.rel_type.verb_ru(), rel.object_text)
    }
}

/// Verbalize a path proof into text.
pub fn verbalize_path(proof: &PathProof) -> String {
    proof.edges.iter()
        .map(verbalize_relation)
        .collect::<Vec<_>>()
        .join(". ")
}
