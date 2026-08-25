use std::collections::BTreeMap;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Represents a single step in the deterministic pipeline execution.
#[derive(Debug, Clone, Serialize)]
pub struct TraceStep {
    pub stage: String,
    pub input_digest: String,
    pub output_digest: String,
    /// Local diagnostic only; intentionally absent from serialized replay
    /// evidence so JSONL traces remain deterministic.
    #[serde(skip_serializing)]
    pub duration: std::time::Duration,
    pub metadata: BTreeMap<String, String>,
}

/// Comprehensive trace of a full pipeline execution.
/// Designed to be compared between runs to verify determinism.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineTrace {
    pub request_id: String,
    pub steps: Vec<TraceStep>,
    /// Optional authority evidence kept outside persisted session state.
    pub authority_receipt: Option<serde_json::Value>,
    /// Private thesis catalog observation evidence. This field is absent from
    /// the default schema and never enters persisted session state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thesis_observation_receipt: Option<qxfx0_types::ThesisObservationReceipt>,
    /// ADR-0043 U2: the V2 subject-core shadow advance of the turn. Same
    /// discipline as the thesis receipt — absent from the default schema,
    /// outside persisted session state, outside the replay signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub essence_advance: Option<qxfx0_self_v2::EssenceAdvanceTrace>,
    /// Final authority/guard boundary result, including turns denied before a
    /// receipt could be created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_guard_classification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_case_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_input_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_expected_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_expected_guard: Option<String>,
    /// Local diagnostic only; intentionally absent from serialized replay
    /// evidence so JSONL traces do not contain wall-clock data.
    #[serde(skip_serializing)]
    pub total_duration: std::time::Duration,
}

impl PipelineTrace {
    pub fn new(request_id: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            steps: Vec::new(),
            authority_receipt: None,
            thesis_observation_receipt: None,
            essence_advance: None,
            authority_guard_classification: None,
            authority_case_id: None,
            authority_input_class: None,
            authority_expected_result: None,
            authority_expected_guard: None,
            total_duration: std::time::Duration::ZERO,
        }
    }

    /// Records a step in the pipeline.
    pub fn record_step(
        &mut self,
        stage: &str,
        input_digest: String,
        output_digest: String,
        duration: std::time::Duration,
        metadata: BTreeMap<String, String>,
    ) {
        self.steps.push(TraceStep {
            stage: stage.to_string(),
            input_digest,
            output_digest,
            duration,
            metadata,
        });
    }

    pub fn set_total_duration(&mut self, duration: std::time::Duration) {
        self.total_duration = duration;
    }

    pub fn set_authority_receipt<T: Serialize>(&mut self, receipt: &T) -> Result<(), String> {
        self.authority_receipt =
            Some(serde_json::to_value(receipt).map_err(|error| error.to_string())?);
        Ok(())
    }

    pub fn set_thesis_observation_receipt(
        &mut self,
        receipt: qxfx0_types::ThesisObservationReceipt,
    ) -> Result<(), qxfx0_types::ThesisObservationValidationError> {
        receipt.validate()?;
        self.thesis_observation_receipt = Some(receipt);
        Ok(())
    }

    /// Record the V2 subject-core shadow advance (ADR-0043 U2). Pure
    /// observation: no validation gate because the summary is evidence, not
    /// an authority decision.
    pub fn record_essence_advance(&mut self, advance: qxfx0_self_v2::EssenceAdvanceTrace) {
        self.essence_advance = Some(advance);
    }

    pub fn set_authority_guard_classification(&mut self, classification: &str) {
        self.authority_guard_classification = Some(classification.into());
        if let Some(receipt) = self.authority_receipt.as_mut() {
            if let Some(object) = receipt.as_object_mut() {
                object.insert(
                    "guard_classification".into(),
                    serde_json::Value::String(classification.into()),
                );
            }
        }
    }

    pub fn set_authority_case_metadata(
        &mut self,
        case_id: Option<&str>,
        input_class: Option<&str>,
        expected_result: Option<&str>,
        expected_guard: Option<&str>,
    ) {
        self.authority_case_id = case_id.map(str::to_owned);
        self.authority_input_class = input_class.map(str::to_owned);
        self.authority_expected_result = expected_result.map(str::to_owned);
        self.authority_expected_guard = expected_guard.map(str::to_owned);
    }

    /// Formats the trace for human-readable output or log files.
    pub fn format_trace(&self) -> String {
        let mut output = format!("--- Pipeline Trace: {} ---\n", self.request_id);
        for (i, step) in self.steps.iter().enumerate() {
            output.push_str(&format!(
                "Step {}: [{}] In: {} -> Out: {} ({:?})\n",
                i + 1,
                step.stage,
                step.input_digest,
                step.output_digest,
                step.duration
            ));
            for (k, v) in &step.metadata {
                output.push_str(&format!("  {} = {}\n", k, v));
            }
        }
        output.push_str(&format!("Total Duration: {:?}\n", self.total_duration));
        output.push_str("-----------------------------");
        output
    }

    /// Deterministic view suitable for replay comparison. Wall-clock
    /// durations are deliberately excluded.
    pub fn replay_signature(&self) -> Vec<(&str, &str, &str)> {
        self.steps
            .iter()
            .map(|step| {
                (
                    step.stage.as_str(),
                    step.input_digest.as_str(),
                    step.output_digest.as_str(),
                )
            })
            .collect()
    }
}

/// Calculate a cross-process SHA-256 digest over deterministic JSON. All
/// persistent maps use ordered containers, so equal state serializes to the
/// same bytes across fresh processes and Rust releases.
pub fn calculate_stable_digest<T: Serialize + ?Sized>(data: &T) -> Result<String, String> {
    let encoded = match serde_json::to_vec(data) {
        Ok(encoded) => encoded,
        Err(error) => {
            // Digest failures degrade replay evidence to placeholders. They
            // are not expected for the deterministic types that reach this
            // function, so they are logged loudly instead of passing
            // silently through every trace step.
            tracing::error!("stable digest serialization failed: {error}");
            return Err(error.to_string());
        }
    };
    let digest = Sha256::digest(encoded);
    Ok(format!("{digest:x}"))
}

/// A tracing guard that measures the duration of a pipeline stage.
pub struct StageGuard<'a> {
    trace: &'a mut PipelineTrace,
    stage: String,
    start: Instant,
    input_digest: String,
}

impl<'a> StageGuard<'a> {
    pub fn new(
        trace: &'a mut PipelineTrace,
        stage: &str,
        input: &impl Serialize,
    ) -> Result<Self, String> {
        Ok(Self {
            trace,
            stage: stage.to_string(),
            start: Instant::now(),
            input_digest: calculate_stable_digest(input)?,
        })
    }

    pub fn finish(
        self,
        output: &impl Serialize,
        metadata: BTreeMap<String, String>,
    ) -> Result<(), String> {
        let duration = self.start.elapsed();
        let output_digest = calculate_stable_digest(output)?;
        self.trace.record_step(
            &self.stage,
            self.input_digest,
            output_digest,
            duration,
            metadata,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_digest_is_repeatable_and_ordered() {
        let first = BTreeMap::from([("alpha", 1_u8), ("beta", 2)]);
        let second = BTreeMap::from([("beta", 2_u8), ("alpha", 1)]);
        assert_eq!(
            calculate_stable_digest(&first).unwrap(),
            calculate_stable_digest(&second).unwrap()
        );
    }

    #[test]
    fn default_trace_schema_omits_thesis_observation() {
        let encoded = serde_json::to_value(PipelineTrace::new("default")).unwrap();
        assert!(encoded.get("thesis_observation_receipt").is_none());
    }

    #[test]
    fn default_trace_schema_omits_essence_advance() {
        let encoded = serde_json::to_value(PipelineTrace::new("default")).unwrap();
        assert!(encoded.get("essence_advance").is_none());
    }

    #[test]
    fn essence_advance_evidence_does_not_change_replay_signature() {
        let mut trace = PipelineTrace::new("essence-v2");
        trace.record_step(
            "finalize",
            "in".into(),
            "out".into(),
            std::time::Duration::ZERO,
            BTreeMap::new(),
        );
        let expected = trace
            .replay_signature()
            .into_iter()
            .map(|(stage, input, output)| (stage.to_owned(), input.to_owned(), output.to_owned()))
            .collect::<Vec<_>>();
        trace.record_essence_advance(qxfx0_self_v2::EssenceAdvanceTrace {
            angst_level: 0.2,
            conatus_scalar: 9.9,
            ..qxfx0_self_v2::EssenceAdvanceTrace::default()
        });
        assert_eq!(
            trace
                .replay_signature()
                .into_iter()
                .map(|(stage, input, output)| (
                    stage.to_owned(),
                    input.to_owned(),
                    output.to_owned()
                ))
                .collect::<Vec<_>>(),
            expected
        );
        let encoded = serde_json::to_value(&trace).unwrap();
        assert!(encoded.get("essence_advance").is_some());
    }

    #[test]
    fn replay_signature_ignores_duration() {
        let mut a = PipelineTrace::new("a");
        let mut b = PipelineTrace::new("b");
        a.record_step(
            "prepare",
            "in".into(),
            "out".into(),
            std::time::Duration::from_nanos(1),
            BTreeMap::new(),
        );
        b.record_step(
            "prepare",
            "in".into(),
            "out".into(),
            std::time::Duration::from_secs(1),
            BTreeMap::new(),
        );
        assert_eq!(a.replay_signature(), b.replay_signature());
    }

    #[test]
    fn authority_evidence_does_not_change_replay_signature() {
        let mut trace = PipelineTrace::new("authority");
        trace.record_step(
            "render",
            "in".into(),
            "out".into(),
            std::time::Duration::ZERO,
            BTreeMap::new(),
        );
        let expected = trace
            .replay_signature()
            .into_iter()
            .map(|(stage, input, output)| (stage.to_owned(), input.to_owned(), output.to_owned()))
            .collect::<Vec<_>>();
        trace.authority_receipt = Some(serde_json::json!({"authority": "Canary"}));
        trace.set_authority_guard_classification("v2_successfully_emitted");
        assert_eq!(
            trace
                .replay_signature()
                .into_iter()
                .map(|(stage, input, output)| (
                    stage.to_owned(),
                    input.to_owned(),
                    output.to_owned()
                ))
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn thesis_observation_evidence_does_not_change_replay_signature() {
        let mut trace = PipelineTrace::new("thesis");
        trace.record_step(
            "render",
            "in".into(),
            "out".into(),
            std::time::Duration::ZERO,
            BTreeMap::new(),
        );
        let expected = trace
            .replay_signature()
            .into_iter()
            .map(|(stage, input, output)| (stage.to_owned(), input.to_owned(), output.to_owned()))
            .collect::<Vec<_>>();
        trace
            .set_thesis_observation_receipt(
                qxfx0_types::ThesisObservationReceipt::new(
                    qxfx0_types::ThesisObservationOutcome::NoAuditedPlan,
                    0,
                    qxfx0_types::calculate_thesis_observation_turn_binding(
                        "private-session",
                        0,
                        "private-input",
                    ),
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            trace
                .replay_signature()
                .into_iter()
                .map(|(stage, input, output)| (
                    stage.to_owned(),
                    input.to_owned(),
                    output.to_owned()
                ))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
