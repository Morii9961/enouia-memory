//! ADR-MEM-220: semantic vectors produced by a local model outside Memory.
//! Memory only stores the format, compares vectors and scores them; it never
//! runs a model or opens a connection.
use serde::{Deserialize, Serialize};

/// Largest vector accepted, in dimensions.
pub const MAX_SEMANTIC_DIMS: usize = 4096;
/// Similarity (thousandths of the cosine) from which a candidate gains
/// recall score. Tuned on Runtime's real-library checks.
pub const SEMANTIC_FLOOR_MILLI: i64 = 550;
/// Similarity from which a candidate is cued with no word in common, and
/// from which another memory is a semantic neighbor.
pub const SEMANTIC_CUE_MILLI: i64 = 650;
/// Recall score per thousandth above the floor.
pub const SEMANTIC_SCALE: i64 = 40;
/// Semantic neighbors listed per memory.
pub const MAX_SEMANTIC_NEIGHBORS: usize = 5;
const UNIT: i64 = 32767;

/// An L2-normalised vector scaled to `i16`, named by the model that made it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticVector {
    pub model: String,
    pub values: Vec<i16>,
}

impl SemanticVector {
    /// Normalise and quantise model output. Refuses an empty or overlong
    /// model name, an empty, oversized, non-finite or zero vector.
    pub fn from_f32(model: &str, raw: &[f32]) -> Option<Self> {
        if model.is_empty()
            || model.chars().count() > 128
            || raw.is_empty()
            || raw.len() > MAX_SEMANTIC_DIMS
            || raw.iter().any(|v| !v.is_finite())
        {
            return None;
        }
        let norm = raw
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        if norm == 0.0 || !norm.is_finite() {
            return None;
        }
        let values = raw
            .iter()
            .map(|v| (f64::from(*v) / norm * UNIT as f64).round() as i16)
            .collect();
        Some(Self {
            model: model.to_owned(),
            values,
        })
    }
    pub fn valid(&self) -> bool {
        !self.model.is_empty()
            && self.model.chars().count() <= 128
            && !self.values.is_empty()
            && self.values.len() <= MAX_SEMANTIC_DIMS
    }
}

/// Cosine similarity in thousandths, in integers so every platform agrees.
/// Vectors of different models or sizes have none.
pub fn semantic_milli(a: &SemanticVector, b: &SemanticVector) -> Option<i64> {
    if !a.valid() || !b.valid() || a.model != b.model || a.values.len() != b.values.len() {
        return None;
    }
    let dot: i64 = a
        .values
        .iter()
        .zip(&b.values)
        .map(|(x, y)| i64::from(*x) * i64::from(*y))
        .sum();
    Some(dot * 1000 / (UNIT * UNIT))
}

/// The query's vector and one optional vector per recall candidate, in the
/// candidates' order.
#[derive(Clone, Copy, Debug)]
pub struct SemanticInput<'a> {
    pub query: &'a SemanticVector,
    pub candidates: &'a [Option<&'a SemanticVector>],
}
