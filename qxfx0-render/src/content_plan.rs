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
