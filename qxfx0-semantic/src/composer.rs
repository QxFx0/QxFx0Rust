use qxfx0_types::atom::{AtomGraph, AtomId, GeneratedSurface, PathProof, Relation};
use qxfx0_types::field::FieldProfile;

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
    Greeting,
    Purpose,
    WorldCause,
}

pub struct PropositionParser;

impl PropositionParser {
    /// Parse user input into a typed proposition.
    pub fn parse(input: &str) -> ParsedProposition {
        let lower = input.to_lowercase();
        let trimmed = lower.trim();

        // Contact must be recognized before the generic fallback. Keep the
        // accepted set deliberately small so that a word such as "приветствие"
        // is not accidentally routed as a greeting.
        let greeting = trimmed
            .trim_matches(|c: char| !c.is_alphanumeric())
            .split_whitespace()
            .collect::<Vec<_>>();
        if !greeting.is_empty()
            && greeting.len() <= 3
            && greeting.iter().any(|word| {
                matches!(
                    *word,
                    "привет" | "здравствуй" | "здравствуйте" | "добрый" | "доброе"
                )
            })
        {
            return ParsedProposition {
                subject: greeting.join(" "),
                object: None,
                mode: PropositionMode::Greeting,
            };
        }

        // Purpose/function questions: "в чём функция стола?", "зачем нужен X?"
        // Preserve an oblique form ("стола") because it is already the
        // grammatically correct form after the word "функция".
        for marker in ["функция ", "назначение ", "роль "] {
            if let Some(idx) = trimmed.find(marker) {
                let topic = Self::clean_topic(&trimmed[idx + marker.len()..]);
                if !topic.is_empty() {
                    return ParsedProposition {
                        subject: topic,
                        object: None,
                        mode: PropositionMode::Purpose,
                    };
                }
            }
        }
        for marker in [
            "зачем нужен ",
            "зачем нужна ",
            "зачем нужно ",
            "для чего нужен ",
        ] {
            if let Some(idx) = trimmed.find(marker) {
                let topic = Self::clean_topic(&trimmed[idx + marker.len()..]);
                if !topic.is_empty() {
                    return ParsedProposition {
                        subject: topic,
                        object: None,
                        mode: PropositionMode::Purpose,
                    };
                }
            }
        }

        // Causal questions about the external world deserve an explicit
        // frame instead of being reduced to a definition of "почему".
        if let Some(rest) = trimmed.strip_prefix("почему ") {
            let topic = Self::clean_topic(rest);
            return ParsedProposition {
                subject: if topic.is_empty() {
                    "явление".to_string()
                } else {
                    topic
                },
                object: None,
                mode: PropositionMode::WorldCause,
            };
        }

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
            let mezi_prefix = "между ";
            let after = if let Some(idx) = trimmed.find(mezi_prefix) {
                &trimmed[idx + mezi_prefix.len()..]
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
        // Note: "об" variants MUST come before "о" variants to match the
        // longer prefix first, otherwise "об" leaves a stray "б" in the topic.
        let reflect_patterns = [
            "что ты думаешь об",
            "что ты думаешь о",
            "что думаешь об",
            "что думаешь о",
            "какова твоя мысль об",
            "какова твоя мысль о",
            "твое мнение об",
            "твое мнение о",
            "твоё мнение об",
            "твоё мнение о",
            "поразмышляй об",
            "поразмышляй о",
            "подумай об",
            "подумай о",
            "как ты считаешь",
            "как ты видишь",
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

        // Connect: "как X связан/связана с Y?", "связь между X и Y"
        if trimmed.contains("связан")
            || trimmed.contains("связана")
            || trimmed.contains("связь между")
        {
            let kak_prefix = "как ";
            if let Some(idx) = trimmed.find(kak_prefix) {
                let after = &trimmed[idx + kak_prefix.len()..];
                // Match " связан с " or " связана с "
                let conn_pattern_masc = " связан с ";
                let conn_pattern_fem = " связана с ";
                if let Some(conn_idx) = after
                    .find(conn_pattern_masc)
                    .or_else(|| after.find(conn_pattern_fem))
                {
                    let subject = after[..conn_idx].trim();
                    let rel_len = if after[conn_idx..].starts_with(conn_pattern_fem) {
                        conn_pattern_fem.len()
                    } else {
                        conn_pattern_masc.len()
                    };
                    let object = after[conn_idx + rel_len..].trim_end_matches('?').trim();
                    return ParsedProposition {
                        subject: Self::clean_topic(subject),
                        object: Some(Self::clean_topic(object)),
                        mode: PropositionMode::Connect,
                    };
                }
            }
        }

        // Fallback: statements are assertions; unknown questions remain
        // definition requests. In both cases prefer the final content word,
        // which is substantially closer to a Russian noun phrase than the
        // old "first word with at least three bytes" heuristic.
        ParsedProposition {
            subject: Self::extract_topic_or_unknown(trimmed),
            object: None,
            mode: if trimmed.ends_with('?') {
                PropositionMode::Define
            } else {
                PropositionMode::Assert
            },
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

    /// Try to normalize a topic to its nominative form by checking if any
    /// graph atom's inflected form matches the input. Returns the original
    /// string if no match is found.
    pub fn normalize_topic(topic: &str, graph: &AtomGraph) -> String {
        use qxfx0_morphology::{Case, MorphologyData};
        let morph = MorphologyData::with_seed();
        let lower = topic.to_lowercase();

        for (id, atom) in &graph.atoms {
            let display = atom.display.to_lowercase();
            if display == lower {
                return id.as_str().to_string();
            }
            for case in [
                Case::Genitive,
                Case::Dative,
                Case::Accusative,
                Case::Instrumental,
                Case::Prepositional,
            ] {
                let inflected = morph.to_case(case, &display).to_lowercase();
                if inflected == lower {
                    return id.as_str().to_string();
                }
            }
        }
        topic.to_string()
    }

    /// Find a graph-backed topic anywhere in the input. This is used after
    /// parsing generic assertions/questions and keeps explicit parser modes
    /// (purpose, cause, connection) in control of their own subject grammar.
    pub fn known_topic_in_input(input: &str, graph: &AtomGraph) -> Option<String> {
        use qxfx0_morphology::{Case, MorphologyData};
        let morph = MorphologyData::with_seed();
        let words: Vec<String> = input
            .to_lowercase()
            .split_whitespace()
            .map(Self::clean_topic)
            .filter(|word| !word.is_empty())
            .collect();

        for word in words {
            for (id, atom) in &graph.atoms {
                let display = atom.display.to_lowercase();
                if word == display {
                    return Some(id.as_str().to_string());
                }
                for case in [
                    Case::Genitive,
                    Case::Dative,
                    Case::Accusative,
                    Case::Instrumental,
                    Case::Prepositional,
                ] {
                    if word == morph.to_case(case, &display).to_lowercase() {
                        return Some(id.as_str().to_string());
                    }
                }
            }
        }
        None
    }

    fn extract_topic_or_unknown(text: &str) -> String {
        Self::content_words(text)
            .into_iter()
            .next_back()
            .unwrap_or_else(|| "неизвестный".to_string())
    }

    fn content_words(text: &str) -> Vec<String> {
        const STOP_WORDS: &[&str] = &[
            "что",
            "как",
            "какова",
            "каков",
            "кто",
            "где",
            "когда",
            "почему",
            "зачем",
            "сколько",
            "какой",
            "какая",
            "какое",
            "какие",
            "это",
            "этот",
            "эта",
            "эти",
            "оно",
            "она",
            "они",
            "его",
            "её",
            "мне",
            "меня",
            "тебе",
            "тебя",
            "наш",
            "ваш",
            "для",
            "про",
            "при",
            "над",
            "под",
            "без",
            "между",
            "через",
            "около",
            "после",
            "перед",
            "или",
            "либо",
            "тоже",
            "очень",
            "просто",
            "лишь",
            "только",
            "нужен",
            "нужна",
            "нужно",
            "есть",
            "быть",
            "был",
            "была",
            "будет",
            "думаю",
            "думаешь",
            "считаю",
            "считаешь",
            "скажи",
            "расскажи",
            "объясни",
            "покажи",
            "купил",
            "купила",
            "хочу",
            "хочешь",
            "можно",
            "нельзя",
            "помоги",
            "помочь",
            "чём",
            "чем",
        ];

        text.split_whitespace()
            .map(Self::clean_topic)
            .filter(|word| {
                let chars = word.chars().count();
                chars >= 2 && !STOP_WORDS.contains(&word.as_str())
            })
            .collect()
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

        let mut result = EngagementResult::default();

        for rel in rels.iter() {
            if rel.rel_type.is_supporting() {
                result.supporting.push((*rel).clone());
            }
            if rel.rel_type.is_counter() {
                result.contradicting.push((*rel).clone());
            }
            if rel.rel_type.is_qualifying() {
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
        use std::collections::VecDeque;

        // Direct edge (depth 1)
        for rel in graph.relations_from(from) {
            if rel.to == *to {
                return vec![rel.clone()];
            }
        }

        // BFS bounded to depth 3 using VecDeque
        let mut queue: VecDeque<(AtomId, Vec<Relation>)> = VecDeque::new();
        queue.push_back((from.clone(), Vec::new()));

        while let Some((current, path)) = queue.pop_front() {
            let depth = path.len();
            if depth >= 3 {
                continue;
            }
            for rel in graph.relations_from(&current) {
                let mut new_path = path.clone();
                new_path.push(rel.clone());
                if rel.to == *to {
                    return new_path;
                }
                queue.push_back((rel.to.clone(), new_path));
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
                let n = if fp.conatus_energy > 1.2 {
                    5
                } else if fp.conatus_energy > 0.6 {
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
            PropositionMode::Greeting => {
                Self::plain_surface(format!("{}. Рад продолжить разговор.", prop.subject))
            }
            PropositionMode::Purpose => Self::plain_surface(format!(
                "Назначение {} определяется его устойчивой ролью в действии.",
                prop.subject
            )),
            PropositionMode::WorldCause => Self::plain_surface(format!(
                "Причину явления «{}» нужно проверять по внешним фактам.",
                prop.subject
            )),
        }
    }

    fn plain_surface(text: String) -> GeneratedSurface {
        GeneratedSurface {
            text,
            paths: Vec::new(),
            provenance: Vec::new(),
            depth_score: 0.0,
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
        crate::pathfinder::PathFinder::compose_definition(graph, fp, n, &topic)
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
            text: full_text,
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
            response.push_str(&support_text);
        }
        if !counter_text.is_empty() {
            if !response.is_empty() {
                response.push_str(". Но ");
            }
            response.push_str(&counter_text);
        }
        if response.is_empty() {
            response = format!(
                "По вопросу о {} у меня нет устоявшейся позиции в графе.",
                topic
            );
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
            text: rel_texts,
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
            (false, false) => format!("{}. Но {}.", support_text, contra_text),
            (false, true) => support_text,
            (true, false) => contra_text,
            (true, true) => "В графе нет данных по этому вопросу.".to_string(),
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

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_reflect_ob() {
        let prop = PropositionParser::parse("что ты думаешь об ответственности?");
        assert_eq!(prop.mode, PropositionMode::Reflect);
        assert_eq!(prop.subject, "ответственности");
    }

    #[test]
    fn test_parse_reflect_o() {
        let prop = PropositionParser::parse("что ты думаешь о свободе?");
        assert_eq!(prop.mode, PropositionMode::Reflect);
        assert_eq!(prop.subject, "свободе");
    }

    #[test]
    fn test_parse_reflect_ob_no_stray_b() {
        let prop = PropositionParser::parse("что ты думаешь об истине?");
        assert_eq!(prop.mode, PropositionMode::Reflect);
        assert!(
            !prop.subject.starts_with("б "),
            "subject should not start with 'б ': {}",
            prop.subject
        );
    }

    #[test]
    fn test_parse_define() {
        let prop = PropositionParser::parse("что такое свобода?");
        assert_eq!(prop.mode, PropositionMode::Define);
        assert_eq!(prop.subject, "свобода");
    }

    #[test]
    fn test_parse_connect_feminine() {
        let prop = PropositionParser::parse("как свобода связана с истиной?");
        assert_eq!(prop.mode, PropositionMode::Connect);
        assert_eq!(prop.subject, "свобода");
        assert_eq!(prop.object.as_deref(), Some("истиной"));
    }

    #[test]
    fn test_parse_greeting() {
        let prop = PropositionParser::parse("Привет!");
        assert_eq!(prop.mode, PropositionMode::Greeting);
        assert_eq!(prop.subject, "привет");
    }

    #[test]
    fn test_parse_purpose_question() {
        let prop = PropositionParser::parse("в чём функция стола?");
        assert_eq!(prop.mode, PropositionMode::Purpose);
        assert_eq!(prop.subject, "стола");
    }

    #[test]
    fn test_parse_world_cause_question() {
        let prop = PropositionParser::parse("почему небо голубое?");
        assert_eq!(prop.mode, PropositionMode::WorldCause);
        assert_eq!(prop.subject, "небо голубое");
    }

    #[test]
    fn test_parse_assertion_prefers_final_content_word() {
        let prop = PropositionParser::parse("я купил дом");
        assert_eq!(prop.mode, PropositionMode::Assert);
        assert_eq!(prop.subject, "дом");
    }

    #[test]
    fn test_known_topic_can_be_found_anywhere_in_input() {
        let graph = crate::seed_graph();
        let topic =
            PropositionParser::known_topic_in_input("мне особенно важна свобода сегодня", &graph);
        assert_eq!(topic.as_deref(), Some("свобода"));
    }
}
