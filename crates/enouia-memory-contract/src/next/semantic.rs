//! ADR-MEM-220: semantic vectors produced by a local model outside Memory.
//! Memory only stores the format, compares vectors and scores them; it never
//! runs a model or opens a connection.
use serde::{Deserialize, Serialize};

/// Largest vector accepted, in dimensions.
pub const MAX_SEMANTIC_DIMS: usize = 4096;
/// ADR-MEM-221: absolute thresholds, used only when a distribution has fewer
/// than `SEMANTIC_MIN_SAMPLE` similarities (or none spread): similarity in
/// thousandths of the cosine from which a candidate gains score, and from
/// which it is cued alone; score per thousandth above the floor.
pub const SEMANTIC_FLOOR_MILLI: i64 = 550;
pub const SEMANTIC_CUE_MILLI: i64 = 650;
pub const SEMANTIC_SCALE: i64 = 40;
/// ADR-MEM-221: on a large enough distribution, similarity is judged against
/// the query's own (or the anchor's own) similarities, in hundredths of a
/// standard deviation above their mean. Runtime's real-library check with
/// bge-m3 (median 385, p99 560, max 648) showed no fixed threshold separates
/// relevant from irrelevant pairs, while relevant ones stand out from their
/// own distribution.
pub const SEMANTIC_MIN_SAMPLE: usize = 30;
/// Score starts 2.0 deviations above the mean.
pub const SEMANTIC_BONUS_Z100: i64 = 200;
/// Cued alone (or a neighbor) from 3.0 deviations above the mean.
pub const SEMANTIC_CUE_Z100: i64 = 300;
/// Score per hundredth of a deviation above `SEMANTIC_BONUS_Z100`: 3.0
/// deviations add 6000, about one strong lexical word and a half.
pub const SEMANTIC_Z_SCALE: i64 = 60;
/// Below this similarity nothing counts, however it stands out. ADR-MEM-222:
/// 530, from Runtime's real library, where two irrelevant tea answers stood
/// 3.01 deviations out at 517 while the lowest relevant cue was 537.
pub const SEMANTIC_GUARD_MILLI: i64 = 530;
/// ADR-MEM-222: the bonus stops growing past 4.0 deviations, so one memory
/// close in meaning (6.3 deviations on the real library) cannot outrank the
/// answer on similarity alone; words and attention still decide among them.
pub const SEMANTIC_BONUS_CAP_Z100: i64 = 400;
/// Semantic neighbors listed per memory.
pub const MAX_SEMANTIC_NEIGHBORS: usize = 5;
/// ADR-MEM-223: the System graph compares every pair of its selected nodes'
/// vectors, so it lists semantic neighbors only up to this many vectors
/// (about two million pairs); above it the graph reports them unavailable.
pub const MAX_SEMANTIC_GRAPH_VECTORS: usize = 2000;
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

/// ADR-MEM-221: the spread of one query's (or one anchor's) similarities over
/// the candidates that have vectors, in integers so every platform agrees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticStats {
    pub count: usize,
    pub mean: i64,
    pub deviation: i64,
}

impl SemanticStats {
    pub fn of(similarities: &[i64]) -> Self {
        let count = similarities.len();
        if count == 0 {
            return Self {
                count,
                mean: 0,
                deviation: 0,
            };
        }
        let n = count as i64;
        let mean = similarities.iter().sum::<i64>() / n;
        let variance = similarities.iter().map(|s| (s - mean).pow(2)).sum::<i64>() / n;
        Self {
            count,
            mean,
            deviation: (variance.max(0) as u64).isqrt() as i64,
        }
    }
    /// The recall score a similarity adds, and whether it cues alone.
    pub fn score(&self, similarity: i64) -> (u64, bool) {
        if self.count < SEMANTIC_MIN_SAMPLE || self.deviation == 0 {
            let bonus = (similarity - SEMANTIC_FLOOR_MILLI).max(0) * SEMANTIC_SCALE;
            return (bonus as u64, similarity >= SEMANTIC_CUE_MILLI);
        }
        if similarity < SEMANTIC_GUARD_MILLI {
            return (0, false);
        }
        let z = (similarity - self.mean) * 100 / self.deviation;
        let bonus =
            (z.min(SEMANTIC_BONUS_CAP_Z100) - SEMANTIC_BONUS_Z100).max(0) * SEMANTIC_Z_SCALE;
        (bonus as u64, z >= SEMANTIC_CUE_Z100)
    }
}
