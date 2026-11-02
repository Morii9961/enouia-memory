//! Shared disposable projection metadata, never a persisted permission grant.
use super::*;
use crate::{common::SourceRevisionRef, error::Violation, hash::Sha256Hex, json::Revision};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProjectionKind {
    #[serde(rename = "projection_metadata")]
    ProjectionMetadata,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCoverage {
    pub scope: String,
    pub complete_in_scope: bool,
    pub authorized_inputs: u32,
    pub included_inputs: u32,
    pub limits: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionMetadata {
    pub schema_version: NextVersion,
    pub kind: ProjectionKind,
    pub algorithm: String,
    pub version: Revision,
    pub snapshot: SnapshotBinding,
    pub withdrawal_watermark: Revision,
    pub scope_hash: Sha256Hex,
    pub coverage: ProjectionCoverage,
    pub source_dependencies: Vec<SourceRevisionRef>,
    pub memory_dependencies: Vec<TargetRef>,
}
impl NextDocument for ProjectionMetadata {
    fn validate(&self) -> Vec<Violation> {
        let c = &self.coverage;
        if !record_fields::text(&self.algorithm, 128)
            || !record_fields::text(&c.scope, 256)
            || c.included_inputs > c.authorized_inputs
            || (c.complete_in_scope && c.included_inputs != c.authorized_inputs)
            || c.limits.len() > 16
            || c.limits.iter().any(|s| !record_fields::text(s, 128))
            || self.source_dependencies.len() > 128
            || self.memory_dependencies.len() > 128
            || self
                .source_dependencies
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.source_dependencies.len()
            || self
                .memory_dependencies
                .iter()
                .map(|r| (&r.memory.memory_id, r.memory.revision))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.memory_dependencies.len()
        {
            vec![Violation::new("ui.projection_metadata", "")]
        } else {
            vec![]
        }
    }
}
impl ProjectionMetadata {
    /// Still requires live dependency/ACL/lineage checks by the native reader.
    pub fn matches_current(
        &self,
        snapshot: &SnapshotBinding,
        withdrawal: Revision,
        scope_hash: &Sha256Hex,
        algorithm: &str,
        version: Revision,
    ) -> bool {
        self.validate().is_empty()
            && &self.snapshot == snapshot
            && self.withdrawal_watermark == withdrawal
            && &self.scope_hash == scope_hash
            && self.algorithm == algorithm
            && self.version == version
    }
}
