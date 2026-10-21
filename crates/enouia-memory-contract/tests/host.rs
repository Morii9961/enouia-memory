//! MV-8.1a pure authority transitions and synthetic transaction failures.
//! Fake transactions do not prove OS locks, persistence or a migrated Vault.
mod support;

use enouia_memory_contract::ContractError;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::host::*;
use enouia_memory_contract::ids::{CommitId, DeviceId, OperationId, VaultId};
use enouia_memory_contract::json::{MAX_SAFE_INTEGER, Revision};
use enouia_memory_contract::store::{StoreDocument, parse_store_value};
use serde_json::Value;
use support::schema::SchemaStore;

fn fixture(name: &str) -> Value {
    support::fixture(&format!("../host/{name}.json"))
}
fn initial() -> AuthorityState {
    parse_store_value(&fixture("initial")).unwrap()
}
fn operation(n: u32) -> OperationId {
    OperationId::parse(&format!("op_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn host(n: u32) -> DeviceId {
    DeviceId::parse(&format!("dev_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn plan(state: &AuthorityState) -> TransferPlan {
    state.prepare_transfer(operation(1), host(2)).unwrap()
}
fn parse(kind: &str, value: &Value) -> Result<Value, ContractError> {
    let bytes = serde_json::to_vec(value).unwrap();
    match kind {
        "authority" => {
            parse_document::<AuthorityState>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "plan" => parse_document::<TransferPlan>(&bytes).map(|v| serde_json::to_value(v).unwrap()),
        _ => panic!("unknown synthetic fixture type"),
    }
}

#[test]
fn host_schemas_and_typed_rules_round_trip_all_valid_fixtures() {
    let schemas = SchemaStore::load();
    for entry in fixture("manifest")["valid"].as_array().unwrap() {
        let file = entry["file"].as_str().unwrap().trim_end_matches(".json");
        let value = fixture(file);
        assert!(
            schemas
                .validate(entry["schema"].as_str().unwrap(), &value)
                .is_empty(),
            "{file}"
        );
        assert_eq!(
            parse(entry["type"].as_str().unwrap(), &value).unwrap(),
            value,
            "{file}"
        );
        let bytes = serde_json::to_vec(&value).unwrap();
        match entry["type"].as_str().unwrap() {
            "authority" => {
                parse_document::<AuthorityState>(&bytes).unwrap();
            }
            "plan" => {
                parse_document::<TransferPlan>(&bytes).unwrap();
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn malformed_and_inconsistent_authority_is_rejected_by_named_rule() {
    let schemas = SchemaStore::load();
    let manifest = fixture("manifest");
    for case in manifest["invalid"].as_array().unwrap() {
        let base = manifest["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == case["base"])
            .unwrap();
        let mut value = support::fixture(&format!("../host/{}", case["base"].as_str().unwrap()));
        support::apply_ops(&mut value, &case["ops"]);
        let accepted = schemas
            .validate(base["schema"].as_str().unwrap(), &value)
            .is_empty();
        assert_eq!(accepted, case["schema"] == "accept", "{}", case["id"]);
        let error = parse(base["type"].as_str().unwrap(), &value).unwrap_err();
        assert!(
            error.rules().contains(&case["rust_rule"].as_str().unwrap()),
            "{}: {error}",
            case["id"]
        );
    }
}

#[test]
fn promotion_requires_quiescence_exact_target_and_checkpoint_then_fences_old_writer() {
    let original = initial();
    let plan = plan(&original);
    assert_eq!(
        original.promote(&host(2), &plan, &plan.plan_hash, &original.checkpoint),
        Err(HostFault::Conflict)
    );
    assert_eq!(
        original.quiesce(&original.primary, &plan, &sha256(b"different approval")),
        Err(HostFault::PlanMismatch)
    );
    let stopped = original
        .quiesce(&original.primary, &plan, &plan.plan_hash)
        .unwrap();
    assert_eq!(
        stopped.check_writer(&original.vault_id, &original.primary),
        Err(HostFault::Quiesced)
    );
    assert_eq!(
        stopped.quiesce(&original.primary, &plan, &plan.plan_hash),
        Ok(stopped.clone())
    );
    assert_eq!(
        stopped.promote(&host(3), &plan, &plan.plan_hash, &stopped.checkpoint),
        Err(HostFault::Fenced)
    );
    for field in [
        "commit_id",
        "snapshot_hash",
        "commit_sequence",
        "policy_epoch",
        "deletion_epoch",
    ] {
        let mut value = serde_json::to_value(&stopped.checkpoint).unwrap();
        value[field] = match field {
            "commit_id" => serde_json::to_value(
                CommitId::parse("cmt_00000002-0000-4000-8000-000000000001").unwrap(),
            )
            .unwrap(),
            "snapshot_hash" => serde_json::to_value(sha256(b"wrong snapshot")).unwrap(),
            _ => serde_json::json!(2),
        };
        let wrong: Checkpoint = serde_json::from_value(value).unwrap();
        assert_eq!(
            stopped.promote(&host(2), &plan, &plan.plan_hash, &wrong),
            Err(HostFault::CheckpointMismatch),
            "{field}"
        );
    }
    let promoted = stopped
        .promote(&host(2), &plan, &plan.plan_hash, &stopped.checkpoint)
        .unwrap();
    assert_eq!(
        promoted,
        parse_store_value::<AuthorityState>(&fixture("promoted")).unwrap()
    );
    assert_eq!(
        promoted.check_writer(&original.vault_id, &original.primary),
        Err(HostFault::Fenced)
    );
    assert_eq!(
        promoted.check_writer(&original.vault_id, &promoted.primary),
        Ok(())
    );
    let wrong_vault = VaultId::parse("vlt_00000002-0000-4000-8000-000000000001").unwrap();
    assert_eq!(
        promoted.check_writer(&wrong_vault, &promoted.primary),
        Err(HostFault::Fenced)
    );
}

fn next_checkpoint(state: &AuthorityState) -> Checkpoint {
    Checkpoint {
        commit_id: CommitId::parse("cmt_00000002-0000-4000-8000-000000000001").unwrap(),
        commit_sequence: Revision::new(state.checkpoint.commit_sequence.get() + 1).unwrap(),
        snapshot_hash: sha256(b"synthetic complete next snapshot"),
        ..state.checkpoint.clone()
    }
}

#[test]
fn data_progress_rejects_stale_head_changed_commit_and_watermark_rollback() {
    let mut original = initial();
    original.checkpoint.policy_epoch = 3;
    original.checkpoint.deletion_epoch = 4;
    let plan = plan(&original);
    let published = next_checkpoint(&original);
    let advanced = original
        .advance(&original.primary, &original.checkpoint, published.clone())
        .unwrap();
    assert_eq!(advanced.primary, original.primary);
    assert_eq!(
        advanced.quiesce(&original.primary, &plan, &plan.plan_hash),
        Err(HostFault::Conflict)
    );
    assert_eq!(
        advanced.advance(&advanced.primary, &original.checkpoint, published.clone()),
        Err(HostFault::Conflict)
    );
    let mut variants = Vec::new();
    let mut lower_policy = published.clone();
    lower_policy.policy_epoch -= 1;
    variants.push(lower_policy);
    let mut lower_delete = published.clone();
    lower_delete.deletion_epoch -= 1;
    variants.push(lower_delete);
    let mut reused_commit = published.clone();
    reused_commit.commit_id = original.checkpoint.commit_id.clone();
    variants.push(reused_commit);
    let mut reused_snapshot = published.clone();
    reused_snapshot.snapshot_hash = original.checkpoint.snapshot_hash.clone();
    variants.push(reused_snapshot);
    let mut skipped = published.clone();
    skipped.commit_sequence = Revision::new(3).unwrap();
    variants.push(skipped);
    let mut unsafe_epoch = published;
    unsafe_epoch.policy_epoch = MAX_SAFE_INTEGER + 1;
    variants.push(unsafe_epoch);
    for checkpoint in variants {
        assert_eq!(
            original.advance(&original.primary, &original.checkpoint, checkpoint),
            Err(HostFault::CheckpointMismatch)
        );
    }
}

#[test]
fn abort_restores_only_the_quiesced_primary_and_old_plan_cannot_promote_afterwards() {
    let original = initial();
    let plan = plan(&original);
    let stopped = original
        .quiesce(&original.primary, &plan, &plan.plan_hash)
        .unwrap();
    let resumed = stopped
        .abort(&original.primary, &plan, &plan.plan_hash)
        .unwrap();
    assert_eq!(resumed.primary, original.primary);
    assert_eq!(resumed.checkpoint, original.checkpoint);
    assert_eq!(
        resumed.check_writer(&resumed.vault_id, &original.primary),
        Ok(())
    );
    assert_eq!(
        resumed.promote(&host(2), &plan, &plan.plan_hash, &resumed.checkpoint),
        Err(HostFault::Conflict)
    );
    assert_eq!(
        resumed.quiesce(&original.primary, &plan, &plan.plan_hash),
        Err(HostFault::Conflict)
    );
    let promoted = stopped
        .promote(&host(2), &plan, &plan.plan_hash, &stopped.checkpoint)
        .unwrap();
    assert_eq!(
        promoted.abort(&original.primary, &plan, &plan.plan_hash),
        Err(HostFault::Fenced)
    );
    assert_eq!(
        promoted.abort(&promoted.primary, &plan, &plan.plan_hash),
        Err(HostFault::Conflict)
    );
}

#[test]
fn promotion_receipts_survive_later_writes_and_migrations_without_republication() {
    let original = initial();
    let first = plan(&original);
    let stopped = original
        .quiesce(&original.primary, &first, &first.plan_hash)
        .unwrap();
    let promoted = stopped
        .promote(&host(2), &first, &first.plan_hash, &stopped.checkpoint)
        .unwrap();
    let advanced = promoted
        .advance(
            &promoted.primary,
            &promoted.checkpoint,
            next_checkpoint(&promoted),
        )
        .unwrap();
    let second = advanced.prepare_transfer(operation(2), host(1)).unwrap();
    let stopped = advanced
        .quiesce(&advanced.primary, &second, &second.plan_hash)
        .unwrap();
    let final_state = stopped
        .promote(&host(1), &second, &second.plan_hash, &stopped.checkpoint)
        .unwrap();
    assert_eq!(final_state.primary.primary_epoch.get(), 3);
    assert_eq!(
        final_state.promote(&host(2), &first, &first.plan_hash, &first.intent.checkpoint),
        Ok(final_state.clone())
    );
    assert_eq!(
        final_state.promote(
            &host(1),
            &second,
            &second.plan_hash,
            &second.intent.checkpoint
        ),
        Ok(final_state.clone())
    );
    assert_eq!(
        final_state.prepare_transfer(operation(1), host(3)),
        Err(HostFault::PlanMismatch)
    );
    let changed = final_state.prepare_transfer(operation(3), host(3)).unwrap();
    let mut changed = changed;
    changed.intent.operation_id = first.intent.operation_id.clone();
    changed.plan_hash =
        sha256(&enouia_memory_contract::json::canonical_bytes(&changed.intent).unwrap());
    assert_eq!(
        final_state.promote(
            &host(3),
            &changed,
            &changed.plan_hash,
            &changed.intent.checkpoint
        ),
        Err(HostFault::PlanMismatch)
    );
    assert_eq!(
        final_state.check_writer(&original.vault_id, &original.primary),
        Err(HostFault::Fenced)
    );
}

#[test]
fn finite_receipt_budget_and_revision_exhaustion_fail_closed() {
    let mut full = initial();
    for n in 1..=MAX_PROMOTIONS {
        let plan = full
            .prepare_transfer(operation(n as u32), host(n as u32 + 1))
            .unwrap();
        let stopped = full.quiesce(&full.primary, &plan, &plan.plan_hash).unwrap();
        full = stopped
            .promote(
                &plan.intent.target_host,
                &plan,
                &plan.plan_hash,
                &stopped.checkpoint,
            )
            .unwrap();
    }
    assert_eq!(full.promotions.len(), MAX_PROMOTIONS);
    assert_eq!(
        full.prepare_transfer(operation(999), host(999)),
        Err(HostFault::ReceiptBudget)
    );
    let mut exhausted = initial();
    exhausted.revision = Revision::new(MAX_SAFE_INTEGER).unwrap();
    let plan = plan(&exhausted);
    assert_eq!(
        exhausted.quiesce(&exhausted.primary, &plan, &plan.plan_hash),
        Err(HostFault::CounterExhausted)
    );
}

#[derive(Clone, Copy, PartialEq)]
enum Failure {
    None,
    BeforePublish,
    AfterPublish,
    Unavailable,
}
struct FakeStore {
    state: AuthorityState,
    data: Checkpoint,
    failure: Failure,
    publications: usize,
}
impl FakeStore {
    fn new() -> Self {
        let state = initial();
        Self {
            data: state.checkpoint.clone(),
            state,
            failure: Failure::None,
            publications: 0,
        }
    }
}
impl AuthorityTransaction for FakeStore {
    fn load(&mut self) -> HostResult<AuthorityState> {
        Ok(self.state.clone())
    }
    fn verified_checkpoint(&mut self) -> HostResult<Checkpoint> {
        Ok(self.data.clone())
    }
    fn publish(&mut self, expected: &AuthorityState, next: &AuthorityState) -> HostResult<()> {
        if &self.state != expected {
            return Err(HostFault::Conflict);
        }
        if self.failure == Failure::BeforePublish {
            return Err(HostFault::StorageFailed);
        }
        assert!(next.validate().is_empty());
        self.state = next.clone();
        self.data = next.checkpoint.clone();
        self.publications += 1;
        if self.failure == Failure::AfterPublish {
            Err(HostFault::OutcomeUnknown)
        } else {
            Ok(())
        }
    }
}
impl AuthorityStore for FakeStore {
    fn transact<T>(
        &mut self,
        vault: &VaultId,
        action: impl FnOnce(&mut dyn AuthorityTransaction) -> HostResult<T>,
    ) -> HostResult<T> {
        if self.failure == Failure::Unavailable {
            return Err(HostFault::Unavailable);
        }
        if vault != &self.state.vault_id {
            return Err(HostFault::Fenced);
        }
        action(self)
    }
}
fn update(
    store: &mut impl AuthorityStore,
    vault: &VaultId,
    action: impl FnOnce(&AuthorityState) -> HostResult<AuthorityState>,
) -> HostResult<AuthorityState> {
    store.transact(vault, |tx| {
        let current = tx.load()?;
        if tx.verified_checkpoint()? != current.checkpoint {
            return Err(HostFault::CheckpointMismatch);
        }
        let next = action(&current)?;
        if next != current {
            tx.publish(&current, &next)?;
        }
        Ok(next)
    })
}

#[test]
fn synthetic_prepublication_failure_stays_quiesced_and_lost_reply_recovers_receipt() {
    let mut store = FakeStore::new();
    let original = store.state.clone();
    let plan = plan(&original);
    update(&mut store, &original.vault_id, |s| {
        s.quiesce(&original.primary, &plan, &plan.plan_hash)
    })
    .unwrap();
    let stopped = store.state.clone();
    store.failure = Failure::BeforePublish;
    let promote =
        |s: &AuthorityState| s.promote(&host(2), &plan, &plan.plan_hash, &plan.intent.checkpoint);
    assert_eq!(
        update(&mut store, &original.vault_id, promote),
        Err(HostFault::StorageFailed)
    );
    assert_eq!(store.state, stopped);
    assert_eq!(store.data, original.checkpoint);
    store.failure = Failure::AfterPublish;
    assert_eq!(
        update(&mut store, &original.vault_id, promote),
        Err(HostFault::OutcomeUnknown)
    );
    let reopened = store.state.clone();
    assert_eq!(reopened.promotions.len(), 1);
    assert_eq!(
        reopened.check_writer(&original.vault_id, &original.primary),
        Err(HostFault::Fenced)
    );
    store.failure = Failure::None;
    let publications = store.publications;
    assert_eq!(
        update(&mut store, &original.vault_id, promote),
        Ok(reopened)
    );
    assert_eq!(store.publications, publications);
}

#[test]
fn unavailable_authority_has_no_cached_fallback_and_stale_transaction_cannot_publish() {
    let mut store = FakeStore::new();
    let cached = store.state.clone();
    assert_eq!(
        cached.check_writer(&cached.vault_id, &cached.primary),
        Ok(())
    );
    store.failure = Failure::Unavailable;
    assert_eq!(
        update(&mut store, &cached.vault_id, |_| panic!(
            "unavailable must not run any action"
        )),
        Err(HostFault::Unavailable)
    );
    store.failure = Failure::None;
    let next = cached
        .advance(
            &cached.primary,
            &cached.checkpoint,
            next_checkpoint(&cached),
        )
        .unwrap();
    store.publish(&cached, &next).unwrap();
    assert_eq!(store.publish(&cached, &next), Err(HostFault::Conflict));
    assert_eq!(store.publications, 1);
    store.data.snapshot_hash = sha256(b"unverified data");
    assert_eq!(
        update(&mut store, &cached.vault_id, |_| panic!(
            "invalid checkpoint must not reach transition"
        )),
        Err(HostFault::CheckpointMismatch)
    );
}

#[test]
fn raw_authority_rejects_duplicate_decoded_fields_and_oversized_or_nonobject_input() {
    let initial = serde_json::to_string(&fixture("initial")).unwrap();
    let duplicates = [
        initial.replacen(r#""revision":1"#, r#""revision":1,"revi\u0073ion":2"#, 1),
        initial.replacen(
            r#""primary_epoch":1"#,
            r#""primary_epoch":1,"primary_epoch":2"#,
            1,
        ),
        initial.replacen(
            r#""state":"active""#,
            r#""state":"active","state":"active""#,
            1,
        ),
    ];
    for bytes in duplicates {
        assert_ne!(bytes, initial);
        assert!(parse_document::<AuthorityState>(bytes.as_bytes()).is_err());
    }
    let plan = serde_json::to_string(&fixture("plan")).unwrap();
    let duplicate_intent = plan.replacen(
        r#""policy_epoch":0"#,
        r#""policy_epoch":0,"policy_epoch":0"#,
        1,
    );
    assert!(parse_document::<TransferPlan>(duplicate_intent.as_bytes()).is_err());
    let oversized = vec![b' '; MAX_HOST_DOCUMENT_BYTES + 1];
    assert!(
        parse_document::<AuthorityState>(&oversized)
            .unwrap_err()
            .rules()
            .contains(&"host.document_budget")
    );
    assert_eq!(
        parse_document::<AuthorityState>(b"[]"),
        Err(ContractError::Malformed)
    );
    assert!(parse_document::<AuthorityState>(b"{ malformed }").is_err());
}
