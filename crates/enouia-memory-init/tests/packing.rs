use enouia_memory_contract::{
    common::*, ids::*, json::Revision, next::*, ports::IdSource, time::Timestamp,
};
use enouia_memory_import::{locate, parsed::ParentLink};
use enouia_memory_init::{packing::pack_analyzed, parse::*, pipeline::*};
use enouia_memory_vault::OsIdSource;
use serde_json::json;

fn prep(count: usize) -> Preparation {
    prepare(&[Material {
        alias: "synthetic-packing".into(),
        account_alias: "synthetic".into(),
        format: Format::Auto,
        bytes: serde_json::to_vec(
            &(0..count)
                .map(|i| {
                    json!({
                        "conversation_id":format!("conversation-{i}"), "title":"Synthetic",
                        "mapping":{"n":{"parent":null,"message":{"id":format!("m{i}"),
                            "author":{"role":"user"}, "create_time":1700000000+i,
                            "content":{"parts":[format!("Synthetic statement {i}. 第二段。") ]}}}}
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    }])
    .unwrap()
}
fn analysis(count: usize, sources: usize) -> AnalyzedBatch {
    AnalyzedBatch {
        provenance: AnalysisProvenance {
            actor: "Synthetic packer test".into(),
            prompt_version: "synthetic-packing-1".into(),
            model_version: None,
        },
        statements: (0..count)
            .map(|i| {
                let source = i % sources;
                AnalyzedStatement {
                    statement: Statement {
                        source_index: source,
                        quote: format!("Synthetic statement {source}."),
                        title: format!("Synthetic {i}"),
                        interpretation: Interpretation::ClearSelfReport,
                        dimensions: dimensions(FormationKind::Fact),
                        event_time: unknown_time(),
                        valid_time: unknown_time(),
                        entities: vec![],
                        note: None,
                        annotations: AnalysisAnnotations::default(),
                        revises_statement: None,
                        derives_from_statements: vec![],
                        context_sources: vec![],
                    },
                    content: format!("New authored synthetic content {i}."),
                    supporting_quotes: vec![],
                }
            })
            .collect(),
    }
}
fn plan(prep: &Preparation, analysis: &AnalyzedBatch) -> Plan {
    let ids = OsIdSource;
    let policy = AutomationPolicy {
        schema_version: NextVersion,
        kind: PolicyKind::AutomationPolicy,
        policy_id: PolicyId::from_random(ids.random_16()),
        revision: Revision::new(1).unwrap(),
        rule_version: RuleVersion::V3,
        system_actor: ActorRef {
            actor_id: PrincipalId::from_random(ids.random_16()),
            actor_type: ActorType::System,
        },
        vault_id: VaultId::from_random(ids.random_16()),
        subject_id: SubjectId::from_random(ids.random_16()),
        project_id: None,
        source_ids: vec![],
        allow_create: true,
        allow_revise_created: false,
        target_ids: vec![],
        allowed_forms: vec![FormationKind::Fact],
        max_sensitivity: Sensitivity::HighlySensitive,
        access_policy_id: PolicyId::from_random(ids.random_16()),
        egress_policy_id: PolicyId::from_random(ids.random_16()),
        active: true,
        valid_from: Timestamp::parse("2026-11-14T00:00:00.000Z").unwrap(),
        valid_until: Timestamp::parse("2026-11-21T00:00:00.000Z").unwrap(),
    };
    Plan::build_analyzed(
        prep,
        analysis,
        policy,
        FormationMode::Backfill,
        Timestamp::parse("2026-11-14T00:00:00.000Z").unwrap(),
        &ids,
    )
    .unwrap()
}

#[test]
fn packs_source_and_statement_boundaries_without_losing_content_or_quotes() {
    let p = prep(65);
    let a = analysis(65, 65);
    let batches = pack_analyzed(&p, &a).unwrap();
    assert_eq!(
        batches
            .iter()
            .map(|b| b.preparation.messages.len())
            .collect::<Vec<_>>(),
        [64, 1]
    );
    assert_eq!(
        batches
            .iter()
            .flat_map(|b| b.statement_indices.clone())
            .collect::<Vec<_>>(),
        (0..65).collect::<Vec<_>>()
    );
    for batch in &batches {
        for (local, &original) in batch.statement_indices.iter().enumerate() {
            assert_eq!(
                batch.analysis.statements[local].content,
                a.statements[original].content
            );
            assert_eq!(
                batch.analysis.statements[local].statement.quote,
                a.statements[original].statement.quote
            );
        }
        let built = plan(&batch.preparation, &batch.analysis);
        assert_eq!(built.preflight.statements, batch.statement_indices.len());
    }
    let b = pack_analyzed(&prep(1), &analysis(129, 1)).unwrap();
    assert_eq!(
        b.iter()
            .map(|p| p.analysis.statements.len())
            .collect::<Vec<_>>(),
        [128, 1]
    );
}

#[test]
fn cross_conversation_support_and_context_keep_raw_locators_and_remapped_indices() {
    let mut p = prep(66);
    p.messages[64].parent = ParentLink::Upstream("original-parent-not-in-batch".into());
    p.messages[64].branch = Some("original-fork".into());
    p.messages[64]
        .attachments
        .push(enouia_memory_import::parsed::ParsedAttachment {
            original_name: Some("synthetic.txt".into()),
            claimed_media_type: Some("text/plain".into()),
            size_bytes: Some(5),
            availability: enouia_memory_contract::source::Availability::Present,
            bytes: Some(b"hello".to_vec()),
            external_reference: None,
        });
    let mut a = analysis(1, 1);
    a.statements[0].statement.source_index = 64;
    a.statements[0].statement.quote = "Synthetic statement 64.".into();
    a.statements[0].statement.context_sources = vec![65];
    a.statements[0].supporting_quotes = vec![
        StatementQuote {
            source_index: 2,
            quote: "Synthetic statement 2.".into(),
        },
        StatementQuote {
            source_index: 64,
            quote: "第二段。".into(),
        },
    ];
    let b = pack_analyzed(&p, &a).unwrap();
    let b = &b[0];
    assert_eq!(b.preparation.messages.len(), 3);
    assert_eq!(b.analysis.statements[0].statement.source_index, 1);
    assert_eq!(b.analysis.statements[0].statement.context_sources, [2]);
    assert_eq!(
        b.analysis.statements[0]
            .supporting_quotes
            .iter()
            .map(|q| q.source_index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    let m = &b.preparation.messages[1];
    assert_eq!(m.parent, p.messages[64].parent);
    assert_eq!(m.branch.as_deref(), Some("original-fork"));
    assert_eq!(m.attachments[0].bytes, p.messages[64].attachments[0].bytes);
    assert_eq!(
        locate::resolve(&b.preparation.objects[&m.raw_hash], &m.locator).unwrap(),
        m.text.as_bytes()
    );
    assert_eq!(b.preparation.objects, p.objects);
    assert_eq!(plan(&b.preparation, &b.analysis).preflight.exact_quotes, 3);
}

#[test]
fn capacity_invalid_sources_and_cross_statement_links_refuse_whole_preparation() {
    let mut p = prep(65);
    let mut a = analysis(1, 1);
    a.statements[0].statement.context_sources = (1..65).collect();
    assert_eq!(
        pack_analyzed(&p, &a).err(),
        Some("init.packing_statement_capacity")
    );
    a.statements[0].statement.context_sources = vec![65];
    assert_eq!(
        pack_analyzed(&p, &a).err(),
        Some("init.packing_source_index")
    );
    a.statements[0].statement.context_sources.clear();
    a.statements[0].statement.revises_statement = Some(0);
    assert_eq!(
        pack_analyzed(&p, &a).err(),
        Some("init.packing_statement_links")
    );
    a.statements[0].statement.revises_statement = None;
    a.statements[0].statement.derives_from_statements = vec![0];
    assert_eq!(
        pack_analyzed(&p, &a).err(),
        Some("init.packing_statement_links")
    );
    a.statements[0].statement.derives_from_statements.clear();
    p.messages[0].text = "x".repeat(16 * 1024 + 1);
    assert_eq!(
        pack_analyzed(&p, &a).err(),
        Some("init.packing_source_capacity")
    );
    let mut p = prep(1);
    p.objects.clear();
    assert_eq!(
        pack_analyzed(&p, &analysis(1, 1)).err(),
        Some("init.packing_raw_missing")
    );
    assert_eq!(
        pack_analyzed(&p, &analysis(0, 1)).err(),
        Some("init.packing_empty_analysis")
    );
}

#[test]
fn selection_watermarks_bind_scope_and_source_values_but_analysis_keeps_resume_identity() {
    let mut p = prep(2);
    let a = analysis(1, 1);
    let first = pack_analyzed(&p, &a).unwrap()[0].preparation.watermark();
    let mut changed = a.clone();
    changed.statements[0].content.push_str(" Changed wording.");
    assert_eq!(
        pack_analyzed(&p, &changed).unwrap()[0]
            .preparation
            .watermark(),
        first
    );
    p.messages[0].sensitivity = Sensitivity::HighlySensitive;
    assert_ne!(
        pack_analyzed(&p, &a).unwrap()[0].preparation.watermark(),
        first
    );
    p.messages[0].sensitivity = Sensitivity::Private;
    p.messages[0].metadata["extra"] = json!(true);
    assert_ne!(
        pack_analyzed(&p, &a).unwrap()[0].preparation.watermark(),
        first
    );
    let mut other = a;
    other.statements[0].statement.source_index = 1;
    assert_ne!(
        pack_analyzed(&p, &other).unwrap()[0]
            .preparation
            .watermark(),
        first
    );
}

#[test]
fn filtering_keeps_original_fork_lineage_and_omits_only_unused_objects() {
    let p=prepare(&[
        Material{alias:"fork".into(),account_alias:"synthetic".into(),format:Format::Auto,
            bytes:serde_json::to_vec(&json!([{"conversation_id":"fork","mapping":{
                "root":{"parent":null,"message":{"id":"root","author":{"role":"user"},"content":{"parts":["Root."]},"create_time":1700000000}},
                "left":{"parent":"root","message":{"id":"left","author":{"role":"user"},"content":{"parts":["Left."]},"create_time":1700000001}},
                "right":{"parent":"root","message":{"id":"right","author":{"role":"user"},"content":{"parts":["Right."]},"create_time":1700000002}},
                "leaf":{"parent":"left","message":{"id":"leaf","author":{"role":"user"},"content":{"parts":["Synthetic statement 0."]},"create_time":1700000003}}
            }}])).unwrap()},
        Material{alias:"unused".into(),account_alias:"synthetic".into(),format:Format::Unsupported,bytes:b"UNSELECTED RAW".to_vec()},
    ]).unwrap();
    assert_eq!(p.unsupported, 1);
    assert_eq!(p.objects.len(), 2);
    let mut a = analysis(1, 1);
    a.statements[0].statement.source_index = p
        .messages
        .iter()
        .position(|m| m.upstream_id.as_deref() == Some("leaf"))
        .unwrap();
    let batches = pack_analyzed(&p, &a).unwrap();
    let b = &batches[0];
    assert_eq!(b.preparation.messages.len(), 1);
    assert_eq!(b.preparation.messages[0].branch.as_deref(), Some("left"));
    assert_eq!(
        b.preparation.messages[0].parent,
        ParentLink::Upstream("left".into())
    );
    assert_eq!(b.preparation.objects.len(), 1);
    assert_eq!(b.preparation.inputs.len(), 1);
    assert_eq!(b.preparation.unsupported, 0);
    assert_eq!(p.unsupported, 1);
    assert!(
        !b.preparation
            .objects
            .values()
            .any(|v| v == b"UNSELECTED RAW")
    );
    let built = plan(&b.preparation, &b.analysis);
    assert_eq!(
        built.sources[0].record.extensions["memory-init.metadata"]["original_branch"],
        "left"
    );
}
