#![allow(dead_code)]
//! Synthetic search fixtures: a Vault in a temporary root, and the golden
//! corpus (`tests/fixtures/memory/search-golden.json`) written through the
//! governance path (owner statement, candidate, reviewed plan).

use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::{Clock, FakeClock};
use enouia_memory_contract::ids::{MemoryId, PolicyId, PrincipalId, ProjectId, SubjectId};
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_govern::{
    Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed, confirm, plan, propose,
};
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);
pub const T0: i64 = 1_790_000_000_000;

pub struct Env {
    pub root: PathBuf,
    pub clock: Arc<FakeClock>,
    pub vault: Vault,
    keys: AtomicU64,
    /// Golden label -> memory ID, and project display name -> project ID.
    pub labels: BTreeMap<String, MemoryId>,
    pub projects: BTreeMap<String, ProjectId>,
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

pub fn golden() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/memory/search-golden.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

impl Env {
    pub fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join("enouia-memory-index-tests");
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
        let ids = Arc::new(SequentialIdSource::new(0xa000));
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
        Self {
            root,
            clock,
            vault,
            keys: AtomicU64::new(0),
            labels: BTreeMap::new(),
            projects: BTreeMap::new(),
        }
    }

    pub fn tick(&self) {
        self.clock.set(self.clock.now_unix_ms() + 1_000);
    }

    fn key(&self) -> Vec<u8> {
        format!("key-{}", self.keys.fetch_add(1, Ordering::SeqCst)).into_bytes()
    }

    fn policy(&self) -> PolicyId {
        let pin = self.vault.pin_current().unwrap();
        let entry = self
            .vault
            .record_entries(&pin, RecordKind::Policy)
            .unwrap()
            .remove(0);
        PolicyId::parse(&entry.record_id).unwrap()
    }

    /// Owner statement -> fact candidate -> reviewed plan. Returns the memory.
    pub fn remember(
        &mut self,
        label: &str,
        claim: &str,
        text: &str,
        project: Option<Value>,
    ) -> MemoryId {
        self.remember_with(label, claim, text, project, Map::new())
    }

    /// `remember` with extra approved memory fields (e.g. `valid_from`).
    pub fn remember_with(
        &mut self,
        label: &str,
        claim: &str,
        text: &str,
        project: Option<Value>,
        extra: Map<String, Value>,
    ) -> MemoryId {
        self.tick();
        let source = self
            .vault
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
            .id;
        let mut details = Map::new();
        details.insert("claim_key".into(), json!(claim));
        details.insert(
            "subject_ids".into(),
            json!([SubjectId::parse("sub_00000001-0000-4000-8000-000000000001").unwrap()]),
        );
        details.extend(extra);
        match project {
            Some(Value::String(name)) => {
                details.insert("project_id".into(), json!(self.projects[&name]));
            }
            Some(spec) => {
                details.insert("new_project".into(), spec);
            }
            None => {}
        }
        let proposal = Proposal::create(
            ProposedType::Fact,
            text,
            details,
            vec![EvidenceSpec::content(source, Revision::new(1).unwrap())],
        );
        self.tick();
        let Proposed::Stored(written) =
            propose(&self.vault, &proposal, &Origin::owner(owner()), &self.key()).unwrap()
        else {
            panic!("stored")
        };
        self.tick();
        let shown = plan(
            &self.vault,
            &[Decision::Accept {
                candidate_id: written.id,
                revision: Revision::new(1).unwrap(),
            }],
            &owner(),
            TrustedSurface::TrustedLocalCli,
        )
        .unwrap();
        confirm(
            &self.vault,
            &shown,
            &OwnerConfirmation {
                owner: owner(),
                surface: TrustedSurface::TrustedLocalCli,
            },
        )
        .unwrap();
        let id = shown.ids[0].memory_id.clone();
        if let Some(name) = shown.diff["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["record_kind"] == "project")
            .and_then(|r| r["value"]["display_name"].as_str())
        {
            self.projects
                .insert(name.to_owned(), shown.ids[0].project_id.clone());
        }
        self.labels.insert(label.to_owned(), id.clone());
        id
    }

    /// Write the golden corpus.
    pub fn golden_corpus(&mut self) {
        for memory in golden()["memories"].as_array().unwrap() {
            let project = memory
                .get("new_project")
                .cloned()
                .or_else(|| memory.get("project").cloned());
            self.remember(
                memory["label"].as_str().unwrap(),
                memory["claim"].as_str().unwrap(),
                memory["text"].as_str().unwrap(),
                project,
            );
        }
    }

    pub fn label_of(&self, id: &MemoryId) -> String {
        self.labels
            .iter()
            .find(|(_, v)| *v == id)
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| id.to_string())
    }
}
