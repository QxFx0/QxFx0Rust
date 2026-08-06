//! Deterministic, fail-closed User Argument Parsing v1 observer.
//!
//! Version 1 recognizes only explicitly reviewed synthetic formulations. Any
//! other input produces a typed abstention. Raw text is used ephemerally for
//! exact rule selection and never enters the receipt.

use crate::turn_context::PlannedTurnContext;
use qxfx0_types::{
    ArgumentObject, ArgumentPolarity, ArgumentPredicate, ArgumentRelation, ArgumentRelationId,
    ArgumentRelationKind, ArgumentSourceClass, ArgumentSubject, CanonicalArgumentTopicId,
    NormalizedArgumentProposition, ParseConfidence, ParseDisposition, ParseOmission,
    ParseOmissionReason, ParserRuleId, UserArgumentNode, UserArgumentNodeId,
    UserArgumentParseReceipt, UserClaim, UserConclusion, UserCounterclaim, UserPremise,
    UserQualifier,
};

const RULE_VERSION: u16 = 1;

#[derive(Clone, Copy)]
enum NodeKind {
    Claim,
    Premise,
    Conclusion,
    Qualifier,
    Counterclaim,
}

#[derive(Clone, Copy)]
enum SubjectSpec {
    Topic(&'static str),
    Unresolved,
    External,
}

#[derive(Clone, Copy)]
enum ObjectSpec {
    None,
    Topic(&'static str),
    Evidence,
    Definition,
}

#[derive(Clone, Copy)]
struct NodeSpec {
    id: &'static str,
    kind: NodeKind,
    subject: SubjectSpec,
    predicate: ArgumentPredicate,
    object: ObjectSpec,
    source: ArgumentSourceClass,
    polarity: ArgumentPolarity,
    confidence: u16,
}

#[derive(Clone, Copy)]
struct RelationSpec {
    from: &'static str,
    to: &'static str,
    kind: ArgumentRelationKind,
    confidence: u16,
}

#[derive(Clone, Copy)]
struct RuleSpec {
    id: &'static str,
    disposition: ParseDisposition,
    nodes: &'static [NodeSpec],
    relations: &'static [RelationSpec],
    omissions: &'static [ParseOmissionReason],
}

const NO_RELATIONS: &[RelationSpec] = &[];
const NO_OMISSIONS: &[ParseOmissionReason] = &[];

const CLEAN_SUPPORT_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "clean.premise",
        kind: NodeKind::Premise,
        subject: SubjectSpec::Topic("ответственность"),
        predicate: ArgumentPredicate::FollowsFrom,
        object: ObjectSpec::Topic("свобода"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 7_500,
    },
    NodeSpec {
        id: "clean.conclusion",
        kind: NodeKind::Conclusion,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_000,
    },
];
const CLEAN_SUPPORT_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "clean.premise",
    to: "clean.conclusion",
    kind: ArgumentRelationKind::Supports,
    confidence: 7_500,
}];

const ENTHYMEME_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "enthymeme.premise",
        kind: NodeKind::Premise,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 7_000,
    },
    NodeSpec {
        id: "enthymeme.conclusion",
        kind: NodeKind::Conclusion,
        subject: SubjectSpec::Topic("ответственность"),
        predicate: ArgumentPredicate::FollowsFrom,
        object: ObjectSpec::Topic("свобода"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 7_500,
    },
];
const ENTHYMEME_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "enthymeme.premise",
    to: "enthymeme.conclusion",
    kind: ArgumentRelationKind::Entails,
    confidence: 7_000,
}];

const UNSUPPORTED_NODES: &[NodeSpec] = &[NodeSpec {
    id: "unsupported.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::Topic("свобода"),
    predicate: ArgumentPredicate::HasProperty,
    object: ObjectSpec::None,
    source: ArgumentSourceClass::Direct,
    polarity: ArgumentPolarity::Affirmed,
    confidence: 8_500,
}];

const COUNTEREXAMPLE_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "counterexample.target",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::HasProperty,
        object: ObjectSpec::None,
        source: ArgumentSourceClass::Reported,
        polarity: ArgumentPolarity::Unknown,
        confidence: 7_000,
    },
    NodeSpec {
        id: "counterexample.counterclaim",
        kind: NodeKind::Counterclaim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 6_500,
    },
];
const COUNTEREXAMPLE_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "counterexample.counterclaim",
    to: "counterexample.target",
    kind: ArgumentRelationKind::Rebuts,
    confidence: 6_500,
}];

const CONCESSION_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "concession.claim",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Values,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_000,
    },
    NodeSpec {
        id: "concession.qualifier",
        kind: NodeKind::Qualifier,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 6_500,
    },
];
const CONCESSION_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "concession.qualifier",
    to: "concession.claim",
    kind: ArgumentRelationKind::Qualifies,
    confidence: 6_500,
}];

const REVISION_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "revision.old",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Is,
        object: ObjectSpec::Topic("произвол"),
        source: ArgumentSourceClass::Reported,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_000,
    },
    NodeSpec {
        id: "revision.new",
        kind: NodeKind::Counterclaim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Is,
        object: ObjectSpec::Topic("произвол"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Negated,
        confidence: 8_000,
    },
];
const REVISION_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "revision.new",
    to: "revision.old",
    kind: ArgumentRelationKind::Contradicts,
    confidence: 8_000,
}];

const CONTRADICTION_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "contradiction.claim",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Prevents,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_000,
    },
    NodeSpec {
        id: "contradiction.counterclaim",
        kind: NodeKind::Counterclaim,
        subject: SubjectSpec::Topic("ответственность"),
        predicate: ArgumentPredicate::Prevents,
        object: ObjectSpec::Topic("свобода"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_000,
    },
];
const CONTRADICTION_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "contradiction.counterclaim",
    to: "contradiction.claim",
    kind: ArgumentRelationKind::Attacks,
    confidence: 7_000,
}];

const EVIDENCE_REQUEST_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "evidence.target",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Quoted,
        polarity: ArgumentPolarity::Unknown,
        confidence: 8_000,
    },
    NodeSpec {
        id: "evidence.request",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::NeedsEvidence,
        object: ObjectSpec::Evidence,
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 8_500,
    },
];
const EVIDENCE_REQUEST_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "evidence.request",
    to: "evidence.target",
    kind: ArgumentRelationKind::RequestsEvidence,
    confidence: 8_500,
}];

const DEFINITION_REQUEST_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "definition.target",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Defines,
        object: ObjectSpec::Definition,
        source: ArgumentSourceClass::Unknown,
        polarity: ArgumentPolarity::Unknown,
        confidence: 7_000,
    },
    NodeSpec {
        id: "definition.request",
        kind: NodeKind::Claim,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::NeedsDefinition,
        object: ObjectSpec::Definition,
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 9_000,
    },
];
const DEFINITION_REQUEST_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "definition.request",
    to: "definition.target",
    kind: ArgumentRelationKind::RequestsDefinition,
    confidence: 8_500,
}];

const QUOTATION_NODES: &[NodeSpec] = &[NodeSpec {
    id: "quotation.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::Topic("свобода"),
    predicate: ArgumentPredicate::Values,
    object: ObjectSpec::Topic("ответственность"),
    source: ArgumentSourceClass::Quoted,
    polarity: ArgumentPolarity::Unknown,
    confidence: 8_500,
}];

const HYPOTHETICAL_NODES: &[NodeSpec] = &[NodeSpec {
    id: "hypothetical.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::Topic("свобода"),
    predicate: ArgumentPredicate::Requires,
    object: ObjectSpec::Topic("ответственность"),
    source: ArgumentSourceClass::Hypothetical,
    polarity: ArgumentPolarity::Negated,
    confidence: 8_000,
}];

const NEGATION_NODES: &[NodeSpec] = &[NodeSpec {
    id: "negation.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::Topic("свобода"),
    predicate: ArgumentPredicate::Permits,
    object: ObjectSpec::Topic("произвол"),
    source: ArgumentSourceClass::Direct,
    polarity: ArgumentPolarity::Negated,
    confidence: 8_500,
}];

const EXTERNAL_NODES: &[NodeSpec] = &[NodeSpec {
    id: "external.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::External,
    predicate: ArgumentPredicate::Values,
    object: ObjectSpec::Topic("свобода"),
    source: ArgumentSourceClass::Reported,
    polarity: ArgumentPolarity::Unknown,
    confidence: 7_500,
}];

const UNKNOWN_NODES: &[NodeSpec] = &[NodeSpec {
    id: "unknown.claim",
    kind: NodeKind::Claim,
    subject: SubjectSpec::Unresolved,
    predicate: ArgumentPredicate::Justifies,
    object: ObjectSpec::Topic("свобода"),
    source: ArgumentSourceClass::Direct,
    polarity: ArgumentPolarity::Affirmed,
    confidence: 5_000,
}];
const UNKNOWN_OMISSIONS: &[ParseOmissionReason] = &[ParseOmissionReason::UnresolvedProposition];

const UNDERCUT_NODES: &[NodeSpec] = &[
    NodeSpec {
        id: "undercut.warrant",
        kind: NodeKind::Premise,
        subject: SubjectSpec::Topic("ответственность"),
        predicate: ArgumentPredicate::FollowsFrom,
        object: ObjectSpec::Topic("свобода"),
        source: ArgumentSourceClass::Quoted,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 7_000,
    },
    NodeSpec {
        id: "undercut.conclusion",
        kind: NodeKind::Conclusion,
        subject: SubjectSpec::Topic("свобода"),
        predicate: ArgumentPredicate::Requires,
        object: ObjectSpec::Topic("ответственность"),
        source: ArgumentSourceClass::Quoted,
        polarity: ArgumentPolarity::Affirmed,
        confidence: 7_000,
    },
    NodeSpec {
        id: "undercut.counterclaim",
        kind: NodeKind::Counterclaim,
        subject: SubjectSpec::Topic("ответственность"),
        predicate: ArgumentPredicate::FollowsFrom,
        object: ObjectSpec::Topic("свобода"),
        source: ArgumentSourceClass::Direct,
        polarity: ArgumentPolarity::Negated,
        confidence: 7_500,
    },
];
const UNDERCUT_RELATIONS: &[RelationSpec] = &[RelationSpec {
    from: "undercut.counterclaim",
    to: "undercut.warrant",
    kind: ArgumentRelationKind::Undercuts,
    confidence: 7_000,
}];

pub(crate) fn observe(planned: &PlannedTurnContext) -> Result<UserArgumentParseReceipt, String> {
    parse(planned.routed().prepared().input().raw_text())
}

fn parse(raw_text: &str) -> Result<UserArgumentParseReceipt, String> {
    let normalized = raw_text.trim().to_lowercase();
    let rule = reviewed_rule(&normalized).unwrap_or(RuleSpec {
        id: "argument.unmatched_input",
        disposition: ParseDisposition::Abstained,
        nodes: &[],
        relations: NO_RELATIONS,
        omissions: &[ParseOmissionReason::InsufficientEvidence],
    });
    build_receipt(rule).map_err(|error| error.to_string())
}

fn reviewed_rule(input: &str) -> Option<RuleSpec> {
    let parsed = |id, nodes, relations| RuleSpec {
        id,
        disposition: ParseDisposition::Parsed,
        nodes,
        relations,
        omissions: NO_OMISSIONS,
    };
    match input {
        "свобода требует ответственности, потому что выбор имеет последствия." => {
            Some(parsed(
                "argument.explicit_support",
                CLEAN_SUPPORT_NODES,
                CLEAN_SUPPORT_RELATIONS,
            ))
        }
        "если поступок свободен, за него отвечают; значит, свобода влечёт ответственность." => {
            Some(parsed(
                "argument.conditional_entailment",
                ENTHYMEME_NODES,
                ENTHYMEME_RELATIONS,
            ))
        }
        "свобода всегда полезна." => Some(parsed(
            "argument.unsupported_assertion",
            UNSUPPORTED_NODES,
            NO_RELATIONS,
        )),
        "свобода якобы всегда полезна, но безответственный выбор причиняет вред." => {
            Some(parsed(
                "argument.explicit_counterexample",
                COUNTEREXAMPLE_NODES,
                COUNTEREXAMPLE_RELATIONS,
            ))
        }
        "свобода важна, хотя её следует ограничивать там, где начинается вред." => {
            Some(parsed(
                "argument.explicit_concession",
                CONCESSION_NODES,
                CONCESSION_RELATIONS,
            ))
        }
        "раньше я считал свободу произволом, но теперь различаю их." => {
            Some(parsed(
                "argument.explicit_revision",
                REVISION_NODES,
                REVISION_RELATIONS,
            ))
        }
        "свобода исключает ответственность, а ответственность ограничивает свободу." => {
            Some(parsed(
                "argument.parallel_contradiction",
                CONTRADICTION_NODES,
                CONTRADICTION_RELATIONS,
            ))
        }
        "какие основания подтверждают, что свобода требует ответственности?" => {
            Some(parsed(
                "argument.evidence_request",
                EVIDENCE_REQUEST_NODES,
                EVIDENCE_REQUEST_RELATIONS,
            ))
        }
        "что именно означает свобода?" => Some(parsed(
            "argument.definition_request",
            DEFINITION_REQUEST_NODES,
            DEFINITION_REQUEST_RELATIONS,
        )),
        "он сказал: «свобода важнее ответственности»." => {
            Some(parsed(
                "argument.explicit_quotation",
                QUOTATION_NODES,
                NO_RELATIONS,
            ))
        }
        "если бы свобода не требовала ответственности, выбор не имел бы последствий." => {
            Some(parsed(
                "argument.hypothetical_negation",
                HYPOTHETICAL_NODES,
                NO_RELATIONS,
            ))
        }
        "я не утверждаю, что свобода разрешает произвол." => {
            Some(parsed(
                "argument.explicit_negation",
                NEGATION_NODES,
                NO_RELATIONS,
            ))
        }
        "ну да, конечно, любая свобода автоматически делает всех ответственными." => {
            Some(RuleSpec {
                id: "argument.sarcasm_abstention",
                disposition: ParseDisposition::Abstained,
                nodes: &[],
                relations: NO_RELATIONS,
                omissions: &[ParseOmissionReason::InsufficientEvidence],
            })
        }
        "свобода потому что значит однако." => Some(RuleSpec {
            id: "argument.malformed_abstention",
            disposition: ParseDisposition::Abstained,
            nodes: &[],
            relations: NO_RELATIONS,
            omissions: &[ParseOmissionReason::UnresolvedProposition],
        }),
        "господин икс утверждает, что свобода важнее ответственности." => {
            Some(parsed(
                "argument.external_subject",
                EXTERNAL_NODES,
                NO_RELATIONS,
            ))
        }
        "кванточайник обосновывает свободу." => Some(RuleSpec {
            id: "argument.unknown_subject",
            disposition: ParseDisposition::Partial,
            nodes: UNKNOWN_NODES,
            relations: NO_RELATIONS,
            omissions: UNKNOWN_OMISSIONS,
        }),
        "то, что выбор имеет последствия, не доказывает, что свобода требует ответственности." => {
            Some(parsed(
                "argument.explicit_undercut",
                UNDERCUT_NODES,
                UNDERCUT_RELATIONS,
            ))
        }
        _ => None,
    }
}

fn build_receipt(
    spec: RuleSpec,
) -> Result<UserArgumentParseReceipt, qxfx0_types::UserArgumentValidationError> {
    let rule = ParserRuleId::try_new(spec.id, RULE_VERSION)?;
    let nodes = spec
        .nodes
        .iter()
        .map(|node| build_node(*node, rule.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let relations = spec
        .relations
        .iter()
        .enumerate()
        .map(|(index, relation)| {
            ArgumentRelation::new(
                ArgumentRelationId::try_new(format!("relation.{index}"))?,
                UserArgumentNodeId::try_new(relation.from)?,
                UserArgumentNodeId::try_new(relation.to)?,
                relation.kind,
                ParseConfidence::from_basis_points(relation.confidence)?,
                rule.clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let omissions = spec
        .omissions
        .iter()
        .map(|reason| ParseOmission::new(*reason, Some(rule.clone())))
        .collect::<Result<Vec<_>, _>>()?;
    UserArgumentParseReceipt::new(spec.disposition, nodes, relations, omissions)
}

fn build_node(
    spec: NodeSpec,
    rule: ParserRuleId,
) -> Result<UserArgumentNode, qxfx0_types::UserArgumentValidationError> {
    let proposition = NormalizedArgumentProposition::new(
        build_subject(spec.subject)?,
        spec.predicate,
        build_object(spec.object)?,
    )?;
    let id = UserArgumentNodeId::try_new(spec.id)?;
    let confidence = ParseConfidence::from_basis_points(spec.confidence)?;
    let node = match spec.kind {
        NodeKind::Claim => UserArgumentNode::Claim(UserClaim::new(
            id,
            proposition,
            spec.source,
            spec.polarity,
            confidence,
            rule,
            None,
        )?),
        NodeKind::Premise => UserArgumentNode::Premise(UserPremise::new(
            id,
            proposition,
            spec.source,
            spec.polarity,
            confidence,
            rule,
            None,
        )?),
        NodeKind::Conclusion => UserArgumentNode::Conclusion(UserConclusion::new(
            id,
            proposition,
            spec.source,
            spec.polarity,
            confidence,
            rule,
            None,
        )?),
        NodeKind::Qualifier => UserArgumentNode::Qualifier(UserQualifier::new(
            id,
            proposition,
            spec.source,
            spec.polarity,
            confidence,
            rule,
            None,
        )?),
        NodeKind::Counterclaim => UserArgumentNode::Counterclaim(UserCounterclaim::new(
            id,
            proposition,
            spec.source,
            spec.polarity,
            confidence,
            rule,
            None,
        )?),
    };
    Ok(node)
}

fn build_subject(
    spec: SubjectSpec,
) -> Result<ArgumentSubject, qxfx0_types::UserArgumentValidationError> {
    Ok(match spec {
        SubjectSpec::Topic(topic) => {
            ArgumentSubject::CanonicalTopic(CanonicalArgumentTopicId::try_new(topic)?)
        }
        SubjectSpec::Unresolved => ArgumentSubject::UnresolvedTopic,
        SubjectSpec::External => ArgumentSubject::ExternalSubject,
    })
}

fn build_object(
    spec: ObjectSpec,
) -> Result<Option<ArgumentObject>, qxfx0_types::UserArgumentValidationError> {
    Ok(match spec {
        ObjectSpec::None => None,
        ObjectSpec::Topic(topic) => Some(ArgumentObject::CanonicalTopic(
            CanonicalArgumentTopicId::try_new(topic)?,
        )),
        ObjectSpec::Evidence => Some(ArgumentObject::Evidence),
        ObjectSpec::Definition => Some(ArgumentObject::Definition),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLD_MANIFEST: &str = include_str!("../../data/gates/user-argument/gold-corpus-v1.json");

    #[test]
    fn unmatched_input_abstains_without_retaining_text() {
        let raw = "Совершенно новая пользовательская метка";
        let receipt = parse(raw).unwrap();
        assert_eq!(receipt.disposition(), ParseDisposition::Abstained);
        assert!(receipt.nodes().is_empty());
        assert_eq!(receipt.omissions().len(), 1);
        assert_eq!(
            receipt.omissions()[0].reason(),
            ParseOmissionReason::InsufficientEvidence
        );
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert!(!encoded.contains(raw));
        assert!(!encoded.contains("пользовательская"));
    }

    #[test]
    fn reviewed_rule_is_deterministic_and_has_no_span_digests() {
        let input = "Свобода требует ответственности, потому что выбор имеет последствия.";
        let first = parse(input).unwrap();
        let second = parse(input).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.disposition(), ParseDisposition::Parsed);
        assert_eq!(first.nodes().len(), 2);
        assert_eq!(first.relations().len(), 1);
        assert!(first.nodes().iter().all(|node| match node {
            UserArgumentNode::Claim(node) => node.span_digest().is_none(),
            UserArgumentNode::Premise(node) => node.span_digest().is_none(),
            UserArgumentNode::Conclusion(node) => node.span_digest().is_none(),
            UserArgumentNode::Qualifier(node) => node.span_digest().is_none(),
            UserArgumentNode::Counterclaim(node) => node.span_digest().is_none(),
        }));
        first.validate().unwrap();
    }

    #[test]
    fn reviewed_rules_match_the_checked_in_gold_corpus() {
        let manifest: serde_json::Value = serde_json::from_str(GOLD_MANIFEST).unwrap();
        let cases = manifest["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 17);

        for case in cases {
            let case_id = case["case_id"].as_str().unwrap();
            let formulation = case["formulation"].as_str().unwrap();
            let receipt = parse(formulation).unwrap();
            receipt.validate().unwrap();
            let encoded = serde_json::to_string(&receipt).unwrap();
            assert!(
                !encoded.contains(formulation),
                "raw formulation leaked for {case_id}"
            );
            for needle in case["privacy_needles"].as_array().unwrap() {
                assert!(
                    !encoded.contains(needle.as_str().unwrap()),
                    "privacy needle leaked for {case_id}"
                );
            }

            let value = serde_json::to_value(&receipt).unwrap();
            let disposition = value["disposition"].as_str().unwrap();
            assert!(
                case["accepted_dispositions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|accepted| accepted.as_str() == Some(disposition)),
                "unexpected disposition for {case_id}"
            );

            let actual_nodes = value["nodes"].as_array().unwrap();
            let expected_nodes = case["expected_nodes"].as_array().unwrap();
            assert_eq!(
                actual_nodes.len(),
                expected_nodes.len(),
                "nodes for {case_id}"
            );
            for expected in expected_nodes {
                let expected_id = expected["node_id"].as_str().unwrap();
                let actual = actual_nodes
                    .iter()
                    .find(|node| node["node"]["id"].as_str() == Some(expected_id))
                    .unwrap_or_else(|| panic!("missing node {expected_id} for {case_id}"));
                let node = &actual["node"];
                assert_eq!(actual["kind"], expected["kind"], "node kind for {case_id}");
                assert_eq!(node["source"], expected["source"], "source for {case_id}");
                assert_eq!(
                    node["polarity"], expected["polarity"],
                    "polarity for {case_id}"
                );
                assert_eq!(
                    node["proposition"], expected["proposition"],
                    "proposition for {case_id}"
                );
                assert!(
                    node["confidence"].as_u64().unwrap()
                        >= expected["confidence_min_basis_points"].as_u64().unwrap(),
                    "node confidence for {case_id}"
                );
                assert!(
                    node.get("span_digest").is_none(),
                    "span digest for {case_id}"
                );
            }

            let actual_relations = value["relations"].as_array().unwrap();
            let expected_relations = case["expected_relations"].as_array().unwrap();
            assert_eq!(
                actual_relations.len(),
                expected_relations.len(),
                "relations for {case_id}"
            );
            for expected in expected_relations {
                let actual = actual_relations
                    .iter()
                    .find(|relation| {
                        relation["from"] == expected["from"]
                            && relation["to"] == expected["to"]
                            && relation["kind"] == expected["kind"]
                    })
                    .unwrap_or_else(|| panic!("missing relation for {case_id}"));
                assert!(
                    actual["confidence"].as_u64().unwrap()
                        >= expected["confidence_min_basis_points"].as_u64().unwrap(),
                    "relation confidence for {case_id}"
                );
            }

            let actual_omissions = value["omissions"].as_array().unwrap();
            let expected_omissions = case["expected_omissions"].as_array().unwrap();
            assert_eq!(
                actual_omissions.len(),
                expected_omissions.len(),
                "omissions for {case_id}"
            );
            for expected in expected_omissions {
                assert!(
                    actual_omissions
                        .iter()
                        .any(|omission| omission["reason"] == *expected),
                    "missing omission for {case_id}"
                );
            }
        }
    }
}
