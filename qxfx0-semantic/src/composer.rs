use qxfx0_types::atom::{AtomGraph, AtomId, GeneratedSurface, PathProof, Relation};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::*;

/// Proposition parser — parses user input into typed proposition.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedProposition {
    pub subject: String,
    pub object: Option<String>,
    pub mode: PropositionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropositionMode {
    Define,
    Assert,
    Challenge,
    Connect,
    Reflect,
}

pub struct PropositionParser;

impl PropositionParser {
    /// Parse user input into a typed proposition.
    pub fn parse(input: &str) -> ParsedProposition {
        let lower = input.to_lowercase();
        let trimmed = lower.trim();

        // Define: "что такое X?"
        if let Some(topic) = Self::extract_after(trimmed, &["что такое", "что есть", "определи"])
        {
            return ParsedProposition {
                subject: Self::clean_topic(&topic),
                object: None,
                mode: PropositionMode::Define,
            };
        }

        // Distinction: "в чем разница между X и Y?"
        if trimmed.contains("разница между") || trimmed.contains("различие между")
        {
            let after = if let Some(idx) = trimmed.find("между ") {
                &trimmed[idx + 6..]
            } else {
                trimmed
            };
            let parts: Vec<&str> = after.splitn(2, " и ").collect();
            if parts.len() == 2 {
                return ParsedProposition {
                    subject: Self::clean_topic(parts[0]),
                    object: Some(Self::clean_topic(parts[1])),
                    mode: PropositionMode::Connect,
                };
            }
        }

        // Challenge: reduction patterns
        let challenge_patterns = [
            "это просто",
            "не более чем",
            "сводится к",
            "всего лишь",
            "это лишь",
        ];
        if challenge_patterns.iter().any(|p| trimmed.contains(p)) {
            // Extract subject — first word before the pattern
            for pattern in &challenge_patterns {
                if let Some(idx) = trimmed.find(pattern) {
                    let before = trimmed[..idx].trim();
                    let subject = before.split_whitespace().next().unwrap_or("неизвестный");
                    return ParsedProposition {
                        subject: Self::clean_topic(subject),
                        object: None,
                        mode: PropositionMode::Challenge,
                    };
                }
            }
        }

        // Challenge: explicit markers
        let challenge_markers = [
            "разве",
            "не согласен",
            "не согласна",
            "противореч",
            "неверно",
            "ошибаешься",
            "не прав",
            "спорю",
            "возраж",
            "сомневаюсь",
            "оспариваю",
        ];
        if challenge_markers.iter().any(|m| trimmed.contains(m)) {
            return ParsedProposition {
                subject: Self::extract_topic_or_unknown(trimmed),
                object: None,
                mode: PropositionMode::Challenge,
            };
        }

        // Reflect: "что ты думаешь о X?", "какова твоя мысль о X?"
        let reflect_patterns = [
            "что ты думаешь о",
            "что думаешь о",
            "какова твоя мысль о",
            "твое мнение о",
            "твоё мнение о",
            "как ты считаешь",
            "как ты видишь",
            "поразмышляй о",
            "подумай о",
        ];
        for pattern in &reflect_patterns {
            if let Some(idx) = trimmed.find(pattern) {
                let after = trimmed[idx + pattern.len()..].trim();
                let topic = after.trim_end_matches('?').trim();
                if !topic.is_empty() {
                    return ParsedProposition {
                        subject: Self::clean_topic(topic),
                        object: None,
                        mode: PropositionMode::Reflect,
                    };
                }
            }
        }

        // Connect: "как X связан с Y?"
        if trimmed.contains("связан с") || trimmed.contains("связь между") {
            if let Some(idx) = trimmed.find("как ") {
                let after = &trimmed[idx + 4..];
                if let Some(conn_idx) = after.find(" связан с ") {
                    let subject = after[..conn_idx].trim();
                    let object = after[conn_idx + 10..].trim_end_matches('?').trim();
                    return ParsedProposition {
                        subject: Self::clean_topic(subject),
                        object: Some(Self::clean_topic(object)),
                        mode: PropositionMode::Connect,
                    };
                }
            }
        }

        // Fallback: try to extract a topic
        ParsedProposition {
            subject: Self::extract_topic_or_unknown(trimmed),
            object: None,
            mode: PropositionMode::Define,
        }
    }

    fn extract_after(text: &str, prefixes: &[&str]) -> Option<String> {
        for prefix in prefixes {
            if let Some(idx) = text.find(prefix) {
                let after = text[idx + prefix.len()..].trim();
                let topic = after.trim_end_matches('?').trim();
                if !topic.is_empty() {
                    return Some(topic.to_string());
                }
            }
        }
        None
    }

    fn clean_topic(s: &str) -> String {
        s.trim()
            .trim_end_matches('?')
            .trim_end_matches(',')
            .trim_end_matches('!')
            .trim()
            .to_string()
    }

    fn extract_topic_or_unknown(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().filter(|w| w.len() >= 3).collect();
        if words.is_empty() {
            "неизвестный".to_string()
        } else {
            Self::clean_topic(words[0])
        }
    }
}

/// Graph engagement — find the system's relationship to a proposition.
#[derive(Debug, Clone, Default)]
pub struct EngagementResult {
    pub supporting: Vec<Relation>,
    pub contradicting: Vec<Relation>,
    pub qualifying: Vec<Relation>,
    pub path: Vec<Relation>,
    pub context: Vec<Relation>,
}

pub struct GraphEngagement;

impl GraphEngagement {
    /// Engage with a proposition — find supporting, contradicting, qualifying edges.
    pub fn engage(graph: &AtomGraph, prop: &ParsedProposition) -> EngagementResult {
        let topic = AtomId::new(prop.subject.clone());
        let rels = graph.relations_from(&topic);

        let supporting_types = [
            RelationType::RelPresupposes,
            RelationType::RelRequires,
            RelationType::RelIncludes,
            RelationType::RelMeans,
            RelationType::RelDetermines,
            RelationType::RelClaims,
        ];
        let contradicting_types = [
            RelationType::RelContrastsWith,
            RelationType::RelDiffersFrom,
            RelationType::RelNotReducibleTo,
            RelationType::RelIsNot,
            RelationType::RelNegates,
            RelationType::RelDestroys,
        ];
        let qualifying_types = [
            RelationType::RelLimitedBy,
            RelationType::RelStructures,
            RelationType::RelPrescribes,
            RelationType::RelNecessaryFor,
        ];

        let mut result = EngagementResult::default();

        for rel in rels.iter() {
            if supporting_types.contains(&rel.rel_type) {
                result.supporting.push((*rel).clone());
            }
            if contradicting_types.contains(&rel.rel_type) {
                result.contradicting.push((*rel).clone());
            }
            if qualifying_types.contains(&rel.rel_type) {
                result.qualifying.push((*rel).clone());
            }
        }

        // BFS path between subject and object (for Connect mode)
        if let Some(obj) = &prop.object {
            let obj_id = AtomId::new(obj.clone());
            result.path = Self::bfs_path(graph, &topic, &obj_id);
        }

        result
    }

    /// BFS shortest path between two atoms (depth ≤ 3).
    pub fn bfs_path(graph: &AtomGraph, from: &AtomId, to: &AtomId) -> Vec<Relation> {
        // Direct edge
        for rel in graph.relations_from(from) {
            if rel.to == *to {
                return vec![rel.clone()];
            }
        }

        // Two-hop
        for e1 in graph.relations_from(from) {
            for e2 in graph.relations_from(&e1.to) {
                if e2.to == *to && e2.to != *from {
                    return vec![e1.clone(), e2.clone()];
                }
            }
        }

        Vec::new()
    }
}

/// Contextual composer — composes responses based on proposition mode.
pub struct ContextualComposer;

impl ContextualComposer {
    /// Compose a contextual response based on proposition mode + Self Layer state.
    /// CF-5 fix: branches generation by conatus/salience/essence signals.
    pub fn compose(
        graph: &AtomGraph,
        fp: &FieldProfile,
        prop: &ParsedProposition,
        engagement: &EngagementResult,
    ) -> GeneratedSurface {
        match prop.mode {
            PropositionMode::Define => {
                // CF-5: Conatus determines how many paths to explore
                let n = if fp.conatus_energy > 10.0 {
                    5
                } else if fp.conatus_energy > 5.0 {
                    3
                } else {
                    1
                };
                // CF-5: Salience determines holistic vs formal phrasing
                if fp.is_holistic() {
                    Self::compose_define_holistic(graph, fp, n, prop)
                } else {
                    Self::compose_define(graph, fp, n, prop)
                }
            }
            PropositionMode::Challenge => Self::compose_challenge(graph, fp, prop, engagement),
            PropositionMode::Connect => Self::compose_connect(prop, engagement),
            PropositionMode::Reflect => {
                // CF-5: Essence anchoring determines depth of reflection
                if fp.anchors_to_trajectory() {
                    Self::compose_reflect_deep(graph, fp, prop, engagement)
                } else {
                    Self::compose_reflect(graph, fp, prop, engagement)
                }
            }
            PropositionMode::Assert => Self::compose_assert(prop, engagement),
        }
    }

    fn compose_define(
        graph: &AtomGraph,
        fp: &FieldProfile,
        n: usize,
        prop: &ParsedProposition,
    ) -> GeneratedSurface {
        let topic = AtomId::new(prop.subject.clone());
        crate::pathfinder::PathFinder::compose_definition(graph, fp, n, &topic)
    }

    /// CF-5: Holistic define — associative, intuitive phrasing.
    /// Uses resonance-favored relation types and broader exploration.
    fn compose_define_holistic(
        graph: &AtomGraph,
        fp: &FieldProfile,
        n: usize,
        prop: &ParsedProposition,
    ) -> GeneratedSurface {
        let topic = AtomId::new(prop.subject.clone());
        let mut surface = crate::pathfinder::PathFinder::compose_definition(graph, fp, n, &topic);

        // Holistic mode: prepend intuitive framing
        if !surface.text.is_empty() {
            surface.text = format!("Когда я чувствую {}: {}", prop.subject, surface.text);
        }

        surface
    }

    /// CF-5: Deep reflection — anchored to essence trajectory.
    /// Includes commitment references and synthesis from prior turns.
    fn compose_reflect_deep(
        graph: &AtomGraph,
        fp: &FieldProfile,
        prop: &ParsedProposition,
        engagement: &EngagementResult,
    ) -> GeneratedSurface {
        let topic = &prop.subject;

        let all_rels: Vec<Relation> = engagement
            .supporting
            .iter()
            .chain(engagement.qualifying.iter())
            .cloned()
            .collect();

        if all_rels.is_empty() {
            let topic_id = AtomId::new(topic.clone());
            return crate::pathfinder::PathFinder::compose_definition(graph, fp, 3, &topic_id);
        }

        // Deep reflection: include synthesis from relations
        let mut rel_texts = Vec::new();
        for rel in &all_rels {
            let text = crate::verbalize_relation(rel);
            rel_texts.push(text);
            // Include rationale if present (essence trajectory depth)
            if let Some(ref rationale) = rel.rationale {
                rel_texts.push(format!("потому что {}", rationale));
            }
            // Include synthesis if present
            if let Some(ref synthesis) = rel.synthesis {
                rel_texts.push(format!("именно поэтому {}", synthesis));
            }
        }

        let full_text = rel_texts.join(". ");

        GeneratedSurface {
            text: format!("Возвращаясь к {}: {}.", topic, full_text),
            paths: vec![PathProof {
                edges: all_rels.clone(),
                topic: topic.clone(),
            }],
            provenance: all_rels.iter().map(|r| r.source).collect(),
            depth_score: all_rels.len() as f64,
        }
    }

    fn compose_challenge(
        _graph: &AtomGraph,
        _fp: &FieldProfile,
        prop: &ParsedProposition,
        engagement: &EngagementResult,
    ) -> GeneratedSurface {
        let topic = &prop.subject;

        // Build defense from supporting edges
        let support_text = engagement
            .supporting
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");

        // Build counter from contradicting edges
        let counter_text = engagement
            .contradicting
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");

        let mut response = String::new();
        if !support_text.is_empty() {
            response.push_str("Я удерживаю позицию. ");
            response.push_str(&support_text);
        }
        if !counter_text.is_empty() {
            response.push_str(". Но ");
            response.push_str(&counter_text);
        }
        if response.is_empty() {
            response = format!("Возможно, ты прав. Я не нахожу достаточных оснований для своей позиции по вопросу о {}.", topic);
        }

        let all_rels: Vec<Relation> = engagement
            .supporting
            .iter()
            .chain(engagement.contradicting.iter())
            .cloned()
            .collect();

        GeneratedSurface {
            text: response,
            paths: vec![PathProof {
                edges: all_rels.clone(),
                topic: topic.clone(),
            }],
            provenance: all_rels.iter().map(|r| r.source).collect(),
            depth_score: all_rels.len() as f64,
        }
    }

    fn compose_connect(
        prop: &ParsedProposition,
        engagement: &EngagementResult,
    ) -> GeneratedSurface {
        let subject = &prop.subject;
        let object = prop.object.as_deref().unwrap_or("");

        if engagement.path.is_empty() {
            return GeneratedSurface {
                text: format!("Я не нахожу прямой связи между {} и {}.", subject, object),
                paths: Vec::new(),
                provenance: Vec::new(),
                depth_score: 0.0,
            };
        }

        let path_text = engagement
            .path
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");

        GeneratedSurface {
            text: format!("Связь прослеживается: {}.", path_text),
            paths: vec![PathProof {
                edges: engagement.path.clone(),
                topic: subject.clone(),
            }],
            provenance: engagement.path.iter().map(|r| r.source).collect(),
            depth_score: engagement.path.len() as f64,
        }
    }

    fn compose_reflect(
        graph: &AtomGraph,
        fp: &FieldProfile,
        prop: &ParsedProposition,
        engagement: &EngagementResult,
    ) -> GeneratedSurface {
        let topic = &prop.subject;

        let all_rels: Vec<Relation> = engagement
            .supporting
            .iter()
            .chain(engagement.qualifying.iter())
            .cloned()
            .collect();

        if all_rels.is_empty() {
            // Fallback to define
            let topic_id = AtomId::new(topic.clone());
            return crate::pathfinder::PathFinder::compose_definition(graph, fp, 3, &topic_id);
        }

        let rel_texts = all_rels
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");

        GeneratedSurface {
            text: format!("Когда я думаю о {}: {}.", topic, rel_texts),
            paths: vec![PathProof {
                edges: all_rels.clone(),
                topic: topic.clone(),
            }],
            provenance: all_rels.iter().map(|r| r.source).collect(),
            depth_score: all_rels.len() as f64,
        }
    }

    fn compose_assert(prop: &ParsedProposition, engagement: &EngagementResult) -> GeneratedSurface {
        let support_text = engagement
            .supporting
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");
        let contra_text = engagement
            .contradicting
            .iter()
            .map(crate::verbalize_relation)
            .collect::<Vec<_>>()
            .join(". ");

        let response = match (support_text.is_empty(), contra_text.is_empty()) {
            (false, false) => format!("Я вижу это иначе. {}. Но {}.", support_text, contra_text),
            (false, true) => format!("Я вижу это так: {}.", support_text),
            (true, false) => format!("Я не могу согласиться. {}.", contra_text),
            (true, true) => "У меня нет устоявшейся позиции по этому вопросу.".to_string(),
        };

        let all_rels: Vec<Relation> = engagement
            .supporting
            .iter()
            .chain(engagement.contradicting.iter())
            .cloned()
            .collect();

        GeneratedSurface {
            text: response,
            paths: vec![PathProof {
                edges: all_rels.clone(),
                topic: prop.subject.clone(),
            }],
            provenance: all_rels.iter().map(|r| r.source).collect(),
            depth_score: all_rels.len() as f64,
        }
    }
}
