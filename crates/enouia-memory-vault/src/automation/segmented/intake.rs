//! Native initializer boundary; no IPC/MCP/default route. Source roles remain
//! data. Existing runtime capture cannot call this by changing a JSON field.
use super::*;
use enouia_memory_contract::{common::EvidenceClass, source::SourceKind};

impl SegmentedAutomationVault {
    /// Original-operation lookup only. This cannot create a receipt, resend
    /// generation or authorize application after a changed deletion barrier.
    pub fn lookup_import_receipt(
        &self,
        system: &ActorRef,
        binding: &GenerationBinding,
    ) -> Result<Option<MemoryReceipt>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let Some((job_key, input_hash)) = state.operations.get(&binding.operation_id) else {
            return Ok(None);
        };
        let receipt = state
            .jobs
            .get(job_key)
            .and_then(|(_, receipts)| {
                receipts
                    .iter()
                    .find(|r| r.operation_id == binding.operation_id)
            })
            .ok_or(AutomationFault::Unavailable)?;
        if input_hash != &binding.input_hash || receipt.response_hash != binding.response_hash {
            return Err(AutomationFault::Conflict);
        }
        let allowed = state.authorized(
            &self.native.descriptor,
            Sensitivity::HighlySensitive,
            self.native.now()?,
        )?;
        Ok(allowed
            .memories
            .iter()
            .find(|m| m.receipt.operation_id == binding.operation_id)
            .map(|m| m.receipt.clone()))
    }
    /// Archive exact selected bytes before publishing imported sources. The
    /// caller is the trusted native parser, which resolves locators against Raw;
    /// this entry verifies object closure and refuses forged owner confirmation.
    /// Legacy confirmed entries must use the separately checked legacy entry.
    pub fn capture_import(
        &self,
        system: &ActorRef,
        sources: Vec<CapturedSource>,
        catalog: RecordCatalog,
        objects: &BTreeMap<Sha256Hex, Vec<u8>>,
    ) -> Result<AutomationCurrent> {
        if sources
            .iter()
            .any(|s| s.record.evidence_class == EvidenceClass::UserConfirmation)
        {
            return Err(AutomationFault::Denied);
        }
        self.capture_import_selected(system, sources, catalog, objects)
    }
    /// The embedding caller must independently authenticate the native owner
    /// selecting this legacy snapshot. Parsed Review JSON never supplies that
    /// authority. This preserves old review evidence; it creates no owner action.
    pub fn capture_legacy_import(
        &self,
        owner: &ActorRef,
        system: &ActorRef,
        sources: Vec<CapturedSource>,
        catalog: RecordCatalog,
        objects: &BTreeMap<Sha256Hex, Vec<u8>>,
    ) -> Result<AutomationCurrent> {
        self.native.owner(owner)?;
        self.capture_import_selected(system, sources, catalog, objects)
    }
    fn capture_import_selected(
        &self,
        system: &ActorRef,
        sources: Vec<CapturedSource>,
        catalog: RecordCatalog,
        objects: &BTreeMap<Sha256Hex, Vec<u8>>,
    ) -> Result<AutomationCurrent> {
        self.native.system(system)?;
        if sources.iter().any(|s| {
            !matches!(
                s.record.source_kind,
                SourceKind::ExportMessage
                    | SourceKind::ImportedDocument
                    | SourceKind::AgentSubmission
            ) || (s.record.evidence_class == EvidenceClass::UserConfirmation
                && !legacy_confirmation(s, objects))
                || s.record
                    .raw_object_hash
                    .as_ref()
                    .is_some_and(|h| !objects.contains_key(h))
                || s.attachments.iter().any(|a| {
                    a.object_hash.as_ref().is_some_and(|h| {
                        objects
                            .get(h)
                            .is_none_or(|b| Some(b.len() as u64) != a.size_bytes || sha256(b) != *h)
                    })
                })
                || !s.validate().is_empty()
        }) {
            return Err(AutomationFault::Denied);
        }
        self.archive_import_objects(system, objects)?;
        self.capture_selected(system, sources, catalog)
    }

    /// Preserve even unsupported inputs and attachments. This creates no memory
    /// or success receipt. Object names are only generated content hashes.
    pub fn archive_import_objects(
        &self,
        system: &ActorRef,
        objects: &BTreeMap<Sha256Hex, Vec<u8>>,
    ) -> Result<()> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        for (hash, bytes) in objects {
            if bytes.len() > 16 * 1024 * 1024 || sha256(bytes) != *hash {
                return Err(AutomationFault::Denied);
            }
            let path = enouia_memory_contract::layout::raw_object(hash);
            match self.native.root.read_bounded(&path, 16 * 1024 * 1024)? {
                Some(old) if old == *bytes => {}
                Some(_) => return Err(AutomationFault::Conflict),
                None => self.native.root.write_new(&path, bytes)?,
            }
        }
        Ok(())
    }
}

fn legacy_confirmation(source: &CapturedSource, objects: &BTreeMap<Sha256Hex, Vec<u8>>) -> bool {
    use enouia_memory_contract::{
        common::Locator,
        set::{RecordSet, validate_set},
    };
    let Some(raw) = source
        .record
        .raw_object_hash
        .as_ref()
        .and_then(|h| objects.get(h))
    else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return false;
    };
    let Ok(set) = RecordSet::from_value(&value) else {
        return false;
    };
    if !validate_set(&set).is_empty() {
        return false;
    }
    let Locator::JsonPointer { pointer } = &source.record.locator else {
        return false;
    };
    let Some(index) = pointer
        .strip_prefix("/memories/")
        .and_then(|s| s.strip_suffix("/content"))
        .and_then(|s| s.parse::<usize>().ok())
    else {
        return false;
    };
    let Some(memory) = set.memories.get(index) else {
        return false;
    };
    let deleted = |kind, id: &str, rev| {
        set.tombstones.iter().any(|t| {
            t.targets.iter().any(|target| {
                target.record_kind == kind
                    && target.record_id == id
                    && target.revision.is_none_or(|r| r == rev)
            })
        })
    };
    use enouia_memory_contract::record::RecordKind;
    if deleted(
        RecordKind::Memory,
        memory.memory_id.as_str(),
        memory.revision,
    ) || memory
        .evidence
        .iter()
        .any(|e| deleted(RecordKind::Source, e.source_id.as_str(), e.source_revision))
        || memory
            .context_source_refs
            .iter()
            .any(|r| deleted(RecordKind::Source, r.source_id.as_str(), r.source_revision))
    {
        return false;
    }
    memory.content == source.text
        && set.reviews.iter().any(|r| {
            r.review_id == memory.review_id
                && r.actor == memory.approved_by
                && r.final_content_hash == sha256(memory.content.as_bytes())
        })
}
