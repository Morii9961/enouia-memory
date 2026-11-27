//! ADR-MEM-220: the semantic index a host may consult. Runtime implements it
//! with a local model and its own vector cache. The remote connector has its
//! own loopback Ollama index (ADR-MEM-229, `mcp::remote::semantic`); the
//! readers and the contract never run a model or open a connection.
use enouia_memory_contract::next::{RecallQuery, SemanticVector, TargetRef};

/// One index may be shared by every host and reader of a process
/// (`Arc<dyn SemanticIndex>`). Memory calls it synchronously, inside the
/// read that authorized the candidates.
pub trait SemanticIndex: Send + Sync {
    /// The model and version every vector of this index comes from.
    fn model(&self) -> &str;
    /// The query's vector, or `None` to recall lexically (no model, model
    /// not running, timed out). Called at most once per recall or
    /// `session_start`, and never by `related`. It may block for the model,
    /// so the implementation should bound it (Runtime: about 1.5 s).
    fn query(&self, text: &str) -> Option<SemanticVector>;
    /// A memory revision's vector, or `None` while it is not indexed. Called
    /// once per authorized candidate of a recall, and once per memory in
    /// scope by `related`, so it must not block: answer from a cache and
    /// return `None` on a miss. Computing missing vectors belongs elsewhere
    /// (Runtime embeds `SegmentedMemoryReader::semantic_texts` in batches in
    /// the background).
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
