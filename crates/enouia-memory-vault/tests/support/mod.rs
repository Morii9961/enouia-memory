#![allow(dead_code)]
//! Test support: isolated temporary data roots under the OS temp directory
//! (never the default data root, never a repository or sync folder), and a
//! replay of the synthetic `lifecycle` record set as real commits.

use enouia_memory_contract::common::{ActorRef, TrustedSurface};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::ids::{CommitId, DeviceId, VaultId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{
    CommitRequest, IdempotencyScope, SequentialIdSource, StagedObject, StagedRecord,
};
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_vault::fault::Faults;
use enouia_memory_vault::{
    GenesisRequest, RootPolicy, Vault, VaultOptions, VerifiedRoot, verify_data_root,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A temporary directory removed on drop. Created only under
/// `<temp>/enouia-memory-vault-tests/`.
pub struct TempRoot {
    path: PathBuf,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn base() -> PathBuf {
    let base = std::env::temp_dir().join("enouia-memory-vault-tests");
    std::fs::create_dir_all(&base).unwrap();
    let canonical = std::fs::canonicalize(&base).unwrap();
    let text = canonical.to_string_lossy().to_string();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

impl TempRoot {
    pub fn new(name: &str) -> Self {
        let unique = format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = base().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// Adopt a directory made by a parent test process (not removed on drop
    /// by the child: the parent owns it).
    pub fn existing(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn child(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        if std::env::var_os("ENOUIA_VAULT_CHILD").is_none() && self.path.starts_with(base()) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub fn verified(path: &Path) -> VerifiedRoot {
    verify_data_root(path, &RootPolicy::default())
        .unwrap_or_else(|e| panic!("temp root rejected: {e:?}"))
}

pub fn fixture(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/memory")
        .join(relative);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn set_key(kind: RecordKind) -> (&'static str, &'static str) {
    match kind {
        RecordKind::Source => ("sources", "source_id"),
        RecordKind::Attachment => ("attachments", "attachment_id"),
        RecordKind::Project => ("projects", "project_id"),
        RecordKind::Memory => ("memories", "memory_id"),
        RecordKind::Candidate => ("candidates", "candidate_id"),
        RecordKind::Review => ("reviews", "review_id"),
        RecordKind::Identity => ("identities", "identity_id"),
        RecordKind::Session => ("sessions", "session_id"),
        RecordKind::SessionEvent => ("session_events", "event_id"),
        RecordKind::Checkpoint => ("checkpoints", "checkpoint_id"),
        RecordKind::Tombstone => ("tombstones", "delete_id"),
        RecordKind::PurgeReceipt => ("purge_receipts", "receipt_id"),
        RecordKind::Approval => ("approvals", "approval_id"),
        RecordKind::Policy => ("policies", "policy_id"),
        RecordKind::Import => ("imports", "import_id"),
        other => panic!("not stored: {other:?}"),
    }
}

/// The synthetic lifecycle set: 19 commits from genesis to a logical delete.
pub struct Lifecycle {
    pub set: Value,
    pub commits: Vec<Value>,
    pub objects: Value,
}

impl Lifecycle {
    pub fn load() -> Self {
        let set = fixture("sets/lifecycle.json");
        let mut commits = set["commits"].as_array().unwrap().clone();
        commits.sort_by_key(|c| c["sequence"].as_u64().unwrap());
        let objects = fixture("sets/lifecycle-objects.json")["objects"].clone();
        Self {
            set,
            commits,
            objects,
        }
    }

    pub fn record(&self, kind: RecordKind, id: &str, revision: u64) -> Value {
        let (key, field) = set_key(kind);
        self.set[key]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r[field] == id
                    && (!kind.is_revisioned() || r["revision"].as_u64() == Some(revision))
            })
            .unwrap_or_else(|| panic!("{kind:?} {id} r{revision} not in fixture"))
            .clone()
    }

    pub fn staged(&self, index: usize) -> Vec<StagedRecord> {
        self.commits[index]["catalog"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["changed"] == true)
            .map(|e| {
                let kind: RecordKind = serde_json::from_value(e["record_kind"].clone()).unwrap();
                let id = e["record_id"].as_str().unwrap();
                let revision = e["revision"].as_u64().unwrap();
                StagedRecord {
                    record_kind: kind,
                    record_id: id.to_owned(),
                    revision: Revision::new(revision).unwrap(),
                    bytes: canonical_bytes(&self.record(kind, id, revision)).unwrap(),
                }
            })
            .collect()
    }

    /// Objects first listed by commit `index` (manifests list all objects).
    pub fn new_objects(&self, index: usize) -> Vec<StagedObject> {
        let listed =
            |i: usize| -> Vec<Value> { self.commits[i]["objects"].as_array().unwrap().clone() };
        let before = if index == 0 {
            Vec::new()
        } else {
            listed(index - 1)
        };
        listed(index)
            .into_iter()
            .filter(|o| !before.contains(o))
            .map(|o| {
                let hash = o["object_hash"].as_str().unwrap();
                let text = self.objects[hash]["text"].as_str().unwrap();
                StagedObject {
                    kind: serde_json::from_value(o["object_kind"].clone()).unwrap(),
                    hash: Sha256Hex::parse(hash).unwrap(),
                    bytes: text.as_bytes().to_vec(),
                }
            })
            .collect()
    }

    pub fn time(&self, index: usize) -> i64 {
        Timestamp::parse(self.commits[index]["created_at"].as_str().unwrap())
            .unwrap()
            .unix_ms()
    }

    fn hash(value: &Value) -> Sha256Hex {
        Sha256Hex::parse(value.as_str().unwrap()).unwrap()
    }

    pub fn owner(&self) -> ActorRef {
        serde_json::from_value(self.commits[0]["principal"].clone()).unwrap()
    }

    pub fn genesis(&self) -> GenesisRequest {
        let c = &self.commits[0];
        GenesisRequest {
            vault_id: VaultId::parse(c["vault_id"].as_str().unwrap()).unwrap(),
            commit_id: CommitId::parse(c["commit_id"].as_str().unwrap()).unwrap(),
            device_id: DeviceId::parse(c["writer_device_id"].as_str().unwrap()).unwrap(),
            owner: self.owner(),
            trusted_surface: TrustedSurface::TrustedLocalCli,
            request_payload_hash: Self::hash(&c["request_payload_hash"]),
            records: self.staged(0),
        }
    }

    /// Commit `index` (1-based sequence minus one) as a request.
    pub fn request(&self, index: usize) -> CommitRequest {
        let c = &self.commits[index];
        let principal: ActorRef = serde_json::from_value(c["principal"].clone()).unwrap();
        let operation_kind = serde_json::from_value(c["operation_kind"].clone()).unwrap();
        CommitRequest {
            commit_id: CommitId::parse(c["commit_id"].as_str().unwrap()).unwrap(),
            expected_commit_id: Some(
                CommitId::parse(c["parent_commit_id"].as_str().unwrap()).unwrap(),
            ),
            idempotency: IdempotencyScope {
                principal_id: principal.actor_id.clone(),
                operation_kind,
                key_hash: Self::hash(&c["idempotency_key_hash"]),
            },
            principal,
            operation_kind,
            request_payload_hash: Self::hash(&c["request_payload_hash"]),
            expected_revisions: Vec::new(),
            records: self.staged(index),
            objects: self.new_objects(index),
        }
    }
}

pub struct Harness {
    pub root: TempRoot,
    pub clock: Arc<FakeClock>,
    pub vault: Vault,
    pub lifecycle: Lifecycle,
}

pub fn options(faults: Faults) -> VaultOptions {
    VaultOptions {
        lock_wait: std::time::Duration::from_millis(300),
        validate_record_set: true,
        faults,
    }
}

pub fn ids() -> Arc<SequentialIdSource> {
    Arc::new(SequentialIdSource::new(0x5000))
}

impl Harness {
    /// A new Vault with lifecycle commits `0..=through` applied.
    pub fn with_commits(name: &str, through: usize) -> Self {
        let root = TempRoot::new(name);
        let lifecycle = Lifecycle::load();
        let clock = Arc::new(FakeClock::new(lifecycle.time(0)));
        let vault = Vault::create(
            &verified(root.path()),
            lifecycle.genesis(),
            clock.clone(),
            ids(),
            options(Faults::none()),
        )
        .expect("genesis");
        let harness = Self {
            root,
            clock,
            vault,
            lifecycle,
        };
        for index in 1..=through {
            harness.apply(index);
        }
        harness
    }

    pub fn apply(&self, index: usize) {
        self.clock.set(self.lifecycle.time(index));
        self.vault
            .commit(self.lifecycle.request(index))
            .unwrap_or_else(|e| panic!("commit {index}: {e}"));
    }

    /// A second handle on the same root (as another process would open it).
    pub fn reopen(&self, faults: Faults) -> Vault {
        Vault::open(
            &verified(self.root.path()),
            None,
            self.clock.clone(),
            ids(),
            options(faults),
        )
        .expect("open")
    }
}

/// Child-process helpers: a test binary re-runs one of its own tests with
/// `ENOUIA_VAULT_CHILD` set, so the child is a separate OS process.
pub fn child_env(name: &str) -> Option<String> {
    std::env::var("ENOUIA_VAULT_CHILD").ok()?;
    std::env::var(name).ok()
}

pub fn spawn_child(test: &str, envs: &[(&str, String)]) -> std::process::Child {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env("ENOUIA_VAULT_CHILD", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.spawn().expect("spawn child test process")
}

pub fn wait_for(path: &Path, seconds: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {path:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Open an existing Vault from a child process at lifecycle time `index`.
pub fn open_at(path: &Path, index: usize, faults: Faults, lock_wait_ms: u64) -> (Vault, Lifecycle) {
    let lifecycle = Lifecycle::load();
    let clock = Arc::new(FakeClock::new(lifecycle.time(index)));
    let mut opts = options(faults);
    opts.lock_wait = std::time::Duration::from_millis(lock_wait_ms);
    let vault = Vault::open(&verified(path), None, clock, ids(), opts).expect("open");
    (vault, lifecycle)
}
