//! ADR103: native Owner plan/gesture/commit. Wire data never creates a consent.
use super::*;
use enouia_memory_contract::{
    common::*,
    json::{Knowable, SchemaVersion},
    source::{Completeness, ConfirmationMethod, ManualAssertion, SourceKind, SourceRecord},
};
use std::collections::{BTreeSet, VecDeque};

#[derive(serde::Serialize)]
pub struct NativeOwnerPreview {
    pub plan: OwnerActionPlan,
    pub effects_hash: Sha256Hex,
    pub affected_current: Vec<TargetRef>,
    pub affected_count_in_scope: usize,
    pub sensitivity: Sensitivity,
    pub proposed_correction: Option<DecisionDraft>,
    pub original_sources_retained: bool,
    pub physical_purge: bool,
}
/// Private, one-use, instance-bound capability. Deliberately not Clone or serde.
/// ```compile_fail
/// use enouia_memory_vault::automation::segmented::NativeOwnerConsent;
/// let _: NativeOwnerConsent = serde_json::from_str("{}").unwrap();
/// ```
pub struct NativeOwnerConsent {
    operation_id: OperationId,
    nonce: [u8; 16],
}
struct Pending {
    plan: OwnerActionPlan,
    effects: OwnerActionEffects,
    confirmation: Option<OwnerActionConfirmation>,
    nonce: Option<[u8; 16]>,
}
#[derive(Default)]
pub(super) struct PendingOwnerPlans {
    plans: BTreeMap<OperationId, Pending>,
}
fn deny<T>() -> Result<T> {
    Err(AutomationFault::Denied)
}
fn action_name(a: &OwnerAction) -> &'static str {
    match a {
        OwnerAction::Correct { .. } => "correct",
        OwnerAction::Withdraw { .. } => "withdraw",
        OwnerAction::Forget { .. } => "forget",
        OwnerAction::Attention { .. } => "attention",
        OwnerAction::ThemeRename { .. } => "theme_rename",
        OwnerAction::ThemePin { .. } => "theme_pin",
        OwnerAction::ThemeDissolve { .. } => "theme_dissolve",
        OwnerAction::EntityMerge { .. } => "entity_merge",
        OwnerAction::EntitySplit { .. } => "entity_split",
        OwnerAction::Tag { .. } => "tag",
    }
}
pub(super) fn action_targets(a: &OwnerAction) -> Vec<&TargetRef> {
    match a {
        OwnerAction::Correct { target, .. }
        | OwnerAction::Withdraw { target, .. }
        | OwnerAction::Forget { target, .. }
        | OwnerAction::Attention { target, .. } => vec![target],
        OwnerAction::ThemeRename { theme, members, .. }
        | OwnerAction::ThemePin { theme, members }
        | OwnerAction::ThemeDissolve { theme, members } => {
            std::iter::once(theme).chain(members).collect()
        }
        OwnerAction::Tag { targets, .. } => targets.iter().collect(),
        _ => vec![],
    }
}
pub(super) fn family_members(
    data: &AuthorizedSegmentedRead<'_>,
    theme: &SegmentedMemory,
) -> Vec<TargetRef> {
    let input = &theme.memory.decision.input;
    let entities: BTreeSet<_> = input
        .record_fields
        .as_ref()
        .unwrap()
        .entities
        .iter()
        .filter(|e| e.role != EntityRole::Subject)
        .map(|e| {
            data.entity_redirects
                .get(&(theme.memory.memory.memory_id.clone(), e.entity_id.clone()))
                .unwrap_or(&e.entity_id)
        })
        .collect();
    data.memories
        .iter()
        .filter(|m| {
            m.memory.memory.memory_id != theme.memory.memory.memory_id
                && (input
                    .dimensions
                    .as_ref()
                    .unwrap()
                    .derived_from
                    .contains(&m.target)
                    || m.memory
                        .decision
                        .input
                        .record_fields
                        .as_ref()
                        .unwrap()
                        .entities
                        .iter()
                        .any(|e| {
                            e.role != EntityRole::Subject
                                && entities.contains(
                                    data.entity_redirects
                                        .get(&(
                                            m.memory.memory.memory_id.clone(),
                                            e.entity_id.clone(),
                                        ))
                                        .unwrap_or(&e.entity_id),
                                )
                        }))
        })
        .map(|m| m.target.clone())
        .collect()
}
fn scope_hash(data: &AuthorizedSegmentedRead<'_>) -> Sha256Hex {
    digest(&(
        data.memories.iter().map(|m| &m.target).collect::<Vec<_>>(),
        data.sources
            .values()
            .map(|s| s.reference())
            .collect::<Vec<_>>(),
        data.entities
            .iter()
            .map(|(id, e)| (id, e.revision))
            .collect::<Vec<_>>(),
        data.conversations
            .iter()
            .map(|(id, c)| (id, c.revision))
            .collect::<Vec<_>>(),
    ))
}
fn impacts(data: &AuthorizedSegmentedRead<'_>, action: &OwnerAction) -> Vec<TargetRef> {
    let mut ids: BTreeSet<_> = action_targets(action)
        .into_iter()
        .map(|t| t.memory.memory_id.clone())
        .collect();
    if let OwnerAction::EntityMerge {
        from_entity,
        into_entity,
        ..
    }
    | OwnerAction::EntitySplit {
        from_entity,
        into_entity,
        ..
    } = action
    {
        ids.extend(
            data.memories
                .iter()
                .filter(|m| {
                    m.memory
                        .decision
                        .input
                        .record_fields
                        .as_ref()
                        .unwrap()
                        .entities
                        .iter()
                        .any(|e| &e.entity_id == from_entity || &e.entity_id == into_entity)
                })
                .map(|m| m.memory.memory.memory_id.clone()),
        );
    }
    if matches!(
        action,
        OwnerAction::Withdraw { .. } | OwnerAction::Forget { .. }
    ) {
        let mut children: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for m in &data.memories {
            for p in &m
                .memory
                .decision
                .input
                .dimensions
                .as_ref()
                .unwrap()
                .derived_from
            {
                children
                    .entry(p.memory.memory_id.clone())
                    .or_default()
                    .push(m.memory.memory.memory_id.clone());
            }
        }
        let mut queue: VecDeque<_> = ids.iter().cloned().collect();
        while let Some(id) = queue.pop_front() {
            for child in children.get(&id).into_iter().flatten() {
                if ids.insert(child.clone()) {
                    queue.push_back(child.clone());
                }
            }
        }
    }
    data.memories
        .iter()
        .filter(|m| ids.contains(&m.memory.memory.memory_id))
        .map(|m| m.target.clone())
        .collect()
}
fn metadata(c: &OwnerCorrection) -> OwnerCorrectionMetadata {
    let i = &c.decision.input;
    let f = i.record_fields.as_ref().unwrap();
    OwnerCorrectionMetadata {
        title: f.title.clone(),
        content_lang: f.content_lang.clone(),
        sensitivity: i.sensitivity,
        interpretation: i.interpretation,
        dimensions: i.dimensions.clone().unwrap(),
        event_time: f.event_time.clone(),
        valid_time: f.valid_time.clone(),
        entities: f
            .entities
            .iter()
            .map(|e| OwnerEntityMention {
                entity_id: e.entity_id.clone(),
                role: e.role,
            })
            .collect(),
    }
}
impl State {
    fn owner_effects(
        &self,
        descriptor: &AutomationDescriptor,
        plan: &OwnerActionPlan,
        max: Sensitivity,
        proposed: Option<&OwnerCorrectionMetadata>,
        confirmed_at: &Timestamp,
    ) -> Result<(OwnerActionEffects, Vec<TargetRef>)> {
        if descriptor.layout != AutomationLayout::SegmentedOwnerV1
            || !descriptor.validate().is_empty()
            || plan.owner != descriptor.created_by
            || !plan.validate().is_empty()
            || plan.snapshot != self.snapshot()?
            || self.owner_actions.len() >= MAX_SEGMENTED_REVISIONS
            || self.operations.contains_key(&plan.operation_id)
            || self.owner_actions.contains_key(&plan.operation_id)
        {
            return deny();
        }
        let data = self.authorized(descriptor, max, confirmed_at.clone())?;
        let current = |t: &TargetRef| {
            data.memories
                .iter()
                .copied()
                .find(|m| &m.target == t)
                .ok_or(AutomationFault::Denied)
        };
        let mut floor = Sensitivity::Public;
        for t in action_targets(&plan.action) {
            floor = floor.max(current(t)?.memory.decision.input.sensitivity);
        }
        let mut family = Vec::new();
        match &plan.action {
            OwnerAction::Forget {
                target,
                impact_preview_hash,
            } => {
                let action = OwnerAction::Withdraw {
                    target: target.clone(),
                    reason: None,
                };
                if *impact_preview_hash
                    != digest(&(
                        self.snapshot()?,
                        impacts(&data, &action),
                        "memory_and_dependents_only_original_sources_retained",
                    ))
                {
                    return deny();
                }
            }
            OwnerAction::ThemeRename { theme, members, .. }
            | OwnerAction::ThemePin { theme, members }
            | OwnerAction::ThemeDissolve { theme, members } => {
                let seed = current(theme)?;
                if seed.memory.decision.input.dimensions.as_ref().unwrap().kind
                    != FormationKind::Theme
                {
                    return deny();
                }
                family = family_members(&data, seed);
                if family.is_empty()
                    || members != &family.iter().take(128).cloned().collect::<Vec<_>>()
                {
                    return deny();
                }
                for m in &family {
                    floor = floor.max(current(m)?.memory.decision.input.sensitivity);
                }
            }
            OwnerAction::EntityMerge {
                from_entity,
                into_entity,
                ..
            }
            | OwnerAction::EntitySplit {
                from_entity,
                into_entity,
                ..
            } => {
                let from = data
                    .entities
                    .get(from_entity)
                    .ok_or(AutomationFault::Denied)?;
                let into = data
                    .entities
                    .get(into_entity)
                    .ok_or(AutomationFault::Denied)?;
                if from.access_policy_id != into.access_policy_id
                    || from.entity_type != into.entity_type
                    || from.reality != into.reality
                    || from.project_id != into.project_id
                {
                    return deny();
                }
                floor = floor.max(from.sensitivity).max(into.sensitivity);
                for m in &data.memories {
                    let i = &m.memory.decision.input;
                    if i.record_fields.as_ref().unwrap().entities.iter().any(|e| {
                        data.entity_redirects
                            .get(&(m.memory.memory.memory_id.clone(), e.entity_id.clone()))
                            .unwrap_or(&e.entity_id)
                            == from_entity
                    }) {
                        let p = self.policy(i).ok_or(AutomationFault::Denied)?;
                        if !self.entity_in_policy(into_entity, p, i.sensitivity) {
                            return deny();
                        }
                    }
                }
                if let OwnerAction::EntitySplit { evidence, .. } = &plan.action
                    && evidence.iter().any(|q| {
                        !data
                            .sources
                            .get(&q.reference.source_id)
                            .is_some_and(|s| q.resolves(std::slice::from_ref(*s)))
                    })
                {
                    return deny();
                }
            }
            _ => {}
        }
        let correction = if let OwnerAction::Correct {
            target,
            source_id,
            assertion_text,
        } = &plan.action
        {
            let old = current(target)?;
            let i = &old.memory.decision.input;
            let m = proposed.ok_or(AutomationFault::Denied)?;
            if !m.validate().is_empty()
                || descriptor.owner_subject_id.as_ref() != Some(&i.subject_id)
                || self.sources.contains_key(source_id)
                || self.deleted_sources.contains(source_id)
                || self.source_revisions >= MAX_SEGMENTED_REVISIONS
                || self.memory_revisions >= MAX_SEGMENTED_REVISIONS
                || self.policies.len() >= MAX_SEGMENTED_POLICIES
                || m.dimensions.kind == FormationKind::Theme
                || m.dimensions.kind != i.dimensions.as_ref().unwrap().kind
                || m.dimensions.change != FormationChange::Correction
                || !m.dimensions.derived_from.is_empty()
                || !m.dimensions.counter_sources.is_empty()
                || m.dimensions.reverts_to.is_some()
                || m.sensitivity < i.sensitivity
                || m.sensitivity > max
                || m.event_time.basis == TimeBasis::SourceTime
                || m.valid_time.basis == TimeBasis::SourceTime
            {
                return deny();
            }
            let original = self.policy(i).ok_or(AutomationFault::Denied)?;
            let mut source_ids = BTreeSet::from([source_id.clone()]);
            for e in &m.entities {
                if !self.entity_in_policy(&e.entity_id, original, m.sensitivity) {
                    return deny();
                }
                let entity = data
                    .entities
                    .get(&e.entity_id)
                    .ok_or(AutomationFault::Denied)?;
                source_ids.extend(
                    entity
                        .evidence
                        .iter()
                        .chain(entity.aliases.iter().flat_map(|a| &a.evidence))
                        .map(|q| q.reference.source_id.clone()),
                );
            }
            if source_ids.len() > MAX_SOURCES {
                return Err(AutomationFault::Capacity);
            }
            let policy = AutomationPolicy {
                schema_version: NextVersion,
                kind: PolicyKind::AutomationPolicy,
                policy_id: PolicyId::parse(&format!("pol_{}", &plan.operation_id.as_str()[3..]))
                    .map_err(|_| AutomationFault::Denied)?,
                revision: Revision::new(1).unwrap(),
                rule_version: RuleVersion::V3,
                system_actor: descriptor.system_actor.clone(),
                vault_id: descriptor.vault_id.clone(),
                subject_id: i.subject_id.clone(),
                project_id: i.project_id.clone(),
                source_ids: source_ids.into_iter().collect(),
                allow_create: false,
                allow_revise_created: false,
                target_ids: vec![target.memory.memory_id.clone()],
                allowed_forms: vec![m.dimensions.kind],
                max_sensitivity: m.sensitivity,
                access_policy_id: i.access_policy_id.clone(),
                egress_policy_id: i.egress_policy_id.clone(),
                active: true,
                valid_from: plan.planned_at.clone(),
                valid_until: Timestamp::parse("9999-12-31T23:59:59.999Z").unwrap(),
            };
            if self.policies.contains_key(&policy.policy_id) {
                return deny();
            }
            let source = CapturedSource {
                attachments: Vec::new(),
                record: SourceRecord {
                    schema_version: SchemaVersion,
                    source_id: source_id.clone(),
                    revision: Revision::new(1).unwrap(),
                    source_kind: SourceKind::ManualAssertion,
                    provider: None,
                    account_scope: None,
                    import_id: None,
                    raw_object_hash: None,
                    content_hash: sha256(assertion_text.as_bytes()),
                    original_conversation_id: None,
                    original_message_id: None,
                    parent_source_ids: Knowable::Known(vec![]),
                    branch_id: Knowable::Known(None),
                    locator: Locator::ManualInput,
                    original_time: None,
                    original_timezone: None,
                    occurred_at: None,
                    captured_at: confirmed_at.clone(),
                    time_precision: TimePrecision::Unknown,
                    speaker_role: SpeakerRole::User,
                    author_label: None,
                    evidence_class: EvidenceClass::UserStatement,
                    completeness: Completeness::Complete,
                    sensitivity: m.sensitivity,
                    access_policy_id: i.access_policy_id.clone(),
                    attachment_refs: vec![],
                    parser_version: Some("owner-native-literal-1".into()),
                    parse_warnings: vec![],
                    manual_assertion: Some(ManualAssertion {
                        input_text: assertion_text.clone(),
                        operator: plan.owner.clone(),
                        trusted_surface: TrustedSurface::TrustedWindowsApp,
                        confirmation_method: ConfirmationMethod::ExactTextConfirmDialog,
                        confirmed_at: confirmed_at.clone(),
                    }),
                    agent_submission: None,
                    created_at: confirmed_at.clone(),
                    extensions: Default::default(),
                },
                text: assertion_text.clone(),
                attributed_subject: descriptor.owner_subject_id.clone(),
                record_fields: Some(SourceRecordFields {
                    content_lang: m.content_lang.clone(),
                    assistant_model: None,
                    conversation_id: None,
                }),
            };
            let evidence = EvidenceRef {
                source_id: source_id.clone(),
                source_revision: Revision::new(1).unwrap(),
                locator: Locator::ManualInput,
                object_hash: source.record.content_hash.clone(),
                evidence_class: EvidenceClass::UserStatement,
                supports: "content".into(),
            };
            let quote: String = assertion_text.chars().take(500).collect();
            let quoted = QuotedEvidence {
                reference: evidence.clone(),
                end: quote.chars().count() as u32,
                quote,
                speaker: SpeakerRole::User,
                start: 0,
            };
            let result = MemoryRevisionRef {
                memory_id: target.memory.memory_id.clone(),
                revision: next(target.memory.revision)?,
            };
            let mut input = i.clone();
            input.operation_id = plan.operation_id.clone();
            input.snapshot = plan.snapshot.clone();
            input.actor = descriptor.system_actor.clone();
            input.policy = policy.reference();
            input.target = Some(target.clone());
            input.result = result.clone();
            input.content = assertion_text.clone();
            input.producer_version = "owner-native-literal-1".into();
            input.model_version = None;
            input.sensitivity = m.sensitivity;
            input.interpretation = m.interpretation;
            input.dimensions = Some(m.dimensions.clone());
            input.input_refs = vec![source.reference()];
            input.context_source_refs.clear();
            input.evidence = vec![evidence];
            input.record_fields = Some(MemoryRecordFields {
                title: m.title.clone(),
                content_lang: m.content_lang.clone(),
                event_time: m.event_time.clone(),
                valid_time: m.valid_time.clone(),
                entities: m
                    .entities
                    .iter()
                    .map(|e| EntityMention {
                        entity_id: e.entity_id.clone(),
                        role: e.role,
                        evidence: vec![quoted.clone()],
                    })
                    .collect(),
                evidence: vec![quoted],
                formation: FormationOrigin {
                    prompt_version: "owner-native-literal-1".into(),
                    import_batch_id: None,
                    mode: FormationMode::Live,
                },
                annotations: None,
            });
            if !source.record.validate().is_empty()
                || !source.record_fields.as_ref().unwrap().valid(&source)
            {
                return deny();
            }
            self.record_fields_with(&input, Some(&policy), Some(&source))?;
            let decision = self.context_with(
                &input,
                &descriptor.system_actor,
                confirmed_at,
                Some(&policy),
                Some(&source),
                |ctx| evaluate(&input, ctx),
            )??;
            floor = floor.max(m.sensitivity);
            Some(OwnerCorrection {
                policy,
                source,
                decision,
            })
        } else {
            if proposed.is_some() {
                return deny();
            }
            None
        };
        if let OwnerAction::Attention {
            until: Some(until), ..
        } = &plan.action
            && until <= confirmed_at
        {
            return deny();
        }
        let mut affected = impacts(&data, &plan.action);
        if !family.is_empty() {
            let mut all: BTreeMap<_, _> = affected
                .into_iter()
                .map(|t| (t.memory.memory_id.clone(), t))
                .collect();
            all.extend(
                family
                    .iter()
                    .cloned()
                    .map(|t| (t.memory.memory_id.clone(), t)),
            );
            affected = all.into_values().collect();
        }
        for target in &affected {
            floor = floor.max(current(target)?.memory.decision.input.sensitivity);
        }
        let effects = OwnerActionEffects {
            scope_hash: scope_hash(&data),
            max_sensitivity: max,
            sensitivity: floor,
            family_members: family,
            correction,
        };
        if !effects.valid() {
            return deny();
        }
        Ok((effects, affected))
    }
    pub(super) fn check_owner(
        &self,
        descriptor: &AutomationDescriptor,
        c: &SegmentedCommit,
        record: &OwnerActionRecord,
        effects: &OwnerActionEffects,
    ) -> Result<Vec<SegmentedMemory>> {
        if c.actor != descriptor.created_by
            || record.committed_in != c.commit_id
            || record.sequence != c.sequence
        {
            return deny();
        }
        validate_owner_action_binding(
            &record.plan,
            &record.confirmation,
            &descriptor.created_by,
            &self.snapshot()?,
            &c.created_at,
        )?;
        if let OwnerAction::Attention {
            until: Some(until), ..
        } = &record.plan.action
            && until <= &c.created_at
        {
            return deny();
        }
        let fresh = self.authorized(descriptor, effects.max_sensitivity, c.created_at.clone())?;
        if scope_hash(&fresh) != effects.scope_hash {
            return deny();
        }
        let proposed = effects.correction.as_ref().map(metadata);
        let (mut expected, _) = self.owner_effects(
            descriptor,
            &record.plan,
            effects.max_sensitivity,
            proposed.as_ref(),
            &record.confirmation.confirmed_at,
        )?;
        if let Some(correction) = &mut expected.correction {
            correction
                .source
                .record
                .manual_assertion
                .as_mut()
                .unwrap()
                .trusted_surface = record.confirmation.surface;
            correction
                .source
                .record
                .manual_assertion
                .as_mut()
                .unwrap()
                .confirmation_method =
                if record.confirmation.surface == TrustedSurface::TrustedLocalCli {
                    ConfirmationMethod::TypedConfirmation
                } else {
                    ConfirmationMethod::ExactTextConfirmDialog
                };
        }
        if &expected != effects {
            return deny();
        }
        let Some(correction) = &effects.correction else {
            return Ok(vec![]);
        };
        let input = &correction.decision.input;
        let response_json = String::from_utf8(canonical_bytes(&correction.decision)?)
            .map_err(|_| AutomationFault::Denied)?;
        let response_hash = sha256(response_json.as_bytes());
        let binding = GenerationBinding {
            operation_id: input.operation_id.clone(),
            input_hash: digest(input),
            response_hash: response_hash.clone(),
        };
        let memory = MachineMemory {
            schema_version: NextVersion,
            kind: MachineKind::MachineMemory,
            memory: input.result.clone(),
            request: input.clone(),
            decision: correction.decision.clone(),
            response_hash: response_hash.clone(),
            response_json,
            committed_in: c.commit_id.clone(),
        };
        let receipt = MemoryReceipt {
            job_key: admission_batch_key(std::slice::from_ref(input)),
            binding_hash: digest(&vec![binding]),
            operation_id: input.operation_id.clone(),
            request_hash: digest(input),
            response_hash,
            decision_hash: digest(&memory.decision),
            committed_in: c.commit_id.clone(),
            sequence: c.sequence,
            result: memory.memory.clone(),
        };
        Ok(vec![SegmentedMemory {
            search_text: format!(
                "{}\n{}",
                input.content,
                input.record_fields.as_ref().unwrap().title
            )
            .to_lowercase(),
            target: memory.target(),
            memory,
            receipt,
            learned_at: c.created_at.clone(),
        }])
    }
}
impl SegmentedAutomationVault {
    pub fn plan_owner_action(
        &self,
        owner: &ActorRef,
        mut action: OwnerAction,
        max: Sensitivity,
        metadata: Option<OwnerCorrectionMetadata>,
    ) -> Result<NativeOwnerPreview> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        if let OwnerAction::Correct { source_id, .. } = &mut action {
            *source_id = SourceId::from_random(self.native.ids.random_16());
        }
        // Full family membership is native-derived; the frozen action keeps its bounded preview.
        if let OwnerAction::ThemeRename { theme, members, .. }
        | OwnerAction::ThemePin { theme, members }
        | OwnerAction::ThemeDissolve { theme, members } = &mut action
        {
            let data = state.authorized(&self.native.descriptor, max, now.clone())?;
            let seed = data
                .memories
                .iter()
                .find(|m| &m.target == theme)
                .ok_or(AutomationFault::Denied)?;
            *members = family_members(&data, seed).into_iter().take(128).collect();
        }
        if let OwnerAction::Forget {
            target,
            impact_preview_hash,
        } = &mut action
        {
            let data = state.authorized(&self.native.descriptor, max, now.clone())?;
            let a = OwnerAction::Withdraw {
                target: target.clone(),
                reason: None,
            };
            *impact_preview_hash = digest(&(
                state.snapshot()?,
                impacts(&data, &a),
                "memory_and_dependents_only_original_sources_retained",
            ));
        }
        let plan = OwnerActionPlan {
            schema_version: NextVersion,
            kind: OwnerPlanKind::OwnerActionPlan,
            operation_id: OperationId::from_random(self.native.ids.random_16()),
            snapshot: state.snapshot()?,
            owner: owner.clone(),
            planned_at: now.clone(),
            expires_at: Timestamp::from_unix_ms(
                now.unix_ms()
                    .checked_add(300_000)
                    .ok_or(AutomationFault::Capacity)?,
            )
            .ok_or(AutomationFault::Capacity)?,
            action,
        };
        let (effects, affected) =
            state.owner_effects(&self.native.descriptor, &plan, max, metadata.as_ref(), &now)?;
        let preview = NativeOwnerPreview {
            plan: plan.clone(),
            effects_hash: digest(&effects),
            affected_current: affected.iter().take(128).cloned().collect(),
            affected_count_in_scope: affected.len(),
            sensitivity: effects.sensitivity,
            proposed_correction: effects.correction.as_ref().map(|c| c.decision.clone()),
            original_sources_retained: true,
            physical_purge: false,
        };
        let mut pending = self
            .owner_pending
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        pending.plans.retain(|_, p| p.plan.expires_at > now);
        if pending.plans.len() >= 64 {
            return Err(AutomationFault::Capacity);
        }
        pending.plans.insert(
            plan.operation_id.clone(),
            Pending {
                plan,
                effects,
                confirmation: None,
                nonce: None,
            },
        );
        Ok(preview)
    }
    /// Call only for an actual authenticated owner gesture on the displayed plan.
    /// The page/model can supply the hashes, but cannot supply the returned capability.
    pub fn confirm_owner_action(
        &self,
        owner: &ActorRef,
        operation: &OperationId,
        plan_hash: &Sha256Hex,
        effects_hash: &Sha256Hex,
        surface: TrustedSurface,
    ) -> Result<NativeOwnerConsent> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let mut pending = self
            .owner_pending
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        let p = pending
            .plans
            .get_mut(operation)
            .ok_or(AutomationFault::Denied)?;
        if &digest(&p.plan) != plan_hash || &digest(&p.effects) != effects_hash || p.nonce.is_some()
        {
            return deny();
        }
        let confirmation = OwnerActionConfirmation {
            operation_id: operation.clone(),
            plan_hash: plan_hash.clone(),
            owner: owner.clone(),
            surface,
            confirmed_at: now.clone(),
        };
        validate_owner_action_binding(&p.plan, &confirmation, owner, &state.snapshot()?, &now)?;
        let proposed = p.effects.correction.as_ref().map(metadata);
        let (mut effects, _) = state.owner_effects(
            &self.native.descriptor,
            &p.plan,
            p.effects.max_sensitivity,
            proposed.as_ref(),
            &now,
        )?;
        if let Some(correction) = &mut effects.correction {
            correction
                .source
                .record
                .manual_assertion
                .as_mut()
                .unwrap()
                .trusted_surface = surface;
            correction
                .source
                .record
                .manual_assertion
                .as_mut()
                .unwrap()
                .confirmation_method = if surface == TrustedSurface::TrustedLocalCli {
                ConfirmationMethod::TypedConfirmation
            } else {
                ConfirmationMethod::ExactTextConfirmDialog
            };
        }
        p.effects = effects;
        p.confirmation = Some(confirmation);
        let nonce = self.native.ids.random_16();
        p.nonce = Some(nonce);
        Ok(NativeOwnerConsent {
            operation_id: operation.clone(),
            nonce,
        })
    }
    pub fn commit_owner_action(
        &self,
        owner: &ActorRef,
        consent: NativeOwnerConsent,
    ) -> Result<NativeOwnerOutcome> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let mut pending = self
            .owner_pending
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        let p = pending
            .plans
            .remove(&consent.operation_id)
            .ok_or(AutomationFault::Denied)?;
        if p.nonce != Some(consent.nonce) {
            return deny();
        }
        let record = OwnerActionRecord {
            schema_version: NextVersion,
            kind: OwnerRecordKind::OwnerActionRecord,
            plan: p.plan,
            confirmation: p.confirmation.ok_or(AutomationFault::Denied)?,
            committed_in: CommitId::from_random(self.native.ids.random_16()),
            sequence: Revision::new(1).unwrap(),
        };
        let operation = record.plan.operation_id.clone();
        self.commit(
            &mut state,
            owner.clone(),
            SegmentedDelta::OwnerAction {
                record: Box::new(record),
                effects: Box::new(p.effects),
            },
        )?;
        self.owner_outcome(&state, &operation, Sensitivity::HighlySensitive)
    }
    pub fn discard_owner_plans(&self, owner: &ActorRef) -> Result<()> {
        if *owner != self.native.descriptor.created_by || owner.actor_type != ActorType::Owner {
            return deny();
        }
        self.owner_pending
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?
            .plans
            .clear();
        Ok(())
    }
    pub fn owner_action_outcome(
        &self,
        owner: &ActorRef,
        operation: &OperationId,
        max: Sensitivity,
    ) -> Result<Option<NativeOwnerOutcome>> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if !state.owner_actions.contains_key(operation) {
            return Ok(None);
        }
        self.owner_outcome(&state, operation, max).map(Some)
    }
    fn owner_outcome(
        &self,
        state: &State,
        operation: &OperationId,
        max: Sensitivity,
    ) -> Result<NativeOwnerOutcome> {
        let (record, effects) = state
            .owner_actions
            .get(operation)
            .ok_or(AutomationFault::Denied)?;
        let data = state.authorized(&self.native.descriptor, max, self.native.now()?)?;
        let result = effects
            .correction
            .as_ref()
            .map(|c| &c.decision.input.result)
            .filter(|r| {
                effects.sensitivity <= max && data.memories.iter().any(|m| &m.memory.memory == *r)
            })
            .cloned();
        Ok(NativeOwnerOutcome {
            schema_version: NextVersion,
            kind: "native_owner_outcome".into(),
            operation_id: operation.clone(),
            plan_hash: digest(&record.plan),
            committed_in: record.committed_in.clone(),
            actor: record.plan.owner.clone(),
            action: action_name(&record.plan.action).into(),
            memory_result: result,
            physical_purge: false,
        })
    }
}
