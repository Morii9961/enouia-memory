//! Re-initialize a v1 library into a new Owner Vault (ADR-MEM-135).
//!
//! `enouia-memory-reinit plan <snapshot-dir>` reports what would be formed
//! and writes nothing. `enouia-memory-reinit run <snapshot-dir> <vault-root>`
//! forms it in a root the native Owner bootstrap has just created, and resumes
//! the same run if it was interrupted. `<snapshot-dir>` is the output of
//! `enouia-memory legacy-snapshot`. Run as the account owning the Vault root.
use enouia_memory_contract::{
    common::{ActorRef, ActorType, Sensitivity},
    foundation::Clock,
    ids::{PolicyId, PrincipalId, SubjectId, VaultId},
    json::Revision,
    next::*,
    ports::IdSource,
    set::RecordSet,
    time::Timestamp,
};
use enouia_memory_init::{legacy::legacy_batches, parse::*, pipeline::*};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, automation::AutomationFault, verify_data_root,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::Arc};

const USAGE: &str =
    "usage: enouia-memory-reinit plan <snapshot-dir> | run <snapshot-dir> <vault-root>";
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn run(args: &[String]) -> Result<Value, String> {
    let (command, snapshot, root) = match args {
        [c, s] if c == "plan" => (c, s, None),
        [c, s, r] if c == "run" => (c, s, Some(r)),
        _ => return Err(USAGE.into()),
    };
    let dir = Path::new(snapshot);
    let read = |name: &str| -> Result<Value, String> {
        let bytes = std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{name}: {e}"))
    };
    let manifest = read("manifest.json")?;
    if manifest["format"] != "enouia-legacy-snapshot/1" {
        return Err("manifest: not an enouia-legacy-snapshot/1".into());
    }
    let set = RecordSet::from_value(&read("snapshot.json")?)
        .map_err(|(at, _)| format!("snapshot.json: invalid record at {at}"))?;
    let mut materials = vec![];
    for object in manifest["objects"].as_array().into_iter().flatten() {
        if object["role"] != "import_input" {
            continue;
        }
        let hash = object["hash"]
            .as_str()
            .ok_or("manifest: object without hash")?;
        materials.push(Material {
            alias: format!("legacy-import-{}", materials.len() + 1),
            format: Format::Auto,
            bytes: std::fs::read(dir.join("objects").join(hash))
                .map_err(|e| format!("object {hash}: {e}"))?,
            account_alias: "legacy-v1".into(),
        });
    }
    let prep = prepare(&materials).map_err(|e| format!("parse: {e}"))?;
    let (batches, report) = legacy_batches(&prep, &set, 64, 128)?;
    let (clock, ids) = (Arc::new(SystemClock), Arc::new(OsIdSource));
    let now = Timestamp::from_unix_ms(clock.now_unix_ms()).ok_or("clock")?;
    let until = Timestamp::from_unix_ms(clock.now_unix_ms() + WEEK_MS).ok_or("clock")?;
    // Plans are built entirely in memory, so `plan` checks every batch too.
    let plans = |system: &ActorRef,
                 vault: &VaultId,
                 subject: &SubjectId,
                 access: &PolicyId,
                 egress: &PolicyId| {
        batches
            .iter()
            .map(|batch| {
                let policy = AutomationPolicy {
                    schema_version: NextVersion,
                    kind: PolicyKind::AutomationPolicy,
                    policy_id: PolicyId::from_random(ids.random_16()),
                    revision: Revision::new(1).unwrap(),
                    rule_version: RuleVersion::V3,
                    system_actor: system.clone(),
                    vault_id: vault.clone(),
                    subject_id: subject.clone(),
                    project_id: None,
                    source_ids: vec![],
                    allow_create: true,
                    allow_revise_created: false,
                    target_ids: vec![],
                    allowed_forms: vec![
                        FormationKind::Fact,
                        FormationKind::Preference,
                        FormationKind::Episode,
                    ],
                    max_sensitivity: batch
                        .preparation
                        .messages
                        .iter()
                        .map(|m| m.sensitivity)
                        .max()
                        .unwrap_or(Sensitivity::Private),
                    access_policy_id: access.clone(),
                    egress_policy_id: egress.clone(),
                    active: true,
                    valid_from: now.clone(),
                    valid_until: until.clone(),
                };
                Plan::build(
                    &batch.preparation,
                    &batch.statements,
                    policy,
                    FormationMode::Backfill,
                    now.clone(),
                    &*ids,
                )
                .map_err(|e| format!("plan: {e:?}"))
            })
            .collect::<Result<Vec<_>, String>>()
    };
    let random_policy = || PolicyId::from_random(ids.random_16());
    let Some(root) = root else {
        let system = ActorRef {
            actor_id: PrincipalId::from_random(ids.random_16()),
            actor_type: ActorType::System,
        };
        let built = plans(
            &system,
            &VaultId::from_random(ids.random_16()),
            &SubjectId::from_random(ids.random_16()),
            &random_policy(),
            &random_policy(),
        )?;
        let sum = |f: fn(&Preflight) -> usize| built.iter().map(|p| f(&p.preflight)).sum::<usize>();
        let mut warnings_by_code = BTreeMap::<&str, usize>::new();
        for warning in built
            .iter()
            .flat_map(|p| &p.sources)
            .flat_map(|s| &s.record.parse_warnings)
        {
            *warnings_by_code.entry(&warning.code).or_default() += 1;
        }
        return Ok(
            json!({"command": command, "report": report, "parsed_messages": prep.messages.len(),
            "unparseable": prep.unparseable, "unsupported": prep.unsupported,
            "planned": {"sources": sum(|p| p.parsed_sources), "statements": sum(|p| p.statements),
                "staged": sum(|p| p.proposed_staged), "warnings": sum(|p| p.warnings),
                "warnings_by_code": warnings_by_code},
            "writes": 0}),
        );
    };
    let root = verify_data_root(Path::new(root), &RootPolicy::default())
        .map_err(|e| format!("vault root: {e:?}"))?;
    let descriptor: AutomationDescriptor = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join(enouia_memory_contract::layout::vault_descriptor()),
        )
        .map_err(|e| format!("vault descriptor: {e}"))?,
    )
    .map_err(|e| format!("vault descriptor: {e}"))?;
    let host = descriptor.host_id.clone();
    let mut init =
        match Initializer::adopt_bootstrapped(&root, host.clone(), clock.clone(), ids.clone()) {
            Err(AutomationFault::Conflict) => {
                Initializer::open_synthetic(&root, host, clock.clone(), ids.clone())
            }
            other => other,
        }
        .map_err(|e| format!("initializer: {e:?}"))?;
    // A resumed run keeps the access and egress policies its plans recorded.
    let (access, egress) = match init.plans().first() {
        Some(p) => (
            p.policy.access_policy_id.clone(),
            p.policy.egress_policy_id.clone(),
        ),
        None => (random_policy(), random_policy()),
    };
    let subject = descriptor
        .owner_subject_id
        .clone()
        .ok_or("vault has no owner subject")?;
    for (plan, batch) in plans(
        &descriptor.system_actor,
        &descriptor.vault_id,
        &subject,
        &access,
        &egress,
    )?
    .into_iter()
    .zip(&batches)
    {
        init.enqueue(plan, &batch.preparation)
            .map_err(|e| format!("enqueue: {e:?}"))?;
    }
    while init.step().map_err(|e| format!("step: {e:?}"))? {}
    Ok(
        json!({"command": command, "report": report, "ledger": init.ledger().map_err(|e| format!("ledger: {e:?}"))?}),
    )
}

fn main() {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!("enouia-memory-reinit: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_explains_warning_totals_without_exposing_source_text_or_creating_a_vault() {
        let input = std::env::temp_dir().join(format!(
            "enouia-reinit-warning-synthetic-{}",
            enouia_memory_contract::ids::format_uuid_v4(OsIdSource.random_16())
        ));
        std::fs::create_dir(&input).unwrap();
        std::fs::create_dir(input.join("objects")).unwrap();
        let bytes = serde_json::to_vec(&json!([{
            "conversation_id": "synthetic-warning", "title": "Synthetic",
            "current_node": "n1", "mapping": {"n0": {"parent": null, "message": {
                "id": "a0", "author": {"role": "assistant"}, "create_time": 1781085599,
                "content": {"parts": ["Synthetic parent omitted by the legacy selection."]}
            }}, "n1": {"parent": "n0", "message": {
                "id": "m1", "author": {"role": "user"}, "create_time": 1781085600,
                "content": {"parts": ["SYNTHETIC PRIVATE TEXT"]}
            }}}
        }]))
        .unwrap();
        let hash = enouia_memory_contract::hash::sha256(&bytes);
        std::fs::write(input.join("objects").join(hash.as_str()), bytes).unwrap();
        let mut source: enouia_memory_contract::source::SourceRecord = serde_json::from_slice(
            include_bytes!("../../../../tests/fixtures/memory/records/source-export-user.json"),
        )
        .unwrap();
        source.original_conversation_id = Some("synthetic-warning".into());
        source.original_message_id = Some("m1".into());
        std::fs::write(
            input.join("snapshot.json"),
            serde_json::to_vec(&json!({"sources": [source]})).unwrap(),
        )
        .unwrap();
        std::fs::write(input.join("manifest.json"), serde_json::to_vec(&json!({
            "format": "enouia-legacy-snapshot/1", "objects": [{"role": "import_input", "hash": hash}]
        })).unwrap()).unwrap();
        let result = run(&["plan".into(), input.to_str().unwrap().into()]).unwrap();
        let codes = result["planned"]["warnings_by_code"].as_object().unwrap();
        assert_eq!(codes["language_unknown"], 1);
        assert_eq!(codes["parent_not_in_selection"], 1);
        assert_eq!(
            codes.values().map(|v| v.as_u64().unwrap()).sum::<u64>(),
            result["planned"]["warnings"].as_u64().unwrap()
        );
        assert_eq!(result["report"]["matched_sources"], 1);
        assert_eq!(result["writes"], 0);
        assert!(!input.join("vault").exists());
        assert!(!result.to_string().contains("SYNTHETIC PRIVATE TEXT"));
        // Retain this small, uniquely allocated synthetic input for diagnosis.
    }
}
