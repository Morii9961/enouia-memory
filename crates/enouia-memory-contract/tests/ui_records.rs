mod support;
use enouia_memory_contract::{
    common::*,
    hash::sha256,
    ids::*,
    json::{Revision, canonical_bytes},
    next::*,
    time::Timestamp,
};
use serde_json::{Value, json};
use support::schema::SchemaStore;

fn fixture(n: &str) -> Value {
    support::fixture(&format!("../ui-records/{n}.json"))
}
fn source() -> CapturedSource {
    serde_json::from_value(fixture("source")).unwrap()
}
fn native(kind: &str, v: &Value) -> bool {
    let bytes = canonical_bytes(v).unwrap();
    match kind {
        "ledger" => serde_json::from_value::<OwnerActionLedger>(v.clone())
            .is_ok_and(|l| l.validate().is_empty()),
        "input" => parse_document::<AdmissionInput>(&bytes).is_ok(),
        "source" => serde_json::from_value::<CapturedSource>(v.clone())
            .is_ok_and(|s| s.validate().is_empty()),
        "catalog" => serde_json::from_value::<RecordCatalog>(v.clone())
            .is_ok_and(|c| c.validate(&[source()]).is_empty()),
        "owner-record" => parse_document::<OwnerActionRecord>(&bytes).is_ok(),
        "plan" => parse_document::<OwnerActionPlan>(&bytes).is_ok(),
        "projection" => parse_document::<ProjectionMetadata>(&bytes).is_ok(),
        "confirmation" => serde_json::from_value::<OwnerActionConfirmation>(v.clone()).is_ok(),
        "descriptor" => parse_document::<AutomationDescriptor>(&bytes).is_ok(),
        _ => panic!("unknown fixture type"),
    }
}
#[test]
fn schema_and_native_positive_negative_corpus() {
    let manifest = fixture("manifest");
    let schemas = SchemaStore::load();
    for entry in manifest["valid"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap().trim_end_matches(".json");
        let v = fixture(name);
        let errors = schemas.validate(entry["schema"].as_str().unwrap(), &v);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert!(native(entry["type"].as_str().unwrap(), &v), "{name}");
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let entry = manifest["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == case["base"])
            .unwrap();
        let mut v = fixture(case["base"].as_str().unwrap().trim_end_matches(".json"));
        support::apply_ops(&mut v, &case["ops"]);
        assert_eq!(
            schemas
                .validate(entry["schema"].as_str().unwrap(), &v)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(
            !native(entry["type"].as_str().unwrap(), &v),
            "{}",
            case["id"]
        );
    }
}
#[test]
fn scalar_quote_offsets_are_exact_and_copy_the_actual_speaker() {
    let mut s = source();
    s.text = "甲😀乙\nfull-width：Ａ".into();
    s.record.content_hash = sha256(s.text.as_bytes());
    let mut q: QuotedEvidence =
        serde_json::from_value(fixture("input")["record_fields"]["evidence"][0].clone()).unwrap();
    q.reference.object_hash = s.record.content_hash.clone();
    q.quote = "😀乙".into();
    q.start = 1;
    q.end = 3;
    assert!(q.resolves(&[s.clone()]));
    q.start = 3;
    q.end = 5;
    assert!(!q.resolves(&[s.clone()]));
    q.quote = "A".into();
    q.start = 14;
    q.end = 15;
    assert!(!q.resolves(&[s.clone()]));
    q.quote = s.text.clone();
    q.start = 0;
    q.end = s.text.chars().count() as u32;
    s.record.speaker_role = SpeakerRole::Assistant;
    s.record.evidence_class = EvidenceClass::ModelClaim;
    assert!(!q.resolves(&[s.clone()]));
    q.speaker = SpeakerRole::Assistant;
    q.reference.evidence_class = EvidenceClass::ModelClaim;
    assert!(q.resolves(&[s]));
}
#[test]
fn time_precision_bounds_are_independent_and_cannot_invent_unknown_dates() {
    let mut t: RecordTime =
        serde_json::from_value(fixture("input")["record_fields"]["event_time"].clone()).unwrap();
    assert!(t.valid());
    t.from = Some(Timestamp::parse("2026-03-01T00:00:00.000Z").unwrap());
    assert!(!t.valid());
    t.precision = RecordPrecision::Month;
    t.basis = TimeBasis::Stated;
    t.tz = Some("Asia/Shanghai".into());
    assert!(!t.valid());
    t.until = Some(Timestamp::parse("2026-04-01T00:00:00.000Z").unwrap());
    assert!(t.valid());
    t.bound = TimeBound::Span;
    assert!(t.valid());
    t.until = t.from.clone();
    assert!(!t.valid());
    assert!(valid_content_lang("zh-Hans"));
    assert!(valid_content_lang("und"));
    assert!(!valid_content_lang("zh_Hans"));
    assert!(!valid_content_lang("en-a"));
    assert!(!valid_content_lang("en-US-US"));
    assert!(!valid_content_lang("de-1901-1901"));
    assert!(valid_content_lang("de-CH-1901"));
    assert!(!valid_timezone("Asia/../Shanghai"));
}
#[test]
fn owner_confirmation_requires_native_owner_exact_plan_snapshot_and_time() {
    let plan: OwnerActionPlan =
        parse_document(&canonical_bytes(&fixture("plan")).unwrap()).unwrap();
    let confirmation: OwnerActionConfirmation =
        serde_json::from_value(fixture("confirmation")).unwrap();
    assert_eq!(confirmation.plan_hash, digest(&plan));
    let check = |p: &OwnerActionPlan,
                 c: &OwnerActionConfirmation,
                 a: &ActorRef,
                 s: &SnapshotBinding,
                 t: &Timestamp| validate_owner_action_binding(p, c, a, s, t);
    assert!(
        check(
            &plan,
            &confirmation,
            &plan.owner,
            &plan.snapshot,
            &confirmation.confirmed_at
        )
        .is_ok()
    );
    for actor_type in [
        ActorType::System,
        ActorType::Agent,
        ActorType::Provider,
        ActorType::Client,
    ] {
        let actor = ActorRef {
            actor_type,
            ..plan.owner.clone()
        };
        assert!(
            check(
                &plan,
                &confirmation,
                &actor,
                &plan.snapshot,
                &confirmation.confirmed_at
            )
            .is_err()
        );
        let mut fake = plan.clone();
        fake.owner = actor.clone();
        let mut c = confirmation.clone();
        c.owner = actor.clone();
        c.plan_hash = digest(&fake);
        assert!(check(&fake, &c, &actor, &plan.snapshot, &c.confirmed_at).is_err());
    }
    let mut stale = plan.snapshot.clone();
    stale.deletion_watermark = Revision::new(2).unwrap();
    assert!(
        check(
            &plan,
            &confirmation,
            &plan.owner,
            &stale,
            &confirmation.confirmed_at
        )
        .is_err()
    );
    let mut changed = plan.clone();
    changed.action = OwnerAction::Withdraw {
        target: serde_json::from_value(fixture("plan")["action"]["target"].clone()).unwrap(),
        reason: Some("changed".into()),
    };
    assert!(
        check(
            &changed,
            &confirmation,
            &plan.owner,
            &plan.snapshot,
            &confirmation.confirmed_at
        )
        .is_err()
    );
    assert!(
        check(
            &plan,
            &confirmation,
            &plan.owner,
            &plan.snapshot,
            &plan.expires_at
        )
        .is_err()
    );
}
#[test]
fn projection_snapshot_policy_deletion_withdrawal_scope_algorithm_changes_invalidate() {
    let p: ProjectionMetadata =
        parse_document(&canonical_bytes(&fixture("projection")).unwrap()).unwrap();
    let one = Revision::new(1).unwrap();
    let two = Revision::new(2).unwrap();
    assert!(p.matches_current(&p.snapshot, one, &p.scope_hash, &p.algorithm, one));
    for key in [
        "base_commit_id",
        "primary_epoch",
        "policy_watermark",
        "deletion_watermark",
    ] {
        let mut v = serde_json::to_value(&p.snapshot).unwrap();
        v[key] = if key == "base_commit_id" {
            json!(CommitId::from_random([9; 16]))
        } else {
            json!(2)
        };
        let s = serde_json::from_value(v).unwrap();
        assert!(!p.matches_current(&s, one, &p.scope_hash, &p.algorithm, one));
    }
    assert!(!p.matches_current(&p.snapshot, two, &p.scope_hash, &p.algorithm, one));
    assert!(!p.matches_current(&p.snapshot, one, &sha256(b"other scope"), &p.algorithm, one));
    assert!(!p.matches_current(&p.snapshot, one, &p.scope_hash, "new-algorithm", one));
    assert!(!p.matches_current(&p.snapshot, one, &p.scope_hash, &p.algorithm, two));
}
#[test]
fn three_message_branches_resolve_independently_and_same_names_keep_distinct_ids() {
    let mut catalog: RecordCatalog = serde_json::from_value(fixture("catalog")).unwrap();
    let original = source();
    let mut sources = vec![original.clone()];
    let mut other = catalog.entities[0].clone();
    other.entity_id = format!(
        "ent_{}",
        enouia_memory_contract::ids::format_uuid_v4([8; 16])
    );
    catalog.entities.push(other); // same display name does not merge identity
    for n in 2..=3 {
        let mut s = original.clone();
        s.record.source_id = SourceId::from_random([n; 16]);
        s.record.original_message_id = Some(format!("message-{n}"));
        s.record.parent_source_ids =
            enouia_memory_contract::json::Knowable::Known(vec![original.record.source_id.clone()]);
        s.record.branch_id =
            enouia_memory_contract::json::Knowable::Known(Some(BranchId::from_random([n; 16])));
        let c = &mut catalog.conversations[0];
        let branch = BranchId::from_random([n; 16]);
        c.branches.push(branch.clone());
        c.turns.push(ConversationTurn {
            source: s.reference(),
            branch_id: branch,
            turn_id: TurnId::from_random([n; 16]),
            sequence: u32::from(n),
            parent_source: Some(original.reference()),
        });
        c.turn_count += 1;
        sources.push(s);
    }
    assert!(catalog.validate(&sources).is_empty());
    assert_eq!(catalog.entities.len(), 2);
    for s in &sources {
        let mut q: QuotedEvidence =
            serde_json::from_value(fixture("input")["record_fields"]["evidence"][0].clone())
                .unwrap();
        q.reference.source_id = s.record.source_id.clone();
        assert!(q.resolves(&sources));
        assert!(
            catalog.conversations[0]
                .turns
                .iter()
                .any(|t| t.source == s.reference())
        );
    }
    sources.pop();
    assert!(!catalog.validate(&sources).is_empty());
}
#[test]
fn annotations_do_not_make_native_generated_result_ids_into_new_logical_work() {
    let input: AdmissionInput = serde_json::from_value(fixture("input")).unwrap();
    let mut retry = input.clone();
    retry.operation_id = OperationId::from_random([7; 16]);
    retry.result.memory_id = MemoryId::from_random([8; 16]);
    retry
        .record_fields
        .as_mut()
        .unwrap()
        .annotations
        .as_mut()
        .unwrap()
        .memory = retry.result.clone();
    assert!(retry.validate().is_empty());
    assert_ne!(digest(&input), digest(&retry));
    assert_eq!(
        admission_batch_key(std::slice::from_ref(&input)),
        admission_batch_key(std::slice::from_ref(&retry))
    );
    retry.record_fields.as_mut().unwrap().title = "Different formation request".into();
    assert_ne!(admission_batch_key(&[input]), admission_batch_key(&[retry]));
}
