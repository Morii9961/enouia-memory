//! Frozen fields after the separate Agent authorization gate, not System reads.
use super::*;
use enouia_memory_contract::common::SourceRevisionRef;
use enouia_memory_vault::automation::AuthorizedReadData;
use std::collections::BTreeSet;

#[derive(Clone, Serialize)]
pub struct AgentRecordProjection {
    pub memory: MemoryRevisionRef,
    pub producer_version: String,
    pub model_version: Option<String>,
    pub fields: MemoryRecordFields,
    pub entities: Vec<EntityRecord>,
}
#[derive(Clone, Serialize)]
pub struct AgentSourceRecordProjection {
    pub source: SourceRevisionRef,
    pub fields: SourceRecordFields,
}
pub(super) fn projections(
    data: &AuthorizedReadData,
) -> ApplicationResult<Option<Vec<AgentRecordProjection>>> {
    let Some(catalog) = &data.record_catalog else {
        return Ok(None);
    };
    let mut seen = BTreeSet::new();
    data.current
        .iter()
        .chain(&data.history)
        .filter(|m| seen.insert((m.memory.memory_id.clone(), m.memory.revision)))
        .map(|m| {
            let fields = m
                .decision
                .input
                .record_fields
                .clone()
                .ok_or(ApplicationFault::Invalid)?;
            let mut ids = BTreeSet::new();
            let entities = fields
                .entities
                .iter()
                .filter(|e| ids.insert(e.entity_id.clone()))
                .map(|mention| {
                    catalog
                        .entities
                        .iter()
                        .rev()
                        .find(|e| e.entity_id == mention.entity_id)
                        .cloned()
                        .ok_or(ApplicationFault::Invalid)
                })
                .collect::<ApplicationResult<Vec<_>>>()?;
            Ok(AgentRecordProjection {
                memory: m.memory.clone(),
                producer_version: m.decision.input.producer_version.clone(),
                model_version: m.decision.input.model_version.clone(),
                fields,
                entities,
            })
        })
        .collect::<ApplicationResult<Vec<_>>>()
        .map(Some)
}
impl AgentReader<'_> {
    pub(super) fn finish_page(
        &self,
        mut page: AgentPage,
        hash: Sha256Hex,
        max_bytes: usize,
    ) -> ApplicationResult<AgentPage> {
        if let Some(fields) = &mut page.record_fields {
            fields.retain(|v| page.items.iter().any(|i| i.memory == v.memory));
        }
        while canonical_bytes(&page)
            .map_err(|_| ApplicationFault::Invalid)?
            .len()
            > max_bytes
        {
            page.items.pop();
            let last = page.items.last().ok_or(ApplicationFault::Invalid)?;
            if let Some(fields) = &mut page.record_fields {
                fields.retain(|v| page.items.iter().any(|i| i.memory == v.memory));
            }
            page.next = Some(MemoryReadCursor {
                snapshot: page.snapshot.clone(),
                filter_hash: hash.clone(),
                after: last.memory.memory_id.clone(),
            });
            page.complete_in_scope = false;
        }
        self.finish(page, max_bytes)
    }
}
