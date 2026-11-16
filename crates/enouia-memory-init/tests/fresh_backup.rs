#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;

use enouia_memory_contract::{
    common::*, hash::sha256, json::canonical_bytes, next::*, time::Timestamp,
};
use enouia_memory_init::{
    driver::{FreshInput, WEEK_MS, load_fresh},
    fresh::{ChatGptSelection, prepare_chatgpt_selection},
    parse::{Format, Material},
    pipeline::*,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{
        AutomationFault,
        segmented::{SegmentedAutomationVault, backup::*},
    },
    fault::Faults,
    fs::ManagedRoot,
};
use fixture::*;
use serde_json::{Value, json};
use std::sync::Arc;

const JOURNAL: &str = "sync-state/initialization-owner-1.json";

fn owner() -> Env {
    let mut env = Env::new();
    env.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    env.descriptor.owner_subject_id = Some(env.policy(1, 1, 1).subject_id);
    env
}

fn input(count: usize) -> (Env, FreshInput) {
    let dir = owner();
    let bytes = serde_json::to_vec(&(0..count).map(|i| json!({
        "conversation_id":format!("synthetic-backup-{i}"), "title":"Synthetic backup",
        "mapping":{"n":{"parent":null,"message":{
            "id":format!("m{i}"),"author":{"role":"user"},"create_time":1700000000+i,
            "content_lang":"en","timezone":"Asia/Shanghai",
            "content":{"parts":[format!("Synthetic source {i}. Later evidence {i}.")]},
            "metadata":{"attachments":[{"id":"synthetic-asset","name":"synthetic.png","mime_type":"image/png"}]}
        }}}
    })).collect::<Vec<_>>()).unwrap();
    let attachment = b"\x89PNG\r\n\x1a\nsynthetic backup attachment";
    std::fs::write(dir.path.join("shard.json"), &bytes).unwrap();
    std::fs::write(dir.path.join("synthetic.png"), attachment).unwrap();
    let selections: Vec<_> = (0..count)
        .map(|i| ChatGptSelection {
            shard_alias: "synthetic-shard".into(),
            conversation_id: format!("synthetic-backup-{i}"),
        })
        .collect();
    let prep = prepare_chatgpt_selection(
        &[Material {
            alias: "synthetic-shard".into(),
            account_alias: "synthetic".into(),
            format: Format::Auto,
            bytes: bytes.clone(),
        }],
        &selections,
    )
    .unwrap();
    let sources: Vec<_> = prep.messages.iter().map(|m| json!({
        "account_alias":m.account_alias,"conversation_id":m.conversation,
        "message_id":m.upstream_id,"raw_object_hash":m.raw_hash,
        "text_hash":sha256(m.text.as_bytes()),"sensitivity":"private",
        "attachments":[{"slot":0,"original":{"path":"synthetic.png","sha256":sha256(attachment)}}]
    })).collect();
    let analysis = AnalyzedBatch {
        provenance: AnalysisProvenance {
            actor: "Synthetic backup test author".into(),
            prompt_version: "synthetic-fresh-backup-1".into(),
            model_version: None,
        },
        statements: prep
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let primary = m.text.split_once(". ").unwrap().0.to_owned() + ".";
                let support = m.text.split_once(". ").unwrap().1.to_owned();
                AnalyzedStatement {
                    statement: Statement {
                        source_index: i,
                        quote: primary,
                        title: format!("Synthetic {i}"),
                        interpretation: Interpretation::ClearSelfReport,
                        dimensions: dimensions(FormationKind::Preference),
                        event_time: unknown_time(),
                        valid_time: unknown_time(),
                        entities: vec![],
                        note: None,
                        annotations: AnalysisAnnotations::default(),
                        revises_statement: None,
                        derives_from_statements: vec![],
                        context_sources: vec![],
                    },
                    content: format!("Synthetic authored backup record {i}."),
                    supporting_quotes: vec![StatementQuote {
                        source_index: i,
                        quote: support,
                    }],
                }
            })
            .collect(),
    };
    let ranges: Vec<_> = analysis
        .statements
        .iter()
        .map(|s| {
            let end = s.statement.quote.chars().count() as u32;
            vec![
                (0, end),
                (
                    end + 1,
                    end + 1 + s.supporting_quotes[0].quote.chars().count() as u32,
                ),
            ]
        })
        .collect();
    let manifest = json!({
        "format":"enouia-fresh-input/1", "shards":[{"alias":"synthetic-shard","account_alias":"synthetic",
            "object":{"path":"shard.json","sha256":sha256(&bytes)},
            "conversation_ids":selections.iter().map(|s|&s.conversation_id).collect::<Vec<_>>()}],
        "sources":sources,"analysis":analysis,"quote_ranges":ranges,
        "formed_at":Timestamp::from_unix_ms(NOW).unwrap(),
        "formation_until":Timestamp::from_unix_ms(NOW+WEEK_MS).unwrap(),"project_id":null
    });
    let path = dir.path.join("manifest.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let input = load_fresh(&path).unwrap();
    (dir, input)
}

fn enqueue(init: &mut Initializer, input: &FreshInput, env: &Env) -> Result<(), AutomationFault> {
    init.enqueue_fresh(
        input.run_hash.clone(),
        input.plans(&env.descriptor, &OsIdSource).unwrap(),
        &input
            .batches
            .iter()
            .map(|b| b.preparation.clone())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn completed_fresh_multibatch_backup_preserves_run_evidence_raw_and_deletion_without_replay() {
    let (inputs, input) = input(65);
    let env = owner();
    let mut init = Initializer::create_synthetic(
        &env.root,
        env.descriptor.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert_eq!(input.batches.len(), 2);
    enqueue(&mut init, &input, &env).unwrap();
    while init.step().unwrap() {}
    let receipts: Vec<_> = init.plans().iter().flat_map(Plan::receipts).collect();
    assert_eq!(receipts.len(), 65);
    let deleted = receipts[0].result.memory_id.clone();
    init.vault
        .delete(&env.descriptor.created_by, vec![], vec![deleted.clone()])
        .unwrap();
    let snapshot = init.vault.snapshot().unwrap();
    let ledger = init.ledger().unwrap();
    let original_plans = serde_json::to_value(init.plans()).unwrap();
    let journal = std::fs::read(env.path.join(JOURNAL)).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&journal).unwrap()["fresh_run"],
        json!(input.run_hash)
    );
    drop(init);

    let export = owner();
    let restored = owner();
    let manifest = export_owner(
        &env.root,
        &export.root,
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert!(manifest.files.iter().any(|f| f.path == JOURNAL));
    assert!(!manifest.files.iter().any(|f| f.path.ends_with(".lock")));
    // The only source is this uniquely allocated synthetic fixture. Make its
    // original location unavailable; verification/restoration must use the export.
    std::fs::rename(&env.path, inputs.path.join("source-offline")).unwrap();
    assert!(!env.path.exists());
    assert_eq!(verify_owner(&export.root).unwrap(), manifest);
    restore_owner(&export.root, &restored.root, env.clock.as_ref()).unwrap();
    assert!(
        SegmentedAutomationVault::open(
            &restored.root,
            env.descriptor.host_id.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert!(
        SegmentedAutomationVault::recover(
            &restored.root,
            env.descriptor.host_id.clone(),
            &env.descriptor.system_actor,
            env.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(restored.path.join(JOURNAL)).unwrap(), journal);
    // Every original, attachment and journal byte is available without the source.
    for file in &manifest.files {
        assert_eq!(
            sha256(&std::fs::read(restored.path.join(&file.path)).unwrap()),
            file.sha256
        );
    }
    activate_owner(&restored.root, &export.root, env.clock.as_ref()).unwrap();
    assert!(Initializer::check_fresh_run(&restored.root, &input.run_hash).unwrap());
    assert!(matches!(
        Initializer::check_fresh_run(&restored.root, &sha256(b"changed synthetic run")),
        Err(AutomationFault::Conflict)
    ));
    let mut init = Initializer::open_synthetic(
        &restored.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert_eq!(init.ledger().unwrap(), ledger);
    assert_eq!(serde_json::to_value(init.plans()).unwrap(), original_plans);
    enqueue(&mut init, &input, &env).unwrap();
    assert!(!init.step().unwrap());
    assert_eq!(init.vault.snapshot().unwrap(), snapshot);
    init.vault
        .read(&env.descriptor.system_actor, Sensitivity::Private, |data| {
            assert_eq!(data.memories.len(), 64);
            assert!(
                !data
                    .memories
                    .iter()
                    .any(|m| m.receipt.result.memory_id == deleted)
            );
            assert!(data.owner_actions.is_empty());
            let sources: Vec<_> = data.sources.values().map(|s| (*s).clone()).collect();
            assert_eq!(sources.len(), 65);
            for memory in &data.memories {
                assert!(receipts.contains(&memory.receipt));
                let fields = memory.memory.decision.input.record_fields.as_ref().unwrap();
                assert_eq!(fields.evidence.len(), 2);
                assert!(fields.evidence.iter().all(|q| q.resolves(&sources)));
                assert!(memory.memory.decision.input.model_version.is_none());
            }
        })
        .unwrap();
    assert_eq!(init.ledger().unwrap()["extraction"]["model_invocations"], 0);
}

#[test]
fn restored_uncertain_fresh_run_keeps_original_packet_and_resolves_only_existing_receipt() {
    for (committed, expired) in [(true, false), (false, false), (true, true)] {
        let (inputs, input) = input(1);
        let env = owner();
        let mut init = Initializer::create_synthetic(
            &env.root,
            env.descriptor.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
        )
        .unwrap();
        enqueue(&mut init, &input, &env).unwrap();
        init.step().unwrap(); // Only the source/policy capture.
        let managed = ManagedRoot::new(env.root.path(), Faults::none());
        let mut journal: Value;
        if committed {
            init.step().unwrap();
            journal = serde_json::from_slice(&managed.read(JOURNAL).unwrap().unwrap()).unwrap();
        } else {
            journal = serde_json::from_slice(&managed.read(JOURNAL).unwrap().unwrap()).unwrap();
            let seed =
                serde_json::from_value(journal["plans"][0]["work"][0]["seed"].clone()).unwrap();
            // Emulate an interruption after persisting the native prepared intent
            // but before apply_batch. No fake conflicting binding or new request.
            let prepared = init
                .vault
                .prepare_formation(&env.descriptor.system_actor, seed)
                .unwrap();
            let work = &mut journal["plans"][0]["work"][0];
            work["request"] = serde_json::to_value(prepared.request).unwrap();
            work["response"] = json!(prepared.response);
            work["binding"] = serde_json::to_value(prepared.binding).unwrap();
        }
        let work = &mut journal["plans"][0]["work"][0];
        work["receipt"] = Value::Null;
        work["state"] = json!("outcome_unknown");
        let original = work.clone();
        let snapshot = init.vault.snapshot().unwrap();
        drop(init);
        managed
            .write_atomic(JOURNAL, &canonical_bytes(&journal).unwrap())
            .unwrap();
        if expired {
            env.clock.set(NOW + WEEK_MS + 1);
        }
        let export = owner();
        let restored = owner();
        export_owner(
            &env.root,
            &export.root,
            env.clock.clone(),
            Arc::new(OsIdSource),
        )
        .unwrap();
        std::fs::rename(&env.path, inputs.path.join("source-offline")).unwrap();
        restore_owner(&export.root, &restored.root, env.clock.as_ref()).unwrap();
        activate_owner(&restored.root, &export.root, env.clock.as_ref()).unwrap();
        let mut init = Initializer::open_synthetic(
            &restored.root,
            env.descriptor.host_id.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
        )
        .unwrap();
        assert!(Initializer::check_fresh_run(&restored.root, &input.run_hash).unwrap());
        enqueue(&mut init, &input, &env).unwrap();
        if committed && !expired {
            assert!(init.step().unwrap());
            assert!(!init.step().unwrap());
            assert_eq!(init.plans()[0].completed(), 1);
        } else {
            assert!(matches!(init.step(), Err(AutomationFault::OutcomeUnknown)));
            assert!(matches!(init.step(), Err(AutomationFault::OutcomeUnknown)));
            assert_eq!(init.plans()[0].completed(), 0);
        }
        assert_eq!(init.vault.snapshot().unwrap(), snapshot);
        drop(init);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(restored.path.join(JOURNAL)).unwrap()).unwrap();
        let work = &saved["plans"][0]["work"][0];
        for field in ["seed", "request", "response", "binding"] {
            assert_eq!(
                work[field], original[field],
                "{field}, committed={committed}, expired={expired}"
            );
        }
        if !committed || expired {
            assert_eq!(work, &original);
        }
    }
}
