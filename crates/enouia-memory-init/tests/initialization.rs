#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*, hash::sha256, json::canonical_bytes, next::*, time::Timestamp,
};
use enouia_memory_init::{parse::*, pipeline::*};
use enouia_memory_vault::{OsIdSource, fault::Faults, fs::ManagedRoot};
use fixture::*;
use serde_json::{Value, json};
use std::sync::Arc;

fn materials() -> Vec<Material> {
    vec![Material{alias:"synthetic-chat".into(),format:Format::Auto,account_alias:"synthetic".into(),bytes:serde_json::to_vec(&json!([{
        "conversation_id":"synthetic-c1","title":"Synthetic choices","current_node":"n3","mapping":{
            "n1":{"parent":null,"message":{"id":"m1","author":{"role":"user","name":"Synthetic speaker"},"create_time":1781085600,
                "content_lang":"en","timezone":"Etc/UTC","content":{"parts":["I prefer quiet rooms. I enjoy pottery."]}}},
            "n2":{"parent":"n1","message":{"id":"m2","author":{"role":"assistant"},"create_time":1781085601,
                "content_lang":"en","metadata":{"model_slug":"synthetic-model"},"content":{"parts":["Maybe you prefer noise."]}}},
            "n3":{"parent":"n1","message":{"id":"m3","author":{"role":"user"},"create_time":1781085602,
                "content_lang":"en","content":{"parts":["I changed my preference to music."]}}}
        }}])).unwrap()}]
}
fn statement(source_index: usize, quote: &str) -> Statement {
    Statement {
        source_index,
        quote: quote.into(),
        title: quote.trim_end_matches('.').into(),
        interpretation: Interpretation::ClearSelfReport,
        dimensions: dimensions(FormationKind::Preference),
        event_time: unknown_time(),
        valid_time: unknown_time(),
        entities: vec![],
        note: Some("Scripted synthetic self-report.".into()),
        annotations: AnalysisAnnotations::default(),
        revises_statement: None,
        derives_from_statements: vec![],
    }
}
fn plan(e: &Env, prep: &Preparation, statements: &[Statement], mode: FormationMode) -> Plan {
    Plan::build(
        prep,
        statements,
        e.policy(1, 1, 1),
        mode,
        Timestamp::from_unix_ms(NOW).unwrap(),
        &OsIdSource,
    )
    .unwrap()
}
fn initializer(e: &Env) -> Initializer {
    Initializer::create_synthetic(
        &e.root,
        e.descriptor.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap()
}
fn bundle(init: &Initializer) -> Value {
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    init.projection_bundle(&profile, "Asia/Shanghai", "2026-06-01", "2026-11-01")
        .unwrap()
}

#[test]
fn full_native_initialization_preflight_reopen_revision_and_seven_deletion_projections() {
    let e = Env::new();
    let mut inputs = materials();
    let mut raw: Value = serde_json::from_slice(&inputs[0].bytes).unwrap();
    raw[0]["mapping"]["n1"]["message"]["content"]["parts"] = json!([
        "I prefer quiet rooms with real Synthetic Alex. I enjoy pottery with fictional Synthetic Alex."
    ]);
    inputs[0].bytes = serde_json::to_vec(&raw).unwrap();
    let prep = prepare(&inputs).unwrap();
    assert_eq!(prep.messages.len(), 3);
    assert_eq!(
        prep.messages[1].assistant_model.as_deref(),
        Some("synthetic-model")
    );
    assert_eq!(prep.messages[1].evidence, EvidenceClass::ModelClaim);
    let mut a = statement(0, "I prefer quiet rooms with real Synthetic Alex.");
    a.entities = vec![(
        "Synthetic Alex".into(),
        EntityType::Person,
        EntityReality::Real,
        EntityRole::Mentioned,
    )];
    let mut b = statement(0, "I enjoy pottery with fictional Synthetic Alex.");
    b.entities = vec![(
        "Synthetic Alex".into(),
        EntityType::Person,
        EntityReality::Fictional,
        EntityRole::Mentioned,
    )];
    let mut c = statement(1, "Maybe you prefer noise.");
    c.interpretation = Interpretation::Inference;
    let mut d = statement(2, "I changed my preference to music.");
    d.dimensions.change = FormationChange::Update;
    d.revises_statement = Some(0);
    let statements = vec![a, b, c, d];
    let p = plan(&e, &prep, &statements, FormationMode::Backfill);
    assert_eq!(p.preflight.exact_quotes, 4);
    assert_eq!(p.preflight.proposed_staged, 1);
    assert!(std::fs::read_dir(e.root.path()).unwrap().next().is_none());
    let mut init = initializer(&e);
    let id = init.enqueue(p.clone(), &prep).unwrap();
    assert_eq!(
        init.enqueue(plan(&e, &prep, &statements, FormationMode::Backfill), &prep)
            .unwrap(),
        id
    );
    assert!(init.step().unwrap()); // capture only
    assert!(init.step().unwrap()); // first formation
    drop(init);
    let mut init = Initializer::open_synthetic(
        &e.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    while init.step().unwrap() {}
    assert_eq!(init.plans()[0].completed(), 4);
    let report = bundle(&init);
    assert_eq!(report["coverage"]["title"]["percent"], 100.0);
    assert_eq!(report["coverage"]["evidence"]["percent"], 100.0);
    assert_eq!(
        report["coverage"]["conversation_evidence"]["percent"],
        100.0
    );
    assert_eq!(report["coverage"]["same_name_unmerged_entities"], 2);
    assert_eq!(report["projections"].as_object().unwrap().len(), 7);
    // Two forks remain distinct, all evidence resolves to a captured turn.
    assert_eq!(p.catalog.conversations[0].branches.len(), 3);
    let before = init.vault.snapshot().unwrap();
    assert!(!init.step().unwrap());
    assert_eq!(init.vault.snapshot().unwrap(), before);
    let target = init.plans()[0].receipts()[1].result.memory_id.clone();
    init.vault
        .delete(&e.descriptor.created_by, vec![], vec![target.clone()])
        .unwrap();
    let after = bundle(&init);
    assert_ne!(
        after["selection"]["snapshot"],
        report["selection"]["snapshot"]
    );
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .contains(target.as_str())
    );
    assert_eq!(after["coverage"]["projection_kinds_built"], 7);
    // Received bytes have not been rewritten by import, deletion or rebuild.
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    for (hash, bytes) in prep.objects {
        assert_eq!(
            managed
                .read_bounded(
                    &enouia_memory_contract::layout::raw_object(&hash),
                    MAX_MATERIAL_BYTES
                )
                .unwrap(),
            Some(bytes)
        );
    }
}

#[test]
fn unknown_application_is_lookup_only_and_does_not_reapply() {
    let e = Env::new();
    let prep = prepare(&materials()).unwrap();
    let p = plan(
        &e,
        &prep,
        &[statement(0, "I enjoy pottery.")],
        FormationMode::Backfill,
    );
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let snapshot = init.vault.snapshot().unwrap();
    drop(init);
    // Emulate interruption after witnessed commit but before local cursor save.
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    let path = "sync-state/initialization-1.json";
    let mut journal: Value = serde_json::from_slice(
        &managed
            .read_bounded(path, 16 * 1024 * 1024)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    journal["plans"][0]["work"][0]["receipt"] = Value::Null;
    journal["plans"][0]["work"][0]["state"] = "applying".into();
    managed
        .write_atomic(path, &canonical_bytes(&journal).unwrap())
        .unwrap();
    let mut init = Initializer::open_synthetic(
        &e.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert!(init.step().unwrap());
    assert_eq!(init.vault.snapshot().unwrap(), snapshot);
    assert!(!init.step().unwrap());
}

#[test]
fn parser_preserves_unknowns_duplicate_bytes_roles_and_unsupported_objects() {
    let mut inputs = materials();
    let mut duplicate = inputs[0].clone();
    duplicate.alias = "same-bytes".into();
    inputs.push(duplicate);
    inputs.push(Material {
        alias: "opaque".into(),
        format: Format::Unsupported,
        bytes: vec![0, 255, 5],
        account_alias: "synthetic".into(),
    });
    let prep = prepare(&inputs).unwrap();
    assert_eq!(prep.duplicates, 1);
    assert_eq!(prep.unsupported, 1);
    assert_eq!(prep.objects.len(), 2);
    assert_eq!(sha256(&inputs[0].bytes), prep.inputs[0].1);
    let e = Env::new();
    let mut bad = statement(0, "A fabricated quote.");
    assert!(
        Plan::build(
            &prep,
            &[bad.clone()],
            e.policy(1, 1, 1),
            FormationMode::Backfill,
            Timestamp::from_unix_ms(NOW).unwrap(),
            &OsIdSource
        )
        .is_err()
    );
    bad.quote = "I enjoy pottery.".into();
    bad.event_time.precision = RecordPrecision::Day;
    assert!(
        Plan::build(
            &prep,
            &[bad],
            e.policy(1, 1, 1),
            FormationMode::Backfill,
            Timestamp::from_unix_ms(NOW).unwrap(),
            &OsIdSource
        )
        .is_err()
    );
}

#[test]
fn s01_to_s32_scripted_inputs_pass_real_native_formation_but_are_not_model_scores() {
    let e = Env::new();
    let suite = enouia_memory_init::synthetic::suite();
    assert_eq!(suite.scenarios.as_array().unwrap().len(), 32);
    let p = plan(
        &e,
        &suite.preparation,
        &suite.statements,
        FormationMode::Backfill,
    );
    let mut init = initializer(&e);
    init.enqueue(p, &suite.preparation).unwrap();
    while init.step().unwrap() {}
    assert_eq!(init.plans()[0].completed(), 32);
    let b = bundle(&init);
    assert_eq!(b["coverage"]["title"]["denominator"], 32);
    assert_eq!(b["coverage"]["evidence"]["percent"], 100.0);
    assert_eq!(b["coverage"]["unknown_time"]["percent"], 100.0);
}

#[test]
fn live_preempts_backfill_and_unwitnessed_unknown_never_reapplies() {
    let e = Env::new();
    let prep = prepare(&materials()).unwrap();
    let mut init = initializer(&e);
    init.enqueue(
        plan(
            &e,
            &prep,
            &[statement(0, "I enjoy pottery.")],
            FormationMode::Backfill,
        ),
        &prep,
    )
    .unwrap();
    let live_inputs = vec![Material {
        alias: "live-synthetic".into(),
        format: Format::Document,
        bytes: b"A synthetic live note.".to_vec(),
        account_alias: "synthetic".into(),
    }];
    let live_prep = prepare(&live_inputs).unwrap();
    let mut live_statement = statement(0, "A synthetic live note.");
    live_statement.interpretation = Interpretation::Inference;
    let mut lp = plan(
        &e,
        &live_prep,
        &[live_statement.clone()],
        FormationMode::Live,
    );
    lp.policy.policy_id = enouia_memory_contract::ids::PolicyId::parse(&id("pol", 900)).unwrap();
    // Seed policy identity must match its distinct batch policy.
    let lp = Plan::build(
        &live_prep,
        &[live_statement],
        lp.policy,
        FormationMode::Live,
        Timestamp::from_unix_ms(NOW).unwrap(),
        &OsIdSource,
    )
    .unwrap();
    init.enqueue(lp, &live_prep).unwrap();
    init.step().unwrap();
    init.step().unwrap();
    assert_eq!(init.plans()[0].completed(), 0);
    assert_eq!(init.plans()[1].completed(), 1);
    init.step().unwrap(); // backfill capture
    drop(init);
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    let path = "sync-state/initialization-1.json";
    let mut journal: Value = serde_json::from_slice(
        &managed
            .read_bounded(path, 16 * 1024 * 1024)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    // A durable application intent without a receipt is never treated as queued.
    journal["plans"][0]["work"][0]["state"] = "outcome_unknown".into();
    managed
        .write_atomic(path, &canonical_bytes(&journal).unwrap())
        .unwrap();
    let mut init = Initializer::open_synthetic(
        &e.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    let before = init.vault.snapshot().unwrap();
    assert!(init.step().is_err());
    assert_eq!(init.vault.snapshot().unwrap(), before);
}

#[test]
fn theme_original_context_deletion_invalidates_all_derived_outputs() {
    let e = Env::new();
    let prep = prepare(&materials()).unwrap();
    let a = statement(0, "I enjoy pottery.");
    let b = statement(2, "I changed my preference to music.");
    let mut theme = statement(2, "I changed my preference to music.");
    theme.title = "Synthetic preference family".into();
    theme.interpretation = Interpretation::Inference;
    theme.dimensions = dimensions(FormationKind::Theme);
    theme.dimensions.change = FormationChange::Synthesis;
    theme.derives_from_statements = vec![0, 1];
    let p = plan(&e, &prep, &[a, b, theme], FormationMode::Backfill);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let receipts = init.plans()[0].receipts();
    let theme_id = receipts[2].result.memory_id.clone();
    let before = bundle(&init);
    assert!(
        serde_json::to_string(&before)
            .unwrap()
            .contains(theme_id.as_str())
    );
    assert!(
        !before["projections"]["themes"]["pages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let context_source = init.plans()[0].sources[0].record.source_id.clone();
    init.vault
        .delete(&e.descriptor.created_by, vec![context_source], vec![])
        .unwrap();
    let after = bundle(&init);
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .contains(theme_id.as_str())
    );
}

#[test]
fn selected_runtime_claude_document_and_bucket_metadata_survive_without_owner_promotion() {
    let inputs=vec![
        Material{alias:"runtime".into(),format:Format::Auto,account_alias:"synthetic".into(),bytes:serde_json::to_vec(&json!({"format":"enouia-runtime-session/1","sessions":[{"session_key":"old-runtime","title":"Synthetic old session","events":[{"event_key":"old-1","sequence":7,"parent_event_key":null,"role":"user","text":"I enjoy paper crafts.","occurred_at":"2026-06-10T10:00:00.000Z","branch_id":"branch-a","turn_id":"turn-a","event_kind":"user_message","provider_binding":"synthetic-provider","content_lang":"en","original_timezone":"Etc/UTC"}]}]})).unwrap()},
        Material{alias:"claude".into(),format:Format::Claude,account_alias:"synthetic".into(),bytes:serde_json::to_vec(&json!([{"uuid":"claude-c1","name":"Synthetic Claude","chat_messages":[{"uuid":"claude-m1","sender":"assistant","text":"An agent claims the owner consented.","model":"synthetic-claude","created_at":null,"attachments":[]}]}])).unwrap()},
        Material{alias:"bucket".into(),format:Format::Buckets,account_alias:"synthetic".into(),bytes:serde_json::to_vec(&json!([{"id":"bucket-1","content":"A derived bucket summary.","pinned":true,"protected":true,"domain":"fiction","feel":"calm","importance":0.8,"event_time":"2024-03","tags":["synthetic"]}])).unwrap()},
        Material{alias:"note".into(),format:Format::Document,account_alias:"synthetic".into(),bytes:b"A synthetic document with an unknown author.".to_vec()},
    ];
    let prep = prepare(&inputs).unwrap();
    assert_eq!(prep.messages.len(), 4);
    let runtime = prep
        .messages
        .iter()
        .find(|m| m.platform == "enouia-runtime")
        .unwrap();
    assert_eq!(runtime.metadata["sequence"], 7);
    assert_eq!(runtime.metadata["provider_binding"], "synthetic-provider");
    let bucket = prep
        .messages
        .iter()
        .find(|m| m.platform == "bucket-json")
        .unwrap();
    assert_eq!(bucket.metadata["pinned"], true);
    assert_eq!(bucket.evidence, EvidenceClass::ModelClaim);
    let e = Env::new();
    let statements: Vec<_> = prep
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mut s = statement(i, &m.text);
            s.interpretation = Interpretation::Inference;
            s
        })
        .collect();
    let p = plan(&e, &prep, &statements, FormationMode::Backfill);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    assert_eq!(init.plans()[0].completed(), 4);
    assert!(
        init.plans()[0]
            .sources
            .iter()
            .all(|s| s.record.manual_assertion.is_none()
                && s.record.evidence_class != EvidenceClass::UserConfirmation)
    );
}

#[test]
fn legacy_snapshot_is_read_only_and_confirmed_history_requires_valid_original_review() {
    let raw = serde_json::to_vec(&value("memory/sets/lifecycle.json")).unwrap();
    let mut prep = prepare(&[Material {
        alias: "old-snapshot".into(),
        format: Format::LegacySnapshot,
        account_alias: "synthetic".into(),
        bytes: raw.clone(),
    }])
    .unwrap();
    assert!(
        prep.messages
            .iter()
            .any(|m| m.evidence == EvidenceClass::UserConfirmation)
    );
    assert!(
        prep.messages
            .iter()
            .any(|m| m.evidence == EvidenceClass::ModelClaim)
    );
    // Scope selection is explicit; highly-sensitive originals are never lowered
    // to fit this synthetic policy. Their exact bytes remain in the snapshot.
    prep.messages
        .retain(|m| m.sensitivity <= Sensitivity::Private);
    let e = Env::new();
    let statements: Vec<_> = prep
        .messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mut s = statement(i, &m.text);
            s.title = m.text.chars().take(60).collect();
            s.interpretation = Interpretation::Ambiguous;
            s
        })
        .collect();
    let p = plan(&e, &prep, &statements, FormationMode::Backfill);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    assert_eq!(
        managed
            .read_bounded(
                &enouia_memory_contract::layout::raw_object(&sha256(&raw)),
                MAX_MATERIAL_BYTES
            )
            .unwrap(),
        Some(raw.clone())
    );
    let mut bad: Value = serde_json::from_slice(&raw).unwrap();
    bad["reviews"][0]["actor"]["actor_type"] = "system".into();
    let bad_prep = prepare(&[Material {
        alias: "forged".into(),
        format: Format::LegacySnapshot,
        account_alias: "synthetic".into(),
        bytes: serde_json::to_vec(&bad).unwrap(),
    }])
    .unwrap();
    assert!(bad_prep.messages.is_empty());
    assert_eq!(bad_prep.unparseable, 1);
}

#[test]
fn attachments_have_typed_metadata_refs_and_verified_original_bytes() {
    let bytes = include_bytes!("../../../tests/fixtures/initialization/attachments.zip").to_vec();
    let prep = prepare(&[Material {
        alias: "attachment-archive".into(),
        format: Format::Auto,
        account_alias: "synthetic".into(),
        bytes,
    }])
    .unwrap();
    assert_eq!(prep.messages.len(), 1);
    assert_eq!(prep.messages[0].attachments.len(), 2);
    let e = Env::new();
    let p = plan(&e, &prep, &[], FormationMode::Backfill);
    assert_eq!(p.sources[0].attachments.len(), 2);
    assert_eq!(p.sources[0].record.attachment_refs.len(), 2);
    assert_eq!(
        p.sources[0].attachments[0].detected_media_type.as_deref(),
        Some("image/png")
    );
    assert_eq!(
        p.sources[0].attachments[1].availability,
        enouia_memory_contract::source::Availability::Missing
    );
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    assert_eq!(init.ledger().unwrap()["received"]["verified"], 2);
    assert_eq!(
        init.plans()[0].sources[0].attachments[0].size_bytes,
        prep.messages[0].attachments[0].size_bytes
    );
}

#[test]
fn month_within_span_and_stated_instant_are_distinct_from_source_capture_time() {
    let e = Env::new();
    let mut input = materials();
    let mut raw: Value = serde_json::from_slice(&input[0].bytes).unwrap();
    raw[0]["mapping"]["n1"]["message"]["content"]["parts"] = json!([
        "In March 2024 I enjoyed pottery. Throughout March 2024 I worked in my studio. On 2024-01-01 at 16:00 UTC I visited a museum."
    ]);
    input[0].bytes = serde_json::to_vec(&raw).unwrap();
    let prep = prepare(&input).unwrap();
    let mut within = statement(0, "In March 2024 I enjoyed pottery.");
    let mut span = statement(0, "Throughout March 2024 I worked in my studio.");
    let mut instant = statement(0, "On 2024-01-01 at 16:00 UTC I visited a museum.");
    let from = Timestamp::parse("2024-03-01T00:00:00.000Z").unwrap();
    let until = Timestamp::parse("2024-04-01T00:00:00.000Z").unwrap();
    for s in [&mut within, &mut span] {
        s.dimensions.occurrence = OccurrenceAssertion::ReportedOccurred;
        s.event_time = RecordTime {
            from: Some(from.clone()),
            until: Some(until.clone()),
            precision: RecordPrecision::Month,
            bound: TimeBound::Within,
            tz: Some("Etc/UTC".into()),
            basis: TimeBasis::Stated,
        };
        s.dimensions.event_time = Some(FormationTime {
            from: from.clone(),
            until: Some(until.clone()),
        });
    }
    span.event_time.bound = TimeBound::Span;
    instant.event_time = enouia_memory_init::synthetic::instant("2024-01-01T16:00:00.000Z");
    instant.dimensions.occurrence = OccurrenceAssertion::ReportedOccurred;
    instant.dimensions.event_time = Some(FormationTime {
        from: instant.event_time.from.clone().unwrap(),
        until: None,
    });
    // These exact source sentences are labelled by a script, not a live model.
    let p = plan(&e, &prep, &[within, span, instant], FormationMode::Backfill);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let report = bundle(&init);
    assert_eq!(report["coverage"]["precision_distribution"]["month"], 2);
    assert_eq!(report["coverage"]["precision_distribution"]["instant"], 1);
    assert_ne!(
        init.plans()[0].receipts()[0].result.memory_id,
        init.plans()[0].receipts()[1].result.memory_id
    );
}

#[test]
fn repeated_export_bytes_dedupe_only_inside_the_same_account_namespace() {
    let mut input = materials();
    let mut other = input[0].clone();
    other.alias = "other-account".into();
    other.account_alias = "other".into();
    input.push(other);
    let prep = prepare(&input).unwrap();
    assert_eq!(prep.objects.len(), 1);
    assert_eq!(prep.messages.len(), 6);
    assert_eq!(prep.duplicates, 0);
    let mut repeated = input[0].clone();
    repeated.alias = "repeated-export".into();
    repeated.bytes.push(b'\n');
    let prep = prepare(&[input[0].clone(), repeated]).unwrap();
    assert_eq!(prep.objects.len(), 2);
    assert_eq!(prep.messages.len(), 3);
    assert_eq!(prep.duplicate_messages, 3);
    let original = materials().remove(0);
    let mut changed: Value = serde_json::from_slice(&original.bytes).unwrap();
    changed[0]["mapping"]["n2"]["message"]["metadata"]["model_slug"] =
        "different-synthetic-model".into();
    let changed = Material {
        alias: "changed-metadata".into(),
        bytes: serde_json::to_vec(&changed).unwrap(),
        ..original.clone()
    };
    let prep = prepare(&[original, changed]).unwrap();
    assert_eq!(prep.messages.len(), 4);
    assert_eq!(prep.duplicate_messages, 2);
    let e = Env::new();
    assert!(
        Plan::build(
            &prep,
            &[],
            e.policy(1, 1, 1),
            FormationMode::Backfill,
            Timestamp::from_unix_ms(NOW).unwrap(),
            &OsIdSource
        )
        .is_err()
    );
}

#[test]
fn unsupported_title_is_not_published_as_new_information() {
    let e = Env::new();
    let prep = prepare(&materials()).unwrap();
    let mut s = statement(0, "I enjoy pottery.");
    s.title = "Owns a palace on Mars".into();
    let p = plan(&e, &prep, &[s], FormationMode::Backfill);
    assert_eq!(p.preflight.title_fallbacks, 1);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let report = bundle(&init);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("palace"));
    assert_eq!(report["pages"][0]["items"][0]["title"], "I enjoy pottery.");
}

#[test]
fn selected_document_metadata_and_advisory_values_survive_native_formation() {
    let material = Material {
        alias: "synthetic-document".into(),
        format: Format::Document,
        bytes: b"The synthetic studio has a window.".to_vec(),
        account_alias: "synthetic".into(),
    };
    let plain = prepare(std::slice::from_ref(&material)).unwrap();
    let metadata = std::collections::BTreeMap::from([(
        "synthetic-document".into(),
        DocumentMetadata {
            author: Some("Synthetic author".into()),
            modified_at: Some("2026-06-10T10:00:00.000Z".into()),
            content_lang: Some("en".into()),
        },
    )]);
    let prep = prepare_with_document_metadata(&[material], &metadata).unwrap();
    assert_ne!(plain.watermark(), prep.watermark());
    assert_eq!(prep.messages[0].author.as_deref(), Some("Synthetic author"));
    assert!(prep.messages[0].occurred_at.is_some());
    let e = Env::new();
    let mut s = statement(0, &prep.messages[0].text);
    s.interpretation = Interpretation::Inference;
    s.annotations = AnalysisAnnotations {
        suggested_tags: vec!["studio".into()],
        tone: Some(ToneGuess {
            valence_milli: 200,
            arousal_milli: 100,
        }),
        importance_guess_milli: Some(300),
        current_state_note: Some("Scripted note, not an owner statement.".into()),
        suggested_relations: vec![],
    };
    let p = plan(&e, &prep, &[s], FormationMode::Backfill);
    let mut init = initializer(&e);
    init.enqueue(p, &prep).unwrap();
    while init.step().unwrap() {}
    let annotation = init
        .vault
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories[0]
                .memory
                .decision
                .input
                .record_fields
                .as_ref()
                .unwrap()
                .annotations
                .clone()
                .unwrap()
        })
        .unwrap();
    assert_eq!(annotation.suggested_tags, vec!["studio"]);
    assert_eq!(annotation.importance_guess_milli, Some(300));
    assert_eq!(annotation.tone.unwrap().arousal_milli, 100);
    assert!(annotation.current_state_note.is_some());
    assert_eq!(bundle(&init)["coverage"]["unknown_time"]["percent"], 100.0);
}
