//! Native Agent-only qualified page. No System reader, MCP route or OS enrollment.
use super::{
    agent_read::AgentRecordProjection,
    application::{ApplicationFault, ApplicationResult},
    catalog::{CatalogReadOptions, CatalogReadRequest, project_page},
};
use enouia_memory_contract::{common::*, hash::Sha256Hex, next::*, time::Timestamp};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        QUALIFIED_AGENT_READ_CAPABILITY, SegmentedAgentPermit, SegmentedAgentScope,
        SegmentedAutomationVault,
    },
};
use serde::Serialize;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

pub const CAPABILITY: &str = QUALIFIED_AGENT_READ_CAPABILITY;
pub struct QualifiedAgentHost {
    vault: Arc<SegmentedAutomationVault>,
    owner: ActorRef,
    generation: RwLock<Option<Arc<AtomicBool>>>,
}
pub struct QualifiedAgentReader<'a> {
    host: &'a QualifiedAgentHost,
    permit: SegmentedAgentPermit,
    generation: Arc<AtomicBool>,
}
/// Data only. Neither parsing nor copying a page grants native authority.
#[derive(Serialize)]
pub struct QualifiedAgentPage {
    pub schema_version: NextVersion,
    pub kind: &'static str,
    pub capability: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub selection: CatalogSelection,
    pub authorized_at: Timestamp,
    pub matched_in_scope: u64,
    pub unknown_time_in_scope: u64,
    pub count_exact: bool,
    pub items: Vec<QualifiedAgentItem>,
    pub complete_in_scope: bool,
    pub next: Option<CatalogCursor>,
    pub facets: Vec<CatalogFacetGroup>,
}
#[derive(Serialize)]
pub struct QualifiedAgentItem {
    pub base: CatalogReadItem,
    pub record: AgentRecordProjection,
}
impl QualifiedAgentHost {
    /// Caller has authenticated the native Owner; not an OS enrollment method.
    pub fn from_authenticated_owner(
        vault: Arc<SegmentedAutomationVault>,
        owner: ActorRef,
    ) -> ApplicationResult<Self> {
        if owner.actor_type != ActorType::Owner
            || owner != vault.descriptor().created_by
            || vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1
        {
            return Err(AutomationFault::Denied.into());
        }
        vault.snapshot()?;
        Ok(Self {
            vault,
            owner,
            generation: RwLock::new(Some(Arc::new(AtomicBool::new(true)))),
        })
    }
    pub fn lock(&self) -> ApplicationResult<()> {
        if let Some(old) = self
            .generation
            .write()
            .map_err(|_| ApplicationFault::Locked)?
            .take()
        {
            old.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    pub fn unlock(&self, owner: &ActorRef) -> ApplicationResult<()> {
        if owner != &self.owner {
            return Err(AutomationFault::Denied.into());
        }
        self.host_live()?;
        let mut gate = self
            .generation
            .write()
            .map_err(|_| ApplicationFault::Locked)?;
        if let Some(old) = gate.replace(Arc::new(AtomicBool::new(true))) {
            old.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    fn host_live(&self) -> ApplicationResult<()> {
        self.vault.snapshot().map(|_| ()).map_err(Into::into)
    }
    pub fn reader(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> ApplicationResult<QualifiedAgentReader<'_>> {
        if owner != &self.owner {
            return Err(AutomationFault::Denied.into());
        }
        let gate = self
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        let generation = gate.as_ref().ok_or(ApplicationFault::Locked)?.clone();
        let permit = self
            .vault
            .authorize_qualified_agent_read(owner, principal, scope)?;
        Ok(QualifiedAgentReader {
            host: self,
            permit,
            generation,
        })
    }
}
impl QualifiedAgentReader<'_> {
    pub fn principal(&self) -> &ActorRef {
        self.permit.principal()
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        self.permit.scope_hash()
    }
    pub fn revoke(&self) {
        self.permit.revoke();
    }
    pub fn page(
        &self,
        profile: &AttentionProfile,
        filter: &CatalogFilter,
        cursor: Option<&CatalogCursor>,
        limit: usize,
        max_bytes: usize,
        include_facets: bool,
    ) -> ApplicationResult<QualifiedAgentPage> {
        let gate = self
            .host
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        if gate.is_none()
            || !self.generation.load(Ordering::SeqCst)
            || self.permit.capability() != CAPABILITY
        {
            return Err(ApplicationFault::Locked);
        }
        self.host.vault.check_agent_read_live(&self.permit)?;
        if !filter.valid()
            || !profile.validate().is_empty()
            || !(1..=100).contains(&limit)
            || !(4096..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
        {
            return Err(ApplicationFault::Invalid);
        }
        let request = CatalogReadRequest {
            profile,
            filter,
            options: CatalogReadOptions {
                limit,
                max_bytes,
                max_sensitivity: self.permit.max_sensitivity(),
                include_facets,
            },
        };
        let result = self.host.vault.read_agent(
            &self.permit,
            None,
            |data| -> ApplicationResult<QualifiedAgentPage> {
                let page = project_page(&data.current, &request, cursor, Some(self.scope_hash()))?;
                let visible: std::collections::BTreeSet<_> = data
                    .current
                    .memories
                    .iter()
                    .map(|m| (m.memory.memory.memory_id.clone(), m.memory.memory.revision))
                    .collect();
                let mut items = Vec::new();
                for base in page.items {
                    let input = &data
                        .current
                        .memories
                        .iter()
                        .find(|m| m.memory.memory == base.base.memory)
                        .ok_or(ApplicationFault::Invalid)?
                        .memory
                        .decision
                        .input;
                    let mut fields = input
                        .record_fields
                        .clone()
                        .ok_or(ApplicationFault::Invalid)?;
                    if let Some(a) = &mut fields.annotations {
                        a.suggested_relations.retain(|r| {
                            visible.contains(&(r.target.memory_id.clone(), r.target.revision))
                        });
                    }
                    let ids: std::collections::BTreeSet<_> = fields
                        .entities
                        .iter()
                        .map(|e| &e.entity_id)
                        .chain(base.entities.iter().map(|e| &e.entity_id))
                        .collect();
                    let entities = ids
                        .into_iter()
                        .map(|id| {
                            data.current
                                .entities
                                .get(id)
                                .map(|e| (*e).clone())
                                .ok_or(ApplicationFault::Invalid)
                        })
                        .collect::<ApplicationResult<Vec<_>>>()?;
                    let record = AgentRecordProjection {
                        memory: base.base.memory.clone(),
                        producer_version: input.producer_version.clone(),
                        model_version: input.model_version.clone(),
                        annotations_scope: "authorized_current_targets",
                        fields,
                        entities,
                    };
                    items.push(QualifiedAgentItem { base, record });
                }
                let native_count = items.len();
                let mut result = QualifiedAgentPage {
                    schema_version: NextVersion,
                    kind: "qualified_agent_page",
                    capability: CAPABILITY,
                    principal: self.principal().clone(),
                    scope_hash: self.scope_hash().clone(),
                    selection: page.selection,
                    authorized_at: page.authorized_at,
                    matched_in_scope: page.matched_in_scope,
                    unknown_time_in_scope: page.unknown_time_in_scope,
                    count_exact: page.count_exact,
                    items,
                    complete_in_scope: page.complete_in_scope,
                    next: page.next,
                    facets: page.facets,
                };
                loop {
                    result.complete_in_scope =
                        page.complete_in_scope && result.items.len() == native_count;
                    result.next = if result.complete_in_scope {
                        None
                    } else {
                        Some(CatalogCursor {
                            selection: result.selection.clone(),
                            after: result
                                .items
                                .last()
                                .ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?
                                .base
                                .base
                                .memory
                                .memory_id
                                .clone(),
                        })
                    };
                    if enouia_memory_contract::json::canonical_bytes(&result)
                        .map_err(|_| ApplicationFault::Invalid)?
                        .len()
                        <= max_bytes
                    {
                        return Ok(result);
                    }
                    result
                        .items
                        .pop()
                        .ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?;
                }
            },
        )??;
        self.host.vault.check_agent_read_live(&self.permit)?;
        Ok(result)
    }
}
