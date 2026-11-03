mod support;
use enouia_memory_contract::{ids::VaultId, next::*};

#[test]
fn agent_correction_records_and_receipts_are_closed_and_keep_native_key_constraints() {
    let cases = support::fixture("../agent-correction/cases.json");
    let vault = VaultId::parse(cases["vault_id"].as_str().unwrap()).unwrap();
    let schemas = support::schema::SchemaStore::load();
    for (field, group) in [("record", "record_cases"), ("receipt", "receipt_cases")] {
        for case in cases[group].as_array().unwrap() {
            let mut value = cases[field].clone();
            support::apply_ops(&mut value, &case["ops"]);
            assert_eq!(
                schemas
                    .validate("next/agent-correction-v1.schema.json", &value)
                    .is_empty(),
                case["schema"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
            let native = if field == "record" {
                serde_json::from_value::<AgentCorrectionRecord>(value)
                    .is_ok_and(|r| r.valid(&vault))
            } else {
                serde_json::from_value::<AgentCorrectionReceipt>(value).is_ok()
            };
            assert_eq!(
                native,
                case["native"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
