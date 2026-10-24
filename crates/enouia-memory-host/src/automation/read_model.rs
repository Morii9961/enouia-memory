//! Memory-region native read ports; no v1 IPC or Runtime shell changes.
use super::application::{ApplicationFault, ApplicationResult, AutomationHost, render_memory};
use super::*;
use enouia_memory_contract::{common::Sensitivity, ids::MemoryId};
use enouia_memory_vault::automation::AuthorizedReadData;
use std::collections::BTreeSet;

fn item(
    data: &AuthorizedReadData,
    m: &MachineMemory,
    profile: &AttentionProfile,
) -> ApplicationResult<MemoryReadItem> {
    let input = &m.decision.input;
    let source_captured_at = input
        .input_refs
        .iter()
        .filter_map(|r| {
            data.sources
                .iter()
                .find(|s| s.reference() == *r)
                .map(|s| s.record.captured_at.clone())
        })
        .max()
        .ok_or(ApplicationFault::Invalid)?;
    Ok(MemoryReadItem {
        memory: m.memory.clone(),
        system_actor: input.actor.clone(),
        subject_id: input.subject_id.clone(),
        project_id: input.project_id.clone(),
        content: input.content.clone(),
        knowledge: m.decision.outcome.knowledge,
        adoption: m.decision.outcome.adoption,
        dimensions: input.dimensions.clone().ok_or(ApplicationFault::Invalid)?,
        policy: input.policy.clone(),
        rule_version: m.decision.rule_version,
        committed_in: m.committed_in.clone(),
        learned_at: data
            .learned_times
            .get(&m.committed_in)
            .cloned()
            .ok_or(ApplicationFault::Invalid)?,
        attention: attention_state(profile, m, &source_captured_at, &data.now),
        source_captured_at,
        validity: memory_validity(input.dimensions.as_ref().unwrap(), &data.now),
        evidence: input.evidence.clone(),
        context_source_refs: input.context_source_refs.clone(),
        receipt: Some(
            data.receipts
                .iter()
                .find(|r| r.result == m.memory)
                .cloned()
                .ok_or(ApplicationFault::Invalid)?,
        ),
    })
}
fn size(value: &impl serde::Serialize) -> ApplicationResult<usize> {
    Ok(canonical_bytes(value)
        .map_err(|_| ApplicationFault::Invalid)?
        .len())
}
fn matches_filter(m: &MachineMemory, filter: &MemoryReadFilter) -> bool {
    let d = m.decision.input.dimensions.as_ref().unwrap();
    filter.form.is_none_or(|form| d.kind == form)
        && filter
            .adoption
            .is_none_or(|a| m.decision.outcome.adoption == a)
        && filter.event_window.as_ref().is_none_or(|w| {
            d.event_time
                .as_ref()
                .map_or(filter.include_unknown_event_time, |t| {
                    if let Some(end) = &t.until {
                        end > &w.from && w.until.as_ref().is_none_or(|e| &t.from < e)
                    } else {
                        t.from >= w.from && w.until.as_ref().is_none_or(|e| &t.from < e)
                    }
                })
        })
}
impl AutomationHost {
    pub fn memory_page_native(
        &self,
        profile: &AttentionProfile,
        filter: &MemoryReadFilter,
        cursor: Option<&MemoryReadCursor>,
        limit: usize,
        max_bytes: usize,
        max: Sensitivity,
    ) -> ApplicationResult<MemoryReadPage> {
        let _gate = self.gate()?;
        if !profile.validate().is_empty()
            || !(1..=32).contains(&limit)
            || !(1024..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
            || filter
                .event_window
                .as_ref()
                .is_some_and(|t| t.until.as_ref().is_some_and(|end| end <= &t.from))
        {
            return Err(ApplicationFault::Invalid);
        }
        let data = self.vault.authorized_read_data(&self.system, max, None)?;
        let filter_hash = digest(&(filter, profile));
        if cursor.is_some_and(|c| c.snapshot != data.snapshot || c.filter_hash != filter_hash) {
            return Err(enouia_memory_vault::automation::AutomationFault::Stale.into());
        }
        let matches: Vec<_> = data
            .current
            .iter()
            .filter(|m| matches_filter(m, filter))
            .collect();
        let unknown = data
            .current
            .iter()
            .filter(|m| {
                filter
                    .form
                    .is_none_or(|f| m.decision.input.dimensions.as_ref().unwrap().kind == f)
                    && filter
                        .adoption
                        .is_none_or(|a| m.decision.outcome.adoption == a)
                    && m.decision
                        .input
                        .dimensions
                        .as_ref()
                        .unwrap()
                        .event_time
                        .is_none()
            })
            .count();
        let remaining: Vec<_> = matches
            .iter()
            .filter(|m| cursor.is_none_or(|c| m.memory.memory_id > c.after))
            .collect();
        let mut page = MemoryReadPage {
            schema_version: NextVersion,
            kind: MemoryPageKind::Page,
            capability: MemoryReadCapability::V1,
            effect: MemoryReadEffect::NativeRead,
            capabilities: MemoryReadCapabilities::native(),
            snapshot: data.snapshot.clone(),
            authorized_at: data.now.clone(),
            profile_hash: digest(profile),
            items: vec![],
            matched_in_scope: matches.len() as u64,
            unknown_event_time_in_scope: unknown as u64,
            complete_in_scope: true,
            next: None,
            maintenance_worker_alive: self.maintenance_running.load(Ordering::SeqCst),
        };
        for m in remaining.iter().take(limit) {
            page.items.push(item(&data, m, profile)?);
            if size(&page)? > max_bytes.saturating_sub(1024) {
                page.items.pop();
                break;
            }
        }
        page.complete_in_scope = page.items.len() == remaining.len();
        if !page.complete_in_scope {
            page.next = page.items.last().map(|i| MemoryReadCursor {
                snapshot: data.snapshot,
                filter_hash,
                after: i.memory.memory_id.clone(),
            });
            if page.items.is_empty() {
                return Err(ApplicationFault::Invalid);
            }
        }
        if size(&page)? > max_bytes || !page.validate().is_empty() {
            return Err(ApplicationFault::Invalid);
        }
        Ok(page)
    }
    pub fn memory_detail_native(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        include_source_text: bool,
        max_bytes: usize,
        max: Sensitivity,
    ) -> ApplicationResult<Option<MemoryReadDetail>> {
        let _gate = self.gate()?;
        if !profile.validate().is_empty() || !(1024..=MAX_DOCUMENT_BYTES).contains(&max_bytes) {
            return Err(ApplicationFault::Invalid);
        }
        let data = self
            .vault
            .authorized_read_data(&self.system, max, Some(id))?;
        if expected.is_some_and(|s| s != &data.snapshot) {
            return Err(enouia_memory_vault::automation::AutomationFault::Stale.into());
        }
        let Some(current) = data.current.iter().find(|m| &m.memory.memory_id == id) else {
            return Ok(None);
        };
        let refs: BTreeSet<_> = data
            .history
            .iter()
            .flat_map(|m| m.decision.input.input_refs.iter().cloned())
            .collect();
        let peers: Vec<_> = data
            .current
            .iter()
            .chain(&data.history)
            .map(|m| m.target())
            .collect();
        let mut neighbors = Vec::new();
        for m in &data.current {
            let d = m.decision.input.dimensions.as_ref().unwrap();
            for (target, kind) in d
                .derived_from
                .iter()
                .map(|p| (p, MemoryLinkKind::DerivedFrom))
                .chain(d.reverts_to.iter().map(|p| (p, MemoryLinkKind::Reversion)))
            {
                if (&m.memory.memory_id == id || &target.memory.memory_id == id)
                    && peers.contains(target)
                {
                    neighbors.push(MemoryReadLink {
                        from: m.memory.clone(),
                        to: target.clone(),
                        kind,
                    });
                }
            }
        }
        let mut detail = MemoryReadDetail {
            schema_version: NextVersion,
            kind: MemoryDetailKind::Detail,
            capability: MemoryReadCapability::V1,
            effect: MemoryReadEffect::NativeRead,
            snapshot: data.snapshot.clone(),
            authorized_at: data.now.clone(),
            profile_hash: digest(profile),
            item: item(&data, current, profile)?,
            history: data
                .history
                .iter()
                .map(|m| item(&data, m, profile))
                .collect::<ApplicationResult<_>>()?,
            sources: data
                .sources
                .iter()
                .filter(|s| refs.contains(&s.reference()))
                .map(|s| MemorySourceView {
                    record: s.record.clone(),
                    attributed_subject: s.attributed_subject.clone(),
                    text: include_source_text.then(|| s.text.clone()),
                })
                .collect(),
            complete_in_scope: neighbors.len() <= 32,
            neighbors: neighbors.into_iter().take(32).collect(),
        };
        if size(&detail)? > max_bytes {
            for source in &mut detail.sources {
                source.text = None;
            }
            detail.complete_in_scope = false;
        }
        while size(&detail)? > max_bytes && !detail.history.is_empty() {
            detail.history.remove(0);
            detail.complete_in_scope = false;
        }
        if size(&detail)? > max_bytes || !detail.validate().is_empty() {
            return Err(ApplicationFault::Invalid);
        }
        Ok(Some(detail))
    }
    pub fn simulate_native_recall(
        &self,
        profile: &AttentionProfile,
        query: &RecallQuery,
        max: Sensitivity,
    ) -> ApplicationResult<MemoryReadSimulation> {
        let read = self.select_context_native(profile, query, max)?;
        Ok(MemoryReadSimulation {
            schema_version: NextVersion,
            kind: MemorySimulationKind::Simulation,
            capability: MemoryReadCapability::V1,
            effect: MemoryReadEffect::NativeRead,
            trace: read.trace,
            rendered_context: read.text,
        })
    }
}
/// Pure synthetic computation over caller-provided fixtures. No root, native
/// principal, credentials, writes or actual successful-commit receipt exists.
pub fn simulate_synthetic_recall(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    fixtures: &[(MachineMemory, Timestamp)],
) -> ApplicationResult<MemoryReadSimulation> {
    let rendered: Vec<_> = fixtures.iter().map(|(m, _)| render_memory(m)).collect();
    let candidates: Vec<_> = fixtures
        .iter()
        .zip(&rendered)
        .map(|((memory, time), text)| RecallCandidate {
            memory,
            source_time: time.clone(),
            rendered_bytes: text.len(),
        })
        .collect();
    let trace = select_recall(profile, query, snapshot, &query.at, &candidates)
        .map_err(|_| ApplicationFault::Invalid)?;
    let rendered_context = trace
        .selected
        .iter()
        .map(|s| {
            rendered[fixtures
                .iter()
                .position(|(m, _)| m.memory == s.memory)
                .unwrap()]
            .as_str()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(MemoryReadSimulation {
        schema_version: NextVersion,
        kind: MemorySimulationKind::Simulation,
        capability: MemoryReadCapability::V1,
        effect: MemoryReadEffect::SyntheticFixture,
        trace,
        rendered_context,
    })
}
