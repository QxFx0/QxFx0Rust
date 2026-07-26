//! SemanticNetwork types — shared between semantic and state layers.
//!
//! Keeping these types in `qxfx0-types` lets `SystemState` cache a built
//! network without introducing a dependency on `qxfx0-semantic`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::atom::AtomId;

/// Source of a semantic edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeSource {
    ExplicitEdge,
    SubstrateEdge,
}

/// A weighted directed edge in the semantic network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEdge {
    pub from: AtomId,
    pub to: AtomId,
    pub weight: f64,
    pub co_occurrence: usize,
    pub source: EdgeSource,
}

/// A single step in the spreading activation trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationStep {
    pub node: AtomId,
    pub source: EdgeSource,
    pub via: AtomId,
    pub hop: usize,
    pub weight: f64,
}

/// The semantic network — nodes and weighted edges with activation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticNetwork {
    pub nodes: BTreeSet<AtomId>,
    pub edges: BTreeMap<(AtomId, AtomId), SemanticEdge>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub activation: BTreeMap<AtomId, f64>,
    #[serde(default)]
    pub decay_rate: f64,
    #[serde(default)]
    pub max_hops: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activation_log: Vec<ActivationStep>,
}

impl Default for SemanticNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticNetwork {
    pub fn new() -> Self {
        SemanticNetwork {
            nodes: BTreeSet::new(),
            edges: BTreeMap::new(),
            activation: BTreeMap::new(),
            decay_rate: 0.5,
            max_hops: 3,
            activation_log: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
