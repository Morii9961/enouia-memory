#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*, hash::sha256, json::canonical_bytes, next::*, ports::IdSource, source::Availability,
    time::Timestamp,
};
use enouia_memory_host::automation::catalog::*;
use enouia_memory_init::{driver::*, fresh::*, parse::*, pipeline::*};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, segmented::SegmentedAutomationVault},
    fault::Faults,
    fs::ManagedRoot,
};
use fixture::*;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

fn input(count: usize, statements: usize) -> (PathBuf, FreshManifest) {
    let dir = std::env::temp_dir().join(format!(
        "enouia-fresh-driver-synthetic-{}",
        enouia_memory_contract::ids::format_uuid_v4(OsIdSource.random_16())
    ));
    std::fs::create_dir(&dir).unwrap();
    let bytes = serde_json::to_vec(&(0..count).map(|i|json!({
        "conversation_id":format!("synthetic-c{i}"),"title":"Synthetic driver", "mapping":{
            "n":{"parent":null,"message":{"id":format!("m{i}"),"author":{"role":"user"},"create_time":1700000000+i,
            "content_lang":"en","timezone":"Asia/Shanghai",
            "content":{"parts":["同文。quiet. 同文。"]},
            "metadata":{"attachments":[{"id":"synthetic-asset","name":"synthetic.png","mime_type":"image/png"}]}}},
            "context":{"parent":"n","message":{"id":format!("unused{i}"),"author":{"role":"assistant"},"create_time":1700000100+i,
            "content":{"parts":["SYNTHETIC SURROUNDING RAW CONTEXT"]}}}
        }})).collect::<Vec<_>>()).unwrap();
    std::fs::write(dir.join("shard.json"), &bytes).unwrap();
    std::fs::write(dir.join("asset.png"), b"\x89PNG\r\n\x1a\nsynthetic").unwrap();
    let ids = (0..count)
        .map(|i| format!("synthetic-c{i}"))
        .collect::<Vec<_>>();
    let prep = prepare_chatgpt_selection(
        &[Material {
            alias: "shard".into(),
            account_alias: "synthetic".into(),
            format: Format::Auto,
            bytes: bytes.clone(),
        }],
        &ids.iter()
            .map(|id| ChatGptSelection {
                shard_alias: "shard".into(),
                conversation_id: id.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let sources = prep
        .messages
        .iter()
        .filter(|m| m.role == SpeakerRole::User)
        .map(|m| SourceInput {
            account_alias: m.account_alias.clone(),
            conversation_id: m.conversation.clone().unwrap(),
            message_id: m.upstream_id.clone().unwrap(),
            raw_object_hash: m.raw_hash.clone(),
            text_hash: sha256(m.text.as_bytes()),
            sensitivity: Sensitivity::Private,
            attachments: vec![AttachmentInput {
                slot: 0,
                original: Some(InputObject {
                    path: "asset.png".into(),
                    sha256: sha256(b"\x89PNG\r\n\x1a\nsynthetic"),
                }),
            }],
            visible_text_only: false,
        })
        .collect();
    let manifest = FreshManifest {
        format: "enouia-fresh-input/1".into(),
        attachment_retention: AttachmentRetention::SelectedOriginals,
        shards: vec![ShardInput {
            alias: "shard".into(),
            account_alias: "synthetic".into(),
            object: InputObject {
                path: "shard.json".into(),
                sha256: sha256(&bytes),
            },
            conversation_ids: ids,
        }],
        sources,
        analysis: AnalyzedBatch {
            provenance: AnalysisProvenance {
                actor: "Synthetic test author".into(),
                prompt_version: "synthetic-fresh-driver-1".into(),
                model_version: None,
            },
            statements: (0..statements)
                .map(|i| AnalyzedStatement {
                    statement: Statement {
                        source_index: i % count,
                        quote: "同文。".into(),
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
                    content: format!("Synthetic distilled content {i}."),
                    supporting_quotes: vec![StatementQuote {
                        source_index: i % count,
                        quote: "同文。".into(),
                    }],
                })
                .collect(),
        },
        quote_ranges: vec![vec![(0, 3), (10, 13)]; statements],
        context_quotes: vec![],
        authored_records: None,
        record_origins: vec![],
        formed_at: Timestamp::from_unix_ms(NOW).unwrap(),
        formation_until: Timestamp::from_unix_ms(NOW + WEEK_MS).unwrap(),
        project_id: None,
    };
    save(&dir, &manifest);
    (dir, manifest)
}
fn save(dir: &std::path::Path, manifest: &FreshManifest) {
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}
fn mixed_input() -> (PathBuf, FreshManifest) {
    let (dir, mut manifest) = input(2, 2);
    let mut shard: Value =
        serde_json::from_slice(&std::fs::read(dir.join("shard.json")).unwrap()).unwrap();
    shard[0]["mapping"]["n"]["message"]["content"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"url":"https://example.invalid/unfetched","mime_type":"image/png"}));
    let bytes = serde_json::to_vec(&shard).unwrap();
    std::fs::write(dir.join("shard.json"), &bytes).unwrap();
    manifest.shards[0].object.sha256 = sha256(&bytes);
    let selected = prepare_chatgpt_selection(
        &[Material {
            alias: "shard".into(),
            account_alias: "synthetic".into(),
            format: Format::Auto,
            bytes,
        }],
        &manifest.shards[0]
            .conversation_ids
            .iter()
            .map(|id| ChatGptSelection {
                shard_alias: "shard".into(),
                conversation_id: id.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for source in &mut manifest.sources {
        source.raw_object_hash = selected
            .messages
            .iter()
            .find(|m| m.upstream_id.as_deref() == Some(&source.message_id))
            .unwrap()
            .raw_hash
            .clone();
    }
    manifest.sources[0].attachments[0].slot = 1;
    manifest.sources[0].attachments.push(AttachmentInput {
        slot: 0,
        original: None,
    });
    save(&dir, &manifest);
    (dir, manifest)
}
fn owner() -> Env {
    let mut env = Env::new();
    env.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    env.descriptor.owner_subject_id = Some(env.policy(1, 1, 1).subject_id);
    env
}
fn load(dir: &std::path::Path) -> FreshInput {
    load_fresh(&dir.join("manifest.json")).unwrap()
}

#[test]
fn dry_run_maps_explicit_revisions_ranges_raw_and_attachment_closure_without_writes() {
    let (dir, mut manifest) = input(65, 65);
    // Deliberately reverse table order, adjusting analysis to the explicit table.
    manifest.sources.reverse();
    for s in &mut manifest.analysis.statements {
        s.statement.source_index = 64 - s.statement.source_index;
        s.supporting_quotes[0].source_index = s.statement.source_index;
    }
    save(&dir, &manifest);
    let prepared = load(&dir);
    assert_eq!(prepared.batches.len(), 2);
    let env = owner();
    let plans = prepared.plans(&env.descriptor, &OsIdSource).unwrap();
    let capacity = fresh_journal_capacity(&plans, &env.descriptor, &prepared.run_hash).unwrap();
    assert!(capacity.fits && capacity.lifecycle_bound_bytes > capacity.queued_bytes);
    assert_eq!(prepared.intake["attachments_present"], 65);
    assert!(std::fs::read_dir(env.root.path()).unwrap().next().is_none());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_enouia-memory-fresh"))
        .args(["plan", dir.join("manifest.json").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["writes"], 0);
    assert_eq!(result["ready_to_run"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Synthetic distilled"));
    for plan in &plans {
        for source in &plan.sources {
            assert_eq!(source.record.speaker_role, SpeakerRole::User);
            assert_eq!(source.attachments[0].availability, Availability::Present);
            assert_eq!(
                source.record.original_timezone.as_deref(),
                Some("Asia/Shanghai")
            );
        }
    }
    assert!(
        prepared.batches[0]
            .preparation
            .objects
            .values()
            .any(|b| String::from_utf8_lossy(b).contains("SYNTHETIC SURROUNDING RAW CONTEXT"))
    );
}

#[test]
fn refuses_missing_changed_and_unbound_inputs_and_reports_explicit_unavailable() {
    let (dir, mut manifest) = input(1, 1);
    manifest.sources[0].attachments.clear();
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.attachment_unbound"
    );
    manifest.sources[0].attachments = vec![AttachmentInput {
        slot: 0,
        original: None,
    }];
    save(&dir, &manifest);
    assert_eq!(load(&dir).intake["attachments_explicitly_unavailable"], 1);
    manifest.sources[0].text_hash = sha256(b"wrong revision");
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.source_revision"
    );
    manifest.sources[0].text_hash = sha256("同文。quiet. 同文。".as_bytes());
    manifest.quote_ranges[0][1] = (9, 12);
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.quote_anchor"
    );
    manifest.quote_ranges[0][1] = (10, 13);
    save(&dir, &manifest);
    std::fs::write(dir.join("shard.json"), b"[]").unwrap();
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.object_revision"
    );
}

#[test]
fn explicit_metadata_only_never_opens_originals_or_reports_them_as_lost() {
    let (dir, mut manifest) = mixed_input();
    manifest.sources[0].visible_text_only = true;
    save(&dir, &manifest);
    let original_mode_hash = load(&dir).run_hash;
    manifest.attachment_retention = AttachmentRetention::MetadataOnly;
    // Conflicting selection refuses before attempting the nonexistent file.
    manifest.sources[0].attachments[0]
        .original
        .as_mut()
        .unwrap()
        .path = "absent.png".into();
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.attachment_retention"
    );
    for source in &mut manifest.sources {
        for attachment in &mut source.attachments {
            attachment.original = None;
        }
    }
    let slot = manifest.sources[0].attachments.pop().unwrap();
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.attachment_unbound"
    );
    manifest.sources[0].attachments.push(slot);
    save(&dir, &manifest);
    let input = load(&dir);
    assert_ne!(input.run_hash, original_mode_hash);
    assert_eq!(input.intake["attachments_present"], 0);
    assert_eq!(input.intake["attachments_explicitly_unavailable"], 0);
    assert_eq!(input.intake["unavailable_attachments"], json!([]));
    assert_eq!(input.intake["attachments_metadata_only"], 3);
    assert_eq!(input.intake["attachment_retention"], "metadata_only");
    let env = owner();
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    let source = &plans[0].sources[0];
    assert!(source.record.has_visible_text_view());
    assert_eq!(
        source.attachments[0].availability,
        Availability::ExternalReference
    );
    assert_eq!(source.attachments[1].availability, Availability::Missing);
    for source in &plans[0].sources {
        assert!(source.validate().is_empty());
        for attachment in &source.attachments {
            assert_eq!(
                attachment.extensions["memory-init.attachment-retention"]["payload"],
                "not_selected"
            );
            assert!(attachment.object_hash.is_none());
            assert!(attachment.detected_media_type.is_none());
        }
    }
    assert!(
        !serde_json::to_value(&plans[0]).unwrap()["object_hashes"]
            .as_array()
            .unwrap()
            .contains(&json!(sha256(b"\x89PNG\r\n\x1a\nsynthetic")))
    );
    let raw = input.batches[0]
        .preparation
        .objects
        .values()
        .next()
        .unwrap();
    assert!(
        std::str::from_utf8(raw)
            .unwrap()
            .contains("SYNTHETIC SURROUNDING RAW CONTEXT")
    );
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_enouia-memory-fresh"))
        .args(["plan", dir.join("manifest.json").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["writes"], 0);
    assert_eq!(result["ready_to_run"], true);
    assert_eq!(result["intake"]["attachments_metadata_only"], 3);
    let mut unknown = serde_json::to_value(&manifest).unwrap();
    unknown["attachment_retention"] = json!("unknown_mode");
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&unknown).unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.manifest_shape"
    );
    // Omitting the new field retains the previous selection semantics.
    let mut old = serde_json::to_value(&manifest).unwrap();
    old.as_object_mut().unwrap().remove("attachment_retention");
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(load(&dir).intake["attachments_explicitly_unavailable"], 3);
    assert_eq!(load(&dir).intake["attachments_metadata_only"], 0);
}

#[test]
fn context_closure_sensitivity_entity_layers_and_later_primary_occurrence_stay_bound() {
    let (dir, mut manifest) = input(3, 3);
    manifest.analysis.statements.pop();
    manifest.quote_ranges.pop();
    manifest.analysis.statements[0].statement.context_sources = vec![2];
    manifest.sources[2].sensitivity = Sensitivity::HighlySensitive;
    manifest.analysis.statements[0].statement.entities = vec![(
        "Synthetic persona".into(),
        EntityType::Person,
        EntityReality::Fictional,
        EntityRole::Mentioned,
    )];
    manifest.quote_ranges[0] = vec![(10, 13), (0, 3)];
    save(&dir, &manifest);
    let input = load(&dir);
    let env = owner();
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    assert!(
        fresh_journal_capacity(&plans, &env.descriptor, &input.run_hash)
            .unwrap()
            .fits
    );
    let serialized = serde_json::to_value(&plans[0]).unwrap();
    let work = serialized["work"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["analysis_index"] == 0)
        .unwrap();
    assert_eq!(
        work["seed"]["context_source_refs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(work["seed"]["input_refs"].as_array().unwrap().len(), 2);
    assert_eq!(work["seed"]["sensitivity"], "highly_sensitive");
    assert_eq!(
        work["seed"]["record_fields"]["entities"][0]["evidence"][0]["start"],
        10
    );
    assert_eq!(serialized["catalog"]["entities"][0]["reality"], "fictional");
    assert_eq!(
        serialized["catalog"]["entities"][0]["evidence"][0]["start"],
        10
    );
    manifest.analysis.statements[0].statement.quote = "not an exact quote".into();
    // Invalid evidence refuses; no convenient supporting quote replaces it.
    save(&dir, &manifest);
    assert!(load_fresh(&dir.join("manifest.json")).is_err());
}

#[test]
fn native_text_only_multibatch_reopen_query_sources_no_multiplication_and_changed_run_conflicts() {
    let (dir, mut manifest) = input(65, 65);
    manifest.attachment_retention = AttachmentRetention::MetadataOnly;
    for source in &mut manifest.sources {
        source.attachments[0].original = None;
    }
    save(&dir, &manifest);
    let input = load(&dir);
    let env = owner();
    // Native witnessed genesis, the same storage constructor used by bootstrap.
    drop(
        SegmentedAutomationVault::create(
            &env.root,
            env.descriptor.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap(),
    );
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    let bound = fresh_journal_capacity(&plans, &env.descriptor, &input.run_hash).unwrap();
    let preparations = input
        .batches
        .iter()
        .map(|b| b.preparation.clone())
        .collect::<Vec<_>>();
    let mut init = Initializer::adopt_bootstrapped(
        &env.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    init.enqueue_fresh(input.run_hash.clone(), plans, &preparations)
        .unwrap();
    init.step().unwrap();
    init.step().unwrap();
    drop(init);
    let mut init = Initializer::open_synthetic(
        &env.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    init.enqueue_fresh(
        input.run_hash.clone(),
        input.plans(&env.descriptor, &OsIdSource).unwrap(),
        &preparations,
    )
    .unwrap();
    while init.step().unwrap() {
        let size = std::fs::metadata(env.path.join("sync-state/initialization-owner-1.json"))
            .unwrap()
            .len();
        assert!(size <= bound.lifecycle_bound_bytes as u64);
    }
    drop(init);
    let mut init = Initializer::open_synthetic(
        &env.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    {
        let reader = SegmentedMemoryReader::from_authenticated_system(
            init.vault.clone(),
            env.descriptor.system_actor.clone(),
        )
        .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let filter = CatalogFilter::default();
        let request = CatalogReadRequest {
            profile: &profile,
            filter: &filter,
            options: CatalogReadOptions {
                limit: 100,
                max_bytes: MAX_DOCUMENT_BYTES,
                max_sensitivity: Sensitivity::Private,
                include_facets: false,
            },
        };
        let mut cursor = None;
        let mut count = 0;
        loop {
            let page = reader
                .memory_page_native(&request, cursor.as_ref())
                .unwrap();
            count += page.items.len();
            cursor = page.next;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(count, 65);
        let detail = reader
            .memory_detail_native(
                &request,
                &init.plans()[0].receipts()[0].result.memory_id,
                None,
            )
            .unwrap();
        assert!(detail.fields.is_some());
        let source = init.plans()[0].sources[0].reference();
        let excerpt = reader
            .memory_source_excerpt_native(&request, &source, 10, 3)
            .unwrap();
        assert_eq!(excerpt.excerpt.as_deref(), Some("同文。"));
    }
    let before = init.vault.snapshot().unwrap();
    init.enqueue_fresh(
        input.run_hash.clone(),
        input.plans(&env.descriptor, &OsIdSource).unwrap(),
        &preparations,
    )
    .unwrap();
    assert!(!init.step().unwrap());
    assert_eq!(init.vault.snapshot().unwrap(), before);
    init.vault
        .read(&env.descriptor.system_actor, Sensitivity::Private, |data| {
            assert_eq!(data.memories.len(), 65);
            assert_eq!(data.sources.len(), 65);
            assert!(data.owner_actions.is_empty());
            for source in data.sources.values() {
                for attachment in &source.attachments {
                    assert_eq!(
                        attachment.extensions["memory-init.attachment-retention"]["payload"],
                        "not_selected"
                    );
                    assert!(attachment.object_hash.is_none());
                }
            }
            let sources = data
                .sources
                .values()
                .map(|s| (*s).clone())
                .collect::<Vec<_>>();
            for memory in &data.memories {
                let fields = memory.memory.decision.input.record_fields.as_ref().unwrap();
                assert_eq!(
                    fields
                        .evidence
                        .iter()
                        .map(|q| (q.start, q.end))
                        .collect::<Vec<_>>(),
                    [(0, 3), (10, 13)]
                );
                assert!(fields.evidence.iter().all(|q| q.resolves(&sources)));
                assert!(memory.memory.decision.input.model_version.is_none());
            }
        })
        .unwrap();
    manifest.attachment_retention = AttachmentRetention::SelectedOriginals;
    save(&dir, &manifest);
    let changed_retention = load(&dir);
    assert!(Initializer::check_fresh_run(&env.root, &changed_retention.run_hash).is_err());
    manifest.attachment_retention = AttachmentRetention::MetadataOnly;
    manifest.analysis.statements[0].content.push_str(" changed");
    save(&dir, &manifest);
    let changed = load(&dir);
    assert!(Initializer::check_fresh_run(&env.root, &changed.run_hash).is_err());
    assert!(
        init.enqueue_fresh(
            changed.run_hash.clone(),
            changed.plans(&env.descriptor, &OsIdSource).unwrap(),
            &preparations
        )
        .is_err()
    );
    assert_eq!(init.vault.snapshot().unwrap(), before);
    println!(
        "synthetic whole journal: queued={}, lifecycle_bound={}, actual={}",
        bound.queued_bytes,
        bound.lifecycle_bound_bytes,
        std::fs::metadata(env.path.join("sync-state/initialization-owner-1.json"))
            .unwrap()
            .len()
    );
}

#[test]
fn whole_lifecycle_overflow_refuses_before_archive_or_source_writes() {
    let (dir, mut manifest) = input(1, 256);
    for s in &mut manifest.analysis.statements {
        s.content = "界".repeat(5000);
    }
    save(&dir, &manifest);
    let input = load(&dir);
    let env = owner();
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    let bound = fresh_journal_capacity(&plans, &env.descriptor, &input.run_hash).unwrap();
    assert!(bound.queued_bytes < bound.limit_bytes && !bound.fits);
    let mut init = Initializer::create_synthetic(
        &env.root,
        env.descriptor.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert!(matches!(
        init.enqueue_fresh(
            input.run_hash,
            plans,
            &input
                .batches
                .iter()
                .map(|b| b.preparation.clone())
                .collect::<Vec<_>>()
        ),
        Err(AutomationFault::Capacity)
    ));
    assert!(init.plans().is_empty());
    assert!(!env.path.join("raw/objects").exists());
    println!(
        "synthetic rejected journal: queued={}, lifecycle_bound={}, limit={}",
        bound.queued_bytes, bound.lifecycle_bound_bytes, bound.limit_bytes
    );
}

#[test]
fn uncertain_original_binding_is_lookup_only_with_or_without_a_receipt() {
    for (receipt_exists, expired) in [(true, false), (false, false), (true, true)] {
        let (dir, _) = input(1, 1);
        let input = load(&dir);
        let env = owner();
        let mut init = Initializer::create_synthetic(
            &env.root,
            env.descriptor.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
        )
        .unwrap();
        let prep = input
            .batches
            .iter()
            .map(|b| b.preparation.clone())
            .collect::<Vec<_>>();
        init.enqueue_fresh(
            input.run_hash.clone(),
            input.plans(&env.descriptor, &OsIdSource).unwrap(),
            &prep,
        )
        .unwrap();
        init.step().unwrap();
        init.step().unwrap();
        if expired {
            // Existing lookup authority can withhold an expired receipt. Keep
            // the unknown barrier; do not turn this into a read grant or resend.
            env.clock.set(NOW + WEEK_MS + 1);
        }
        let before = init.vault.snapshot().unwrap();
        drop(init);
        let managed = ManagedRoot::new(&env.path, Faults::none());
        let path = "sync-state/initialization-owner-1.json";
        let mut journal: Value =
            serde_json::from_slice(&managed.read(path).unwrap().unwrap()).unwrap();
        let work = &mut journal["plans"][0]["work"][0];
        work["receipt"] = Value::Null;
        work["state"] = json!("outcome_unknown");
        if !receipt_exists {
            work["binding"]["input_hash"] = json!("0".repeat(64));
        }
        let binding = work["binding"].clone();
        let request = work["request"].clone();
        managed
            .write_atomic(path, &canonical_bytes(&journal).unwrap())
            .unwrap();
        let mut init = Initializer::open_synthetic(
            &env.root,
            env.descriptor.host_id.clone(),
            env.clock.clone(),
            Arc::new(OsIdSource),
        )
        .unwrap();
        if receipt_exists && !expired {
            assert!(init.step().unwrap());
            assert!(!init.step().unwrap());
        } else {
            assert!(init.step().is_err());
            assert!(init.step().is_err());
        }
        assert_eq!(init.vault.snapshot().unwrap(), before);
        drop(init);
        let saved: Value = serde_json::from_slice(&managed.read(path).unwrap().unwrap()).unwrap();
        assert_eq!(saved["plans"][0]["work"][0]["binding"], binding);
        assert_eq!(saved["plans"][0]["work"][0]["request"], request);
    }
}

#[test]
fn visible_text_view_opt_in_preserves_partial_and_native_raw_proof_refuses_forgery() {
    let (dir, mut manifest) = mixed_input();
    let env = owner();
    let default = load(&dir);
    let plans = default.plans(&env.descriptor, &OsIdSource).unwrap();
    assert!(fresh_journal_capacity(&plans, &env.descriptor, &default.run_hash).is_err());
    manifest.sources[0].visible_text_only = true;
    save(&dir, &manifest);
    let input = load(&dir);
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    let source = &plans[0].sources[0];
    assert_eq!(
        source.record.completeness,
        enouia_memory_contract::source::Completeness::Partial
    );
    assert!(source.record.has_visible_text_view());
    assert_eq!(source.attachments.len(), 2);
    assert_eq!(
        source.attachments[0].availability,
        Availability::ExternalReference
    );
    assert_eq!(source.attachments[1].availability, Availability::Present);
    let mut init = Initializer::create_synthetic(
        &env.root,
        env.descriptor.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    let before = init.vault.snapshot().unwrap();
    for alteration in 0..7 {
        let mut forged = plans[0].sources.clone();
        match alteration {
            0 => {
                forged[0].text = "cut text".into();
                forged[0].record.content_hash = sha256(b"cut text");
            }
            1 => forged[0].record.original_message_id = Some("other-message".into()),
            2 => {
                forged[0].record.locator = Locator::JsonPointer {
                    pointer: "/0/mapping/n/message/content/parts/0".into(),
                }
            }
            3 => {
                forged[0].record.extensions.insert(
                    "memory-init.visible-text-view".into(),
                    json!({"version":1,"scope":"images"}),
                );
            }
            4 => forged[0].record.raw_object_hash = Some(sha256(b"missing raw")),
            5 => {
                forged[0].record.speaker_role = SpeakerRole::Assistant;
                forged[0].record.evidence_class = EvidenceClass::ModelClaim;
                forged[0].attributed_subject = None;
            }
            _ => forged[0].record.original_conversation_id = Some("other-conversation".into()),
        }
        assert!(
            init.vault
                .capture_import(
                    &env.descriptor.system_actor,
                    forged,
                    plans[0].catalog.clone(),
                    &input.batches[0].preparation.objects
                )
                .is_err()
        );
        assert_eq!(init.vault.snapshot().unwrap(), before);
    }
    let capacity = fresh_journal_capacity(&plans, &env.descriptor, &input.run_hash).unwrap();
    let prep = input
        .batches
        .iter()
        .map(|b| b.preparation.clone())
        .collect::<Vec<_>>();
    init.enqueue_fresh(input.run_hash, plans, &prep).unwrap();
    while init.step().unwrap() {}
    drop(init);
    let init = Initializer::open_synthetic(
        &env.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    init.vault
        .read(&env.descriptor.system_actor, Sensitivity::Private, |data| {
            assert_eq!(data.memories.len(), 2);
            assert!(data.owner_actions.is_empty());
            let source = data
                .sources
                .values()
                .find(|s| s.record.has_visible_text_view())
                .unwrap();
            assert_eq!(
                source.record.completeness,
                enouia_memory_contract::source::Completeness::Partial
            );
            assert_eq!(
                source.attachments[0].availability,
                Availability::ExternalReference
            );
            let all = data
                .sources
                .values()
                .map(|s| (*s).clone())
                .collect::<Vec<_>>();
            assert!(data.memories.iter().all(|m| {
                m.memory
                    .decision
                    .input
                    .record_fields
                    .as_ref()
                    .unwrap()
                    .evidence
                    .iter()
                    .all(|q| q.resolves(&all))
            }));
        })
        .unwrap();
    assert!(
        std::fs::metadata(env.path.join("sync-state/initialization-owner-1.json"))
            .unwrap()
            .len()
            <= capacity.lifecycle_bound_bytes as u64
    );
}

#[test]
fn precise_context_and_original_authored_records_are_bound_and_archived_without_promotion() {
    let (dir, mut manifest) = input(3, 3);
    manifest.analysis.statements.pop();
    manifest.quote_ranges.pop();
    manifest.analysis.statements[0].statement.context_sources = vec![2];
    manifest.context_quotes = vec![
        vec![
            ContextQuote {
                source_index: 2,
                quote: "quiet.".into(),
                start: 3,
                end: 9,
            },
            ContextQuote {
                source_index: 0,
                quote: "quiet.".into(),
                start: 3,
                end: 9,
            },
        ],
        vec![],
    ];
    let records=manifest.analysis.statements.iter().rev().enumerate().map(|(i,s)|json!({
        "id":format!("synthetic-authored-{i}"),"revision":7,"content":s.content,"title":s.statement.title,
        "analysis_provenance":{"actor":"Original synthetic author","model_version":null},
        "layer":"fictional_setting","fine_time":{"precision":"historical_day","basis":"source author metadata, never expiry"},
        "cautions":["Synthetic original metadata remains unchanged"]})).collect::<Vec<_>>();
    let raw = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap() + "\n")
        .collect::<String>()
        .into_bytes();
    std::fs::write(dir.join("authored.jsonl"), &raw).unwrap();
    let hash = sha256(&raw);
    manifest.authored_records = Some(InputObject {
        path: "authored.jsonl".into(),
        sha256: hash.clone(),
    });
    manifest.record_origins = vec![1, 0];
    save(&dir, &manifest);
    let input = load(&dir);
    let env = owner();
    let plans = input.plans(&env.descriptor, &OsIdSource).unwrap();
    let serialized = serde_json::to_value(&plans[0]).unwrap();
    let quote = &serialized["analysis_artifact"]["context_evidence"][0]["quotes"][0];
    assert_eq!(quote["quote"], "quiet.");
    assert_eq!(quote["start"], 3);
    assert_eq!(quote["end"], 9);
    assert_eq!(
        serialized["analysis_artifact"]["authored_origins"][0]["record_index"],
        1
    );
    let mut init = Initializer::create_synthetic(
        &env.root,
        env.descriptor.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    let prep = input
        .batches
        .iter()
        .map(|b| b.preparation.clone())
        .collect::<Vec<_>>();
    init.enqueue_fresh(input.run_hash.clone(), plans, &prep)
        .unwrap();
    while init.step().unwrap() {}
    let managed = ManagedRoot::new(&env.path, Faults::none());
    assert_eq!(
        managed
            .read_bounded(
                &enouia_memory_contract::layout::raw_object(&hash),
                16 * 1024 * 1024
            )
            .unwrap()
            .unwrap(),
        raw
    );
    init.vault
        .read(&env.descriptor.system_actor, Sensitivity::Private, |data| {
            let memory = data
                .memories
                .iter()
                .find(|m| {
                    m.memory.decision.input.content == manifest.analysis.statements[0].content
                })
                .unwrap();
            assert_eq!(memory.memory.decision.input.context_source_refs.len(), 1);
            assert_eq!(
                memory
                    .memory
                    .decision
                    .input
                    .record_fields
                    .as_ref()
                    .unwrap()
                    .evidence
                    .len(),
                2
            );
        })
        .unwrap();
    let before = init.vault.snapshot().unwrap();
    drop(init);
    let mut init = Initializer::open_synthetic(
        &env.root,
        env.descriptor.host_id.clone(),
        env.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    init.enqueue_fresh(
        input.run_hash.clone(),
        input.plans(&env.descriptor, &OsIdSource).unwrap(),
        &prep,
    )
    .unwrap();
    assert!(!init.step().unwrap());
    assert_eq!(init.vault.snapshot().unwrap(), before);
    manifest.context_quotes[0][0].start = 4;
    save(&dir, &manifest);
    assert!(load_fresh(&dir.join("manifest.json")).is_err());
    manifest.context_quotes[0][0].start = 3;
    manifest.analysis.statements[0].content.push_str(" changed");
    save(&dir, &manifest);
    assert_eq!(
        load_fresh(&dir.join("manifest.json")).err().unwrap(),
        "fresh.authored_record_content"
    );
    manifest.analysis.statements[0].content = records[1]["content"].as_str().unwrap().into();
    let changed = String::from_utf8(raw)
        .unwrap()
        .replace("Original synthetic author", "Changed synthetic author")
        .into_bytes();
    std::fs::write(dir.join("authored.jsonl"), &changed).unwrap();
    manifest.authored_records.as_mut().unwrap().sha256 = sha256(&changed);
    save(&dir, &manifest);
    let changed = load(&dir);
    assert!(Initializer::check_fresh_run(&env.root, &changed.run_hash).is_err());
}

/// Opt-in because native bootstrap requires a durable owner-only home outside
/// repositories/temp roots. Creates a uniquely named, synthetic-only protected
/// home beneath the explicitly supplied parent; retains it with its markers.
#[test]
#[ignore = "set ENOUIA_SYNTHETIC_DURABLE_PARENT to an explicit durable test parent"]
fn trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run() {
    use enouia_memory_contract::foundation::Clock;
    use enouia_memory_host::owner_bootstrap::{registered_owner_root, run_cli};
    use enouia_memory_vault::{RootPolicy, SystemClock, platform, verify_data_root};
    let parent = PathBuf::from(
        std::env::var_os("ENOUIA_SYNTHETIC_DURABLE_PARENT").expect("explicit synthetic parent"),
    );
    assert!(parent.is_absolute() && parent.is_dir());
    let home = parent.join(format!(
        "enouia-fresh-driver-synthetic-{}",
        enouia_memory_contract::ids::format_uuid_v4(OsIdSource.random_16())
    ));
    std::fs::create_dir(&home).unwrap();
    platform::restrict_to_owner(&home).unwrap();
    std::fs::write(
        home.join("DO_NOT_DELETE.USER_DATA"),
        b"Synthetic CLI validation only. Retain this test home; no real owner data.",
    )
    .unwrap();
    std::fs::write(home.join(".enouia-owner-data.json"),serde_json::to_vec(&json!({
        "format":"enouia-owner-data-home/1","storage_class":"owner_data","allow_development_cleanup":false,
        "allow_git_upload":false,"production_vault_created":false,"active_vault":null,
        "synthetic_validation_only":true
    })).unwrap()).unwrap();
    let home_arg = home.to_str().unwrap().to_owned();
    let registration = run_cli(&[
        "prepare".into(),
        home_arg.clone(),
        "--bind-current-owner-self".into(),
    ])
    .unwrap();
    let operation = registration["operation_id"].as_str().unwrap().to_owned();
    run_cli(&[
        "commit".into(),
        home_arg.clone(),
        operation.clone(),
        "--bind-current-owner-self".into(),
    ])
    .unwrap();
    let (dir, mut manifest) = mixed_input();
    manifest.sources[0].visible_text_only = true;
    manifest.attachment_retention = AttachmentRetention::MetadataOnly;
    for source in &mut manifest.sources {
        for attachment in &mut source.attachments {
            attachment.original = None;
        }
    }
    let now = SystemClock.now_unix_ms();
    manifest.formed_at = Timestamp::from_unix_ms(now).unwrap();
    manifest.formation_until = Timestamp::from_unix_ms(now + WEEK_MS).unwrap();
    save(&dir, &manifest);
    let invoke = |command: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_enouia-memory-fresh"))
            .args([
                command,
                dir.join("manifest.json").to_str().unwrap(),
                &home_arg,
                &operation,
                "--bind-current-owner-self",
            ])
            .output()
            .unwrap()
    };
    for _ in 0..2 {
        let result = invoke("run");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(result["ledger"]["extraction"]["committed_revisions"], 2);
        assert_eq!(result["ledger"]["extraction"]["model_invocations"], 0);
    }
    let (root, descriptor) = registered_owner_root(
        &home,
        &enouia_memory_contract::ids::OperationId::parse(&operation).unwrap(),
    )
    .unwrap();
    let mut init = Initializer::open_synthetic(
        &root,
        descriptor.host_id.clone(),
        Arc::new(SystemClock),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert!(!init.step().unwrap());
    let before = init.vault.snapshot().unwrap();
    init.vault
        .read(&descriptor.system_actor, Sensitivity::Private, |data| {
            assert_eq!(data.memories.len(), 2);
            assert_eq!(data.sources.len(), 2);
            assert!(data.owner_actions.is_empty());
            assert!(data.sources.values().flat_map(|s| &s.attachments).all(|a| {
                a.object_hash.is_none()
                    && a.extensions["memory-init.attachment-retention"]["payload"] == "not_selected"
            }));
            let sources = data
                .sources
                .values()
                .map(|s| (*s).clone())
                .collect::<Vec<_>>();
            assert!(data.memories.iter().all(|m| {
                m.memory
                    .decision
                    .input
                    .record_fields
                    .as_ref()
                    .unwrap()
                    .evidence
                    .iter()
                    .all(|q| q.resolves(&sources))
            }));
        })
        .unwrap();
    drop(init);
    let recovered = invoke("recover");
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    manifest.analysis.statements[0].content.push_str(" changed");
    save(&dir, &manifest);
    let conflict = invoke("recover");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("Conflict"));
    let reopened = Initializer::open_synthetic(
        &root,
        descriptor.host_id.clone(),
        Arc::new(SystemClock),
        Arc::new(OsIdSource),
    )
    .unwrap();
    assert_eq!(reopened.vault.snapshot().unwrap(), before);
    // Explicitly supplied target is a protected test home; never remove it.
    assert!(verify_data_root(&home, &RootPolicy::default()).is_ok());
}
