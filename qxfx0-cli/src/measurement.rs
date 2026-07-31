use crate::fresh_state;
use qxfx0_pipeline::{process_turn, process_turn_with_trace, TurnInput};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

const BENCHMARK_INPUT: &str = "что такое свобода?";
const MAX_BENCHMARK_SAMPLES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LatencyDistributionMicros {
    pub samples: usize,
    pub min: u64,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
    pub mean: u64,
}

impl LatencyDistributionMicros {
    fn from_samples(mut samples: Vec<u64>) -> Self {
        debug_assert!(!samples.is_empty());
        samples.sort_unstable();
        let sum = samples.iter().map(|value| u128::from(*value)).sum::<u128>();
        let count = samples.len();
        Self {
            samples: count,
            min: samples[0],
            p50: nearest_rank(&samples, 50),
            p95: nearest_rank(&samples, 95),
            max: samples[count - 1],
            mean: (sum / count as u128).try_into().unwrap_or(u64::MAX),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeBenchmarkReport {
    pub schema_version: u8,
    pub benchmark_input: &'static str,
    pub first_turn_micros: u64,
    pub warmup_turns: usize,
    pub steady_state_micros: LatencyDistributionMicros,
    pub rss_before_bytes: Option<u64>,
    pub rss_after_first_turn_bytes: Option<u64>,
    pub rss_after_steady_state_bytes: Option<u64>,
    pub executable_bytes: Option<u64>,
    pub morphology_lexemes_bytes: usize,
    pub morphology_manifest_bytes: usize,
    pub morphology_bundle_bytes: usize,
    pub pack_set_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepeatedText {
    pub text: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopicRenderMeasurement {
    pub topic: String,
    pub ready: bool,
    pub blocked: bool,
    pub response_digest: String,
    pub sentence_count: usize,
    pub normalized_opening: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RendererDiversityReport {
    pub schema_version: u8,
    pub audited_topics: usize,
    pub ready_plans: usize,
    pub blocked_topics: Vec<String>,
    pub unique_responses: usize,
    pub total_sentences: usize,
    pub unique_sentences: usize,
    pub repeated_sentence_kinds: usize,
    pub repeated_sentence_occurrences: usize,
    pub max_repeated_sentence: Option<RepeatedText>,
    pub opening_ngram_words: usize,
    pub unique_normalized_openings: usize,
    pub max_repeated_normalized_opening: Option<RepeatedText>,
    pub topics: Vec<TopicRenderMeasurement>,
}

/// Measure the first lazily initialized in-memory turn and a steady-state
/// distribution. Every sample gets a fresh state, so history growth and FSM
/// transitions do not contaminate the comparison.
pub fn run_runtime_benchmark(
    steady_samples: usize,
    warmup_turns: usize,
) -> Result<RuntimeBenchmarkReport, String> {
    if steady_samples == 0 || steady_samples > MAX_BENCHMARK_SAMPLES {
        return Err(format!(
            "steady sample count must be between 1 and {MAX_BENCHMARK_SAMPLES}"
        ));
    }
    if warmup_turns > MAX_BENCHMARK_SAMPLES {
        return Err(format!(
            "warmup turn count must not exceed {MAX_BENCHMARK_SAMPLES}"
        ));
    }

    let rss_before_bytes = resident_set_size_bytes();
    let first_started = Instant::now();
    run_measured_turn("__benchmark_first__")?;
    let first_turn_micros = elapsed_micros(first_started);
    let rss_after_first_turn_bytes = resident_set_size_bytes();

    for index in 0..warmup_turns {
        run_measured_turn(&format!("__benchmark_warmup_{index}__"))?;
    }

    let mut steady = Vec::with_capacity(steady_samples);
    for index in 0..steady_samples {
        let started = Instant::now();
        run_measured_turn(&format!("__benchmark_steady_{index}__"))?;
        steady.push(elapsed_micros(started));
    }

    Ok(RuntimeBenchmarkReport {
        schema_version: 1,
        benchmark_input: BENCHMARK_INPUT,
        first_turn_micros,
        warmup_turns,
        steady_state_micros: LatencyDistributionMicros::from_samples(steady),
        rss_before_bytes,
        rss_after_first_turn_bytes,
        rss_after_steady_state_bytes: resident_set_size_bytes(),
        executable_bytes: std::env::current_exe()
            .ok()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| metadata.len()),
        morphology_lexemes_bytes: qxfx0_morphology::EMBEDDED_LEXEMES_SIZE_BYTES,
        morphology_manifest_bytes: qxfx0_morphology::EMBEDDED_MANIFEST_SIZE_BYTES,
        morphology_bundle_bytes: qxfx0_morphology::EMBEDDED_BUNDLE_SIZE_BYTES,
        pack_set_fingerprint: qxfx0_semantic::active_pack_set().fingerprint().into(),
    })
}

/// Render every admitted topic in an isolated session and quantify repeated
/// response structure. Topic words at the start are replaced by `<topic>` so
/// opening diversity is not inflated merely by substituting the subject.
pub fn run_renderer_diversity_audit(
    opening_ngram_words: usize,
) -> Result<RendererDiversityReport, String> {
    if !(1..=12).contains(&opening_ngram_words) {
        return Err("opening n-gram size must be between 1 and 12 words".into());
    }
    let registry = qxfx0_semantic::argued_topic_registry().map_err(str::to_owned)?;
    let mut topics = Vec::new();
    let mut response_digests = BTreeSet::new();
    let mut sentence_frequencies = BTreeMap::<String, usize>::new();
    let mut opening_frequencies = BTreeMap::<String, usize>::new();
    let mut ready_plans = 0;
    let mut blocked_topics = Vec::new();
    let mut total_sentences = 0;

    for (index, admitted) in registry.topics().enumerate() {
        let topic = admitted.topic().as_str();
        let session_id = format!("__renderer_audit_{index}__");
        let input = TurnInput {
            raw_text: format!("что такое {topic}?"),
            session_id: session_id.clone(),
        };
        let mut state = fresh_state(&session_id);
        let (output, trace) = process_turn_with_trace(&input, &mut state);
        let ready = trace.steps.iter().any(|step| {
            step.stage == "plan_shadow"
                && step.metadata.get("plan_outcome").map(String::as_str) == Some("ready")
        });
        if ready {
            ready_plans += 1;
        }
        if output.blocked {
            blocked_topics.push(topic.to_string());
        }

        let response_digest =
            qxfx0_pipeline::execution_trace::calculate_stable_digest(&output.response)
                .map_err(|error| error.to_string())?;
        response_digests.insert(response_digest.clone());
        let sentences = normalized_sentences(&output.response);
        total_sentences += sentences.len();
        for sentence in &sentences {
            *sentence_frequencies.entry(sentence.clone()).or_default() += 1;
        }
        let normalized_opening = normalized_opening(&output.response, topic, opening_ngram_words);
        *opening_frequencies
            .entry(normalized_opening.clone())
            .or_default() += 1;
        topics.push(TopicRenderMeasurement {
            topic: topic.into(),
            ready,
            blocked: output.blocked,
            response_digest,
            sentence_count: sentences.len(),
            normalized_opening,
        });
    }

    let repeated_sentence_kinds = sentence_frequencies
        .values()
        .filter(|count| **count > 1)
        .count();
    let repeated_sentence_occurrences = sentence_frequencies
        .values()
        .map(|count| count.saturating_sub(1))
        .sum();

    Ok(RendererDiversityReport {
        schema_version: 1,
        audited_topics: topics.len(),
        ready_plans,
        blocked_topics,
        unique_responses: response_digests.len(),
        total_sentences,
        unique_sentences: sentence_frequencies.len(),
        repeated_sentence_kinds,
        repeated_sentence_occurrences,
        max_repeated_sentence: most_repeated(&sentence_frequencies),
        opening_ngram_words,
        unique_normalized_openings: opening_frequencies.len(),
        max_repeated_normalized_opening: most_repeated(&opening_frequencies),
        topics,
    })
}

fn run_measured_turn(session_id: &str) -> Result<(), String> {
    let mut state = fresh_state(session_id);
    let input = TurnInput {
        raw_text: BENCHMARK_INPUT.into(),
        session_id: session_id.into(),
    };
    let output = process_turn(&input, &mut state);
    if output.blocked || output.response.trim().is_empty() {
        return Err("benchmark turn did not produce an admissible response".into());
    }
    let violations = state.validate();
    if !violations.is_empty() {
        return Err(format!(
            "benchmark turn produced invalid state: {}",
            violations.join("; ")
        ));
    }
    Ok(())
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn resident_set_size_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_linux_rss_bytes(&status)
}

fn parse_linux_rss_bytes(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kibibytes = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    kibibytes.checked_mul(1024)
}

fn normalized_sentences(response: &str) -> Vec<String> {
    response
        .split_inclusive(['.', '!', '?'])
        .map(|sentence| {
            sentence
                .trim()
                .trim_end_matches(['.', '!', '?'])
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

fn normalized_opening(response: &str, topic: &str, word_count: usize) -> String {
    let response_words = words(response);
    let topic_words = words(topic);
    let mut normalized = Vec::new();
    let topic_is_prefix = !topic_words.is_empty()
        && response_words
            .iter()
            .take(topic_words.len())
            .eq(topic_words.iter());
    let start = if topic_is_prefix {
        normalized.push("<topic>".to_string());
        topic_words.len()
    } else {
        0
    };
    normalized.extend(response_words.into_iter().skip(start));
    normalized
        .into_iter()
        .take(word_count)
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn most_repeated(frequencies: &BTreeMap<String, usize>) -> Option<RepeatedText> {
    frequencies
        .iter()
        .max_by(|(left_text, left_count), (right_text, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_text.cmp(left_text))
        })
        .map(|(text, count)| RepeatedText {
            text: text.clone(),
            count: *count,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_distribution_uses_nearest_rank_percentiles() {
        let distribution = LatencyDistributionMicros::from_samples(vec![50, 10, 40, 20, 30]);
        assert_eq!(distribution.min, 10);
        assert_eq!(distribution.p50, 30);
        assert_eq!(distribution.p95, 50);
        assert_eq!(distribution.max, 50);
        assert_eq!(distribution.mean, 30);
    }

    #[test]
    fn parses_linux_resident_set_size() {
        assert_eq!(
            parse_linux_rss_bytes("Name:\tqxfx0\nVmRSS:\t   1234 kB\n"),
            Some(1_263_616)
        );
    }

    #[test]
    fn renderer_opening_replaces_the_topic_prefix() {
        assert_eq!(
            normalized_opening("Свобода предполагает возможность выбора.", "свобода", 3),
            "<topic> предполагает возможность"
        );
    }

    #[test]
    fn renderer_audit_covers_every_admitted_topic() {
        let report = run_renderer_diversity_audit(3).unwrap();
        assert_eq!(report.audited_topics, 30);
        assert_eq!(report.ready_plans, 30);
        assert!(report.blocked_topics.is_empty());
        assert_eq!(report.topics.len(), 30);
        assert_eq!(report.unique_responses, 30);
    }
}
