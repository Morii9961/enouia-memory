//! One formation route for backfill and live. This pilot executes only scripted
//! synthetic analysis, through the existing evaluator and witnessed writer.
use crate::parse::{PARSER, Preparation};
use enouia_memory_contract::{
    common::*,
    foundation::Clock,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::{Knowable, Revision, SchemaVersion, canonical_bytes},
    next::*,
    ports::IdSource,
    source::*,
    time::Timestamp,
};
use enouia_memory_vault::{
    VerifiedRoot,
    automation::{
        AutomationFault,
        segmented::{
            CaptureAnalyzer, CapturedFormation, FormationSeed, SegmentedAutomationVault,
            formation_seed, quoted_evidence,
        },
    },
    fault::Faults,
    fs::ManagedRoot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs::File, sync::Arc};

type Result<T> = std::result::Result<T, AutomationFault>;
const JOURNAL: &str = "sync-state/initialization-owner-1.json";
const LEASE: &str = "sync-state/initialization-owner-1.lock";
fn revision() -> Revision {
    Revision::new(1).unwrap()
}
pub fn unknown_time() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
pub fn dimensions(kind: FormationKind) -> FormationDimensions {
    FormationDimensions {
        kind,
        occurrence: OccurrenceAssertion::Unknown,
        state: None,
        change: FormationChange::Initial,
        conditions: vec![],
        event_time: None,
        valid_time: None,
        counter_sources: vec![],
        derived_from: vec![],
        reverts_to: None,
    }
}
fn branch_lineage(prep: &Preparation, index: usize, depth: usize) -> Option<String> {
    use enouia_memory_import::parsed::ParentLink;
    if depth > prep.messages.len() {
        return None;
    }
    let m = &prep.messages[index];
    let same = |p: &crate::parse::Message| {
        p.platform == m.platform
            && p.account_alias == m.account_alias
            && p.conversation == m.conversation
    };
    if let ParentLink::Upstream(parent) = &m.parent {
        let siblings = prep
            .messages
            .iter()
            .filter(|p| same(p) && matches!(&p.parent,ParentLink::Upstream(id) if id==parent))
            .count();
        if siblings > 1 {
            return m.upstream_id.clone();
        }
        if let Some(pi) = prep
            .messages
            .iter()
            .position(|p| same(p) && p.upstream_id.as_ref() == Some(parent))
        {
            return branch_lineage(prep, pi, depth + 1);
        }
    }
    m.upstream_id.clone()
}

/// Scripted synthetic analysis. This is untrusted analysis output, not approval.
#[derive(Clone, Serialize)]
pub struct Statement {
    pub source_index: usize,
    pub quote: String,
    pub title: String,
    pub interpretation: Interpretation,
    pub dimensions: FormationDimensions,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub entities: Vec<(String, EntityType, EntityReality, EntityRole)>,
    pub note: Option<String>,
    pub annotations: AnalysisAnnotations,
    /// Explicit analysis link, not a name/similarity-based automatic merge.
    pub revises_statement: Option<usize>,
    pub derives_from_statements: Vec<usize>,
}

/// Advisory values retained verbatim; these never become factual dimensions.
#[derive(Clone, Default, Serialize)]
pub struct AnalysisAnnotations {
    pub suggested_tags: Vec<String>,
    pub tone: Option<ToneGuess>,
    pub importance_guess_milli: Option<u16>,
    pub current_state_note: Option<String>,
    pub suggested_relations: Vec<SuggestedRelation>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preflight {
    pub parser_version: String,
    pub input_objects: usize,
    pub received_inputs: usize,
    pub duplicate_inputs: usize,
    pub duplicate_messages: usize,
    pub parsed_sources: usize,
    pub unparseable: usize,
    pub unsupported: usize,
    pub statements: usize,
    pub proposed_staged: usize,
    pub exact_quotes: usize,
    pub title_fallbacks: usize,
    pub known_source_time: usize,
    pub unknown_source_time: usize,
    pub warnings: usize,
    pub excluded_by_legacy_deletion: usize,
    pub field_coverage: Value,
    pub later_layers: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Work {
    analysis_index: usize,
    target_analysis: Option<usize>,
    derived_analysis: Vec<usize>,
    seed: AdmissionInput,
    request: Option<AdmissionInput>,
    response: Option<String>,
    binding: Option<GenerationBinding>,
    receipt: Option<MemoryReceipt>,
    /// persisted before native application; only receipt lookup may follow an
    /// uncertain application, never a fresh generation or new identity.
    state: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub batch_id: ImportId,
    pub mode: FormationMode,
    pub preflight: Preflight,
    pub sources: Vec<CapturedSource>,
    pub catalog: RecordCatalog,
    pub policy: AutomationPolicy,
    pub input_watermark: Sha256Hex,
    analysis_hash: Sha256Hex,
    analysis_artifact: Value,
    object_hashes: Vec<Sha256Hex>,
    captured: bool,
    work: Vec<Work>,
}

impl Plan {
    /// Entirely in memory: no CURRENT, archive, journal or job write.
    pub fn build(
        prep: &Preparation,
        statements: &[Statement],
        mut policy: AutomationPolicy,
        mode: FormationMode,
        captured_at: Timestamp,
        ids: &dyn IdSource,
    ) -> Result<Self> {
        if prep.messages.len() > 64 || statements.len() > 128 {
            return Err(AutomationFault::Capacity);
        }
        if prep
            .messages
            .iter()
            .any(|m| m.sensitivity > policy.max_sensitivity)
        {
            return Err(AutomationFault::Denied);
        }
        let batch_id = ImportId::from_random(ids.random_16());
        let mut catalog = RecordCatalog::default();
        let mut sessions = BTreeMap::new();
        let mut upstream = BTreeMap::new();
        let source_ids: Vec<_> = prep
            .messages
            .iter()
            .map(|_| SourceId::from_random(ids.random_16()))
            .collect();
        for (i, m) in prep.messages.iter().enumerate() {
            if let (Some(c), Some(id)) = (&m.conversation, &m.upstream_id) {
                let key = (
                    m.platform.clone(),
                    m.account_alias.clone(),
                    c.clone(),
                    id.clone(),
                );
                if upstream.insert(key, source_ids[i].clone()).is_some() {
                    return Err(AutomationFault::Conflict);
                }
            }
        }
        let mut sources = Vec::new();
        let mut branch_ids = BTreeMap::new();
        for (index, m) in prep.messages.iter().enumerate() {
            let raw = prep
                .objects
                .get(&m.raw_hash)
                .ok_or(AutomationFault::Denied)?;
            if enouia_memory_import::locate::resolve(raw, &m.locator)
                .ok()
                .as_deref()
                != Some(m.text.as_bytes())
            {
                return Err(AutomationFault::Denied);
            }
            let conversation_key = (
                m.platform.clone(),
                m.account_alias.clone(),
                m.conversation.clone(),
            );
            let session = m.conversation.as_ref().map(|_| {
                sessions
                    .entry(conversation_key.clone())
                    .or_insert_with(|| SessionId::from_random(ids.random_16()))
                    .clone()
            });
            // Each fork retains its own leaf identity. Parent references remain
            // explicit; chronology alone never makes messages a single branch.
            let branch_key = (
                conversation_key,
                m.branch.clone().or_else(|| branch_lineage(prep, index, 0)),
            );
            let branch = BranchId::from_random(
                *branch_ids
                    .entry(branch_key)
                    .or_insert_with(|| ids.random_16()),
            );
            let parent = match (&m.parent, &m.conversation) {
                (enouia_memory_import::parsed::ParentLink::Upstream(p), Some(c)) => upstream
                    .get(&(
                        m.platform.clone(),
                        m.account_alias.clone(),
                        c.clone(),
                        p.clone(),
                    ))
                    .cloned(),
                _ => None,
            };
            let agent = m.agent_submission;
            let kind = if agent {
                SourceKind::AgentSubmission
            } else if m.platform == "document" {
                SourceKind::ImportedDocument
            } else {
                SourceKind::ExportMessage
            };
            let mut warnings = m.warnings.clone();
            if matches!(
                m.parent,
                enouia_memory_import::parsed::ParentLink::Upstream(_)
            ) && parent.is_none()
            {
                warnings.push(Warning {
                    code: "parent_not_in_selection".into(),
                    pointer: None,
                });
            }
            if m.text.len() > 16 * 1024 {
                return Err(AutomationFault::Capacity);
            }
            if m.language == "und" {
                warnings.push(Warning {
                    code: "language_unknown".into(),
                    pointer: None,
                });
            }
            let attachments: Vec<_> = m
                .attachments
                .iter()
                .map(|a| AttachmentRecord {
                    schema_version: SchemaVersion,
                    attachment_id: AttachmentId::from_random(ids.random_16()),
                    revision: revision(),
                    source_id: source_ids[index].clone(),
                    original_name: a.original_name.clone(),
                    claimed_media_type: a.claimed_media_type.clone(),
                    detected_media_type: a
                        .bytes
                        .as_deref()
                        .and_then(enouia_memory_import::parsed::sniff_media_type)
                        .map(str::to_owned),
                    size_bytes: a.bytes.as_ref().map(|b| b.len() as u64),
                    object_hash: a.bytes.as_ref().map(|b| sha256(b)),
                    availability: a.availability,
                    external_reference: a.external_reference.clone(),
                    sensitivity: m.sensitivity,
                    created_at: captured_at.clone(),
                    extensions: BTreeMap::new(),
                })
                .collect();
            let record = SourceRecord {
                schema_version: SchemaVersion,
                source_id: source_ids[index].clone(),
                revision: revision(),
                source_kind: kind,
                provider: Some(m.platform.clone()),
                account_scope: Some(m.account_alias.clone()),
                import_id: kind.is_import().then_some(batch_id.clone()),
                raw_object_hash: kind.is_import().then_some(m.raw_hash.clone()),
                content_hash: sha256(m.text.as_bytes()),
                original_conversation_id: m.conversation.clone(),
                original_message_id: m.upstream_id.clone(),
                parent_source_ids: if matches!(
                    m.parent,
                    enouia_memory_import::parsed::ParentLink::Missing
                ) || (matches!(
                    m.parent,
                    enouia_memory_import::parsed::ParentLink::Upstream(_)
                ) && parent.is_none())
                {
                    Knowable::Unknown
                } else {
                    Knowable::Known(parent.iter().cloned().collect())
                },
                branch_id: session.as_ref().map_or(Knowable::Known(None), |_| {
                    Knowable::Known(Some(branch.clone()))
                }),
                locator: if agent {
                    Locator::ByteRange {
                        start: 0,
                        end: m.text.len() as u64,
                    }
                } else {
                    m.locator.clone()
                },
                original_time: m.original_time.clone(),
                original_timezone: m.original_timezone.clone(),
                occurred_at: m.occurred_at.clone(),
                captured_at: captured_at.clone(),
                time_precision: m.precision,
                speaker_role: m.role,
                author_label: m.author.clone(),
                evidence_class: m.evidence,
                completeness: m.completeness,
                sensitivity: m.sensitivity,
                access_policy_id: policy.access_policy_id.clone(),
                attachment_refs: attachments
                    .iter()
                    .map(|a| a.attachment_id.clone())
                    .collect(),
                parser_version: Some(PARSER.into()),
                parse_warnings: warnings,
                manual_assertion: None,
                agent_submission: agent.then(|| AgentSubmission {
                    submitting_principal: policy.system_actor.clone(),
                    submitted_text: m.text.clone(),
                    claimed_user_consent: false,
                }),
                created_at: captured_at.clone(),
                extensions: BTreeMap::from([(
                    "memory-init.metadata".into(),
                    json!({
                        "raw_object_hash":m.raw_hash,"original_locator":m.locator,"metadata":m.metadata,"original_branch":m.branch,
                        "attachments":m.attachments.iter().map(|a| json!({"original_name":a.original_name,"claimed_media_type":a.claimed_media_type,
                            "detected_media_type":a.bytes.as_deref().and_then(enouia_memory_import::parsed::sniff_media_type),
                            "size_bytes":a.size_bytes,"object_hash":a.bytes.as_ref().map(|b|sha256(b)),"availability":a.availability,
                            "external_reference":a.external_reference,"sensitivity":m.sensitivity})).collect::<Vec<_>>()
                    }),
                )]),
            };
            let source = CapturedSource {
                record,
                text: m.text.clone(),
                attachments,
                attributed_subject: (m.role == SpeakerRole::User)
                    .then_some(policy.subject_id.clone()),
                record_fields: Some(SourceRecordFields {
                    content_lang: m.language.clone(),
                    assistant_model: m.assistant_model.clone(),
                    conversation_id: session.clone(),
                }),
            };
            if !source.validate().is_empty() {
                return Err(AutomationFault::Denied);
            }
            if let Some(session) = session {
                if !catalog
                    .conversations
                    .iter()
                    .any(|c| c.conversation_id == session)
                {
                    catalog.conversations.push(ConversationRecord {
                        conversation_id: session.clone(),
                        revision: revision(),
                        platform: m.platform.clone(),
                        account_scope: m.account_alias.clone(),
                        original_conversation_id: m.conversation.clone(),
                        title: m.conversation_title.clone(),
                        participants: vec![],
                        started_at: unknown_time(),
                        ended_at: unknown_time(),
                        turns: vec![],
                        turn_count: 0,
                        branches: vec![],
                        import_batch_id: Some(batch_id.clone()),
                        complete_in_scope: prep.unparseable == 0,
                        sensitivity: Sensitivity::Private,
                        access_policy_id: policy.access_policy_id.clone(),
                    });
                }
                let conversation = catalog
                    .conversations
                    .iter_mut()
                    .find(|c| c.conversation_id == session)
                    .unwrap();
                if !conversation.branches.contains(&branch) {
                    conversation.branches.push(branch.clone());
                }
                let label = m.author.clone().unwrap_or_else(|| format!("{:?}", m.role));
                if !conversation
                    .participants
                    .iter()
                    .any(|p| p.label == label && p.speaker == m.role)
                {
                    conversation.participants.push(ConversationParticipant {
                        label,
                        speaker: m.role,
                        entity_id: None,
                    });
                }
                let sequence = index as u32 + 1;
                conversation.turns.push(ConversationTurn {
                    source: source.reference(),
                    branch_id: branch,
                    turn_id: TurnId::from_random(ids.random_16()),
                    sequence,
                    parent_source: parent.map(|source_id| SourceRevisionRef {
                        source_id,
                        source_revision: revision(),
                    }),
                });
                conversation.turn_count += 1;
                if let Some(time) = &source.record.occurred_at {
                    let t = RecordTime {
                        from: Some(time.clone()),
                        until: None,
                        precision: RecordPrecision::Instant,
                        bound: TimeBound::Within,
                        tz: m
                            .original_timezone
                            .as_ref()
                            .filter(|s| valid_timezone(s))
                            .cloned(),
                        basis: TimeBasis::SourceTime,
                    };
                    if conversation
                        .started_at
                        .from
                        .as_ref()
                        .is_none_or(|old| time < old)
                    {
                        conversation.started_at = t.clone();
                    }
                    if conversation
                        .ended_at
                        .from
                        .as_ref()
                        .is_none_or(|old| time > old)
                    {
                        conversation.ended_at = t;
                    }
                }
                conversation.sensitivity = conversation.sensitivity.max(m.sensitivity);
            }
            sources.push(source);
        }
        policy.source_ids = source_ids;
        let mut work = Vec::new();
        for (analysis_index, statement) in statements.iter().enumerate() {
            if statement
                .revises_statement
                .is_some_and(|i| i >= analysis_index)
                || statement
                    .derives_from_statements
                    .iter()
                    .any(|i| *i >= analysis_index)
            {
                return Err(AutomationFault::Denied);
            }
            let source = sources
                .get(statement.source_index)
                .ok_or(AutomationFault::Denied)?;
            let q = quoted_evidence(source, &statement.quote).ok_or(AutomationFault::Denied)?;
            let result = MemoryRevisionRef {
                memory_id: MemoryId::from_random(ids.random_16()),
                revision: revision(),
            };
            let mut entities = Vec::new();
            for (name, entity_type, reality, role) in &statement.entities {
                // No name-based merge: a separate supported identity per analysis.
                let id = format!(
                    "ent_{}",
                    enouia_memory_contract::ids::format_uuid_v4(ids.random_16())
                );
                catalog.entities.push(EntityRecord {
                    entity_id: id.clone(),
                    revision: revision(),
                    entity_type: *entity_type,
                    reality: *reality,
                    display_name: name.clone(),
                    aliases: vec![],
                    evidence: vec![q.clone()],
                    project_id: if *entity_type == EntityType::Project {
                        policy.project_id.clone()
                    } else {
                        None
                    },
                    first_seen: source.record.occurred_at.clone(),
                    last_seen: source.record.occurred_at.clone(),
                    sensitivity: source.record.sensitivity,
                    access_policy_id: policy.access_policy_id.clone(),
                });
                entities.push(EntityMention {
                    entity_id: id,
                    role: *role,
                    evidence: vec![q.clone()],
                });
            }
            let seed = formation_seed(FormationSeed {
                policy: &policy,
                source,
                evidence: q,
                snapshot_commit: CommitId::from_random(ids.random_16()),
                operation_id: OperationId::from_random(ids.random_16()),
                result: result.clone(),
                title: &statement.title,
                interpretation: statement.interpretation,
                dimensions: statement.dimensions.clone(),
                event_time: statement.event_time.clone(),
                valid_time: statement.valid_time.clone(),
                entities,
                prompt_version: "scripted-init-1",
                model_version: None,
                import_batch_id: Some(batch_id.clone()),
                mode,
                annotations: Some(FormationAnnotations {
                    memory: result,
                    model_version: None,
                    prompt_version: "scripted-init-1".into(),
                    formation_note: statement.note.clone(),
                    suggested_tags: statement.annotations.suggested_tags.clone(),
                    tone: statement.annotations.tone.clone(),
                    importance_guess_milli: statement.annotations.importance_guess_milli,
                    current_state_note: statement.annotations.current_state_note.clone(),
                    suggested_relations: statement.annotations.suggested_relations.clone(),
                }),
            })
            .ok_or(AutomationFault::Denied)?;
            work.push(Work {
                analysis_index,
                target_analysis: statement.revises_statement,
                derived_analysis: statement.derives_from_statements.clone(),
                seed,
                request: None,
                response: None,
                binding: None,
                receipt: None,
                state: "queued".into(),
            });
        }
        // Formation order follows original source time even if analysis output
        // arrives in a different order; within one source preserve analysis order.
        work.sort_by_key(|w| {
            sources
                .iter()
                .position(|s| s.record.source_id == w.seed.input_refs[0].source_id)
                .unwrap()
        });
        // Refuse dependency/time-order inversions before any root writes.
        for (position, w) in work.iter().enumerate() {
            if w.target_analysis
                .iter()
                .chain(&w.derived_analysis)
                .any(|ai| {
                    !work[..position]
                        .iter()
                        .any(|parent| parent.analysis_index == *ai)
                })
            {
                return Err(AutomationFault::Denied);
            }
        }
        if !catalog.validate(&sources).is_empty() {
            return Err(AutomationFault::Denied);
        }
        let fields: Vec<_> = work
            .iter()
            .map(|w| w.seed.record_fields.as_ref().unwrap())
            .collect();
        let mut precision = BTreeMap::<String, usize>::new();
        for f in &fields {
            *precision
                .entry(
                    serde_json::to_value(f.event_time.precision)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .into(),
                )
                .or_default() += 1;
        }
        let fraction = |count: usize| json!({"numerator":count,"denominator":fields.len(),"percent":if fields.is_empty(){None}else{Some(count as f64*100.0/fields.len() as f64)}});
        let field_coverage = json!({"title":fraction(fields.iter().filter(|f|!f.title.is_empty()).count()),
            "time_fields":fraction(fields.len()),"precision_distribution":precision,
            "language":fraction(fields.iter().filter(|f|!f.content_lang.is_empty()).count()),
            "entities":fraction(fields.iter().filter(|f|!f.entities.is_empty()).count()),
            "evidence":fraction(fields.iter().filter(|f|f.evidence.iter().all(|q|q.resolves(&sources))).count()),
            "semantic_audit":null});
        let preflight = Preflight {
            parser_version: PARSER.into(),
            input_objects: prep.objects.len(),
            received_inputs: prep.inputs.len(),
            duplicate_inputs: prep.duplicates,
            duplicate_messages: prep.duplicate_messages,
            parsed_sources: sources.len(),
            unparseable: prep.unparseable,
            unsupported: prep.unsupported,
            statements: work.len(),
            proposed_staged: statements
                .iter()
                .filter(|s| {
                    s.interpretation != Interpretation::ClearSelfReport
                        || prep.messages[s.source_index].role != SpeakerRole::User
                })
                .count(),
            exact_quotes: work.len(),
            title_fallbacks: statements
                .iter()
                .filter(|s| !s.quote.to_lowercase().contains(&s.title.to_lowercase()))
                .count(),
            known_source_time: sources
                .iter()
                .filter(|s| s.record.occurred_at.is_some())
                .count(),
            unknown_source_time: sources
                .iter()
                .filter(|s| s.record.occurred_at.is_none())
                .count(),
            warnings: sources.iter().map(|s| s.record.parse_warnings.len()).sum(),
            excluded_by_legacy_deletion: prep.excluded_by_legacy_deletion,
            field_coverage,
            later_layers: json!({"owner_review":null,"semantic_quality":null,"usage":null,"platform_total_history":null,"staged_count_is_estimate":true}),
        };
        Ok(Self {
            batch_id,
            mode,
            preflight,
            sources,
            catalog,
            policy,
            input_watermark: prep.watermark(),
            analysis_hash: digest(&statements),
            analysis_artifact: serde_json::to_value(statements)
                .map_err(|_| AutomationFault::Unavailable)?,
            object_hashes: prep
                .objects
                .keys()
                .cloned()
                .chain(
                    prep.messages
                        .iter()
                        .flat_map(|m| &m.attachments)
                        .filter_map(|a| a.bytes.as_ref())
                        .map(|b| sha256(b)),
                )
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            captured: false,
            work,
        })
    }
    pub fn completed(&self) -> usize {
        self.work.iter().filter(|w| w.receipt.is_some()).count()
    }
    pub fn receipts(&self) -> Vec<MemoryReceipt> {
        self.work.iter().filter_map(|w| w.receipt.clone()).collect()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    vault_id: VaultId,
    descriptor_hash: Sha256Hex,
    plans: Vec<Plan>,
}

pub struct Initializer {
    pub vault: Arc<SegmentedAutomationVault>,
    managed: ManagedRoot,
    system: ActorRef,
    owner: ActorRef,
    journal: Journal,
    _lease: File,
    poisoned: bool,
}
impl Initializer {
    pub fn create_synthetic(
        root: &VerifiedRoot,
        descriptor: AutomationDescriptor,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
    ) -> Result<Self> {
        if descriptor.layout != AutomationLayout::SegmentedOwnerV1
            || descriptor.owner_subject_id.is_none()
        {
            return Err(AutomationFault::Denied);
        }
        let descriptor_hash = digest(&descriptor);
        let vault = Arc::new(SegmentedAutomationVault::create(
            root,
            descriptor.clone(),
            clock,
            ids,
            Faults::none(),
        )?);
        let managed = ManagedRoot::new(root.path(), Faults::none());
        let lease = managed.open_lock_file(LEASE)?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        let this = Self {
            vault,
            managed,
            system: descriptor.system_actor,
            owner: descriptor.created_by,
            journal: Journal {
                version: 1,
                vault_id: descriptor.vault_id,
                descriptor_hash,
                plans: vec![],
            },
            _lease: lease,
            poisoned: false,
        };
        this.save()?;
        Ok(this)
    }
    pub fn open_synthetic(
        root: &VerifiedRoot,
        host: DeviceId,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
    ) -> Result<Self> {
        let managed = ManagedRoot::new(root.path(), Faults::none());
        let lease = managed.open_lock_file(LEASE)?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        let raw = managed
            .read_bounded(JOURNAL, 16 * 1024 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        let journal: Journal =
            serde_json::from_slice(&raw).map_err(|_| AutomationFault::Unavailable)?;
        let vault = Arc::new(SegmentedAutomationVault::open(
            root,
            host,
            clock,
            ids,
            Faults::none(),
        )?);
        if journal.version != 1
            || journal.vault_id != vault.descriptor().vault_id
            || vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1
            || journal.descriptor_hash != digest(vault.descriptor())
            || journal
                .plans
                .iter()
                .any(|p| p.preflight.parser_version != PARSER)
        {
            return Err(AutomationFault::Denied);
        }
        Ok(Self {
            system: vault.descriptor().system_actor.clone(),
            owner: vault.descriptor().created_by.clone(),
            vault,
            managed,
            journal,
            _lease: lease,
            poisoned: false,
        })
    }
    fn save(&self) -> Result<()> {
        let bytes = canonical_bytes(&self.journal)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(AutomationFault::Capacity);
        }
        self.managed
            .write_atomic(JOURNAL, &bytes)
            .map_err(AutomationFault::from)
    }
    pub fn enqueue(&mut self, plan: Plan, prep: &Preparation) -> Result<ImportId> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        if plan.input_watermark != prep.watermark() {
            return Err(AutomationFault::Conflict);
        }
        if plan.policy.vault_id != self.vault.descriptor().vault_id
            || plan.policy.system_actor != self.system
            || plan.sources.iter().any(|s| {
                s.attributed_subject.is_some()
                    && s.attributed_subject.as_ref()
                        != self.vault.descriptor().owner_subject_id.as_ref()
            })
        {
            return Err(AutomationFault::Denied);
        }
        if let Some(old) = self
            .journal
            .plans
            .iter()
            .find(|p| p.input_watermark == plan.input_watermark && p.mode == plan.mode)
        {
            // Changed analysis must not silently reuse an existing batch.
            if old.analysis_hash != plan.analysis_hash
                || old.policy.subject_id != plan.policy.subject_id
                || old.policy.access_policy_id != plan.policy.access_policy_id
            {
                return Err(AutomationFault::Conflict);
            }
            return Ok(old.batch_id.clone());
        }
        if self
            .journal
            .plans
            .iter()
            .any(|p| p.policy.policy_id == plan.policy.policy_id && p.policy != plan.policy)
        {
            return Err(AutomationFault::Conflict);
        }
        let mut objects = prep.objects.clone();
        for m in &prep.messages {
            for a in &m.attachments {
                if let Some(bytes) = &a.bytes {
                    objects.insert(sha256(bytes), bytes.clone());
                }
            }
        }
        self.vault.archive_import_objects(&self.system, &objects)?;
        let id = plan.batch_id.clone();
        self.journal.plans.push(plan);
        if self.save().is_err() {
            self.poisoned = true;
            return Err(AutomationFault::OutcomeUnknown);
        }
        Ok(id)
    }
    pub fn plans(&self) -> &[Plan] {
        &self.journal.plans
    }
    pub fn ledger(&self) -> Result<Value> {
        let hashes = self
            .journal
            .plans
            .iter()
            .flat_map(|p| &p.object_hashes)
            .collect::<std::collections::BTreeSet<_>>();
        let mut verified = 0;
        let mut bytes = 0;
        for hash in &hashes {
            let raw = self
                .managed
                .read_bounded(
                    &enouia_memory_contract::layout::raw_object(hash),
                    16 * 1024 * 1024,
                )?
                .ok_or(AutomationFault::Unavailable)?;
            if sha256(&raw) != **hash {
                return Err(AutomationFault::Denied);
            }
            verified += 1;
            bytes += raw.len();
        }
        let plans = &self.journal.plans;
        Ok(
            json!({"root":{"vault_id":self.vault.descriptor().vault_id,"schema_version":2,"layout":self.vault.descriptor().layout,
                "owner_subject_id":self.vault.descriptor().owner_subject_id,"descriptor_hash":self.journal.descriptor_hash},
            "received":{"objects":hashes.len(),"verified":verified,"bytes":bytes,"platform_total_history":null},
            "parsed":{"sources":plans.iter().map(|p|p.sources.len()).sum::<usize>(),"unparseable":plans.iter().map(|p|p.preflight.unparseable).sum::<usize>(),
                "unsupported":plans.iter().map(|p|p.preflight.unsupported).sum::<usize>(),"excluded_by_legacy_deletion":plans.iter().map(|p|p.preflight.excluded_by_legacy_deletion).sum::<usize>()},
            "extraction":{"batches":plans.len(),"planned_statements":plans.iter().map(|p|p.work.len()).sum::<usize>(),"committed_revisions":plans.iter().map(Plan::completed).sum::<usize>(),
                "outcome_unknown":plans.iter().flat_map(|p|&p.work).filter(|w|matches!(w.state.as_str(),"applying"|"outcome_unknown")).count(),"model_invocations":0},
            "candidates":{"semantic_quality":null,"independent_model_evaluation":"not_run"},
            "owner_review":{"imported_exact_confirmation_sources":plans.iter().flat_map(|p|&p.sources).filter(|s|s.record.evidence_class==EvidenceClass::UserConfirmation).count(),
                "new_owner_actions":0,"unchecked_is_correct":false},"usage":{"recall_quality":null,"runtime_acceptance":"not_run"}}),
        )
    }
    /// One scheduler for both queues: captured live work runs before any plan,
    /// so a backfill never delays a dispatchable daily job.
    pub fn step_with_captures(
        &mut self,
        captures: &mut CapturedFormation,
        analyzer: &mut impl CaptureAnalyzer,
    ) -> Result<bool> {
        if captures.step(analyzer)? {
            return Ok(true);
        }
        self.step()
    }
    /// One bounded dispatch at a time. Live always wins over backfill. No model
    /// call occurs; scripted response, input and original identity are durable
    /// before application. A journal failure poisons this instance.
    pub fn step(&mut self) -> Result<bool> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        let position = self
            .journal
            .plans
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.captured || p.completed() < p.work.len())
            .min_by_key(|(i, p)| (if p.mode == FormationMode::Live { 0 } else { 1 }, *i))
            .map(|(i, _)| i);
        let Some(pi) = position else {
            return Ok(false);
        };
        if !self.journal.plans[pi].captured {
            let plan = &self.journal.plans[pi];
            let objects = plan
                .object_hashes
                .iter()
                .map(|hash| {
                    let bytes = self
                        .managed
                        .read_bounded(
                            &enouia_memory_contract::layout::raw_object(hash),
                            16 * 1024 * 1024,
                        )?
                        .ok_or(AutomationFault::Unavailable)?;
                    if sha256(&bytes) != *hash {
                        return Err(AutomationFault::Denied);
                    }
                    Ok((hash.clone(), bytes))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            self.vault.set_policy(&self.owner, plan.policy.clone())?;
            if !plan.sources.is_empty() {
                if plan
                    .sources
                    .iter()
                    .any(|s| s.record.evidence_class == EvidenceClass::UserConfirmation)
                {
                    self.vault.capture_legacy_import(
                        &self.owner,
                        &self.system,
                        plan.sources.clone(),
                        plan.catalog.clone(),
                        &objects,
                    )?;
                } else {
                    self.vault.capture_import(
                        &self.system,
                        plan.sources.clone(),
                        plan.catalog.clone(),
                        &objects,
                    )?;
                }
            }
            self.journal.plans[pi].captured = true;
            if self.save().is_err() {
                self.poisoned = true;
                return Err(AutomationFault::OutcomeUnknown);
            }
            return Ok(true);
        }
        let wi = self.journal.plans[pi]
            .work
            .iter()
            .position(|w| w.receipt.is_none())
            .unwrap();
        if matches!(
            self.journal.plans[pi].work[wi].state.as_str(),
            "applying" | "outcome_unknown"
        ) && self.journal.plans[pi].work[wi].request.is_none()
        {
            return Err(AutomationFault::OutcomeUnknown);
        }
        if matches!(
            self.journal.plans[pi].work[wi].state.as_str(),
            "applying" | "outcome_unknown"
        ) {
            let binding = self.journal.plans[pi].work[wi]
                .binding
                .as_ref()
                .ok_or(AutomationFault::OutcomeUnknown)?;
            // Resolve the original operation before preparing dependencies again.
            let receipt = self
                .vault
                .lookup_import_receipt(&self.system, binding)?
                .ok_or(AutomationFault::OutcomeUnknown)?;
            let work = &mut self.journal.plans[pi].work[wi];
            work.receipt = Some(receipt);
            work.state = "committed".into();
            if self.save().is_err() {
                self.poisoned = true;
                return Err(AutomationFault::OutcomeUnknown);
            }
            return Ok(true);
        }
        let target = if let Some(ai) = self.journal.plans[pi].work[wi].target_analysis {
            let receipt = self.journal.plans[pi]
                .work
                .iter()
                .find(|w| w.analysis_index == ai)
                .and_then(|w| w.receipt.as_ref())
                .ok_or(AutomationFault::Stale)?;
            self.vault
                .read(&self.system, Sensitivity::Private, |data| {
                    data.memories
                        .iter()
                        .find(|m| m.memory.memory == receipt.result)
                        .map(|m| m.target.clone())
                })?
                .ok_or(AutomationFault::Stale)
                .map(Some)?
        } else {
            None
        };
        let derived = self.journal.plans[pi].work[wi]
            .derived_analysis
            .iter()
            .map(|ai| {
                let receipt = self.journal.plans[pi]
                    .work
                    .iter()
                    .find(|w| w.analysis_index == *ai)
                    .and_then(|w| w.receipt.as_ref())
                    .ok_or(AutomationFault::Stale)?;
                self.vault
                    .read(&self.system, Sensitivity::Private, |data| {
                        data.memories
                            .iter()
                            .find(|m| m.memory.memory == receipt.result)
                            .map(|m| (*m).clone())
                    })?
                    .ok_or(AutomationFault::Stale)
            })
            .collect::<Result<Vec<_>>>()?;
        let work = &mut self.journal.plans[pi].work[wi];
        if work.request.is_none() {
            work.seed.target = target;
            if !derived.is_empty() {
                let d = work
                    .seed
                    .dimensions
                    .as_mut()
                    .ok_or(AutomationFault::Denied)?;
                d.derived_from = derived.iter().map(|m| m.target.clone()).collect();
                for m in &derived {
                    for r in &m.memory.decision.input.input_refs {
                        if !work.seed.input_refs.contains(r) {
                            work.seed.input_refs.push(r.clone());
                        }
                        if !work.seed.evidence.iter().any(|e| {
                            e.source_id == r.source_id && e.source_revision == r.source_revision
                        }) && !work.seed.context_source_refs.contains(r)
                        {
                            work.seed.context_source_refs.push(r.clone());
                        }
                    }
                }
            }
            let prepared = self
                .vault
                .prepare_formation(&self.system, work.seed.clone())?;
            work.binding = Some(prepared.binding);
            work.request = Some(prepared.request);
            work.response = Some(prepared.response);
            work.state = "prepared".into();
            if self.save().is_err() {
                self.poisoned = true;
                return Err(AutomationFault::OutcomeUnknown);
            }
        }
        let work = &self.journal.plans[pi].work[wi];
        let request = work.request.clone().ok_or(AutomationFault::Unavailable)?;
        let response = work.response.clone().ok_or(AutomationFault::Unavailable)?;
        let binding = work.binding.clone().ok_or(AutomationFault::Unavailable)?;
        self.journal.plans[pi].work[wi].state = "applying".into();
        if self.save().is_err() {
            self.poisoned = true;
            return Err(AutomationFault::OutcomeUnknown);
        }
        let result =
            self.vault
                .apply_batch(&self.system, vec![request], vec![response], vec![binding]);
        match result {
            Ok(receipts) => {
                let work = &mut self.journal.plans[pi].work[wi];
                work.receipt = receipts.into_iter().next();
                work.state = "committed".into();
            }
            Err(e) => {
                self.journal.plans[pi].work[wi].state = "outcome_unknown".into();
                let _ = self.save();
                return Err(e);
            }
        }
        if self.save().is_err() {
            self.poisoned = true;
            return Err(AutomationFault::OutcomeUnknown);
        }
        Ok(true)
    }
}
