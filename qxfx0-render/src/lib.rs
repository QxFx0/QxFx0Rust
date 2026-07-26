pub mod content_plan;

use qxfx0_semantic::{
    cached_semantic_network, network::activate as network_activate, verbalize_path,
    verbalize_relation, ContentSelector, DiscourseComposer, DiscourseStyle, PathFinder,
    PropositionMode, Verbosity,
};
use qxfx0_types::atom::{AtomGraph, AtomId, Relation};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::frame::SemanticFrame;
use qxfx0_types::system_state::SemanticState;

/// Render engine — dispatches semantic frames to surface text generation.
/// Uses DiscourseComposer for rich multi-sentence output, with PathFinder
/// as a fallback for empty composition.
pub struct RenderEngine;

impl RenderEngine {
    /// Render a semantic frame into surface text.
    ///
    /// Uses the cached semantic network in `semantic` when available,
    /// rebuilding it only when the runtime graph has grown.
    pub fn render_frame(
        frame: &SemanticFrame,
        semantic: &mut SemanticState,
        fp: &FieldProfile,
        commitment_ref: &str,
    ) -> String {
        let sn = cached_semantic_network(semantic);
        let graph = &semantic.runtime_graph;
        match frame {
            SemanticFrame::DefinitionFrame { topic, authority, .. } => {
                let topic_id = AtomId::new(topic.clone());
                let auth = Self::authority_text(authority);

                // Primary: DiscourseComposer for rich multi-sentence output
                let cs = ContentSelector::build(graph);
                let activated = network_activate(&topic_id, &sn);
                let selected = cs.compose_from_activation(fp, topic, &activated);
                let composer = DiscourseComposer::new();
                let style = DiscourseStyle {
                    register: "philosophical".into(),
                    complexity: if fp.conatus_energy > 0.8 { 3 } else { 2 },
                    hedging: 0.0,
                    verbosity: if fp.narrative_tone() == qxfx0_types::NarrativeTone::Terse {
                        Verbosity::Medium
                    } else {
                        Verbosity::Elaborate
                    },
                    use_transitions: fp.conatus_energy > 0.6,
                };
                let response = composer.compose(&selected, topic, &style, 0, &[]);

                if !response.is_empty() {
                    let prefix = if response.starts_with(topic.as_str()) {
                        auth.to_string()
                    } else {
                        format!("{} {} — ", auth, topic)
                    };
                    return format!("{}{} {}", commitment_ref, prefix, response);
                }

                // Fallback: PathFinder
                let surface = PathFinder::compose_definition(graph, fp, 3, &topic_id);
                if surface.text.is_empty() {
                    format!("{} {} — содержание не прошло проверку качества.", auth, topic)
                } else {
                    let prefix = if surface.text.starts_with(topic.as_str()) {
                        auth.to_string()
                    } else {
                        format!("{} {} — ", auth, topic)
                    };
                    format!("{}{} {}", commitment_ref, prefix, surface.text)
                }
            }

            SemanticFrame::DistinctionFrame { left, right, .. } => {
                format!(
                    "Различим {} и {}. {}",
                    left,
                    right,
                    Self::render_distinction_body(graph, left, right)
                )
            }

            SemanticFrame::ChallengeFrame { target, .. } => {
                let topic_id = AtomId::new(target.clone());
                // Primary: DiscourseComposer for rich defense + counterpoint
                let cs = ContentSelector::build(graph);
                let activated = network_activate(&topic_id, &sn);
                let selected = cs.compose_from_activation(fp, target, &activated);
                let composer = DiscourseComposer::new();
                let style = DiscourseStyle {
                    register: "philosophical".into(),
                    complexity: 2,
                    hedging: 0.1,
                    verbosity: Verbosity::Medium,
                    use_transitions: true,
                };
                let response = composer.compose(&selected, target, &style, 0, &[]);
                if !response.is_empty() {
                    return if commitment_ref.is_empty() {
                        response
                    } else {
                        format!("{} {}", commitment_ref, response)
                    };
                }
                // Fallback: PathFinder
                let surface = PathFinder::compose_definition(graph, fp, 3, &topic_id);
                if surface.text.is_empty() {
                    format!("По вопросу «{}» в графе недостаточно данных для ответа.", target)
                } else if commitment_ref.is_empty() {
                    surface.text
                } else {
                    format!("{} {}", commitment_ref, surface.text)
                }
            }

            SemanticFrame::ReflectFrame { topic } => {
                let topic_id = AtomId::new(topic.clone());
                // Primary: DiscourseComposer for rich reflection
                let cs = ContentSelector::build(graph);
                let activated = network_activate(&topic_id, &sn);
                let selected = cs.compose_from_activation(fp, topic, &activated);
                let composer = DiscourseComposer::new();
                let style = DiscourseStyle {
                    register: "conversational".into(),
                    complexity: 2,
                    hedging: 0.05,
                    verbosity: if fp.narrative_tone() == qxfx0_types::NarrativeTone::Terse {
                        Verbosity::Medium
                    } else {
                        Verbosity::Elaborate
                    },
                    use_transitions: fp.conatus_energy > 0.6,
                };
                let response = composer.compose(&selected, topic, &style, 0, &[]);
                if !response.is_empty() {
                    return if commitment_ref.is_empty() {
                        response
                    } else {
                        format!("{} {}", commitment_ref, response)
                    };
                }
                // Fallback: PathFinder
                let surface = PathFinder::compose_definition(graph, fp, 3, &topic_id);
                if surface.text.is_empty() {
                    format!("Поле смыслов по теме «{}» пока не сформировано.", topic)
                } else if commitment_ref.is_empty() {
                    surface.text
                } else {
                    format!("{} {}", commitment_ref, surface.text)
                }
            }

            SemanticFrame::RepairFrame => {
                "Вижу сигнал перегруза. Я не буду наращивать интерпретации: сначала восстановим опору.".into()
            }

            SemanticFrame::ContactFrame { greeting } => {
                let _ = greeting;
                "Привет. Я готов продолжить разговор и помочь с конкретной задачей.".into()
            }

            SemanticFrame::GroundFrame { topic, .. } => {
                // L-5: `depth` is intentionally unused here. The depth signal
                // for grounding is carried by `FieldProfile` at runtime
                // composition time, not by the frame variant itself.
                format!("Держу {} как устойчивую опору для дальнейшего разбора.", topic)
            }

            SemanticFrame::HelpFrame { task } => {
                format!("Помогу с {}. Лучше всего я работаю, когда задача задана явно.", task)
            }

            SemanticFrame::PurposeFrame { topic } => {
                format!(
                    "Функция {} определяется устойчивой ролью объекта и результатом его использования.",
                    topic
                )
            }

            SemanticFrame::WorldCauseFrame { topic } => {
                format!(
                    "Причину того, почему {}, нужно проверять по внешним фактам; локальный граф может только обозначить рамку рассуждения.",
                    topic
                )
            }

            SemanticFrame::DeepenFrame { topic } => {
                format!("Углубимся в {} через одно устойчивое фокусирование.", topic)
            }

            SemanticFrame::NextStepFrame => {
                "Следующий шаг: конкретизируй задачу в одном действии.".into()
            }

            SemanticFrame::ExploratoryFrame => {
                "Если представить другой контекст, можно увидеть новые связи.".into()
            }

            SemanticFrame::OperationalFrame => {
                "Я работаю. Ограничение сейчас не в запуске, а в точности разбора входа.".into()
            }

            SemanticFrame::SelfReferenceFrame => {
                "Я — локальная система диалога. О себе я знаю свою роль и способ удерживать разговор.".into()
            }

            SemanticFrame::LearnFrame { topic, .. } => {
                // L-5: `depth` is intentionally unused here. The depth signal
                // for learning is carried by `FieldProfile` at runtime
                // composition time, not by the frame variant itself.
                format!("Если говорить о {}, зафиксирую рабочее определение.", topic)
            }

            SemanticFrame::GenericFrame { content } => content.clone(),
        }
    }

    /// Render distinction body — find differentiators in graph.
    fn render_distinction_body(graph: &AtomGraph, left: &str, right: &str) -> String {
        let left_id = AtomId::new(left.to_lowercase());
        let right_id = AtomId::new(right.to_lowercase());

        // M-6: Check if there's a direct contrast relation in either
        // direction. The graph can encode a contrast with `left_id` as
        // either the source (`left --contrasts--> right`) or the target
        // (`right --contrasts--> left`). Both must be considered.
        let is_contrast = |r: &Relation| {
            r.rel_type == qxfx0_types::RelationType::RelContrastsWith
                || r.rel_type == qxfx0_types::RelationType::RelDiffersFrom
                || r.rel_type == qxfx0_types::RelationType::RelNotReducibleTo
        };
        // Clone the matching relation out of the temporary borrow vectors so
        // it can outlive the `relations_from` / `relations_to` calls.
        let contrast: Option<Relation> = graph
            .relations_from(&left_id)
            .into_iter()
            .find(|r| is_contrast(r))
            .cloned()
            .or_else(|| {
                graph
                    .relations_to(&left_id)
                    .into_iter()
                    .find(|r| is_contrast(r))
                    .cloned()
            });

        if let Some(ref rel) = contrast {
            return format!(
                "{} и {} различаются: {}.",
                left,
                right,
                verbalize_relation(rel)
            );
        }

        // Check for path between left and right
        let path = qxfx0_semantic::GraphEngagement::bfs_path(graph, &left_id, &right_id);
        if !path.is_empty() {
            let path_text = verbalize_path(&qxfx0_types::atom::PathProof {
                edges: path,
                topic: left.into(),
            });
            return format!("{} и {} различаются: {}.", left, right, path_text);
        }

        format!(
            "{} и {} различаются по набору признаков. Без явной рамки сравнение остаётся зависимым от принятых допущений.",
            left, right
        )
    }

    fn authority_text(authority: &qxfx0_types::frame::FrameAuthority) -> &'static str {
        match authority {
            qxfx0_types::frame::FrameAuthority::Known => "Известно, что",
            qxfx0_types::frame::FrameAuthority::Probable => "Вероятно,",
            qxfx0_types::frame::FrameAuthority::Uncertain => "Мне кажется,",
        }
    }

    /// Determine which frame to build from a parsed proposition.
    pub fn frame_from_proposition(prop: &qxfx0_semantic::ParsedProposition) -> SemanticFrame {
        match prop.mode {
            PropositionMode::Define => SemanticFrame::DefinitionFrame {
                topic: prop.subject.clone(),
                scope: qxfx0_types::frame::FrameScope::GeneralScope,
                authority: qxfx0_types::frame::FrameAuthority::Known,
            },
            PropositionMode::Challenge => SemanticFrame::ChallengeFrame {
                target: prop.subject.clone(),
                basis: String::new(),
                strength: qxfx0_types::frame::ChallengeStrength::Soft,
                raw_obj: prop.subject.clone(),
            },
            PropositionMode::Connect => SemanticFrame::DistinctionFrame {
                left: prop.subject.clone(),
                right: prop.object.clone().unwrap_or_default(),
                criteria: Vec::new(),
            },
            PropositionMode::Reflect => SemanticFrame::ReflectFrame {
                topic: prop.subject.clone(),
            },
            PropositionMode::Assert => SemanticFrame::DefinitionFrame {
                topic: prop.subject.clone(),
                scope: qxfx0_types::frame::FrameScope::SpecificScope,
                authority: qxfx0_types::frame::FrameAuthority::Probable,
            },
            PropositionMode::Greeting => SemanticFrame::ContactFrame {
                greeting: prop.subject.clone(),
            },
            PropositionMode::Purpose => SemanticFrame::PurposeFrame {
                topic: prop.subject.clone(),
            },
            PropositionMode::WorldCause => SemanticFrame::WorldCauseFrame {
                topic: prop.subject.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::seed_graph;
    use qxfx0_types::system_state::SemanticState;

    fn semantic_with_graph(graph: AtomGraph) -> SemanticState {
        SemanticState {
            runtime_graph: graph,
            ..SemanticState::default()
        }
    }

    #[test]
    fn test_render_definition() {
        let mut semantic = semantic_with_graph(seed_graph());
        let frame = SemanticFrame::DefinitionFrame {
            topic: "свобода".into(),
            scope: qxfx0_types::frame::FrameScope::GeneralScope,
            authority: qxfx0_types::frame::FrameAuthority::Known,
        };
        let text = RenderEngine::render_frame(&frame, &mut semantic, &FieldProfile::default(), "");
        assert!(!text.is_empty());
        assert!(text.contains("свобода"));
    }

    #[test]
    fn test_render_distinction() {
        let mut semantic = semantic_with_graph(seed_graph());
        let frame = SemanticFrame::DistinctionFrame {
            left: "свобода".into(),
            right: "произвол".into(),
            criteria: Vec::new(),
        };
        let text = RenderEngine::render_frame(&frame, &mut semantic, &FieldProfile::default(), "");
        assert!(text.contains("Различим"));
        assert!(text.contains("свобода"));
        assert!(text.contains("произвол"));
    }

    #[test]
    fn test_render_distinction_reverse_contrast() {
        // M-6: when the contrast edge points *into* `left_id` (i.e. only
        // reachable via `relations_to`), the body must still verbalize it.
        let mut graph = AtomGraph::new();
        let alpha = AtomId::new("alpha");
        let beta = AtomId::new("beta");
        for (id, display) in [(&alpha, "alpha"), (&beta, "beta")] {
            graph.atoms.insert(
                id.clone(),
                qxfx0_types::atom::Atom {
                    id: id.clone(),
                    display: display.into(),
                    category: qxfx0_types::atom::AtomCategory::CatTopic,
                },
            );
        }
        // Contrast goes in the reverse direction: beta -> alpha
        // (no outgoing contrast from alpha exists).
        graph.add_relation(Relation {
            from: beta.clone(),
            to: alpha.clone(),
            rel_type: qxfx0_types::RelationType::RelContrastsWith,
            object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
            object_text: "alpha".into(),
            verb_override: None,
            ru_original: "beta контрастирует с alpha".into(),
            en_original: "beta contrasts with alpha".into(),
            source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
            topic: "beta".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        });

        let frame = SemanticFrame::DistinctionFrame {
            left: "alpha".into(),
            right: "beta".into(),
            criteria: Vec::new(),
        };
        let mut semantic = semantic_with_graph(graph);
        let text = RenderEngine::render_frame(&frame, &mut semantic, &FieldProfile::default(), "");
        assert!(text.contains("Различим"));
        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
        assert!(
            text.contains("контрастирует"),
            "expected contrast relation to be verbalized via reverse lookup, got: {}",
            text
        );
    }

    #[test]
    fn test_render_repair() {
        let frame = SemanticFrame::RepairFrame;
        let mut semantic = semantic_with_graph(seed_graph());
        let text = RenderEngine::render_frame(&frame, &mut semantic, &FieldProfile::default(), "");
        assert!(text.contains("перегруз"));
    }

    #[test]
    fn test_frame_from_proposition_define() {
        let prop = qxfx0_semantic::PropositionParser::parse("что такое свобода?");
        let frame = RenderEngine::frame_from_proposition(&prop);
        assert!(matches!(frame, SemanticFrame::DefinitionFrame { .. }));
    }

    #[test]
    fn test_frame_from_proposition_challenge() {
        let prop = qxfx0_semantic::PropositionParser::parse("свобода это просто вседозволенность");
        let frame = RenderEngine::frame_from_proposition(&prop);
        assert!(matches!(frame, SemanticFrame::ChallengeFrame { .. }));
    }
}
