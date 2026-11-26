//! ADR-MEM-220: the semantic index a host may consult. Runtime implements it
//! with a local model and its own vector cache; Memory never runs a model or
//! opens a connection.
use enouia_memory_contract::next::{RecallQuery, SemanticVector, TargetRef};

pub trait SemanticIndex: Send + Sync {
    /// The model and version every vector of this index comes from.
    fn model(&self) -> &str;
    /// The query's vector, or `None` to recall lexically (no model, model
    /// not running).
    fn query(&self, text: &str) -> Option<SemanticVector>;
    /// A memory revision's vector, or `None` while it is not indexed.
    fn memory(&self, target: &TargetRef) -> Option<SemanticVector>;
}

/// The query's vector and each target's, when the index answers for this
/// query. Vectors of another model or size are dropped.
pub(crate) fn vectors(
    index: &dyn SemanticIndex,
    query: &RecallQuery,
    targets: &[&TargetRef],
) -> Option<(SemanticVector, Vec<Option<SemanticVector>>)> {
    if query.text.trim().is_empty() {
        return None;
    }
    let wanted = index.model();
    let q = index
        .query(&query.text)
        .filter(|v| v.valid() && v.model == wanted)?;
    let memories = targets
        .iter()
        .map(|t| {
            index
                .memory(t)
                .filter(|v| v.model == q.model && v.values.len() == q.values.len())
        })
        .collect();
    Some((q, memories))
}
