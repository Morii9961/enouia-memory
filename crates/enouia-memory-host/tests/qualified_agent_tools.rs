#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, json::Revision, next::*, time::Timestamp};
use enouia_memory_host::automation::{owner::NativeMemoryOwner, qualified_agent::*};
use enouia_memory_vault::{
    automation::segmented::{SegmentedAgentScope, SegmentedAutomationVault},
    fault::Faults,
};
use fixture::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

fn env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
fn agent(n: u32) -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", n)).unwrap(),
    }
}
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn scope(e: &Env, ids: &[u32], source_read: bool) -> SegmentedAgentScope {
    let p = e.policy(1, 1, 8);
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: vec![p.policy_id],
        source_ids: ids
            .iter()
            .map(|n| SourceId::parse(&id("src", *n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn request(n: u32) -> RequestId {
    RequestId::parse(&id("req", n)).unwrap()
}
fn query() -> RecallQuery {
    RecallQuery {
        text: String::new(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Browse,
        limit: 8,
        max_bytes: 16_384,
    }
}
fn bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                if relative == Path::new("indexes/host.lock") || relative == Path::new("vault/LOCK")
                {
                    continue;
                }
                out.insert(relative.to_path_buf(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn no_sequence(value: &Value) {
    match value {
        Value::Object(map) => {
            assert!(!map.contains_key("sequence"));
            for value in map.values() {
                no_sequence(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                no_sequence(item)
            }
        }
        _ => {}
    }
}
fn target(v: &SegmentedAutomationVault, e: &Env, receipt: &MemoryReceipt) -> TargetRef {
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
        data.memories
            .iter()
            .find(|m| m.memory.memory == receipt.result)
            .unwrap()
            .target
            .clone()
    })
    .unwrap()
}
fn execute(owner: &NativeMemoryOwner, action: OwnerAction) {
    let preview = owner.plan(action, Sensitivity::Private, None).unwrap();
    let consent = owner
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    owner.commit(consent).unwrap();
}
fn publish(v: &SegmentedAutomationVault, e: &Env, input: AdmissionInput) -> MemoryReceipt {
    let input = v.prepare(&e.descriptor.system_actor, input).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &input).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: enouia_memory_contract::hash::sha256(response.as_bytes()),
    };
    v.apply_batch(
        &e.descriptor.system_actor,
        vec![input],
        vec![response],
        vec![binding],
    )
    .unwrap()
    .remove(0)
}

#[test]
fn related_edges_only_use_selected_current_endpoints_and_never_claim_independent_facts() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let sources: Vec<_> = (1..=3).map(|n| e.source(n, Sensitivity::Public)).collect();
    let mut inputs: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(n, s)| e.input(s, &p, n as u32 + 1))
        .collect();
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.conversations.clear();
    let mut entity = catalog.entities.remove(0);
    entity.entity_id = id("ent", 500);
    entity.display_name = "Cedar".into();
    entity.aliases.clear();
    entity.evidence = inputs[0].record_fields.as_ref().unwrap().evidence.clone();
    entity.sensitivity = Sensitivity::Public;
    catalog.entities = vec![entity.clone()];
    v.capture(&e.descriptor.system_actor, sources, catalog)
        .unwrap();
    let mut receipts = Vec::new();
    for input in &mut inputs {
        let evidence = input.record_fields.as_ref().unwrap().evidence.clone();
        input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
            entity_id: entity.entity_id.clone(),
            role: EntityRole::Mentioned,
            evidence,
        }];
        receipts.push(publish(&v, &e, input.clone()));
    }
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let reader = h
        .reader(
            &e.descriptor.created_by,
            agent(77),
            scope(&e, &[1, 2], false),
        )
        .unwrap();
    let before = bytes(e.root.path());
    let result = reader
        .related(&profile(), &receipts[0].result.memory_id, None, 64 * 1024)
        .unwrap()
        .unwrap();
    assert_eq!(result["recorded_edges"].as_array().unwrap().len(), 1);
    assert_eq!(result["recorded_edges"][0]["relation"], "same_entity");
    assert_eq!(result["recorded_edges"][0]["class"], "recorded_relation");
    assert_eq!(
        result["recorded_edges"][0]["to"],
        json!(receipts[1].result.memory_id)
    );
    assert!(result["suggested_edges"].as_array().unwrap().is_empty());
    let coverage = result["edge_coverage"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["relation"] == "same_entity")
        .unwrap();
    assert_eq!(coverage["candidates"], 1);
    assert_eq!(coverage["retained"], 1);
    assert_eq!(
        result["unavailable"],
        json!(["same_conversation", "semantic_neighbors"])
    );
    assert!(
        !result
            .to_string()
            .contains(receipts[2].result.memory_id.as_str())
    );
    assert!(
        reader
            .related(&profile(), &receipts[2].result.memory_id, None, 64 * 1024)
            .unwrap()
            .is_none()
    );
    no_sequence(&result);
    assert_eq!(bytes(e.root.path()), before);
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[1].result.memory_id.clone()],
    )
    .unwrap();
    let result = reader
        .related(&profile(), &receipts[0].result.memory_id, None, 64 * 1024)
        .unwrap()
        .unwrap();
    assert!(result["recorded_edges"].as_array().unwrap().is_empty());
    assert!(
        reader
            .related(
                &profile(),
                &receipts[0].result.memory_id,
                Some(&serde_json::from_value(result["snapshot"].clone()).unwrap()),
                4096
            )
            .is_ok()
    );
    reader.revoke();
    assert!(
        reader
            .related(&profile(), &receipts[0].result.memory_id, None, 64 * 1024)
            .is_err()
    );
}

#[test]
fn detail_and_sources_enforce_independent_source_permission_utf8_bounds_and_absence() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let denied = h
        .reader(
            &e.descriptor.created_by,
            agent(77),
            scope(&e, &[1, 2], false),
        )
        .unwrap();
    let sid = SourceId::parse(&id("src", 2)).unwrap();
    assert!(denied.source(&sid, None, 0, None, 8).is_err());
    assert!(
        denied
            .detail(
                &profile(),
                &receipts[1].result.memory_id,
                None,
                true,
                64 * 1024
            )
            .is_err()
    );
    let detail = denied
        .detail(
            &profile(),
            &receipts[1].result.memory_id,
            None,
            false,
            64 * 1024,
        )
        .unwrap()
        .unwrap();
    assert_eq!(detail["source_read"], false);
    assert_eq!(detail["source_count_in_scope"], 0);
    assert_eq!(
        detail["item"]["base"]["base"]["knowledge"],
        "attributed_statement"
    );
    assert!(
        !detail["item"]["record"]["fields"]["evidence"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        denied
            .detail(
                &profile(),
                &receipts[7].result.memory_id,
                None,
                false,
                64 * 1024
            )
            .unwrap()
            .is_none()
    );
    let allowed = h
        .reader(
            &e.descriptor.created_by,
            agent(78),
            scope(&e, &[1, 2], true),
        )
        .unwrap();
    let before = bytes(e.root.path());
    let excerpt = allowed
        .source(&sid, Some(Revision::new(1).unwrap()), 0, None, 4)
        .unwrap()
        .unwrap();
    assert_eq!(excerpt["excerpt"], "合");
    assert_eq!(excerpt["next_start"], 3);
    assert!(allowed.source(&sid, None, 1, Some(4), 8).is_err());
    assert!(allowed.source(&sid, None, 0, Some(1), 8).is_err());
    assert!(
        allowed
            .source(&sid, Some(Revision::new(2).unwrap()), 0, None, 8)
            .unwrap()
            .is_none()
    );
    for n in [8, 999] {
        assert!(
            allowed
                .source(&SourceId::parse(&id("src", n)).unwrap(), None, 0, None, 8)
                .unwrap()
                .is_none()
        );
    }
    for n in 0..100 {
        let result = allowed
            .detail(
                &profile(),
                &receipts[n % 2].result.memory_id,
                None,
                false,
                64 * 1024,
            )
            .unwrap()
            .unwrap();
        no_sequence(&result);
    }
    assert_eq!(bytes(e.root.path()), before);
    allowed.revoke();
    assert!(allowed.source(&sid, None, 0, None, 8).is_err());
    assert!(
        allowed
            .detail(
                &profile(),
                &receipts[0].result.memory_id,
                None,
                false,
                64 * 1024
            )
            .is_err()
    );
}

#[test]
fn qualified_owner_protection_drives_recall_and_context_is_the_actual_scoped_trace() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let owner =
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    execute(
        &owner,
        OwnerAction::Attention {
            target: target(&v, &e, &receipts[0]),
            annotation: AttentionAnnotation::Protect,
            until: None,
        },
    );
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let reader = h
        .reader(
            &e.descriptor.created_by,
            agent(77),
            scope(&e, &[1, 2], false),
        )
        .unwrap();
    let mut q = query();
    q.at = Timestamp::from_unix_ms(NOW + 365 * 86_400_000).unwrap();
    let before = bytes(e.root.path());
    let result = reader.recall(&profile(), &q, request(1)).unwrap();
    assert_eq!(result["trace"]["considered"], 2);
    assert_eq!(result["trace"]["selected"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["trace"]["selected"][0]["memory"],
        json!(receipts[0].result)
    );
    assert_eq!(result["trace"]["selected"][0]["attention"], "protected");
    assert_eq!(result["items"][0]["base"]["base"]["attention"], "protected");
    assert_eq!(
        result["items"][0]["base"]["base"]["knowledge"],
        "attributed_statement"
    );
    assert!(result["rendered_context"].as_str().unwrap().len() <= q.max_bytes);
    assert_eq!(
        result["trace"]["rendered_bytes"].as_u64().unwrap() as usize,
        result["rendered_context"].as_str().unwrap().len()
    );
    assert_eq!(reader.context_trace(&request(1)).unwrap().unwrap(), result);
    assert!(reader.context_trace(&request(2)).unwrap().is_none());
    no_sequence(&result);
    assert_eq!(bytes(e.root.path()), before);
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[0].result.memory_id.clone()],
    )
    .unwrap();
    assert!(reader.context_trace(&request(1)).is_err());
    let result = reader.recall(&profile(), &q, request(2)).unwrap();
    assert_eq!(result["trace"]["considered"], 1);
    assert!(result["trace"]["selected"].as_array().unwrap().is_empty());
    h.lock().unwrap();
    h.unlock(&e.descriptor.created_by).unwrap();
    assert!(reader.recall(&profile(), &q, request(3)).is_err());
    assert!(reader.context_trace(&request(2)).is_err());
}

#[test]
fn qualified_selector_keeps_old_age_only_selection_and_requires_one_state_per_candidate() {
    let e = env();
    let v = e.create(Faults::none());
    let receipts = e.populate(&v, 8);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
        let entry = data
            .memories
            .iter()
            .find(|m| m.memory.memory == receipts[0].result)
            .unwrap();
        let sources = &data.sources;
        let source_time = sources.values().next().unwrap().record.captured_at.clone();
        let candidates = [RecallCandidate {
            memory: &entry.memory,
            source_time,
            rendered_bytes: 10,
        }];
        let p = profile();
        let mut q = query();
        q.at = Timestamp::from_unix_ms(NOW + 365 * 86_400_000).unwrap();
        let old = select_catalog_recall(&p, &q, &data.snapshot, &data.now, &candidates).unwrap();
        assert!(old.selected.is_empty());
        let new = select_qualified_catalog_recall(
            &p,
            &q,
            &data.snapshot,
            &data.now,
            &candidates,
            &[AttentionState::Protected],
        )
        .unwrap();
        assert_eq!(new.selected[0].memory, receipts[0].result);
        assert!(
            select_qualified_catalog_recall(&p, &q, &data.snapshot, &data.now, &candidates, &[])
                .is_err()
        );
        q.max_bytes = 1;
        let budget = select_qualified_catalog_recall(
            &p,
            &q,
            &data.snapshot,
            &data.now,
            &candidates,
            &[AttentionState::Protected],
        )
        .unwrap();
        assert!(budget.selected.is_empty());
        assert_eq!(budget.excluded[0].1, RecallExclusionReason::Budget);
        let staged = data
            .memories
            .iter()
            .find(|m| m.memory.memory == receipts[2].result)
            .unwrap();
        let staged_candidates = [RecallCandidate {
            memory: &staged.memory,
            source_time: candidates[0].source_time.clone(),
            rendered_bytes: 1,
        }];
        q.max_bytes = 16_384;
        let protected_staged = select_qualified_catalog_recall(
            &p,
            &q,
            &data.snapshot,
            &data.now,
            &staged_candidates,
            &[AttentionState::Protected],
        )
        .unwrap();
        assert!(protected_staged.selected.is_empty());
        assert_eq!(
            protected_staged.excluded[0].1,
            RecallExclusionReason::NotAdopted
        );
    })
    .unwrap();
}

#[test]
fn native_detail_retains_original_history_and_declares_actual_byte_truncation() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let t = target(&v, &e, &receipts[0]);
    let mut dimensions = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories
                .iter()
                .find(|m| m.memory.memory == receipts[0].result)
                .unwrap()
                .memory
                .decision
                .input
                .dimensions
                .clone()
                .unwrap()
        })
        .unwrap();
    dimensions.change = FormationChange::Correction;
    dimensions.event_time = None;
    dimensions.valid_time = None;
    dimensions.derived_from.clear();
    dimensions.counter_sources.clear();
    dimensions.reverts_to = None;
    let unknown = RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    };
    let owner =
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let preview = owner
        .plan(
            OwnerAction::Correct {
                target: t,
                source_id: SourceId::parse(&id("src", 999)).unwrap(),
                assertion_text: "I prefer mornings; synthetic native correction.".into(),
            },
            Sensitivity::Private,
            Some(OwnerCorrectionMetadata {
                title: "Native corrected preference".into(),
                content_lang: "en".into(),
                sensitivity: Sensitivity::Public,
                interpretation: Interpretation::ClearSelfReport,
                dimensions,
                event_time: unknown.clone(),
                valid_time: unknown,
                entities: vec![],
            }),
        )
        .unwrap();
    let consent = owner
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    owner.commit(consent).unwrap();
    let current = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories
                .iter()
                .find(|m| m.memory.memory.memory_id == receipts[0].result.memory_id)
                .unwrap()
                .memory
                .decision
                .input
                .clone()
        })
        .unwrap();
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let mut s = scope(&e, &[1], true);
    s.max_sensitivity = Sensitivity::Public;
    s.policy_ids.push(current.policy.policy_id);
    s.source_ids
        .extend(current.input_refs.iter().map(|r| r.source_id.clone()));
    let reader = h.reader(&e.descriptor.created_by, agent(77), s).unwrap();
    let full = reader
        .detail(
            &profile(),
            &receipts[0].result.memory_id,
            None,
            true,
            64 * 1024,
        )
        .unwrap()
        .unwrap();
    assert_eq!(full["history_count_in_scope"], 2);
    assert_eq!(full["source_count_in_scope"], 2);
    assert_eq!(full["history"][0]["memory"]["memory"]["revision"], 1);
    assert_eq!(full["history"][0]["is_current"], false);
    assert_eq!(
        full["history"][0]["record"]["fields"]["formation"]["prompt_version"],
        "synthetic-ui-1"
    );
    assert_eq!(
        full["history"][1]["record"]["fields"]["formation"]["prompt_version"],
        "owner-native-literal-1"
    );
    assert!(full["item"]["base"]["owner_marks"]["corrected"].is_object());
    no_sequence(&full);
    let budget = enouia_memory_contract::json::canonical_bytes(&full)
        .unwrap()
        .len()
        - 100;
    assert!(budget >= 4096);
    let trimmed = reader
        .detail(
            &profile(),
            &receipts[0].result.memory_id,
            None,
            true,
            budget,
        )
        .unwrap()
        .unwrap();
    assert!(
        enouia_memory_contract::json::canonical_bytes(&trimmed)
            .unwrap()
            .len()
            <= budget
    );
    assert_eq!(trimmed["complete_in_scope"], false);
    assert_eq!(trimmed["history_count_in_scope"], 2);
    assert_eq!(trimmed["item"], full["item"]);
    let mut stale = v.snapshot().unwrap();
    stale.deletion_watermark = Revision::new(999).unwrap();
    assert!(
        reader
            .detail(
                &profile(),
                &receipts[0].result.memory_id,
                Some(&stale),
                false,
                64 * 1024
            )
            .is_err()
    );
}
