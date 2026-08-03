use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable identity of an immutable fact in the active knowledge-pack set.
/// The record and its authority remain process-global; session state may keep
/// only this typed reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FactId(pub String);

impl FactId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, FactIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(FactIdError::Empty)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactIdError {
    #[error("fact id must not be empty")]
    Empty,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_id_rejects_empty_identity() {
        assert_eq!(FactId::try_new("  "), Err(FactIdError::Empty));
        assert_eq!(
            FactId::try_new("fact.freedom").unwrap().as_str(),
            "fact.freedom"
        );
    }
}
