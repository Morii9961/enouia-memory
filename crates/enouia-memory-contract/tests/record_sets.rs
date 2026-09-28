//! Cross-record constraints (reference closure, review binding, status
//! history, supersession, temporal validity, deletion/policy barriers, capsule
//! ↔ inspection ↔ dispatch agreement) over consistent synthetic sets and
//! single-rule mutations, plus the MoriMeta story expectations.

mod support;

use enouia_memory_contract::context::{Currency, Decision};
use enouia_memory_contract::memory::MemoryBody;
use enouia_memory_contract::set::{RecordSet, validate_set};
use enouia_memory_contract::temporal::{Effect, currency, effect};
use enouia_memory_contract::time::Timestamp;
use serde_json::Value;
use support::schema::SchemaStore;
use support::{apply_ops, fixture};

const SET_SCHEMAS: &[(&str, &str)] = &[
    ("sources", "memory/source-v1.schema.json"),
    ("attachments", "memory/attachment-v1.schema.json"),
    ("projects", "memory/project-v1.schema.json"),
    ("memories", "memory/memory-v1.schema.json"),
    ("candidates", "memory/candidate-v1.schema.json"),
    ("reviews", "memory/review-v1.schema.json"),
    ("identities", "memory/identity-v1.schema.json"),
    ("sessions", "memory/session-v1.schema.json"),
    ("session_events", "memory/session-event-v1.schema.json"),
    ("checkpoints", "memory/checkpoint-v1.schema.json"),
    ("commits", "memory/commit-v1.schema.json"),
    ("tombstones", "memory/tombstone-v1.schema.json"),
    ("purge_receipts", "memory/purge-receipt-v1.schema.json"),
    ("audit_events", "memory/audit-event-v1.schema.json"),
    ("capsules", "context/capsule-v1.schema.json"),
    ("inspections", "context/inspection-v1.schema.json"),
    ("dispatches", "context/dispatch-v1.schema.json"),
];

fn load(value: &Value) -> RecordSet {
    RecordSet::from_value(value).unwrap_or_else(|(path, e)| panic!("record at {path} invalid: {e}"))
}

#[test]
fn consistent_sets_pass_schema_record_and_set_validation() {
    let store = SchemaStore::load();
    let manifest = fixture("sets-manifest.json");
    for file in manifest["valid"].as_array().unwrap() {
        let file = file.as_str().unwrap();
        let value = fixture(file);
        for (key, schema) in SET_SCHEMAS {
            for (index, record) in value[key].as_array().into_iter().flatten().enumerate() {
                let errors = store.validate(schema, record);
                assert!(errors.is_empty(), "{file} {key}/{index}: {errors:#?}");
            }
        }
        let set = load(&value);
        let violations = validate_set(&set);
        assert!(violations.is_empty(), "{file}: {violations:#?}");
    }
}

#[test]
fn single_rule_mutations_produce_the_named_violation() {
    let manifest = fixture("sets-manifest.json");
    let cases = manifest["invalid"].as_array().unwrap();
    assert!(cases.len() >= 25);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let mut value = fixture(case["base"].as_str().unwrap());
        apply_ops(&mut value, &case["ops"]);
        let set = RecordSet::from_value(&value)
            .unwrap_or_else(|(path, e)| panic!("{id}: mutation broke record {path}: {e}"));
        let rule = case["rust_rule"].as_str().unwrap();
        let violations = validate_set(&set);
        assert!(
            violations.iter().any(|v| v.rule == rule),
            "{id}: expected {rule}, got {violations:#?}"
        );
    }
}

#[test]
fn lifecycle_valid_time_rules_match_expectations() {
    let set = load(&fixture("sets/lifecycle.json"));
    let expectations = fixture("expectations/lifecycle-temporal.json");
    let now = Timestamp::parse(expectations["now"].as_str().unwrap()).unwrap();
    let latest = set.latest_memories();
    for case in expectations["cases"].as_array().unwrap() {
        let id = case["memory"].as_str().unwrap();
        let as_of = Timestamp::parse(case["as_of"].as_str().unwrap()).unwrap();
        let memory = latest.iter().find(|m| m.memory_id.as_str() == id).unwrap();
        let actual = match effect(memory, &latest, &as_of) {
            Effect::InEffect { .. } => "in_effect",
            Effect::NotYetEffective => "not_yet_effective",
            Effect::Ended => "ended",
            Effect::Superseded { .. } => "superseded",
            Effect::Archived => "archived",
        };
        assert_eq!(actual, case["effect"], "{id} at {as_of}");
        let expected: Option<Currency> = serde_json::from_value(case["currency"].clone()).unwrap();
        assert_eq!(
            currency(memory, &latest, &as_of, &now),
            expected,
            "{id} at {as_of}"
        );
    }
}

#[test]
fn future_effective_replacement_does_not_hide_the_current_fact() {
    let set = load(&fixture("sets/lifecycle.json"));
    let latest = set.latest_memories();
    let capsule = &set.capsules[0];
    let old = latest
        .iter()
        .find(|m| m.status == enouia_memory_contract::memory::MemoryStatus::Superseded)
        .unwrap();
    // Status says superseded, yet the fact is still in effect before 12-01.
    assert!(matches!(
        effect(old, &latest, &capsule.generated_at),
        Effect::InEffect { .. }
    ));
    assert!(capsule.memory_items().any(|i| i.memory_id == old.memory_id));
}

#[test]
fn morimeta_confirmed_story_supports_only_the_approved_decision() {
    let expected = fixture("expectations/morimeta.json");
    let expected = &expected["confirmed"];
    let set = load(&fixture(expected["set"].as_str().unwrap()));
    let capsule = &set.capsules[0];
    let included: Vec<&str> = capsule
        .memory_items()
        .map(|i| i.memory_id.as_str())
        .collect();
    let want: Vec<&str> = expected["included_memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(included, want);
    let memory = set
        .latest_memories()
        .into_iter()
        .find(|m| m.memory_id.as_str() == want[0])
        .unwrap();
    // Decision is recorded as decided, from a user statement; nothing says implemented/released.
    let MemoryBody::ProjectState(fields) = &memory.body else {
        panic!("project_state")
    };
    assert!(fields.state.is_empty() && fields.decisions.len() == 1);
    assert_eq!(
        fields.decisions[0].item_id.as_str(),
        expected["decision_item"]
    );
    assert!(
        memory
            .evidence
            .iter()
            .all(|e| e.evidence_class.is_user_evidence())
    );
    assert!(
        capsule
            .verification_needed
            .iter()
            .any(|v| { serde_json::to_value(v.reason).unwrap() == "implementation_unverified" })
    );
    assert!(
        capsule
            .recent_session_checkpoints
            .iter()
            .all(|c| { serde_json::to_value(c.status).unwrap() == expected["checkpoint_status"] })
    );
    assert!(capsule.open_loops.iter().all(|l| l.provisional));
    let inspection = &set.inspections[0];
    for (id, reason) in expected["excluded"].as_object().unwrap() {
        let decision = inspection
            .decisions
            .iter()
            .find(|d| &d.record_id == id)
            .unwrap();
        assert_eq!(decision.decision, Decision::Excluded);
        assert_eq!(
            serde_json::to_value(decision.reason).unwrap(),
            *reason,
            "{id}"
        );
    }
    // The model's three-direction proposal never became canonical.
    assert!(
        set.latest_memories()
            .iter()
            .all(|m| !m.content.contains("三种"))
    );
}

#[test]
fn morimeta_without_confirmation_has_no_chosen_design() {
    let expected = fixture("expectations/morimeta.json");
    let expected = &expected["insufficient_evidence"];
    let set = load(&fixture(expected["set"].as_str().unwrap()));
    assert!(validate_set(&set).is_empty());
    assert!(
        set.latest_memories()
            .iter()
            .all(|m| !m.content.contains("Professional Darkroom"))
    );
    let capsule = &set.capsules[0];
    assert_eq!(capsule.memory_items().count(), 0);
    assert!(!capsule.completeness.complete);
    assert_eq!(
        serde_json::to_value(&capsule.completeness.limitations).unwrap(),
        serde_json::json!([expected["limitation"]])
    );
    assert!(
        capsule
            .verification_needed
            .iter()
            .any(|v| { serde_json::to_value(v.reason).unwrap() == "no_supported_evidence" })
    );
    // The provisional checkpoint that mentions Darkroom stays provisional.
    assert!(
        capsule
            .recent_session_checkpoints
            .iter()
            .all(|c| { serde_json::to_value(c.status).unwrap() == expected["checkpoint_status"] })
    );
    // Adding a canonical Darkroom decision without any review is rejected.
    let mut value = fixture(expected["set"].as_str().unwrap());
    let forged = fixture("records/memory-project-state.json");
    value["memories"].as_array_mut().unwrap().push(forged);
    let set = load(&value);
    let rules: Vec<&str> = validate_set(&set).iter().map(|v| v.rule).collect();
    assert!(rules.contains(&"memory.review_missing"), "{rules:?}");
    assert!(rules.contains(&"evidence.unresolved"), "{rules:?}");
}
