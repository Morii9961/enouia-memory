mod support;
use enouia_memory_contract::{
    ContractError,
    common::ActorType,
    json::canonical_bytes,
    next::*,
    store::{CurrentPointer, VaultDescriptor, parse_store},
};
use serde_json::Value;
use support::schema::SchemaStore;

fn fixture(name: &str) -> Value {
    support::fixture(&format!("../automation-vault/{name}.json"))
}
fn parse(kind: &str, value: &Value) -> Result<Value, ContractError> {
    let bytes = canonical_bytes(value)?;
    match kind {
        "descriptor" => {
            parse_document::<AutomationDescriptor>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "current" => {
            parse_document::<AutomationCurrent>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "publication" => parse_document::<AutomationPublication>(&bytes)
            .map(|v| serde_json::to_value(v).unwrap()),
        "commit" => {
            parse_document::<AutomationCommit>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        _ => panic!("unknown synthetic type"),
    }
}
#[test]
fn automation_storage_schemas_and_native_set_validation_agree() {
    let manifest = fixture("manifest");
    let schemas = SchemaStore::load();
    for entry in manifest["valid"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap().trim_end_matches(".json");
        let value = fixture(name);
        assert!(
            schemas
                .validate("next/storage-v2.schema.json", &value)
                .is_empty(),
            "{name}"
        );
        assert_eq!(
            parse(entry["type"].as_str().unwrap(), &value).unwrap(),
            value
        );
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let base = case["base"].as_str().unwrap();
        let kind = manifest["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == base)
            .unwrap()["type"]
            .as_str()
            .unwrap();
        let mut value = fixture(base.trim_end_matches(".json"));
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/storage-v2.schema.json", &value)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(parse(kind, &value).is_err(), "{}", case["id"]);
    }
}
#[test]
fn the_complete_synthetic_chain_has_real_system_decisions_and_bound_receipts() {
    let descriptor: AutomationDescriptor = serde_json::from_value(fixture("descriptor")).unwrap();
    let mut previous = None;
    for name in ["genesis", "capture", "admitted"] {
        let commit: AutomationCommit =
            parse_document(&canonical_bytes(&fixture(name)).unwrap()).unwrap();
        validate_delta(&descriptor, previous.as_ref(), &commit).unwrap();
        previous = Some(commit);
    }
    let commit = previous.unwrap();
    let memory = &commit.data.memories[0];
    let receipt = &commit.data.receipts[0];
    assert_eq!(memory.decision.input.actor.actor_type, ActorType::System);
    assert_eq!(
        memory.decision.outcome.knowledge,
        KnowledgeState::AttributedStatement
    );
    assert_eq!(receipt.result, memory.memory);
    assert_eq!(receipt.committed_in, commit.commit_id);
    assert!(
        parse_store::<VaultDescriptor>(&canonical_bytes(&fixture("descriptor")).unwrap()).is_err()
    );
    assert!(parse_store::<CurrentPointer>(&canonical_bytes(&fixture("current")).unwrap()).is_err());
}
#[test]
fn delta_rejects_owner_impersonation_hidden_policy_changes_and_history_rewrites() {
    let descriptor: AutomationDescriptor = serde_json::from_value(fixture("descriptor")).unwrap();
    let before: AutomationCommit = serde_json::from_value(fixture("capture")).unwrap();
    let after: AutomationCommit = serde_json::from_value(fixture("admitted")).unwrap();
    let mut fake = after.clone();
    fake.actor = descriptor.created_by.clone();
    assert!(validate_delta(&descriptor, Some(&before), &fake).is_err());
    for field in ["policy", "history", "receipt", "sensitivity"] {
        let mut data = after.data.clone();
        match field {
            "policy" => data.policy.active = false,
            "history" => {
                data.sources[0].text.push_str(" altered");
                data.sources[0].record.content_hash =
                    enouia_memory_contract::hash::sha256(data.sources[0].text.as_bytes());
            }
            "receipt" => {
                data.receipts[0].binding_hash =
                    enouia_memory_contract::hash::sha256(b"false binding")
            }
            _ => {
                data.memories[0].decision.input.sensitivity =
                    enouia_memory_contract::common::Sensitivity::Public
            }
        }
        assert!(
            AutomationCommit::assemble(
                &descriptor,
                Some(&before),
                after.actor.clone(),
                AutomationChange::AdmitBatch,
                data,
                after.commit_id.clone(),
                after.created_at.clone()
            )
            .is_err(),
            "{field}"
        );
    }
}
