use qxfx0_semantic::{verbalize_path, verbalize_relation, PathFinder, PropositionMode};
use qxfx0_types::atom::{AtomGraph, AtomId};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::frame::SemanticFrame;

/// Render engine — dispatches semantic frames to surface text generation.
/// Replaces inline rendering in pipeline (F5 fix).
pub struct RenderEngine;

impl RenderEngine {
    /// Render a semantic frame into surface text.
    pub fn render_frame(
        frame: &SemanticFrame,
        graph: &AtomGraph,
        fp: &FieldProfile,
        commitment_ref: &str,
    ) -> String {
        match frame {
            SemanticFrame::DefinitionFrame { topic, authority, .. } => {
                let topic_id = AtomId::new(topic.clone());
                let surface = PathFinder::compose_definition(graph, fp, 3, &topic_id);

                if surface.text.is_empty() {
                    let auth = Self::authority_text(authority);
                    format!("{} {} — содержание не прошло проверку качества.", auth, topic)
                } else {
                    let auth = Self::authority_text(authority);
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

            SemanticFrame::ChallengeFrame { target, basis: _, .. } => {
                if commitment_ref.is_empty() {
                    format!("Возражение принято. {} требует проверки.", target)
                } else {
                    format!("{} Я удерживаю позицию по вопросу {}.", commitment_ref, target)
                }
            }

            SemanticFrame::ReflectFrame { topic } => {
                if commitment_ref.is_empty() {
                    format!("Когда я думаю о {}: поле смыслов.", topic)
                } else {
                    format!("{} Когда я думаю о {}: поле смыслов.", commitment_ref, topic)
                }
            }

            SemanticFrame::RepairFrame => {
                "Вижу сигнал перегруза. Я не буду наращивать интерпретации: сначала восстановим опору.".into()
            }

            SemanticFrame::ContactFrame { greeting } => {
                format!("{}. Слышу, что сейчас нужна опора.", greeting)
            }

            SemanticFrame::GroundFrame { topic, .. } => {
                format!("Держу {} как устойчивую опору для дальнейшего разбора.", topic)
            }

            SemanticFrame::HelpFrame { task } => {
                format!("Помогу с {}. Лучше всего я работаю, когда задача задана явно.", task)
            }

            SemanticFrame::PurposeFrame { topic } => {
                format!("Функция {} проявляется через повторяемую роль в действии.", topic)
            }

            SemanticFrame::WorldCauseFrame { topic } => {
                format!(
                    "Если говорить о причине {}: различаю локальное рассуждение и знание о внешнем мире.",
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
                format!("Если говорить о {}, зафиксирую рабочее определение.", topic)
            }

            SemanticFrame::GenericFrame { content } => content.clone(),
        }
    }

    /// Render distinction body — find differentiators in graph.
    fn render_distinction_body(graph: &AtomGraph, left: &str, right: &str) -> String {
        let left_id = AtomId::new(left.to_lowercase());
        let right_id = AtomId::new(right.to_lowercase());

        // Check if there's a direct contrast relation
        let left_rels = graph.relations_from(&left_id);
        let contrast = left_rels.iter().find(|r| {
            r.rel_type == qxfx0_types::RelationType::RelContrastsWith
                || r.rel_type == qxfx0_types::RelationType::RelDiffersFrom
                || r.rel_type == qxfx0_types::RelationType::RelNotReducibleTo
        });

        if let Some(rel) = contrast {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::seed_graph;

    #[test]
    fn test_render_definition() {
        let graph = seed_graph();
        let frame = SemanticFrame::DefinitionFrame {
            topic: "свобода".into(),
            scope: qxfx0_types::frame::FrameScope::GeneralScope,
            authority: qxfx0_types::frame::FrameAuthority::Known,
        };
        let text = RenderEngine::render_frame(&frame, &graph, &FieldProfile::default(), "");
        assert!(!text.is_empty());
        assert!(text.contains("свобода"));
    }

    #[test]
    fn test_render_distinction() {
        let graph = seed_graph();
        let frame = SemanticFrame::DistinctionFrame {
            left: "свобода".into(),
            right: "произвол".into(),
            criteria: Vec::new(),
        };
        let text = RenderEngine::render_frame(&frame, &graph, &FieldProfile::default(), "");
        assert!(text.contains("Различим"));
        assert!(text.contains("свобода"));
        assert!(text.contains("произвол"));
    }

    #[test]
    fn test_render_repair() {
        let frame = SemanticFrame::RepairFrame;
        let text = RenderEngine::render_frame(&frame, &seed_graph(), &FieldProfile::default(), "");
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
