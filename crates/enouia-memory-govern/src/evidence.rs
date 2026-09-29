//! Evidence citations resolved against the Vault. The evidence class, the
//! anchor object, and the default locator come from the cited source
//! revision itself: a proposer names which source revision and claim, and
//! cannot declare a model statement to be an owner statement (M04).

use crate::util::{Result, invalid, revision};
use enouia_memory_contract::common::{EvidenceRef, Locator, Sensitivity};
use enouia_memory_contract::ids::SourceId;
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::SourceRecord;
use enouia_memory_vault::Vault;

/// What a proposer cites: a source revision, optionally a narrower place in
/// it, and which claim of the proposal it supports (`content` or an item ID).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSpec {
    pub source_id: SourceId,
    pub source_revision: Revision,
    pub supports: String,
    pub locator: Option<Locator>,
}

impl EvidenceSpec {
    /// Cite a whole source revision for the main statement.
    pub fn content(source_id: SourceId, source_revision: Revision) -> Self {
        Self {
            source_id,
            source_revision,
            supports: "content".to_owned(),
            locator: None,
        }
    }
}

/// Resolved citations plus the cited sources, in order.
pub struct Resolved {
    pub evidence: Vec<EvidenceRef>,
    pub sources: Vec<SourceRecord>,
}

impl Resolved {
    /// The strictest sensitivity of the cited sources.
    pub fn strictest(&self) -> Sensitivity {
        self.sources
            .iter()
            .map(|s| s.sensitivity)
            .max()
            .unwrap_or(Sensitivity::Public)
    }
}

pub fn resolve(vault: &Vault, pin: &CommitPin, specs: &[EvidenceSpec]) -> Result<Resolved> {
    if specs.is_empty() {
        return Err(invalid("candidate.evidence_required"));
    }
    let mut evidence = Vec::new();
    let mut sources = Vec::new();
    for spec in specs {
        let source: SourceRecord = revision(
            vault,
            pin,
            RecordKind::Source,
            spec.source_id.as_str(),
            spec.source_revision,
        )?
        .ok_or_else(|| invalid("evidence.unresolved"))?;
        let locator = match &spec.locator {
            Some(locator) if !locator.within(&source.locator) => {
                return Err(invalid("evidence.locator_mismatch"));
            }
            Some(locator) => locator.clone(),
            None => source.locator.clone(),
        };
        evidence.push(EvidenceRef {
            source_id: spec.source_id.clone(),
            source_revision: spec.source_revision,
            locator,
            object_hash: source.anchor_hash().clone(),
            evidence_class: source.evidence_class,
            supports: spec.supports.clone(),
        });
        sources.push(source);
    }
    Ok(Resolved { evidence, sources })
}
