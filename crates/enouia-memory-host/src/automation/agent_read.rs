//! Distinct native Agent read capability; no calls to System native read ports.
use super::application::{ApplicationFault, ApplicationResult, AutomationHost};
use super::read_model::{PageScope, project_detail, project_page};
use super::*;
use enouia_memory_contract::ids::MemoryId;
use enouia_memory_vault::automation::{AgentReadPermit, AgentReadScope, AutomationFault};
use serde::Serialize;

#[path = "agent_tools.rs"]
mod tools;
pub use tools::*;

pub const CAPABILITY: &str = "memory-agent-read-1";
pub const RECORD_CAPABILITY: &str = "memory-agent-record-read-1";
#[path = "agent_record_fields.rs"]
mod record_fields;
pub use record_fields::{AgentRecordProjection, AgentSourceRecordProjection};
#[derive(Serialize)]
pub struct AgentPage {
    schema_version: NextVersion,
    kind: &'static str,
    capability: &'static str,
    effect: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub profile_hash: Sha256Hex,
    pub items: Vec<MemoryReadItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_fields: Option<Vec<AgentRecordProjection>>,
    pub matched_in_scope: u64,
    pub unknown_event_time_in_scope: u64,
    pub complete_in_scope: bool,
    pub next: Option<MemoryReadCursor>,
}
#[derive(Serialize)]
pub struct AgentDetail {
    schema_version: NextVersion,
    kind: &'static str,
    capability: &'static str,
    effect: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub profile_hash: Sha256Hex,
    pub item: MemoryReadItem,
    pub history: Vec<MemoryReadItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_fields: Option<Vec<AgentRecordProjection>>,
    pub sources: Vec<MemorySourceView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_fields: Option<Vec<AgentSourceRecordProjection>>,
    pub source_read: bool,
    pub neighbors: Vec<MemoryReadLink>,
    pub complete_in_scope: bool,
}
/// Borrowed native authority. A transport must separately authenticate its OS
/// peer and bind it to this principal; no request can construct this object.
pub struct AgentReader<'a> {
    host: &'a AutomationHost,
    permit: AgentReadPermit,
    generation: Arc<AtomicBool>,
    context: Mutex<Option<AgentRecall>>,
}
impl AutomationHost {
    pub fn agent_reader(
        &self,
        native_owner: &ActorRef,
        principal: ActorRef,
        scope: AgentReadScope,
    ) -> ApplicationResult<AgentReader<'_>> {
        self.agent_reader_for(native_owner, principal, scope, false)
    }
    pub fn agent_record_reader(
        &self,
        native_owner: &ActorRef,
        principal: ActorRef,
        scope: AgentReadScope,
    ) -> ApplicationResult<AgentReader<'_>> {
        self.agent_reader_for(native_owner, principal, scope, true)
    }
    fn agent_reader_for(
        &self,
        native_owner: &ActorRef,
        principal: ActorRef,
        scope: AgentReadScope,
        records: bool,
    ) -> ApplicationResult<AgentReader<'_>> {
        let _gate = self.gate()?;
        let permit = if records {
            self.vault
                .authorize_agent_record_read(native_owner, principal, scope)?
        } else {
            self.vault
                .authorize_agent_read(native_owner, principal, scope)?
        };
        let generation = self
            .agent_generation
            .lock()
            .map_err(|_| ApplicationFault::Locked)?
            .clone();
        Ok(AgentReader {
            host: self,
            permit,
            generation,
            context: Mutex::new(None),
        })
    }
}
impl AgentReader<'_> {
    pub fn records(&self) -> bool {
        self.permit.records()
    }
    pub fn capability(&self) -> &'static str {
        if self.records() {
            RECORD_CAPABILITY
        } else {
            CAPABILITY
        }
    }
    pub fn vault_id(&self) -> &enouia_memory_contract::ids::VaultId {
        &self.host.vault.descriptor().vault_id
    }
    pub(crate) fn check_connection(&self) -> ApplicationResult<()> {
        let _gate = self.host.gate()?;
        self.live()
    }
    pub fn principal(&self) -> &ActorRef {
        self.permit.principal()
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        self.permit.scope_hash()
    }
    pub fn revoke(&self) {
        self.permit.revoke();
    }
    fn live(&self) -> ApplicationResult<()> {
        if !self.generation.load(Ordering::SeqCst) {
            return Err(AutomationFault::Denied.into());
        }
        self.host.vault.check_agent_read_live(&self.permit)?;
        Ok(())
    }
    fn finish<T: Serialize>(&self, value: T, max_bytes: usize) -> ApplicationResult<T> {
        self.live()?;
        if canonical_bytes(&value)
            .map_err(|_| ApplicationFault::Invalid)?
            .len()
            > max_bytes
        {
            return Err(ApplicationFault::Invalid);
        }
        Ok(value)
    }
    pub fn page(
        &self,
        profile: &AttentionProfile,
        filter: &MemoryReadFilter,
        cursor: Option<&MemoryReadCursor>,
        limit: usize,
        max_bytes: usize,
    ) -> ApplicationResult<AgentPage> {
        let _gate = self.host.gate()?;
        self.live()?;
        if !(2048..=MAX_DOCUMENT_BYTES).contains(&max_bytes) {
            return Err(ApplicationFault::Invalid);
        }
        let data = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, None)?;
        let fields = record_fields::projections(&data)?;
        let hash = digest(&(
            filter,
            profile,
            self.permit.max_sensitivity(),
            self.permit.scope_hash(),
        ));
        let p = project_page(
            data,
            profile,
            filter,
            cursor,
            limit,
            max_bytes - 1024,
            PageScope {
                filter_hash: hash.clone(),
                maintenance_worker_alive: false,
            },
        )?;
        self.finish_page(
            AgentPage {
                schema_version: NextVersion,
                kind: "agent_memory_page",
                capability: self.capability(),
                effect: "agent_read",
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                snapshot: p.snapshot,
                authorized_at: p.authorized_at,
                profile_hash: p.profile_hash,
                items: p.items,
                record_fields: fields,
                matched_in_scope: p.matched_in_scope,
                unknown_event_time_in_scope: p.unknown_event_time_in_scope,
                complete_in_scope: p.complete_in_scope,
                next: p.next,
            },
            hash,
            max_bytes,
        )
    }
    pub fn detail(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        include_source_text: bool,
        max_bytes: usize,
    ) -> ApplicationResult<Option<AgentDetail>> {
        let _gate = self.host.gate()?;
        self.live()?;
        if !(2048..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
            || (include_source_text && !self.permit.source_read())
        {
            return Err(AutomationFault::Denied.into());
        }
        let data = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, Some(id))?;
        let fields = record_fields::projections(&data)?;
        let source_fields = self.records().then(|| {
            data.sources
                .iter()
                .filter_map(|s| {
                    s.record_fields
                        .clone()
                        .map(|fields| AgentSourceRecordProjection {
                            source: s.reference(),
                            fields,
                        })
                })
                .collect::<Vec<_>>()
        });
        let Some(d) = project_detail(
            data,
            profile,
            id,
            expected,
            include_source_text,
            self.permit.source_read(),
            max_bytes - 1024,
        )?
        else {
            self.live()?;
            return Ok(None);
        };
        let references: Vec<_> = std::iter::once(d.item.memory.clone())
            .chain(d.history.iter().map(|i| i.memory.clone()))
            .collect();
        let fields = fields.map(|v| {
            v.into_iter()
                .filter(|v| references.contains(&v.memory))
                .collect()
        });
        let source_fields = source_fields.map(|v| {
            v.into_iter()
                .filter(|v| {
                    d.sources.iter().any(|s| {
                        s.record.source_id == v.source.source_id
                            && s.record.revision == v.source.source_revision
                    })
                })
                .collect()
        });
        self.finish(
            AgentDetail {
                schema_version: NextVersion,
                kind: "agent_memory_detail",
                capability: self.capability(),
                effect: "agent_read",
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                snapshot: d.snapshot,
                authorized_at: d.authorized_at,
                profile_hash: d.profile_hash,
                item: d.item,
                history: d.history,
                record_fields: fields,
                sources: d.sources,
                source_record_fields: source_fields,
                source_read: self.permit.source_read(),
                neighbors: d.neighbors,
                complete_in_scope: d.complete_in_scope,
            },
            max_bytes,
        )
        .map(Some)
    }
}
