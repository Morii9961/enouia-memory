mod support;
use enouia_memory_contract::extraction::ExtractionJob;
use support::{fixture, schema::SchemaStore};

#[test]
fn extraction_schema_and_domain_boundaries_are_explicit() {
    let schemas = SchemaStore::load();
    let cases = fixture("extraction-job-cases.json");
    for case in cases["cases"].as_array().unwrap() {
        let mut value = cases["base"].clone();
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("provider/extraction-job-v1.schema.json", &value)
                .is_empty(),
            case["schema"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::from_value::<ExtractionJob>(value).is_ok_and(|j| j.validate().is_empty()),
            case["domain"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn extraction_source_hash_and_candidate_references_are_store_invariants() {
    use enouia_memory_contract::{hash::sha256, ids::CandidateId, set::validate_records};
    let mut world = support::extraction_world();
    assert!(
        validate_records(&world).is_empty(),
        "{:?}",
        validate_records(&world)
    );
    let correct = world.sessions[0].extraction_jobs[0].sources[0]
        .content_hash
        .clone();
    world.sessions[0].extraction_jobs[0].sources[0].content_hash =
        sha256(b"different-synthetic-source");
    assert!(
        validate_records(&world)
            .iter()
            .any(|v| v.rule == "extraction.source")
    );
    world.sessions[0].extraction_jobs[0].sources[0].content_hash = correct;
    world.sessions[0].extraction_jobs[0].cursor = 1;
    world.sessions[0].extraction_jobs[0].candidates = vec![Some(
        CandidateId::parse("cand_00009999-0000-4000-8000-000000009999").unwrap(),
    )];
    assert!(
        validate_records(&world)
            .iter()
            .any(|v| v.rule == "extraction.candidate")
    );
}
