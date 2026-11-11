//! ADR137: shared seed behavior without a filesystem, model or Provider.
#[path = "support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{hash::sha256, json::Revision, next::*};
use enouia_memory_vault::automation::segmented::{FormationSeed, formation_seed, quoted_evidence};
use fixture::*;

#[test]
fn fallback_uses_first_nonempty_span_and_keeps_exact_multiline_evidence() {
    let policy: AutomationPolicy = serde_json::from_value(value("formation/policy.json")).unwrap();
    let template: AdmissionInput = serde_json::from_value(value("ui-records/input.json")).unwrap();
    let long = "合".repeat(65);
    for (quote, title, expected) in [
        ("First line\nSecond line", "absent", Some("First line")),
        ("\r\n\t  \t 第一段 \r\n第二段", "absent", Some("第一段")),
        ("First\tsecond", "absent", Some("First")),
        ("First\u{0000}second", "absent", Some("First")),
        ("Single line", "absent", Some("Single line")),
        ("before EXISTING after\nnext", "existing", Some("existing")),
        (long.as_str(), "absent", Some(&long[..60 * "合".len()])),
        ("First\nSecond", "First\nSecond", None),
        ("Single line", "", None),
        (long.as_str(), long.as_str(), None),
    ] {
        let mut source: CapturedSource =
            serde_json::from_value(value("ui-records/source.json")).unwrap();
        source.text = quote.into();
        source.record.content_hash = sha256(quote.as_bytes());
        let input = formation_seed(FormationSeed {
            policy: &policy,
            source: &source,
            evidence: quoted_evidence(&source, quote).unwrap(),
            snapshot_commit: template.snapshot.base_commit_id.clone(),
            operation_id: template.operation_id.clone(),
            result: MemoryRevisionRef {
                memory_id: template.result.memory_id.clone(),
                revision: Revision::new(1).unwrap(),
            },
            title,
            interpretation: template.interpretation,
            dimensions: template.dimensions.clone().unwrap(),
            event_time: template.record_fields.as_ref().unwrap().event_time.clone(),
            valid_time: template.record_fields.as_ref().unwrap().valid_time.clone(),
            entities: vec![],
            prompt_version: "synthetic-title-1",
            model_version: None,
            import_batch_id: None,
            mode: FormationMode::Live,
            annotations: None,
        });
        let Some(expected) = expected else {
            assert!(
                input.is_none(),
                "invalid supplied title must still be refused"
            );
            continue;
        };
        let input = input.unwrap();
        let fields = input.record_fields.as_ref().unwrap();
        assert_eq!(fields.title, expected);
        assert!(!fields.title.chars().any(char::is_control));
        assert_eq!(input.content, quote);
        assert_eq!(fields.evidence[0].quote, quote);
        assert!(fields.evidence[0].resolves(&[source]));
    }
}
