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
    /// Final authority/guard boundary result, including turns denied before a
    /// receipt could be created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_guard_classification: Option<String>,
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
            authority_guard_classification: None,
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
    let encoded = serde_json::to_vec(data).map_err(|error| error.to_string())?;
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
}
