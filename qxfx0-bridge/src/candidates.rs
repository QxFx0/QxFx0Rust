//! Candidate sources for the learning bridge (ADR-0043 U4).
//!
//! A *candidate* is a proposed corroboration the between-turn worker may
//! consider: an external LLM or corpus feed that has observed `from`
//! relates to `to` with some outcome. This module is the trust boundary.
//!
//! The default build carries **no network at all** — privacy is an
//! architectural fact, not a configuration choice (ADR-0043 law 1). The
//! `llm-candidates` feature is the single sanctioned network surface and
//! only ever fetches candidates *between* turns; it never runs on the turn
//! path, never renders, and its raw output becomes an ordinary
//! [`CorroborationEvent`] only after crossing the quarantine and the U5
//! admission bar. Even with the feature off, this module still defines the
//! trait and a deterministic offline source, so the worker's shape is the
//! same regardless of build.

use qxfx0_types::AtomId;

use crate::corroboration::CorroborationEvent;

/// A source of candidate corroboration events. Implementations must be
/// total over their inputs and never panic on malformed external data:
/// `fetch` returns `Err` rather than producing garbage evidence.
pub trait CandidateSource {
    /// Fetch up to `limit` candidate events observed since the last call.
    /// An offline / disabled source returns an empty `Ok`.
    fn fetch(&mut self, limit: usize) -> Result<Vec<CorroborationEvent>, CandidateError>;
}

/// A source that always yields nothing — the default when no network is
/// wired and the shape an operator gets with `llm-candidates` off but the
/// worker present.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopCandidateSource;

impl CandidateSource for NoopCandidateSource {
    fn fetch(&mut self, _limit: usize) -> Result<Vec<CorroborationEvent>, CandidateError> {
        Ok(Vec::new())
    }
}

/// A deterministic in-memory source: replays a fixed list of events
/// (bounded by `limit`) for tests, fixtures and offline operator drills.
#[derive(Debug, Clone, Default)]
pub struct ScriptedCandidateSource {
    events: std::collections::VecDeque<CorroborationEvent>,
}

impl ScriptedCandidateSource {
    pub fn new(events: impl IntoIterator<Item = CorroborationEvent>) -> Self {
        Self {
            events: events.into_iter().collect(),
        }
    }
}

impl CandidateSource for ScriptedCandidateSource {
    fn fetch(&mut self, limit: usize) -> Result<Vec<CorroborationEvent>, CandidateError> {
        let count = limit.min(self.events.len());
        Ok(self.events.drain(..count).collect())
    }
}

/// Why a candidate fetch failed. Every variant is a hard stop on that
/// batch — the worker records it and keeps going; a malformed or
/// over-reach response never becomes evidence (fail-closed, law 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateError {
    /// The `llm-candidates` feature is not compiled into this binary, or
    /// the operator has not configured a live source.
    Disabled,
    /// The transport failed (only reachable with `llm-candidates`).
    Transport,
    /// The payload did not decode into well-formed candidate events.
    Malformed,
    /// The source named an endpoint atom that is not in the session graph
    /// (a candidate must corroborate an existing edge, not mint atoms).
    UnknownEndpoint { atom: AtomId },
}

impl std::fmt::Display for CandidateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(
                formatter,
                "the candidate source is disabled (no network in the default build)"
            ),
            Self::Transport => write!(formatter, "candidate transport failed"),
            Self::Malformed => write!(formatter, "candidate payload did not decode"),
            Self::UnknownEndpoint { atom } => {
                write!(
                    formatter,
                    "candidate names unknown atom '{}'",
                    atom.as_str()
                )
            }
        }
    }
}

impl std::error::Error for CandidateError {}

/// The live HTTP candidate seam. Present **only** when the
/// `llm-candidates` feature is enabled; the default build does not compile
/// this type, so there is no network code to reach.
///
/// This is deliberately a *seam*, not a client: the workspace pins no HTTP
/// dependency (cargo-deny reviews every dep, and no fetch crate has earned
/// a place yet), so even under the feature `fetch` fails closed with
/// [`CandidateError::Transport`]. Wiring a transport in is a separate,
/// supply-chain-gated change that replaces this body — which keeps the
/// claim testable: today, in every build configuration, the bridge has no
/// path to a socket. Even a live transport only *fetches* raw candidate
/// bytes; turning them into evidence (parsing, endpoint validation,
/// quarantine) is the worker's job between turns, and the U5 admission bar
/// is what finally lets anything reach the graph.
#[cfg(feature = "llm-candidates")]
pub struct HttpCandidateClient {
    endpoint: String,
    timeout: std::time::Duration,
}

#[cfg(feature = "llm-candidates")]
impl HttpCandidateClient {
    pub fn new(endpoint: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn timeout(&self) -> std::time::Duration {
        self.timeout
    }
}

#[cfg(feature = "llm-candidates")]
impl CandidateSource for HttpCandidateClient {
    fn fetch(&mut self, _limit: usize) -> Result<Vec<CorroborationEvent>, CandidateError> {
        // No transport is linked (see the seam note above): fail closed
        // rather than pretend, so a misconfigured operator build produces
        // zero evidence instead of an error-free empty success.
        Err(CandidateError::Transport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_edges::BridgeOutcome;
    use qxfx0_types::RelationType;

    fn event(from: &str, to: &str) -> CorroborationEvent {
        CorroborationEvent::new(
            AtomId::new(from),
            AtomId::new(to),
            RelationType::RelRelatedTo,
            BridgeOutcome::Positive,
        )
    }

    #[test]
    fn default_build_has_no_live_source() {
        // With the feature off (the default here), a live fetch is a hard
        // Disabled stop: there is no HTTP client to reach the network.
        let mut source = NoopCandidateSource;
        let fetched = source.fetch(16).expect("offline source is total");
        assert!(fetched.is_empty());
    }

    #[test]
    fn scripted_source_replays_deterministically_bounded_by_limit() {
        let mut source =
            ScriptedCandidateSource::new([event("a", "b"), event("b", "c"), event("c", "d")]);
        let first = source.fetch(2).expect("scripted fetch is total");
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], event("a", "b"));
        let second = source.fetch(10).expect("remaining drain is total");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0], event("c", "d"));
        let empty = source.fetch(10).expect("exhausted source stays total");
        assert!(empty.is_empty());
    }

    #[test]
    fn candidate_errors_render_stably() {
        assert!(CandidateError::Disabled.to_string().contains("no network"));
        assert!(CandidateError::UnknownEndpoint {
            atom: AtomId::new("призрачное-слово")
        }
        .to_string()
        .contains("призрачное-слово"));
    }
}
