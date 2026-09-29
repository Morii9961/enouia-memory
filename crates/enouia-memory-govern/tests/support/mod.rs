#![allow(dead_code)]
//! Synthetic governance fixtures: a Vault in a temporary root, the owner and
//! an agent, owner statements, and a synthetic ChatGPT-shaped export with a
//! user message and an assistant reply. Nothing real is read or written.

use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::{Clock, FakeClock};
use enouia_memory_contract::ids::{PolicyId, PrincipalId, SourceId, SubjectId};
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::ports::{CommitPin, SequentialIdSource};
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::source::{ConfirmationMethod, SourceRecord};
use enouia_memory_govern::{EvidenceSpec, Origin, Proposal, Proposed, propose};
use enouia_memory_import::pipeline::NeverCancel;
use enouia_memory_import::{ImportOptions, import_file};
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub const T0: i64 = 1_790_000_000_000;
pub const POLICY: &str = "pol_00000000-0000-4000-8000-000000000000";
pub const SUBJECT: &str = "sub_00000001-0000-4000-8000-000000000001";

pub struct Env {
    pub root: PathBuf,
    pub clock: Arc<FakeClock>,
    pub vault: Vault,
    keys: AtomicU64,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn owner() -> ActorRef {
    ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    }
}

pub fn agent() -> ActorRef {
    ActorRef {
        actor_id: PrincipalId::parse("prn_00000002-0000-4000-8000-000000000002").unwrap(),
        actor_type: ActorType::Agent,
    }
}

impl Env {
    pub fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join("enouia-memory-govern-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let root = base.join(format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("vault")).unwrap();
        let verified = verify_data_root(&root.join("vault"), &RootPolicy::default()).unwrap();
        let clock = Arc::new(FakeClock::new(T0));
        let ids = Arc::new(SequentialIdSource::new(0x9000));
        let now = enouia_memory_contract::time::Timestamp::from_unix_ms(T0).unwrap();
        let genesis =
            new_genesis(ids.as_ref(), owner(), TrustedSurface::TrustedLocalCli, &now).unwrap();
        let vault = Vault::create(
            &verified,
            genesis,
            clock.clone(),
            ids,
            VaultOptions::default(),
        )
        .unwrap();
        let env = Self {
            root,
            clock,
            vault,
            keys: AtomicU64::new(0),
        };
        assert_eq!(env.policy().as_str().len(), POLICY.len());
        env
    }

    pub fn policy(&self) -> PolicyId {
        let pin = self.vault.pin_current().unwrap();
        let entry = self
            .vault
            .record_entries(&pin, RecordKind::Policy)
            .unwrap()
            .remove(0);
        PolicyId::parse(&entry.record_id).unwrap()
    }

    pub fn tick(&self) {
        self.clock.set(self.clock.now_unix_ms() + 1_000);
    }

    pub fn advance(&self, ms: i64) {
        self.clock.set(self.clock.now_unix_ms() + ms);
    }

    pub fn key(&self) -> Vec<u8> {
        format!("key-{}", self.keys.fetch_add(1, Ordering::SeqCst)).into_bytes()
    }

    pub fn pin(&self) -> CommitPin {
        self.vault.pin_current().unwrap()
    }

    /// An owner statement recorded as a manual-assertion source.
    pub fn statement(&self, text: &str) -> SourceId {
        self.tick();
        self.vault
            .record_manual_assertion(
                &ManualAssertionInput {
                    text: text.to_owned(),
                    operator: owner(),
                    trusted_surface: TrustedSurface::TrustedLocalCli,
                    confirmation: ConfirmationMethod::TypedConfirmation,
                    sensitivity: Sensitivity::Private,
                    access_policy_id: self.policy(),
                    time_precision: TimePrecision::Millisecond,
                },
                &self.key(),
            )
            .unwrap()
            .id
    }

    /// Import a synthetic conversation; returns its sources in message order.
    pub fn import_chat(&self, messages: &[(&str, &str)]) -> Vec<SourceRecord> {
        self.tick();
        let mut mapping = Map::new();
        mapping.insert(
            "root".into(),
            json!({"id": "root", "message": null, "parent": null, "children": []}),
        );
        let mut parent = "root".to_owned();
        for (index, (role, text)) in messages.iter().enumerate() {
            let id = format!("m{index}");
            mapping.insert(
                id.clone(),
                json!({"id": id, "parent": parent, "children": [], "message": {
                    "id": id, "author": {"role": role},
                    "create_time": 1_780_000_000.0 + index as f64,
                    "content": {"content_type": "text", "parts": [text]}}}),
            );
            parent = id;
        }
        let export = json!([{"conversation_id": format!("conv-{}", self.keys.load(Ordering::SeqCst)),
            "mapping": mapping, "current_node": parent}]);
        let file = self.root.join(format!(
            "chat-{}.json",
            self.keys.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&file, serde_json::to_vec(&export).unwrap()).unwrap();
        let options = ImportOptions::new(owner(), "acct-main", self.policy());
        let report = import_file(&self.vault, &file, &options, &NeverCancel).unwrap();
        let pin = self.pin();
        let mut sources: Vec<SourceRecord> = self
            .vault
            .record_entries(&pin, RecordKind::Source)
            .unwrap()
            .into_iter()
            .map(|e| {
                let bytes = self
                    .vault
                    .read_record(
                        &pin,
                        &RecordRef::new(RecordKind::Source, &e.record_id, e.revision),
                    )
                    .unwrap();
                enouia_memory_contract::parse_record::<SourceRecord>(&bytes).unwrap()
            })
            .filter(|s| s.import_id.as_ref() == Some(&report.manifest.import_id))
            .collect();
        sources.sort_by(|a, b| a.original_message_id.cmp(&b.original_message_id));
        sources
    }

    /// Propose a fact citing `source` and return the candidate ID.
    pub fn propose_fact(
        &self,
        origin: &Origin,
        source: &SourceId,
        claim: &str,
        text: &str,
    ) -> enouia_memory_contract::ids::CandidateId {
        self.tick();
        match propose(&self.vault, &fact(source, claim, text), origin, &self.key()).unwrap() {
            Proposed::Stored(written) => written.id,
            other => panic!("{other:?}"),
        }
    }
}

pub fn rev(n: u64) -> Revision {
    Revision::new(n).unwrap()
}

pub fn fact_details(claim: &str) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("claim_key".into(), json!(claim));
    details.insert(
        "subject_ids".into(),
        json!([SubjectId::parse(SUBJECT).unwrap()]),
    );
    details.insert("volatility".into(), json!("changing"));
    details
}

pub fn fact(source: &SourceId, claim: &str, text: &str) -> Proposal {
    Proposal::create(
        ProposedType::Fact,
        text,
        fact_details(claim),
        vec![EvidenceSpec::content(source.clone(), rev(1))],
    )
}

const STORED: &[RecordKind] = &[
    RecordKind::Source,
    RecordKind::Attachment,
    RecordKind::Project,
    RecordKind::Memory,
    RecordKind::Candidate,
    RecordKind::Review,
    RecordKind::Identity,
    RecordKind::Session,
    RecordKind::SessionEvent,
    RecordKind::Checkpoint,
    RecordKind::Tombstone,
    RecordKind::PurgeReceipt,
    RecordKind::Approval,
    RecordKind::Policy,
    RecordKind::Import,
];

impl Env {
    /// Every revision and every commit of the head, checked by the whole-set
    /// rules (not the scoped ones the store used).
    pub fn assert_history_valid(&self) {
        let pin = self.pin();
        let mut set = enouia_memory_contract::set::RecordSet::default();
        for kind in STORED {
            for entry in self.vault.record_entries(&pin, *kind).unwrap() {
                for r in 1..=entry.revision.get() {
                    let bytes = self
                        .vault
                        .read_revision(&pin, &RecordRef::new(*kind, &entry.record_id, rev(r)))
                        .unwrap();
                    let value = serde_json::from_slice(&bytes).unwrap();
                    set.push(enouia_memory_contract::parse_any(*kind, &value).unwrap());
                }
            }
        }
        let mut next = Some(pin.clone());
        while let Some(at) = next {
            let manifest = self.vault.read_manifest(&at).unwrap();
            next = manifest.parent_commit_id.clone().map(|id| CommitPin {
                commit_id: id,
                sequence: at.sequence - 1,
                policy_epoch: 0,
                deletion_epoch: 0,
            });
            set.commits.push(manifest);
        }
        let violations = enouia_memory_contract::set::validate_set(&set);
        assert!(violations.is_empty(), "{violations:#?}");
    }
}
