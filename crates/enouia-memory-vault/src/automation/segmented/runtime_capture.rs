//! Native Runtime-only source capture. No Agent/Workspace/System-read forwarding.
use super::*;
use enouia_memory_contract::{
    common::*,
    json::Knowable,
    source::{Completeness, SourceKind, SourceRecord},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
pub const PROVENANCE_KEY: &str = "runtime.capture";
#[derive(Clone, Serialize)]
pub struct RuntimeCaptureScope {
    pub policy_id: PolicyId,
    pub source_ids: Vec<SourceId>,
    pub max_sensitivity: Sensitivity,
    pub platform: String,
    pub account_scope: String,
    pub default_timezone: String,
    pub valid_until: Timestamp,
}
/// Native data delegation, not OS authentication. The Host must separately bind
/// its retained Runtime peer. No Deserialize/Clone/Serialize permit exists.
pub struct RuntimeCapturePermit {
    instance: Arc<()>,
    principal: ActorRef,
    scope: RuntimeCaptureScope,
    policy: PolicyRef,
    config: Sha256Hex,
    hash: Sha256Hex,
    active: AtomicBool,
    until: Instant,
    last_seen: Mutex<Timestamp>,
}
impl RuntimeCapturePermit {
    pub fn principal(&self) -> &ActorRef {
        &self.principal
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        &self.hash
    }
    pub fn revoke(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
    fn live(&self, v: &SegmentedAutomationVault) -> Result<Timestamp> {
        if !Arc::ptr_eq(&self.instance, &v.native.agent_instance) {
            return Err(AutomationFault::Denied);
        }
        let mut last = self.last_seen.lock().map_err(|_| AutomationFault::Denied)?;
        let now = match v.native.now() {
            Ok(v) => v,
            Err(e) => {
                self.revoke();
                return Err(e);
            }
        };
        if !self.active.load(Ordering::SeqCst)
            || Instant::now() >= self.until
            || now < *last
            || now >= self.scope.valid_until
        {
            self.revoke();
            return Err(AutomationFault::Denied);
        }
        *last = now.clone();
        Ok(now)
    }
}
#[derive(Clone, Serialize)]
pub struct NativeRuntimeCapture {
    pub schema_version: NextVersion,
    pub kind: &'static str,
    pub capability: &'static str,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub producer: ActorRef,
    pub scope_hash: Sha256Hex,
    pub observed_in: SnapshotBinding,
    pub sources: Vec<SourceRevisionRef>,
    pub captured: bool,
    pub preflight: bool,
    pub replayed: bool,
    pub formation_scheduled: bool,
}
struct Plan {
    sources: Vec<CapturedSource>,
    catalog: RecordCatalog,
    replay: bool,
}
fn deny() -> AutomationFault {
    AutomationFault::Denied
}
fn invalid() -> AutomationFault {
    AutomationFault::Conflict
}
fn provenance(s: &CapturedSource) -> Option<RuntimeCaptureProvenance> {
    serde_json::from_value(s.record.extensions.get(PROVENANCE_KEY)?.clone()).ok()
}
fn uuid(prefix: &str, value: &impl Serialize) -> String {
    let h = digest(value);
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&h.as_str()[2 * i..2 * i + 2], 16).expect("native sha256");
    }
    format!(
        "{prefix}_{}",
        enouia_memory_contract::ids::format_uuid_v4(bytes)
    )
}
fn record_time(value: Option<Timestamp>, zone: &str) -> RecordTime {
    RecordTime {
        from: value.clone(),
        until: None,
        precision: if value.is_some() {
            RecordPrecision::Instant
        } else {
            RecordPrecision::Unknown
        },
        bound: TimeBound::Within,
        tz: Some(zone.into()),
        basis: if value.is_some() {
            TimeBasis::SourceTime
        } else {
            TimeBasis::Unknown
        },
    }
}
impl SegmentedAutomationVault {
    pub fn authorize_runtime_capture(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        mut scope: RuntimeCaptureScope,
    ) -> Result<RuntimeCapturePermit> {
        self.native.owner(owner)?;
        if principal.actor_type != ActorType::Client
            || scope.platform != "enouia-runtime"
            || !enouia_memory_contract::source::is_account_alias(&scope.account_scope)
            || !valid_timezone(&scope.default_timezone)
        {
            return Err(deny());
        }
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let ttl = scope.valid_until.unix_ms() - now.unix_ms();
        let p = state.policies.get(&scope.policy_id).ok_or_else(deny)?;
        if !(1..=300_000).contains(&ttl)
            || !p.active
            || now < p.valid_from
            || now >= p.valid_until
            || scope.valid_until > p.valid_until
            || scope.max_sensitivity > p.max_sensitivity
            || scope.source_ids.is_empty()
            || scope.source_ids.len() > 128
            || scope.source_ids.iter().collect::<BTreeSet<_>>().len() != scope.source_ids.len()
            || scope
                .source_ids
                .iter()
                .any(|id| !p.source_ids.contains(id) || state.deleted_sources.contains(id))
        {
            return Err(deny());
        }
        scope.source_ids.sort();
        let policy = p.reference();
        let config = digest(
            &serde_json::json!({"producer":principal,"policy":policy,"sources":scope.source_ids,"sensitivity":scope.max_sensitivity,"platform":scope.platform,"account":scope.account_scope,"timezone":scope.default_timezone}),
        );
        let hash = digest(&(
            RUNTIME_CAPTURE_CAPABILITY,
            &self.native.descriptor.vault_id,
            &principal,
            &scope,
            &config,
            self.native.ids.random_16(),
        ));
        Ok(RuntimeCapturePermit {
            instance: self.native.agent_instance.clone(),
            principal,
            scope,
            policy,
            config,
            hash,
            active: AtomicBool::new(true),
            until: Instant::now() + Duration::from_millis(ttl as u64),
            last_seen: Mutex::new(now),
        })
    }
    pub fn runtime_capture_live(&self, permit: &RuntimeCapturePermit) -> Result<()> {
        permit.live(self).map(|_| ())
    }
    pub fn capture_runtime(
        &self,
        permit: &RuntimeCapturePermit,
        request: &RuntimeCaptureRequest,
        preflight: bool,
    ) -> Result<NativeRuntimeCapture> {
        permit.live(self)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = permit.live(self)?;
        let p = state
            .policies
            .get(&permit.scope.policy_id)
            .ok_or_else(deny)?;
        if p.reference() != permit.policy {
            permit.revoke();
            return Err(deny());
        }
        let plan = plan(&state, p, permit, request, &now)?;
        let refs = plan.sources.iter().map(CapturedSource::reference).collect();
        let published = !preflight && !plan.replay;
        if !plan.replay {
            self.commit_checked(
                &mut state,
                self.native.descriptor.system_actor.clone(),
                SegmentedDelta::Capture {
                    sources: plan.sources,
                    catalog: plan.catalog,
                },
                preflight,
            )?;
        }
        if let Err(error) = permit.live(self) {
            return Err(if published {
                AutomationFault::OutcomeUnknown
            } else {
                error
            });
        }
        Ok(NativeRuntimeCapture {
            schema_version: NextVersion,
            kind: "native_runtime_capture",
            capability: RUNTIME_CAPTURE_CAPABILITY,
            request_id: request.request_id.clone(),
            request_hash: digest(request),
            producer: permit.principal.clone(),
            scope_hash: permit.hash.clone(),
            observed_in: state.snapshot()?,
            sources: refs,
            captured: !preflight || plan.replay,
            preflight,
            replayed: plan.replay,
            formation_scheduled: false,
        })
    }
}
struct SourceContext<'a> {
    policy: &'a AutomationPolicy,
    permit: &'a RuntimeCapturePermit,
    request: &'a RuntimeCaptureRequest,
    session: &'a SessionId,
    now: &'a Timestamp,
}
impl SourceContext<'_> {
    fn build(
        &self,
        t: &RuntimeCaptureTurn,
        id: SourceId,
        branch: &BranchId,
        parent: Option<SourceId>,
    ) -> CapturedSource {
        let (p, permit, r, session, now) = (
            self.policy,
            self.permit,
            self.request,
            self.session,
            self.now,
        );
        let meta = RuntimeCaptureProvenance {
            schema_version: Revision::new(1).unwrap(),
            producer: permit.principal.clone(),
            owner_config_hash: permit.config.clone(),
            request_id: r.request_id.clone(),
            request_hash: digest(r),
            turn_hash: digest(t),
            resolved_timezone: t
                .original_timezone
                .clone()
                .unwrap_or_else(|| permit.scope.default_timezone.clone()),
            timezone_origin: if t.original_timezone.is_some() {
                RuntimeTimezoneOrigin::Source
            } else {
                RuntimeTimezoneOrigin::OwnerDefault
            },
            attachments: t.attachments.clone(),
            attachment_bytes_captured: false,
        };
        CapturedSource {
            record: SourceRecord {
                schema_version: enouia_memory_contract::json::SchemaVersion,
                source_id: id,
                revision: Revision::new(1).unwrap(),
                source_kind: SourceKind::RuntimeEvent,
                provider: None,
                account_scope: Some(permit.scope.account_scope.clone()),
                import_id: None,
                raw_object_hash: None,
                content_hash: sha256(t.text.as_bytes()),
                original_conversation_id: Some(r.conversation_id.clone()),
                original_message_id: Some(t.message_id.clone()),
                parent_source_ids: Knowable::Known(parent.into_iter().collect()),
                branch_id: Knowable::Known(Some(branch.clone())),
                locator: Locator::RuntimeEvent {
                    event_id: EventId::parse(&uuid(
                        "evt",
                        &(
                            &permit.principal,
                            &r.conversation_id,
                            &t.branch_id,
                            &t.message_id,
                        ),
                    ))
                    .unwrap(),
                },
                original_time: t.original_time.clone(),
                original_timezone: t.original_timezone.clone(),
                occurred_at: t.occurred_at.clone(),
                captured_at: now.clone(),
                time_precision: if t.occurred_at.is_some() {
                    TimePrecision::Millisecond
                } else {
                    TimePrecision::Unknown
                },
                speaker_role: t.speaker,
                author_label: None,
                evidence_class: if t.speaker == SpeakerRole::User {
                    EvidenceClass::UserStatement
                } else {
                    EvidenceClass::ModelClaim
                },
                // The selected runtime-event object is complete raw text/metadata;
                // no attachment object or captured bytes are claimed.
                completeness: Completeness::Complete,
                sensitivity: permit.scope.max_sensitivity,
                access_policy_id: p.access_policy_id.clone(),
                attachment_refs: vec![],
                parser_version: Some("runtime-capture-1".into()),
                parse_warnings: vec![],
                manual_assertion: None,
                agent_submission: None,
                created_at: now.clone(),
                extensions: BTreeMap::from([(
                    PROVENANCE_KEY.into(),
                    serde_json::to_value(meta).expect("typed native metadata"),
                )]),
            },
            text: t.text.clone(),
            attributed_subject: (t.speaker == SpeakerRole::User).then(|| p.subject_id.clone()),
            record_fields: Some(SourceRecordFields {
                content_lang: t.content_lang.clone(),
                assistant_model: t.assistant_model.clone(),
                conversation_id: Some(session.clone()),
            }),
        }
    }
}
fn plan(
    state: &State,
    p: &AutomationPolicy,
    permit: &RuntimeCapturePermit,
    r: &RuntimeCaptureRequest,
    now: &Timestamp,
) -> Result<Plan> {
    if !r.valid() {
        return Err(invalid());
    }
    let request_hash = digest(r);
    let scoped: Vec<_> = permit
        .scope
        .source_ids
        .iter()
        .filter_map(|id| state.sources.get(id).and_then(|v| v.last()))
        .collect();
    // Producer request keys are root-wide, even after a narrower re-enrollment.
    // Never let a changed packet reuse its key through a different source pool.
    let previous: Vec<_> = state
        .sources
        .values()
        .filter_map(|v| v.last())
        .filter_map(|s| {
            provenance(s)
                .filter(|m| m.producer == permit.principal && m.request_id == r.request_id)
                .map(|m| (s, m))
        })
        .collect();
    let session = SessionId::parse(&uuid(
        "ses",
        &(
            &permit.principal,
            &permit.scope.platform,
            &permit.scope.account_scope,
            &r.conversation_id,
        ),
    ))
    .unwrap();
    if !previous.is_empty() {
        if previous.len() != r.turns.len()
            || previous.iter().any(|(s, m)| {
                m.request_hash != request_hash
                    || !permit.scope.source_ids.contains(&s.record.source_id)
                    || state.deleted_sources.contains(&s.record.source_id)
            })
        {
            return Err(invalid());
        }
        let mut sources = vec![];
        for t in &r.turns {
            let (s, m) = previous
                .iter()
                .find(|(s, _)| s.record.original_message_id.as_ref() == Some(&t.message_id))
                .ok_or_else(invalid)?;
            if m.turn_hash != digest(t) || m.owner_config_hash != permit.config {
                return Err(invalid());
            }
            let branch = BranchId::parse(&uuid("br", &(&session, &t.branch_id))).unwrap();
            let parent = match &s.record.parent_source_ids {
                Knowable::Known(v) => v.first().cloned(),
                _ => return Err(invalid()),
            };
            if *s
                != &(SourceContext {
                    policy: p,
                    permit,
                    request: r,
                    session: &session,
                    now: &s.record.captured_at,
                })
                .build(t, s.record.source_id.clone(), &branch, parent)
            {
                return Err(invalid());
            }
            sources.push((*s).clone());
        }
        return Ok(Plan {
            sources,
            catalog: RecordCatalog::default(),
            replay: true,
        });
    }
    if scoped.iter().any(|s| {
        s.record.original_conversation_id.as_ref() == Some(&r.conversation_id)
            && r.turns
                .iter()
                .any(|t| s.record.original_message_id.as_ref() == Some(&t.message_id))
    }) {
        return Err(invalid());
    }
    let mut free = permit
        .scope
        .source_ids
        .iter()
        .filter(|id| !state.sources.contains_key(*id) && !state.deleted_sources.contains(*id));
    let mut conversation = state
        .conversations
        .get(&session)
        .and_then(|v| v.last())
        .cloned()
        .unwrap_or_else(|| ConversationRecord {
            conversation_id: session.clone(),
            revision: Revision::new(1).unwrap(),
            platform: permit.scope.platform.clone(),
            account_scope: permit.scope.account_scope.clone(),
            original_conversation_id: Some(r.conversation_id.clone()),
            title: r.title.clone(),
            participants: vec![],
            started_at: record_time(None, &permit.scope.default_timezone),
            ended_at: record_time(None, &permit.scope.default_timezone),
            turns: vec![],
            turn_count: 0,
            branches: vec![],
            import_batch_id: None,
            complete_in_scope: false,
            sensitivity: permit.scope.max_sensitivity,
            access_policy_id: p.access_policy_id.clone(),
        });
    let mut messages: BTreeMap<String, SourceRevisionRef> = BTreeMap::new();
    for turn in &conversation.turns {
        if !permit.scope.source_ids.contains(&turn.source.source_id)
            || !state.source_in_policy(&turn.source, p, permit.scope.max_sensitivity)
        {
            return Err(deny());
        }
        let s = state.source(&turn.source).ok_or_else(deny)?;
        messages.insert(
            s.record.original_message_id.clone().ok_or_else(deny)?,
            s.reference(),
        );
    }
    if state.conversations.contains_key(&session) {
        conversation.revision = next(conversation.revision)?;
    }
    if r.title.is_some() {
        conversation.title = r.title.clone();
    }
    let mut sources = vec![];
    for t in &r.turns {
        let id = free.next().cloned().ok_or(AutomationFault::Capacity)?;
        let branch = BranchId::parse(&uuid("br", &(&session, &t.branch_id))).unwrap();
        let parent = t
            .parent_message_id
            .as_ref()
            .map(|id| messages.get(id).cloned().ok_or_else(invalid))
            .transpose()?;
        let s = SourceContext {
            policy: p,
            permit,
            request: r,
            session: &session,
            now,
        }
        .build(t, id, &branch, parent.as_ref().map(|r| r.source_id.clone()));
        if !s.validate().is_empty() || state.locators.contains_key(&digest(&s.record.locator)) {
            return Err(invalid());
        }
        conversation.turns.push(ConversationTurn {
            source: s.reference(),
            branch_id: branch.clone(),
            turn_id: TurnId::parse(&uuid("turn", &(&session, &t.branch_id, &t.message_id)))
                .unwrap(),
            sequence: t.sequence,
            parent_source: parent,
        });
        if !conversation.branches.contains(&branch) {
            conversation.branches.push(branch);
        }
        if !conversation
            .participants
            .iter()
            .any(|p| p.speaker == t.speaker)
        {
            conversation.participants.push(ConversationParticipant {
                label: format!("{:?}", t.speaker),
                speaker: t.speaker,
                entity_id: None,
            });
        }
        messages.insert(t.message_id.clone(), s.reference());
        sources.push(s);
    }
    conversation.turn_count = conversation.turns.len() as u32;
    let times: Vec<_> = conversation
        .turns
        .iter()
        .filter_map(|t| {
            sources
                .iter()
                .find(|s| s.reference() == t.source)
                .or_else(|| state.source(&t.source))
                .and_then(|s| s.record.occurred_at.clone())
        })
        .collect();
    conversation.started_at =
        record_time(times.iter().min().cloned(), &permit.scope.default_timezone);
    conversation.ended_at =
        record_time(times.iter().max().cloned(), &permit.scope.default_timezone);
    let catalog = RecordCatalog {
        entities: vec![],
        conversations: vec![conversation],
    };
    state.check_catalog(&sources, &catalog, now)?;
    if sources.len() + state.sources.values().map(Vec::len).sum::<usize>() > MAX_SEGMENTED_REVISIONS
    {
        return Err(AutomationFault::Capacity);
    }
    Ok(Plan {
        sources,
        catalog,
        replay: false,
    })
}
