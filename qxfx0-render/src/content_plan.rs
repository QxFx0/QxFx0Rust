//! Renderer for audited content plans.
//!
//! It resolves grounded leaves exclusively through the audited admission
//! registry. It intentionally has no access to the runtime semantic graph.

use qxfx0_semantic::{
    argued_topic_registry, ArguedTopic, ClaimRole, DialogueObligation, PlanSubject, PlannedClaim,
    PredicateRef, ReadyResponsePlan, SemanticProposition,
};

/// Render a ready audited plan into a stable Russian response.
///
/// This rejects any plan whose claim structure or predicate references drift
/// from its admitted topic, rather than silently substituting graph content.
pub fn render_audited_plan(plan: &ReadyResponsePlan) -> Result<String, String> {
    plan.validate()?;
    let PlanSubject::Topic(topic_id) = plan.subject() else {
        return Err("audited content renderer requires a topic subject".into());
    };
    let registry = argued_topic_registry().map_err(str::to_owned)?;
    let topic = registry
        .get(topic_id.as_str())
        .ok_or_else(|| format!("topic '{}' is not admitted", topic_id.as_str()))?;

    let thesis = exactly_one_claim(plan, ClaimRole::Thesis)?;
    validate_thesis(topic, thesis)?;
    let counterpoint = exactly_one_claim(plan, ClaimRole::Counterpoint)?;
    validate_counterpoint(topic, counterpoint)?;
    let consequence = match topic.consequence() {
        Some(_) => {
            let claim = exactly_one_claim(plan, ClaimRole::Consequence)?;
            validate_consequence(topic, claim)?;
            Some(claim)
        }
        None => {
            if plan
                .claims()
                .iter()
                .any(|claim| claim.role() == ClaimRole::Consequence)
            {
                return Err(format!(
                    "topic '{}' has no admitted consequence",
                    topic.topic().as_str()
                ));
            }
            None
        }
    };

    if plan.claims().len() != 2 + usize::from(consequence.is_some()) {
        return Err("audited content plan contains an unsupported claim role".into());
    }
    if !matches!(
        plan.obligation(),
        Some(DialogueObligation::CheckAgreement { claim_id }) if claim_id == thesis.id()
    ) {
        return Err("audited content plan must check agreement with its thesis".into());
    }

    let thesis_surface = surface_for(topic, statement_ref(thesis)?)?;
    let counterpoint_surface = surface_for(topic, statement_ref(counterpoint)?)?;
    let mut sentences = vec![
        sentence("Тезис", thesis_surface),
        sentence("Контрпункт", counterpoint_surface),
    ];
    if let Some(consequence) = consequence {
        sentences.push(sentence(
            "Следствие",
            surface_for(topic, statement_ref(consequence)?)?,
        ));
    }
    sentences.push("Проверка: верно ли это?".into());
    Ok(sentences.join(" "))
}

fn exactly_one_claim(plan: &ReadyResponsePlan, role: ClaimRole) -> Result<&PlannedClaim, String> {
    let mut matches = plan.claims().iter().filter(|claim| claim.role() == role);
    let claim = matches
        .next()
        .ok_or_else(|| format!("audited content plan is missing {role:?}"))?;
    if matches.next().is_some() {
        return Err(format!("audited content plan repeats {role:?}"));
    }
    Ok(claim)
}

fn validate_thesis(topic: &ArguedTopic, claim: &PlannedClaim) -> Result<(), String> {
    if claim.proposition() != topic.primary_proposition() {
        return Err("thesis proposition does not match the admitted canonical slots".into());
    }
    validate_refs(claim, &[topic.primary_predicate_ref()])
}

fn validate_counterpoint(topic: &ArguedTopic, claim: &PlannedClaim) -> Result<(), String> {
    let counterpoint = topic.counterpoint().predicate_ref();
    if !matches!(
        claim.proposition(),
        SemanticProposition::Counterpoint { statement, counters }
            if statement == counterpoint && counters == topic.primary_predicate_ref()
    ) {
        return Err("counterpoint proposition does not match the admitted topic".into());
    }
    validate_refs(claim, &[counterpoint, topic.primary_predicate_ref()])
}

fn validate_consequence(topic: &ArguedTopic, claim: &PlannedClaim) -> Result<(), String> {
    let consequence = topic
        .consequence()
        .ok_or_else(|| {
            format!(
                "topic '{}' has no admitted consequence",
                topic.topic().as_str()
            )
        })?
        .predicate_ref();
    if !matches!(
        claim.proposition(),
        SemanticProposition::Consequence {
            statement,
            follows_from,
        } if statement == consequence && follows_from == topic.primary_predicate_ref()
    ) {
        return Err("consequence proposition does not match the admitted topic".into());
    }
    validate_refs(claim, &[consequence, topic.primary_predicate_ref()])
}

fn validate_refs(claim: &PlannedClaim, expected: &[&PredicateRef]) -> Result<(), String> {
    let actual = claim.predicate_refs().iter().collect::<Vec<_>>();
    if actual.len() != expected.len()
        || actual
            .iter()
            .zip(expected)
            .any(|(actual, expected)| *actual != *expected)
    {
        return Err(format!(
            "claim '{}' has predicate references outside its admitted role",
            claim.id().as_str()
        ));
    }
    Ok(())
}

fn statement_ref(claim: &PlannedClaim) -> Result<&PredicateRef, String> {
    match claim.proposition() {
        SemanticProposition::CanonicalPredicate { .. } => Ok(claim.predicate_refs().first()),
        SemanticProposition::Counterpoint { statement, .. }
        | SemanticProposition::Consequence { statement, .. } => Ok(statement),
        SemanticProposition::DialogueAct(_) | SemanticProposition::ExternalReference(_) => {
            Err(format!(
                "claim '{}' is not an audited content proposition",
                claim.id().as_str()
            ))
        }
    }
}

fn surface_for<'a>(
    topic: &'a ArguedTopic,
    predicate_ref: &PredicateRef,
) -> Result<&'a str, String> {
    topic
        .statement_for(predicate_ref)
        .map(|statement| statement.surface())
        .ok_or_else(|| {
            format!(
                "predicate '{}' is not admitted for topic '{}'",
                predicate_ref.as_str(),
                topic.topic().as_str()
            )
        })
}

fn sentence(label: &str, surface: &str) -> String {
    let surface = surface.trim();
    let terminal = surface
        .chars()
        .last()
        .filter(|character| matches!(character, '.' | '!' | '?'))
        .unwrap_or('.');
    let surface = surface.trim_end_matches(['.', '!', '?']);
    format!("{label}: {surface}{terminal}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::{
        ArguedTopic, ClaimEvidence, ClaimId, ClaimRole, Confidence, DerivationRule, DerivationStep,
        DialogueObligation, DiscoursePlan, DiscourseRelation, NonEmptyVec, PlannedClaim,
        ResponseGoal, SentenceBudget,
    };

    fn claim_for(topic: &ArguedTopic, role: ClaimRole) -> PlannedClaim {
        let primary_ref = topic.primary_predicate_ref().clone();
        let evidence = ClaimEvidence::curated(topic.evidence_record());
        match role {
            ClaimRole::Thesis => PlannedClaim::new(
                ClaimId::try_new(format!("{}.thesis", primary_ref.as_str())).unwrap(),
                ClaimRole::Thesis,
                Some(topic.thesis().fact_id().clone()),
                topic.primary_proposition().clone(),
                NonEmptyVec::one(primary_ref.clone()),
                evidence,
                Confidence::from_basis_points(9_500).unwrap(),
            ),
            ClaimRole::Counterpoint => {
                let counter_ref = topic.counterpoint().predicate_ref().clone();
                let mut refs = NonEmptyVec::one(counter_ref.clone());
                refs.push(primary_ref.clone());
                PlannedClaim::new(
                    ClaimId::try_new(format!("{}.counterpoint", primary_ref.as_str())).unwrap(),
                    ClaimRole::Counterpoint,
                    Some(topic.counterpoint().fact_id().clone()),
                    SemanticProposition::Counterpoint {
                        statement: counter_ref,
                        counters: primary_ref,
                    },
                    refs,
                    evidence,
                    Confidence::from_basis_points(9_000).unwrap(),
                )
            }
            ClaimRole::Consequence => {
                let consequence = topic.consequence().expect("topic has a consequence");
                let consequence_ref = consequence.predicate_ref().clone();
                let mut refs = NonEmptyVec::one(consequence_ref.clone());
                refs.push(primary_ref.clone());
                PlannedClaim::new(
                    ClaimId::try_new(format!("{}.consequence", primary_ref.as_str())).unwrap(),
                    ClaimRole::Consequence,
                    Some(consequence.fact_id().clone()),
                    SemanticProposition::Consequence {
                        statement: consequence_ref,
                        follows_from: primary_ref,
                    },
                    refs,
                    evidence,
                    Confidence::from_basis_points(9_000).unwrap(),
                )
            }
            other => panic!("unexpected claim role {other:?}"),
        }
    }

    fn correct_plan(topic: &ArguedTopic) -> qxfx0_semantic::ReadyResponsePlan {
        let primary_ref = topic.primary_predicate_ref().clone();
        let thesis_id = ClaimId::try_new(format!("{}.thesis", primary_ref.as_str())).unwrap();
        let mut claims = NonEmptyVec::one(claim_for(topic, ClaimRole::Thesis));
        claims.push(claim_for(topic, ClaimRole::Counterpoint));

        let mut derivation = vec![
            DerivationStep::new(
                ClaimId::try_new(format!("{}.thesis", primary_ref.as_str())).unwrap(),
                NonEmptyVec::one(primary_ref.clone()),
                DerivationRule::SelectedAdmittedPredicate,
            ),
            DerivationStep::new(
                ClaimId::try_new(format!("{}.counterpoint", primary_ref.as_str())).unwrap(),
                NonEmptyVec::one(topic.counterpoint().predicate_ref().clone()),
                DerivationRule::AddedCounterpoint,
            ),
        ];
        let sentence_budget = if topic.consequence().is_some() {
            claims.push(claim_for(topic, ClaimRole::Consequence));
            derivation.push(DerivationStep::new(
                ClaimId::try_new(format!("{}.consequence", primary_ref.as_str())).unwrap(),
                NonEmptyVec::one(topic.consequence().unwrap().predicate_ref().clone()),
                DerivationRule::AddedConsequence,
            ));
            SentenceBudget::Three
        } else {
            SentenceBudget::Two
        };

        qxfx0_semantic::ReadyResponsePlan::new(
            ResponseGoal::Define,
            PlanSubject::Topic(topic.topic().clone()),
            claims,
            DiscoursePlan::new(DiscourseRelation::Counterpoint, sentence_budget),
            Some(DialogueObligation::CheckAgreement {
                claim_id: thesis_id,
            }),
            derivation,
        )
        .expect("correct audited plan must construct")
    }

    fn registry_topic(name: &str) -> ArguedTopic {
        argued_topic_registry()
            .unwrap()
            .get(name)
            .cloned()
            .expect("audited topic must exist")
    }

    #[test]
    fn renders_the_admitted_surfaces_with_labels_for_a_three_claim_topic() {
        let topic = registry_topic("свобода");
        let plan = correct_plan(&topic);
        let rendered = render_audited_plan(&plan).expect("correct plan renders");
        assert!(rendered.starts_with("Тезис: "));
        assert!(rendered.contains("Контрпункт: "));
        assert!(rendered.contains("Следствие: "));
        assert!(rendered.ends_with("Проверка: верно ли это?"));
    }

    #[test]
    fn renders_a_two_claim_topic_without_a_consequence_label() {
        let registry = argued_topic_registry().unwrap();
        let topic = registry
            .topics()
            .find(|topic| topic.consequence().is_none())
            .cloned()
            .expect("corpus contains two-claim topics");
        let plan = correct_plan(&topic);
        let rendered = render_audited_plan(&plan).expect("correct plan renders");
        assert!(rendered.contains("Тезис: "));
        assert!(rendered.contains("Контрпункт: "));
        assert!(!rendered.contains("Следствие: "));
    }

    #[test]
    fn a_non_topic_subject_is_rejected() {
        let topic = registry_topic("свобода");
        let thesis = claim_for(&topic, ClaimRole::Thesis);
        let external = qxfx0_semantic::ReadyResponsePlan::new(
            ResponseGoal::Define,
            PlanSubject::External(qxfx0_semantic::ExternalSubject::new(
                qxfx0_semantic::ExternalSubjectKind::Entity,
                "что-то",
            )),
            NonEmptyVec::one(thesis.clone()),
            DiscoursePlan::new(DiscourseRelation::None, SentenceBudget::One),
            None,
            vec![DerivationStep::new(
                thesis.id().clone(),
                NonEmptyVec::one(topic.primary_predicate_ref().clone()),
                DerivationRule::SelectedAdmittedPredicate,
            )],
        )
        .expect("plan with external subject must construct");
        let error = render_audited_plan(&external).unwrap_err();
        assert!(error.contains("requires a topic subject"), "got: {error}");
    }

    #[test]
    fn an_unknown_topic_is_rejected() {
        let topic = registry_topic("свобода");
        let thesis = claim_for(&topic, ClaimRole::Thesis);
        let counterpoint = claim_for(&topic, ClaimRole::Counterpoint);
        let unknown = qxfx0_semantic::ReadyResponsePlan::new(
            ResponseGoal::Define,
            PlanSubject::Topic(qxfx0_types::AtomId::new("не-аудированная-тема")),
            {
                let mut claims = NonEmptyVec::one(thesis.clone());
                claims.push(counterpoint.clone());
                claims
            },
            DiscoursePlan::new(DiscourseRelation::Counterpoint, SentenceBudget::Two),
            None,
            vec![
                DerivationStep::new(
                    thesis.id().clone(),
                    NonEmptyVec::one(topic.primary_predicate_ref().clone()),
                    DerivationRule::SelectedAdmittedPredicate,
                ),
                DerivationStep::new(
                    counterpoint.id().clone(),
                    NonEmptyVec::one(topic.counterpoint().predicate_ref().clone()),
                    DerivationRule::AddedCounterpoint,
                ),
            ],
        );
        match unknown {
            Ok(plan) => {
                let error = render_audited_plan(&plan).unwrap_err();
                assert!(error.contains("not admitted"), "got: {error}");
            }
            Err(error) => panic!("plan must construct, got: {error}"),
        }
    }

    #[test]
    fn a_missing_thesis_or_counterpoint_is_rejected() {
        let topic = registry_topic("свобода");
        // Build a plan with only the counterpoint claim.
        let counterpoint = claim_for(&topic, ClaimRole::Counterpoint);
        let counterpoint_only = qxfx0_semantic::ReadyResponsePlan::new(
            ResponseGoal::Define,
            PlanSubject::Topic(topic.topic().clone()),
            NonEmptyVec::one(counterpoint.clone()),
            DiscoursePlan::new(DiscourseRelation::Counterpoint, SentenceBudget::One),
            None,
            vec![DerivationStep::new(
                counterpoint.id().clone(),
                NonEmptyVec::one(topic.counterpoint().predicate_ref().clone()),
                DerivationRule::AddedCounterpoint,
            )],
        );
        match counterpoint_only {
            Ok(plan) => {
                let error = render_audited_plan(&plan).unwrap_err();
                assert!(error.contains("missing"), "got: {error}");
            }
            Err(error) => panic!("plan must construct, got: {error}"),
        }
    }

    #[test]
    fn a_consequence_on_a_two_claim_topic_is_rejected() {
        let registry = argued_topic_registry().unwrap();
        let topic = registry
            .topics()
            .find(|topic| topic.consequence().is_none())
            .cloned()
            .expect("corpus contains two-claim topics");
        // Take a consequence claim from another topic and attach it.
        let donor = registry_topic("свобода");
        let donor_consequence = claim_for(&donor, ClaimRole::Consequence);
        let primary_ref = topic.primary_predicate_ref().clone();
        let thesis_id = ClaimId::try_new(format!("{}.thesis", primary_ref.as_str())).unwrap();
        let mut claims = NonEmptyVec::one(claim_for(&topic, ClaimRole::Thesis));
        claims.push(claim_for(&topic, ClaimRole::Counterpoint));
        claims.push(donor_consequence);
        let plan = qxfx0_semantic::ReadyResponsePlan::new(
            ResponseGoal::Define,
            PlanSubject::Topic(topic.topic().clone()),
            claims,
            DiscoursePlan::new(DiscourseRelation::Counterpoint, SentenceBudget::Three),
            Some(DialogueObligation::CheckAgreement {
                claim_id: thesis_id.clone(),
            }),
            vec![DerivationStep::new(
                thesis_id,
                NonEmptyVec::one(primary_ref),
                DerivationRule::SelectedAdmittedPredicate,
            )],
        )
        .expect("plan with foreign consequence must construct");
        let error = render_audited_plan(&plan).unwrap_err();
        assert!(
            error.contains("no admitted consequence") || error.contains("does not match"),
            "got: {error}"
        );
    }
}
