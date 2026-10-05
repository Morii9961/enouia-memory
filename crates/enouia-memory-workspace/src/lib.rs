//! The embedded Core of the Windows Memory Workspace (MV-6, ADR-MEM-44).
//!
//! A host shell forwards every page request to [`Workspace::call`] and
//! nothing else, after checking the caller's [`HostSurface`]: Memory's
//! reference shell in `apps/workspace`, or Runtime's Windows client through
//! its own adapter (ADR-MEM-45). This crate owns the rules. It opens only an
//! explicitly chosen data root, turns native-dialog choices into single-use
//! tokens, keeps review plans until the owner confirms the exact diff, and
//! runs long work on worker threads that the page observes by operation ID.
//!
//! The owner is the principal that created the Vault and the surface is
//! `trusted_windows_app`, as the CLI is `trusted_local_cli`. MV-8's Host
//! will authenticate the caller; until then the process boundary is the
//! trust boundary.

pub mod ops;
pub mod views;

/// The caller scope every host checks before forwarding (ADR-MEM-45).
pub use enouia_memory_contract::workspace::HostSurface;

use enouia_memory_context::{CompileInput, answer_saved, compile, session};
use enouia_memory_contract::candidate::{CandidateRecord, ProposalKind, ProposedType};
use enouia_memory_contract::commit::{DeleteMode, DeleteScope};
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::context::{ContextCapsule, ContextInspection, DispatchRecord};
use enouia_memory_contract::error::{ContractError, MemoryErrorCode};
use enouia_memory_contract::foundation::{Clock, ComponentId};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{
    CandidateId, CapsuleId, DispatchId, MemoryId, OperationId, PolicyId, PrincipalId, RequestId,
    SubjectId,
};
use enouia_memory_contract::import::ImportManifest;
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::memory::CanonicalMemory;
use enouia_memory_contract::ports::{CommitPin, IdSource};
use enouia_memory_contract::record::{Record, RecordKind, RecordRef};
use enouia_memory_contract::session::{ClientSurface, SessionRecord};
use enouia_memory_contract::source::{ConfirmationMethod, SourceRecord};
use enouia_memory_contract::workspace::{
    self as wire, Command, DecisionAction, ForgetMode, MergeKind, Response, WorkspaceError,
};
use enouia_memory_govern::delete::{complete_purge, delete_proposal, purge_preview};
use enouia_memory_govern::review::PLAN_TTL_MS;
use enouia_memory_govern::{
    Canonical, Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed, ReviewPlan,
    canonical_memories, confirm, pending_candidates, plan, propose,
};
use enouia_memory_index::{Index, SearchRequest, search};
use enouia_memory_vault::backup::{export_pinned, network_allowed, verify_export};
use enouia_memory_vault::error::Fault;
use enouia_memory_vault::health::HealthState;
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, RootRejection, SystemClock, Vault, VaultError, VaultOptions,
    verify_data_root,
};
use ops::{Operations, Ticket};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, TryLockError};

/// How long a picker token stays valid.
pub const PICK_TTL_MS: i64 = 10 * 60 * 1000;
/// Reply budget for the local Mock.
const OUTPUT_TOKENS: u64 = 4096;
const SURFACE: TrustedSurface = TrustedSurface::TrustedWindowsApp;

/// A failure on its way to the page: code, retryability, rule identifiers.
#[derive(Debug)]
pub struct Fail(pub WorkspaceError);

type R<T> = Result<T, Fail>;

fn rule_ok(rule: &str) -> bool {
    !rule.is_empty()
        && rule.len() <= 64
        && rule
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.'))
}

fn fault_rules(fault: &Fault) -> Vec<String> {
    let rules: Vec<String> = match fault {
        Fault::Contract(rules) => rules.iter().map(|r| (*r).to_owned()).collect(),
        Fault::UnsafePath(r) | Fault::Corrupt(r) | Fault::Recovering(r) | Fault::Injected(r) => {
            vec![format!("fault.{r}")]
        }
        Fault::Io(_) => vec!["fault.io".into()],
        other => {
            let name = format!("{other:?}");
            let mut snake = String::from("fault.");
            for (i, c) in name.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    snake.push('_');
                }
                snake.push(c.to_ascii_lowercase());
            }
            vec![snake]
        }
    };
    rules.into_iter().filter(|r| rule_ok(r)).take(16).collect()
}

impl From<VaultError> for Fail {
    fn from(error: VaultError) -> Self {
        Self(WorkspaceError::from_memory(
            &error.error,
            fault_rules(&error.fault),
        ))
    }
}

impl From<ContractError> for Fail {
    fn from(error: ContractError) -> Self {
        Self(WorkspaceError::from_contract(&error))
    }
}

/// One embedded Core per Vault (ADR-MEM-46): hold `indexes/host.lock`
/// exclusively while the Vault is open. The OS releases it when the handle
/// drops or the process ends, so a crash never leaves it held.
fn host_lock(vault: &Vault) -> R<std::fs::File> {
    let file = vault
        .managed_root()
        .open_lock_file(&enouia_memory_contract::layout::host_lock_file())?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            // Another process has this Vault open; retrying now cannot help.
            let mut error = WorkspaceError::new(MemoryErrorCode::Busy, &["workspace.vault_in_use"]);
            error.retryable = false;
            Err(Fail(error))
        }
        Err(std::fs::TryLockError::Error(_)) => {
            Err(fail(MemoryErrorCode::StorageFailed, "workspace.host_lock"))
        }
    }
}

/// A rejected data root names its reason as a rule, never the path.
fn root_rejected(reason: RootRejection) -> Fail {
    let rule = match reason {
        RootRejection::NotAbsolute => "root.not_absolute",
        RootRejection::NetworkPath => "root.network_path",
        RootRejection::Missing => "root.missing",
        RootRejection::NotADirectory => "root.not_a_directory",
        RootRejection::NotCanonical => "root.not_canonical",
        RootRejection::ReparsePoint => "root.reparse_point",
        RootRejection::NotLocalFixedDisk => "root.not_local_fixed_disk",
        RootRejection::UnsupportedFilesystem => "root.unsupported_filesystem",
        RootRejection::CloudSyncFolder => "root.cloud_sync_folder",
        RootRejection::InsideRepository => "root.inside_repository",
        RootRejection::SystemLocation => "root.system_location",
        RootRejection::InsufficientSpace => "root.insufficient_space",
        RootRejection::Unreadable => "root.unreadable",
    };
    Fail(WorkspaceError::new(
        MemoryErrorCode::InvalidRequest,
        &["workspace.root_rejected", rule],
    ))
}

fn fail(code: MemoryErrorCode, rule: &str) -> Fail {
    Fail(WorkspaceError::new(code, &[rule]))
}

/// What the Core is built with. Tests pass a fake clock and ID source.
#[derive(Clone)]
pub struct Config {
    pub clock: Arc<dyn Clock + Send + Sync>,
    pub ids: Arc<dyn IdSource + Send + Sync>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            ids: Arc::new(OsIdSource),
        }
    }
}

/// What a native dialog chose. Only the shell registers picks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickKind {
    ImportFile,
    VaultRoot,
    BackupDestination,
    ExportFolder,
}

/// What the page learns about a pick: the token, a base name, a size.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub token: String,
    pub display_name: String,
    pub bytes: Option<u64>,
}

struct Pick {
    kind: PickKind,
    path: PathBuf,
    expires_ms: i64,
    in_use: bool,
}

/// Release admission on errors or unwind, consume only on successful admission.
/// The picker map is never locked while a command performs IO.
struct PickLease<'a> {
    picks: &'a Mutex<BTreeMap<String, Pick>>,
    token: &'a str,
    consume: bool,
}

impl Drop for PickLease<'_> {
    fn drop(&mut self) {
        let mut picks = self.picks.lock().unwrap_or_else(|e| e.into_inner());
        if self.consume {
            picks.remove(self.token);
        } else if let Some(pick) = picks.get_mut(self.token) {
            pick.in_use = false;
        }
    }
}

struct Open {
    vault: Arc<Vault>,
    index: Arc<Mutex<Option<Index>>>,
    root: PathBuf,
    owner: ActorRef,
}

enum Slot {
    Empty,
    Open(Arc<Open>),
    Locked(PathBuf),
}

struct PendingPlan {
    plan: ReviewPlan,
    purge: bool,
}

pub struct Workspace {
    config: Config,
    /// Lifecycle changes exclude complete page calls and native picks. Status
    /// and operation controls bypass this gate so close can still be observed.
    lifecycle: RwLock<()>,
    slot: Mutex<Slot>,
    ops: Operations,
    picks: Mutex<BTreeMap<String, Pick>>,
    plans: Mutex<BTreeMap<String, PendingPlan>>,
    /// Results and exact diff hashes of confirmed plans in this open Vault.
    confirmed: Mutex<BTreeMap<String, (enouia_memory_contract::hash::Sha256Hex, Value)>>,
    /// Started imports by idempotency key: (request fingerprint, operation).
    started: Mutex<BTreeMap<String, (String, OperationId)>>,
    /// The exclusive host lock of the open Vault (ADR-MEM-46). It belongs to
    /// the open/close lifecycle, not to `Open`'s reference count, so a read
    /// still in flight cannot keep it held after close.
    host: Mutex<Option<std::fs::File>>,
    /// The last backup export since this Vault was opened (not persisted).
    last_backup: Mutex<Option<OperationId>>,
    /// Companion status reported by the shell (tray, hotkey, overlay).
    companion: Mutex<Value>,
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn one() -> Revision {
    Revision::new(1).expect("one")
}

/// A deterministic request ID for an idempotency key, so a retried page
/// request reaches the same saved input, capsule, and reply.
fn request_id_for(key: &str, purpose: &str) -> RequestId {
    let digest = sha256(format!("{purpose}\n{key}").as_bytes());
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest.as_str()[i * 2..i * 2 + 2], 16).expect("hex");
    }
    RequestId::from_random(bytes)
}

fn parse<T: Record>(bytes: &[u8]) -> R<T> {
    enouia_memory_contract::parse_record(bytes).map_err(|_| {
        fail(
            MemoryErrorCode::StorageFailed,
            "workspace.record_unreadable",
        )
    })
}

fn latest_records<T: Record>(vault: &Vault, pin: &CommitPin, kind: RecordKind) -> R<Vec<T>> {
    let mut out = Vec::new();
    for entry in vault.record_entries(pin, kind)? {
        let bytes =
            vault.read_record(pin, &RecordRef::new(kind, &entry.record_id, entry.revision))?;
        out.push(parse(&bytes)?);
    }
    Ok(out)
}

/// Latest revisions as plain JSON (records shown, not interpreted).
fn latest_values(vault: &Vault, pin: &CommitPin, kind: RecordKind) -> R<Vec<Value>> {
    let mut out = Vec::new();
    for entry in vault.record_entries(pin, kind)? {
        let bytes =
            vault.read_record(pin, &RecordRef::new(kind, &entry.record_id, entry.revision))?;
        out.push(serde_json::from_slice(&bytes).map_err(|_| {
            fail(
                MemoryErrorCode::StorageFailed,
                "workspace.record_unreadable",
            )
        })?);
    }
    Ok(out)
}

fn page<T>(
    items: Vec<T>,
    cursor: Option<&str>,
    limit: Option<u32>,
    pin: &CommitPin,
) -> R<(Vec<T>, Option<String>)> {
    let offset = match cursor {
        None => 0,
        Some(c) => {
            let (commit, offset) = c
                .split_once(':')
                .ok_or_else(|| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?;
            if commit != pin.commit_id.as_str() {
                return Err(fail(
                    MemoryErrorCode::RevisionConflict,
                    "workspace.cursor_stale",
                ));
            }
            offset
                .parse::<usize>()
                .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?
        }
    };
    let limit = limit.unwrap_or(wire::LIST_DEFAULT_LIMIT) as usize;
    let total = items.len();
    let rows: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let next =
        (offset + rows.len() < total).then(|| format!("{}:{}", pin.commit_id, offset + rows.len()));
    Ok((rows, next))
}

fn health_word(state: HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::Unavailable => "unavailable",
        HealthState::Recovering => "recovering",
    }
}

fn import_row(m: &ImportManifest) -> Value {
    json!({
        "importId": m.import_id, "revision": m.revision, "status": m.status,
        "inputKind": m.input_kind, "provider": m.provider, "accountScope": m.account_scope,
        "inputSizeBytes": m.input_size_bytes, "duplicateOf": m.duplicate_of,
        "receivedAt": m.received_at, "updatedAt": m.updated_at, "counts": m.counts,
        "conversationsCovered": m.coverage.len(),
        "membersQuarantined": m.members.iter().filter(|x| x.disposition == enouia_memory_contract::import::MemberDisposition::Quarantined).count(),
        "warnings": m.warnings.iter().map(|w| w.code.clone()).collect::<Vec<_>>(),
    })
}

impl Workspace {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            lifecycle: RwLock::new(()),
            slot: Mutex::new(Slot::Empty),
            ops: Operations::default(),
            picks: Mutex::new(BTreeMap::new()),
            plans: Mutex::new(BTreeMap::new()),
            confirmed: Mutex::new(BTreeMap::new()),
            started: Mutex::new(BTreeMap::new()),
            host: Mutex::new(None),
            last_backup: Mutex::new(None),
            companion: Mutex::new(
                json!({"tray": "absent", "hotkey": {"state": "absent"}, "overlay": "absent"}),
            ),
        }
    }

    pub fn operations(&self) -> &Operations {
        &self.ops
    }

    pub fn set_companion(&self, value: Value) {
        *self.companion.lock().expect("companion") = value;
    }

    fn now_ms(&self) -> i64 {
        self.config.clock.now_unix_ms()
    }

    /// Register a native-dialog choice. Only the shell calls this; the page
    /// receives the token, the base name, and the size.
    pub fn register_pick(&self, kind: PickKind, path: &Path) -> Result<Picked, WorkspaceError> {
        self.with_lifecycle(false, || self.register_pick_at(kind, path).map_err(Fail))
            .map_err(|f| f.0)
    }

    fn register_pick_at(&self, kind: PickKind, path: &Path) -> Result<Picked, WorkspaceError> {
        let meta = std::fs::symlink_metadata(path).map_err(|_| {
            WorkspaceError::new(MemoryErrorCode::NotFound, &["workspace.pick_missing"])
        })?;
        let regular = meta.is_file() && !meta.file_type().is_symlink();
        let directory = meta.is_dir() && !meta.file_type().is_symlink();
        let ok = match kind {
            PickKind::ImportFile => regular,
            _ => directory,
        };
        if !ok {
            return Err(WorkspaceError::new(
                MemoryErrorCode::InvalidRequest,
                &["workspace.pick_kind"],
            ));
        }
        let token = format!("tok_{}", hex(&self.config.ids.random_16()));
        let mut picks = self.picks.lock().expect("picks");
        let now = self.now_ms();
        picks.retain(|_, p| p.in_use || p.expires_ms > now);
        picks.insert(
            token.clone(),
            Pick {
                kind,
                path: path.to_path_buf(),
                expires_ms: now + PICK_TTL_MS,
                in_use: false,
            },
        );
        Ok(Picked {
            token,
            display_name: base_name(path),
            bytes: regular.then_some(meta.len()),
        })
    }

    /// Reserve a picker token through command admission. Failure (including
    /// unwind) allows retry until expiry; a successful preview is reusable.
    /// Async starts consume on scheduling success, not on worker completion.
    /// Callers hold the lifecycle gate through this helper.
    fn with_pick<T>(
        &self,
        token: &str,
        kind: PickKind,
        consume_on_success: bool,
        work: impl FnOnce(PathBuf) -> R<T>,
    ) -> R<T> {
        let path = {
            let mut picks = self.picks.lock().expect("picks");
            let now = self.now_ms();
            let pick = picks
                .get_mut(token)
                .filter(|p| p.expires_ms > now)
                .ok_or_else(|| fail(MemoryErrorCode::NotFound, "workspace.token_unknown"))?;
            if pick.kind != kind {
                return Err(fail(
                    MemoryErrorCode::InvalidRequest,
                    "workspace.token_kind",
                ));
            }
            if pick.in_use {
                return Err(fail(MemoryErrorCode::Busy, "workspace.token_busy"));
            }
            pick.in_use = true;
            pick.path.clone()
        };
        let mut lease = PickLease {
            picks: &self.picks,
            token,
            consume: false,
        };
        let result = work(path);
        lease.consume = result.is_ok() && consume_on_success;
        result
    }

    /// Open a Vault at an explicit root for the shell's `--vault` argument.
    pub fn open_root(&self, root: &Path) -> Result<(), WorkspaceError> {
        self.with_lifecycle(true, || self.open_at(root))
            .map_err(|f| f.0)
    }

    fn open_at(&self, root: &Path) -> R<()> {
        self.close(None);
        let verified = verify_data_root(root, &RootPolicy::default()).map_err(root_rejected)?;
        let vault = Vault::open(
            &verified,
            None,
            self.config.clock.clone(),
            self.config.ids.clone(),
            VaultOptions::default(),
        )?;
        let host = host_lock(&vault)?;
        let owner = vault.descriptor().created_by.clone();
        *self.host.lock().expect("host") = Some(host);
        *self.slot.lock().expect("slot") = Slot::Open(Arc::new(Open {
            vault: Arc::new(vault),
            index: Arc::new(Mutex::new(None)),
            root: root.to_path_buf(),
            owner,
        }));
        Ok(())
    }

    fn create_at(&self, root: &Path) -> R<()> {
        self.close(None);
        let verified = verify_data_root(root, &RootPolicy::default()).map_err(root_rejected)?;
        let owner = ActorRef {
            actor_id: PrincipalId::from_random(self.config.ids.random_16()),
            actor_type: ActorType::Owner,
        };
        let now = enouia_memory_contract::ports::commit_time(
            self.config.clock.as_ref(),
            None,
            ComponentId::Vault,
        )
        .map_err(|e| Fail(WorkspaceError::from_memory(&e, vec![])))?;
        let genesis = new_genesis(self.config.ids.as_ref(), owner, SURFACE, &now)?;
        Vault::create(
            &verified,
            genesis,
            self.config.clock.clone(),
            self.config.ids.clone(),
            VaultOptions::default(),
        )?;
        self.open_at(root)
    }

    /// Cancel and join every operation, then forget the open Vault's page
    /// state. Root picker tokens remain usable for an open/create retry.
    /// `then` is the slot left behind (`Locked` or `Empty`).
    fn close(&self, then: Option<Slot>) {
        // Detach the Vault first, so no new command can start work on it
        // while running operations are cancelled and joined.
        *self.slot.lock().expect("slot") = then.unwrap_or(Slot::Empty);
        self.ops.close();
        self.plans.lock().expect("plans").clear();
        self.confirmed.lock().expect("confirmed").clear();
        self.started.lock().expect("started").clear();
        *self.last_backup.lock().expect("backup") = None;
        self.picks
            .lock()
            .expect("picks")
            .retain(|_, pick| pick.kind == PickKind::VaultRoot);
        if let Some(host) = self.host.lock().expect("host").take() {
            let _ = host.unlock();
        }
    }

    /// Exit path for the shell: stop accepting work, cancel and join
    /// operations, release every handle.
    pub fn shutdown(&self) {
        // Shutdown still attempts cleanup if a lifecycle writer panicked.
        let _guard = self.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        self.close(None);
    }

    fn with_lifecycle<T>(&self, exclusive: bool, work: impl FnOnce() -> R<T>) -> R<T> {
        let unavailable = || {
            let mut error = WorkspaceError::new(
                MemoryErrorCode::StorageFailed,
                &["workspace.lifecycle_failed"],
            );
            error.retryable = false;
            Fail(error)
        };
        if exclusive {
            let _guard = self.lifecycle.write().map_err(|_| unavailable())?;
            work()
        } else {
            let _guard = self.lifecycle.read().map_err(|_| unavailable())?;
            work()
        }
    }

    fn open(&self) -> R<Arc<Open>> {
        match &*self.slot.lock().expect("slot") {
            Slot::Open(open) => Ok(open.clone()),
            Slot::Locked(_) => Err(fail(MemoryErrorCode::VaultLocked, "workspace.locked")),
            Slot::Empty => Err(fail(MemoryErrorCode::VaultLocked, "workspace.no_vault")),
        }
    }

    /// Bring the index to the Vault's head and use it. A busy index (a
    /// rebuild is running) answers `index_not_ready` at once; it never waits.
    fn with_index<T>(&self, open: &Open, f: impl FnOnce(&Index) -> R<T>) -> R<T> {
        let mut guard = match open.index.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                return Err(fail(MemoryErrorCode::IndexNotReady, "index.busy"));
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                // A panic while the index was held leaves it in an unknown
                // state. The index is disposable: reopen it from the Vault.
                let mut guard = poisoned.into_inner();
                *guard = None;
                open.index.clear_poison();
                guard
            }
        };
        if guard.is_none() {
            *guard = Some(Index::open(&open.vault)?);
        }
        let index = guard.as_mut().expect("opened");
        index.update(&open.vault, &|| false)?;
        f(index)
    }

    /// Handle one page request. Never panics on input; every failure is an
    /// error envelope without paths or record text.
    pub fn call(&self, request: &Value) -> Value {
        let request_id = request
            .get("requestId")
            .and_then(Value::as_str)
            .and_then(|r| RequestId::parse(r).ok())
            .unwrap_or_else(|| RequestId::from_random(self.config.ids.random_16()));
        let response = match wire::parse_request(request) {
            Err(error) => Response::failed(request_id, WorkspaceError::from_contract(&error)),
            Ok((parsed, command)) => {
                let key = parsed.idempotency_key.clone();
                let handle = || {
                    Ok(match self.route(&command, key.as_deref()) {
                        Ok(done) => {
                            let mut response = Response::ok(
                                parsed.request_id,
                                command.success_kind(),
                                done.result,
                            );
                            response.operation_id = done.operation_id;
                            response.vault_commit_id = self
                                .open()
                                .ok()
                                .and_then(|o| o.vault.pin_current().ok())
                                .map(|p| p.commit_id);
                            response
                        }
                        Err(Fail(error)) => Response::failed(parsed.request_id, error),
                    })
                };
                let handled = if matches!(
                    command,
                    Command::WorkspaceStatus(_)
                        | Command::OperationGet(_)
                        | Command::OperationList(_)
                        | Command::OperationCancel(_)
                ) {
                    handle()
                } else {
                    let exclusive = matches!(
                        command,
                        Command::VaultOpen(_)
                            | Command::VaultCreate(_)
                            | Command::VaultLock(_)
                            | Command::VaultUnlock(_)
                    );
                    self.with_lifecycle(exclusive, handle)
                };
                handled.unwrap_or_else(|Fail(error)| Response::failed(request_id, error))
            }
        };
        serde_json::to_value(response).expect("responses serialize")
    }

    fn route(&self, command: &Command, key: Option<&str>) -> R<Done> {
        let key = key.unwrap_or_default();
        Ok(match command {
            Command::WorkspaceStatus(_) => Done::of(self.status()),
            Command::VaultOpen(a) => {
                self.with_pick(&a.root_token, PickKind::VaultRoot, true, |root| {
                    self.open_at(&root)
                })?;
                Done::of(self.status())
            }
            Command::VaultCreate(a) => {
                self.with_pick(&a.root_token, PickKind::VaultRoot, true, |root| {
                    self.create_at(&root)
                })?;
                Done::of(self.status())
            }
            Command::VaultLock(_) => {
                let root = self.open()?.root.clone();
                self.close(Some(Slot::Locked(root)));
                Done::of(self.status())
            }
            Command::VaultUnlock(_) => {
                let locked = match &*self.slot.lock().expect("slot") {
                    Slot::Locked(root) => Some(root.clone()),
                    Slot::Open(_) => None,
                    Slot::Empty => {
                        return Err(fail(MemoryErrorCode::VaultLocked, "workspace.no_vault"));
                    }
                };
                // Already open: answer the status (outside the slot guard).
                let Some(root) = locked else {
                    return Ok(Done::of(self.status()));
                };
                // A refused unlock (for example `workspace.vault_in_use`)
                // leaves the Vault locked, so Unlock can be retried.
                if let Err(error) = self.open_at(&root) {
                    *self.slot.lock().expect("slot") = Slot::Locked(root);
                    return Err(error);
                }
                Done::of(self.status())
            }
            Command::MemoryList(a) => Done::of(self.memory_list(a)?),
            Command::MemorySearch(a) => Done::of(self.memory_search(a)?),
            Command::MemoryRead(a) => Done::of(self.memory_read(&a.memory_id)?),
            Command::SourceExcerpt(a) => Done::of(self.source_excerpt(a)?),
            Command::CandidateList(a) => Done::of(self.candidate_list(a)?),
            Command::ReviewPlan(a) => Done::of(self.review_plan(a)?),
            Command::ReviewDiscard(a) => {
                self.plans.lock().expect("plans").remove(&a.plan_id);
                Done::of(json!({"discarded": true}))
            }
            Command::ReviewConfirm(a) => Done::of(self.review_confirm(&a.plan_id, &a.diff_hash)?),
            Command::ForgetPlan(a) => Done::of(self.forget_plan(a, key)?),
            Command::DeletePreview(a) => Done::of(self.delete_preview(a)?),
            Command::Remember(a) => Done::of(self.remember(a, key)?),
            Command::CorrectionPropose(a) => Done::of(self.correction(a, key)?),
            Command::ImportPreview(a) => Done::of(self.import_preview(&a.import_token)?),
            Command::ImportStart(a) => self.import_start(a, key)?,
            Command::ImportResume(a) => self.import_resume(a, key)?,
            Command::ImportList(_) => Done::of(self.import_list()?),
            Command::SessionList(_) => Done::of(self.session_list()?),
            Command::SessionDetail(a) => Done::of(self.session_detail(a)?),
            Command::SessionNew(_) => Done::of(self.session_new(key)?),
            Command::SessionAsk(a) => Done::of(self.session_ask(a, key)?),
            Command::SessionCheckpoint(a) => Done::of(self.session_checkpoint(a, key)?),
            Command::ContextPreview(a) => Done::of(self.context_preview(a, key)?),
            Command::ContextInspect(a) => Done::of(self.context_inspect(&a.capsule_id)?),
            Command::DispatchInspect(a) => Done::of(self.dispatch_inspect(&a.dispatch_id)?),
            Command::IndexRebuild(_) => self.index_rebuild()?,
            Command::VaultVerify(_) => self.vault_verify()?,
            Command::BackupExport(a) => self.backup_export(&a.destination_token)?,
            Command::RestorePreview(a) => Done::of(self.restore_preview(&a.export_token)?),
            Command::OperationGet(a) => {
                Done::of(self.ops.status(&a.operation_id).ok_or_else(|| {
                    fail(MemoryErrorCode::NotFound, "workspace.operation_unknown")
                })?)
            }
            Command::OperationCancel(a) => {
                Done::of(self.ops.cancel(&a.operation_id).ok_or_else(|| {
                    fail(MemoryErrorCode::NotFound, "workspace.operation_unknown")
                })?)
            }
            Command::OperationList(_) => Done::of(json!({"items": self.ops.list()})),
        })
    }

    // ----- status -------------------------------------------------------

    fn status(&self) -> Value {
        let companion = self.companion.lock().expect("companion").clone();
        let fixed = [
            json!({"component": "core", "state": "healthy", "mode": "embedded"}),
            json!({"component": "provider", "state": "unavailable", "mode": "local_mock_only"}),
            json!({"component": "sync", "state": "unavailable", "mode": "not_configured_before_mv9"}),
            json!({"component": "activity", "state": "unavailable", "mode": "independent_not_managed"}),
        ];
        let open = match &*self.slot.lock().expect("slot") {
            Slot::Open(open) => open.clone(),
            Slot::Locked(root) => {
                return json!({"vault": {"state": "locked", "rootName": base_name(root)}, "components": fixed, "operationsRunning": self.ops.running(), "companion": companion});
            }
            Slot::Empty => {
                return json!({"vault": {"state": "none"}, "components": fixed, "operationsRunning": self.ops.running(), "companion": companion});
            }
        };
        let vault = &open.vault;
        let health = vault.health();
        let head = health.head_sequence;
        let index = match open.index.try_lock() {
            Err(_) => {
                json!({"component": "memory_index", "state": "recovering", "mode": "rebuilding_or_updating"})
            }
            Ok(guard) => match guard.as_ref().map(|i| i.watermark()) {
                None => {
                    json!({"component": "memory_index", "state": "degraded", "mode": "not_opened"})
                }
                Some(Ok(mark)) => {
                    let at = mark.as_ref().map(|w| w.sequence);
                    let state = if at.is_some() && at == head {
                        "healthy"
                    } else {
                        "degraded"
                    };
                    json!({"component": "memory_index", "state": state, "mode": "local_sqlite", "watermarkSequence": at})
                }
                Some(Err(_)) => {
                    json!({"component": "memory_index", "state": "unavailable", "mode": "rebuild_needed"})
                }
            },
        };
        let pending = vault
            .pin_current()
            .ok()
            .and_then(|pin| pending_candidates(vault, &pin).ok())
            .map(|c| c.len());
        let free = verify_data_root(&open.root, &RootPolicy::default())
            .ok()
            .map(|r| r.free_bytes);
        let acl = enouia_memory_vault::platform::inspect_acl(&open.root)
            .ok()
            .map(|r| r.is_owner_only());
        let mut components = vec![
            fixed[0].clone(),
            json!({"component": "vault", "state": health_word(health.state), "errorCode": health.error_code}),
            index,
        ];
        components.extend(fixed[1..].iter().cloned());
        json!({
            "vault": {
                "state": "open",
                "rootName": base_name(&open.root),
                "vaultId": vault.vault_id(),
                "health": health_word(health.state),
                "errorCode": health.error_code,
                "headCommitId": health.head_commit_id,
                "headSequence": head,
                "lastCommitAt": health.last_commit_at,
                "policyEpoch": health.policy_epoch,
                "deletionEpoch": health.deletion_epoch,
                "networkAllowed": network_allowed(vault).ok(),
                "freeBytes": free,
                "ownerOnlyAcl": acl,
                "notes": health.notes.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>(),
            },
            "components": components,
            "pendingCandidates": pending,
            "operationsRunning": self.ops.running(),
            "lastBackup": self.last_backup.lock().expect("backup").as_ref().and_then(|id| self.ops.status(id)),
            "lastVerifiedRestore": null,
            "companion": companion,
        })
    }

    // ----- memories -----------------------------------------------------

    fn memory_list(&self, a: &wire::MemoryListArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let which = if a.include_inactive {
            Canonical::AllStatuses
        } else {
            Canonical::Active
        };
        let mut memories = canonical_memories(&open.vault, &pin, which)?;
        memories.sort_by(|x, y| {
            y.updated_at
                .cmp(&x.updated_at)
                .then_with(|| x.memory_id.as_str().cmp(y.memory_id.as_str()))
        });
        let total = memories.len();
        let now = open.vault.now()?;
        let (rows, next) = page(memories, a.cursor.as_deref(), a.limit, &pin)?;
        Ok(json!({
            "items": rows.iter().map(|m| views::memory_row(m, &now)).collect::<Vec<_>>(),
            "nextCursor": next,
            "total": total,
            "snapshotCommitId": pin.commit_id,
        }))
    }

    fn memory_search(&self, a: &wire::SearchArgs) -> R<Value> {
        let open = self.open()?;
        let request = SearchRequest {
            query: a.query.clone(),
            include_historical: a.include_historical,
            cursor: a.cursor.clone(),
            limit: a.limit,
            ..SearchRequest::default()
        };
        self.with_index(&open, |index| {
            let found = search(index, &open.vault, &open.owner, &request)?;
            Ok(json!({
                "items": found.items.iter().map(|h| json!({
                    "memoryId": h.memory_id, "revision": h.revision, "type": h.memory_type,
                    "projectId": h.project_id, "snippet": h.snippet, "currency": h.currency,
                    "evidenceCount": h.evidence_count, "updatedAt": h.updated_at,
                })).collect::<Vec<_>>(),
                "projects": found.projects.iter().map(|p| json!({
                    "projectId": p.project_id, "name": p.display_name, "exact": p.exact,
                })).collect::<Vec<_>>(),
                "nextCursor": found.next_cursor,
                "partial": found.partial,
                "snapshotSequence": found.snapshot,
                "ranking": enouia_memory_index::RANKING_VERSION,
            }))
        })
    }

    fn memory_read(&self, id: &MemoryId) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let all = canonical_memories(&open.vault, &pin, Canonical::AllStatuses)?;
        let memory = all
            .iter()
            .find(|m| &m.memory_id == id)
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "memory.not_found"))?;
        let superseded_by: Vec<&MemoryId> = all
            .iter()
            .filter(|m| m.supersedes.iter().any(|s| s.memory_id == *id))
            .map(|m| &m.memory_id)
            .collect();
        let evidence: Vec<Value> = memory
            .evidence
            .iter()
            .map(|e| {
                let available = open
                    .vault
                    .record_entry(&pin, RecordKind::Source, e.source_id.as_str())
                    .ok()
                    .flatten()
                    .is_some_and(|entry| entry.revision >= e.source_revision);
                json!({
                    "sourceId": e.source_id, "sourceRevision": e.source_revision,
                    "supports": e.supports, "evidenceClass": e.evidence_class,
                    "locator": e.locator, "available": available,
                })
            })
            .collect();
        let now = open.vault.now()?;
        Ok(json!({
            "summary": views::memory_row(memory, &now),
            "record": memory,
            "supersededBy": superseded_by,
            "evidence": evidence,
        }))
    }

    fn source_excerpt(&self, a: &wire::ExcerptArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let source: SourceRecord = parse(&open.vault.read_record(
            &pin,
            &RecordRef::new(RecordKind::Source, a.source_id.as_str(), a.source_revision),
        )?)?;
        let text = source_text(&open.vault, &pin, &source)?;
        let start = a.start_byte.unwrap_or(0);
        let (excerpt, byte_start, byte_end, truncated) =
            views::excerpt(&text, start, a.max_bytes.min(wire::EXCERPT_MAX_BYTES));
        // Never report a successful empty page before EOF: its cursor would
        // repeat forever. Keep the byte cap and require room for one character.
        if byte_end == byte_start && byte_start < text.len() as u64 {
            return Err(fail(
                MemoryErrorCode::InvalidRequest,
                "workspace.excerpt_budget",
            ));
        }
        Ok(json!({
            "sourceId": source.source_id, "sourceRevision": source.revision,
            "sourceKind": source.source_kind, "speakerRole": source.speaker_role,
            "occurredAt": source.occurred_at, "capturedAt": source.captured_at,
            "importId": source.import_id, "sensitivity": source.sensitivity,
            "excerpt": excerpt, "byteStart": byte_start, "byteEnd": byte_end,
            "totalBytes": text.len(), "truncated": truncated,
            "attachments": source.attachment_refs,
            "untrusted": true,
        }))
    }

    // ----- candidates and review ----------------------------------------

    fn candidate_list(&self, a: &wire::PageArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let pending = pending_candidates(&open.vault, &pin)?;
        let memories = canonical_memories(&open.vault, &pin, Canonical::AllStatuses)?;
        let total = pending.len();
        let (rows, next) = page(pending, a.cursor.as_deref(), a.limit, &pin)?;
        let items: Vec<Value> = rows
            .iter()
            .map(|c: &CandidateRecord| {
                let target = c
                    .target_memory_id
                    .as_ref()
                    .and_then(|t| memories.iter().find(|m| &m.memory_id == t));
                views::candidate_row(c, target)
            })
            .collect();
        Ok(json!({"items": items, "nextCursor": next, "total": total}))
    }

    fn keep_plan(&self, plan: ReviewPlan, purge: bool) -> Value {
        let plan_id = format!("pln_{}", hex(&self.config.ids.random_16()));
        let shown = json!({
            "planId": plan_id,
            "operationKind": plan.operation_kind,
            "diffHash": plan.diff_hash,
            "confirmCode": &plan.diff_hash.as_str()[..8],
            "issuedAt": plan.issued_at,
            "expiresAt": plan.expires_at,
            "ttlMs": PLAN_TTL_MS,
            "records": plan.diff["records"],
            "objects": plan.diff["objects"],
            "purge": purge,
        });
        self.plans
            .lock()
            .expect("plans")
            .insert(plan_id, PendingPlan { plan, purge });
        shown
    }

    fn review_plan(&self, a: &wire::ReviewPlanArgs) -> R<Value> {
        let open = self.open()?;
        let decisions: Vec<Decision> = a
            .decisions
            .iter()
            .map(|d| {
                let candidate_id = d.candidate_id.clone();
                let revision = d.revision;
                Ok(match d.action {
                    DecisionAction::Accept => Decision::Accept {
                        candidate_id,
                        revision,
                    },
                    DecisionAction::EditAccept => Decision::EditAccept {
                        candidate_id,
                        revision,
                        content: d.edited_content.clone().unwrap_or_default(),
                        details: None,
                    },
                    DecisionAction::Reject => Decision::Reject {
                        candidate_id,
                        revision,
                        reason_code: None,
                    },
                    DecisionAction::Merge => {
                        let target = d.merge_target.as_ref().expect("validated");
                        let into = match target.kind {
                            MergeKind::Candidate => {
                                enouia_memory_contract::candidate::MergeTarget::Candidate {
                                    candidate_id: CandidateId::parse(&target.id).map_err(|_| {
                                        fail(MemoryErrorCode::InvalidRequest, "ref.kind_prefix")
                                    })?,
                                }
                            }
                            MergeKind::Memory => {
                                enouia_memory_contract::candidate::MergeTarget::Memory {
                                    memory_id: MemoryId::parse(&target.id).map_err(|_| {
                                        fail(MemoryErrorCode::InvalidRequest, "ref.kind_prefix")
                                    })?,
                                }
                            }
                        };
                        Decision::Merge {
                            candidate_id,
                            revision,
                            into,
                        }
                    }
                })
            })
            .collect::<R<_>>()?;
        let shown = plan(&open.vault, &decisions, &open.owner, SURFACE)?;
        Ok(self.keep_plan(shown, false))
    }

    fn review_confirm(
        &self,
        plan_id: &str,
        diff_hash: &enouia_memory_contract::hash::Sha256Hex,
    ) -> R<Value> {
        let open = self.open()?;
        if let Some((seen, done)) = self.confirmed.lock().expect("confirmed").get(plan_id) {
            if seen != diff_hash {
                return Err(fail(
                    MemoryErrorCode::RevisionConflict,
                    "workspace.diff_hash_mismatch",
                ));
            }
            return Ok(done.clone());
        }
        let pending = {
            let plans = self.plans.lock().expect("plans");
            let pending = plans
                .get(plan_id)
                .ok_or_else(|| fail(MemoryErrorCode::NotFound, "workspace.plan_unknown"))?;
            if &pending.plan.diff_hash != diff_hash {
                return Err(fail(
                    MemoryErrorCode::RevisionConflict,
                    "workspace.diff_hash_mismatch",
                ));
            }
            PendingPlan {
                plan: pending.plan.clone(),
                purge: pending.purge,
            }
        };
        let confirmation = OwnerConfirmation {
            owner: open.owner.clone(),
            surface: SURFACE,
        };
        let finished = self.finish_confirm(&open, &pending, &confirmation);
        // A retryable failure keeps the plan, so the same confirm (same key,
        // same diff) can be retried; the commit and the file purge are both
        // idempotent. Any other outcome ends the plan (ADR-MEM-46).
        if !matches!(&finished, Err(Fail(error)) if error.retryable) {
            self.plans.lock().expect("plans").remove(plan_id);
        }
        let result = finished?;
        self.confirmed
            .lock()
            .expect("confirmed")
            .insert(plan_id.to_owned(), (diff_hash.clone(), result.clone()));
        Ok(result)
    }

    fn finish_confirm(
        &self,
        open: &Open,
        pending: &PendingPlan,
        confirmation: &OwnerConfirmation,
    ) -> R<Value> {
        confirm(&open.vault, &pending.plan, confirmation)?;
        let ids = &pending.plan.ids;
        let mut result = json!({
            "commitId": pending.plan.commit_id,
            "reviewIds": ids.iter().map(|i| &i.review_id).collect::<Vec<_>>(),
            "operationKind": pending.plan.operation_kind,
        });
        if pending.purge {
            // The confirm commit already used the plan nonce under the purge
            // scope; the file purge needs its own key.
            let done = complete_purge(
                &open.vault,
                &ids[0].delete_id,
                &open.owner,
                &purge_key(&pending.plan.nonce),
            )?;
            result["purge"] = json!({
                "deleteId": ids[0].delete_id, "receiptId": done.receipt_id,
                "filesRemoved": done.files.removed.len(),
                "orphansRemoved": done.files.orphans_removed.len(),
                "overallState": "backup_purge_pending",
            });
        }
        Ok(result)
    }

    fn stored_memory(&self, open: &Open, id: &MemoryId) -> R<CanonicalMemory> {
        let pin = open.vault.pin_current()?;
        let entry = open
            .vault
            .record_entry(&pin, RecordKind::Memory, id.as_str())?
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "memory.not_found"))?;
        parse(&open.vault.read_record(
            &pin,
            &RecordRef::new(RecordKind::Memory, id.as_str(), entry.revision),
        )?)
    }

    fn forget_plan(&self, a: &wire::ForgetArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let memory = self.stored_memory(&open, &a.memory_id)?;
        let (mode, purge) = match a.mode {
            ForgetMode::Forget => (DeleteMode::LogicalDelete, false),
            ForgetMode::Purge => (DeleteMode::Purge, true),
        };
        let scope = if a.with_dependents {
            DeleteScope::WithDependents
        } else {
            DeleteScope::AllRevisions
        };
        let proposed = propose(
            &open.vault,
            &delete_proposal(&memory, mode, scope),
            &Origin::owner(open.owner.clone()),
            format!("forget\n{key}").as_bytes(),
        )?;
        let id = match proposed {
            Proposed::Stored(written) => written.id,
            Proposed::DuplicateOf(id) => id,
        };
        let pin = open.vault.pin_current()?;
        let revision = open
            .vault
            .record_entry(&pin, RecordKind::Candidate, id.as_str())?
            .map(|e| e.revision)
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "candidate.missing"))?;
        let shown = plan(
            &open.vault,
            &[Decision::Accept {
                candidate_id: id,
                revision,
            }],
            &open.owner,
            SURFACE,
        )?;
        Ok(self.keep_plan(shown, purge))
    }

    fn delete_preview(&self, a: &wire::DeletePreviewArgs) -> R<Value> {
        let open = self.open()?;
        let scope = if a.with_dependents {
            DeleteScope::WithDependents
        } else {
            DeleteScope::AllRevisions
        };
        let impact = purge_preview(&open.vault, &a.memory_id, DeleteMode::Purge, scope)?;
        Ok(json!({
            "memoryId": a.memory_id,
            "withDependents": a.with_dependents,
            "targets": impact.targets.iter().map(|(k, i, r)| json!({"recordKind": k, "recordId": i, "revision": r})).collect::<Vec<_>>(),
            "objectCount": impact.object_hashes.len(),
            "losingProvenance": impact.losing_provenance,
            "sharedRaw": impact.shared_raw.iter().map(|(h, n)| json!({"objectHash": h, "otherSources": n})).collect::<Vec<_>>(),
        }))
    }

    fn policy(&self, open: &Open) -> R<PolicyId> {
        let pin = open.vault.pin_current()?;
        open.vault
            .record_entries(&pin, RecordKind::Policy)?
            .first()
            .and_then(|e| PolicyId::parse(&e.record_id).ok())
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "policy.missing"))
    }

    /// The owner's exact words become a manual-assertion source.
    fn assertion(
        &self,
        open: &Open,
        text: &str,
        key: &[u8],
    ) -> R<enouia_memory_contract::ids::SourceId> {
        Ok(open
            .vault
            .record_manual_assertion(
                &ManualAssertionInput {
                    text: text.to_owned(),
                    operator: open.owner.clone(),
                    trusted_surface: SURFACE,
                    confirmation: ConfirmationMethod::ExactTextConfirmDialog,
                    sensitivity: Sensitivity::Private,
                    access_policy_id: self.policy(open)?,
                    time_precision: TimePrecision::Millisecond,
                },
                key,
            )?
            .id)
    }

    fn proposed(
        &self,
        open: &Open,
        proposal: &Proposal,
        key: &[u8],
        source: impl serde::Serialize,
    ) -> R<Value> {
        let (id, state) = match propose(
            &open.vault,
            proposal,
            &Origin::owner(open.owner.clone()),
            key,
        )? {
            Proposed::Stored(written) => (written.id, "pending"),
            Proposed::DuplicateOf(id) => (id, "duplicate"),
        };
        let pin = open.vault.pin_current()?;
        let revision = open
            .vault
            .record_entry(&pin, RecordKind::Candidate, id.as_str())?
            .map(|e| e.revision);
        Ok(json!({"candidateId": id, "revision": revision, "state": state, "sourceId": source}))
    }

    fn remember(&self, a: &wire::RememberArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let source =
            self.assertion(&open, &a.text, format!("remember-source\n{key}").as_bytes())?;
        let subject = SubjectId::parse(&open.owner.actor_id.as_str().replacen("prn_", "sub_", 1))
            .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.subject"))?;
        let mut details = Map::new();
        details.insert("claim_key".into(), json!(a.claim_key));
        details.insert("subject_ids".into(), json!([subject]));
        let proposal = Proposal::create(
            ProposedType::Fact,
            &a.text,
            details,
            vec![EvidenceSpec::content(source.clone(), one())],
        );
        self.proposed(
            &open,
            &proposal,
            format!("remember\n{key}").as_bytes(),
            source,
        )
    }

    fn correction(&self, a: &wire::CorrectionArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let memory = self.stored_memory(&open, &a.memory_id)?;
        if memory.revision != a.revision {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "workspace.stale_revision",
            ));
        }
        let proposed_type: ProposedType = serde_json::from_value(json!(memory.memory_type()))
            .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.type"))?;
        let source = self.assertion(
            &open,
            &a.text,
            format!("correction-source\n{key}").as_bytes(),
        )?;
        let proposal = Proposal {
            kind: ProposalKind::Revise,
            proposed_type,
            content: a.text.clone(),
            details: None,
            evidence: vec![EvidenceSpec::content(source.clone(), one())],
            reason: "owner_correction".into(),
            sensitivity: None,
            target_memory_id: Some(a.memory_id.clone()),
            target_identity_id: None,
            expected_revision: Some(a.revision),
            effective_from: None,
            reopens_candidate_id: None,
        };
        self.proposed(
            &open,
            &proposal,
            format!("correction\n{key}").as_bytes(),
            source,
        )
    }

    // ----- import -------------------------------------------------------

    fn imports(&self, open: &Open) -> R<Vec<ImportManifest>> {
        let pin = open.vault.pin_current()?;
        let mut all: Vec<ImportManifest> = latest_records(&open.vault, &pin, RecordKind::Import)?;
        all.sort_by(|a, b| b.received_at.cmp(&a.received_at));
        Ok(all)
    }

    fn import_preview(&self, token: &str) -> R<Value> {
        let open = self.open()?;
        self.with_pick(token, PickKind::ImportFile, false, |path| {
            self.import_preview_at(&open, &path)
        })
    }

    fn import_preview_at(&self, open: &Open, path: &Path) -> R<Value> {
        let options = enouia_memory_import::ImportOptions::new(
            open.owner.clone(),
            "preview",
            self.policy(open)?,
        );
        let meta = std::fs::metadata(path)
            .map_err(|_| fail(MemoryErrorCode::NotFound, "workspace.pick_missing"))?;
        if meta.len() > options.max_input_bytes {
            return Err(fail(
                MemoryErrorCode::InvalidRequest,
                "import.input_too_large",
            ));
        }
        let bytes = std::fs::read(path)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed, "workspace.pick_unreadable"))?;
        let hash = sha256(&bytes);
        let duplicate = self
            .imports(open)?
            .into_iter()
            .find(|m| m.input_object_hash == hash && m.duplicate_of.is_none())
            .map(|m| m.import_id);
        let (recognized, kind, units, warnings) =
            match enouia_memory_import::detect::detect(&bytes, &options.zip) {
                enouia_memory_import::detect::Detection::Parsed(p) => {
                    (true, json!(p.input_kind), p.units.len(), p.warnings)
                }
                enouia_memory_import::detect::Detection::Unsupported {
                    input_kind,
                    warnings,
                    ..
                } => (false, json!(input_kind), 0, warnings),
            };
        Ok(json!({
            "displayName": base_name(path), "bytes": bytes.len(), "inputKind": kind,
            "recognized": recognized, "units": units, "duplicateOf": duplicate,
            "warnings": warnings.iter().map(|w| w.code.clone()).collect::<Vec<_>>(),
            "archivedEvenIfUnsupported": true,
        }))
    }

    fn import_options(&self, open: &Open, alias: &str) -> R<enouia_memory_import::ImportOptions> {
        Ok(enouia_memory_import::ImportOptions::new(
            open.owner.clone(),
            alias,
            self.policy(open)?,
        ))
    }

    /// Start an operation once per idempotency key (ADR-MEM-46): a retry with
    /// the same key and request answers the operation already started; the
    /// same key with a different request is a conflict.
    fn start_once(
        &self,
        kind: &'static str,
        key: &str,
        fingerprint: String,
        start: impl FnOnce() -> R<Done>,
    ) -> R<Done> {
        let mut started = self.started.lock().expect("started");
        let scope = format!("{kind}\n{key}");
        if let Some((seen, id)) = started.get(&scope) {
            if *seen != fingerprint {
                return Err(fail(
                    MemoryErrorCode::IdempotencyConflict,
                    "workspace.key_reuse",
                ));
            }
            return Ok(Done {
                result: json!({"operationId": id, "kind": kind}),
                operation_id: Some(id.clone()),
            });
        }
        let done = start()?;
        if let Some(id) = &done.operation_id {
            started.insert(scope, (fingerprint, id.clone()));
        }
        Ok(done)
    }

    fn import_start(&self, a: &wire::ImportStartArgs, key: &str) -> R<Done> {
        let fingerprint = format!("{}\n{}", a.import_token, a.account_alias);
        self.start_once("import", key, fingerprint, || {
            let open = self.open()?;
            self.with_pick(&a.import_token, PickKind::ImportFile, true, |path| {
                let options = self.import_options(&open, &a.account_alias)?;
                Ok(self.spawn("import", move |ticket| {
                    let report =
                        enouia_memory_import::import_file(&open.vault, &path, &options, ticket)?;
                    Ok(import_outcome(&report.manifest, ticket))
                }))
            })
        })
    }

    fn import_resume(&self, a: &wire::ImportResumeArgs, key: &str) -> R<Done> {
        let fingerprint = format!("{}\n{}", a.import_id, a.account_alias);
        self.start_once("import_resume", key, fingerprint, || {
            let open = self.open()?;
            let options = self.import_options(&open, &a.account_alias)?;
            let id = a.import_id.clone();
            Ok(self.spawn("import_resume", move |ticket| {
                let report =
                    enouia_memory_import::resume_import(&open.vault, &id, &options, ticket)?;
                Ok(import_outcome(&report.manifest, ticket))
            }))
        })
    }

    fn import_list(&self) -> R<Value> {
        let open = self.open()?;
        Ok(json!({"items": self.imports(&open)?.iter().map(import_row).collect::<Vec<_>>()}))
    }

    fn spawn(
        &self,
        kind: &'static str,
        work: impl FnOnce(&Ticket) -> R<Value> + Send + 'static,
    ) -> Done {
        let id = OperationId::from_random(self.config.ids.random_16());
        let id = self
            .ops
            .spawn(id, kind, move |ticket| work(ticket).map_err(|f| f.0));
        Done {
            result: json!({"operationId": id, "kind": kind}),
            operation_id: Some(id),
        }
    }

    // ----- sessions -----------------------------------------------------

    fn session_list(&self) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let mut sessions: Vec<SessionRecord> =
            latest_records(&open.vault, &pin, RecordKind::Session)?;
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let items: Vec<Value> = sessions
            .iter()
            .map(|s| {
                json!({
                    "sessionId": s.session_id, "status": s.status, "originSurface": s.origin_surface,
                    "defaultBranchId": s.default_branch_id, "lastEventSeq": s.last_event_seq,
                    "createdAt": s.created_at, "updatedAt": s.updated_at,
                    "branches": s.branches.iter().map(|b| json!({
                        "branchId": b.branch_id, "parentBranchId": b.parent_branch_id,
                        "forkedFromEventId": b.forked_from_event_id, "lastEventSeq": b.last_event_seq,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({"items": items}))
    }

    fn session_detail(&self, a: &wire::SessionArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let turns = session::turns(&open.vault, &open.owner, &a.session_id, &a.branch_id)?;
        let events = session::events(&open.vault, &pin, &a.session_id, &a.branch_id)?;
        let transcript: Vec<Value> = events
            .iter()
            .rev()
            .take(200)
            .rev()
            .map(|e| {
                let text = e
                    .content_ref
                    .as_ref()
                    .and_then(|_| session::text(&open.vault, &pin, e).ok());
                json!({
                    "eventId": e.event_id, "sequence": e.sequence, "kind": e.kind,
                    "turnId": e.turn_id, "deliveryState": e.delivery_state,
                    "occurredAt": e.occurred_at, "text": text,
                })
            })
            .collect();
        let checkpoints: Vec<Value> = latest_values(&open.vault, &pin, RecordKind::Checkpoint)?
            .into_iter()
            .filter(|c| {
                c["session_id"] == json!(a.session_id) && c["branch_id"] == json!(a.branch_id)
            })
            .map(|c| {
                let mut out = Map::new();
                for key in [
                    "checkpoint_id",
                    "revision",
                    "status",
                    "summary",
                    "decisions",
                    "open_loops",
                    "coverage_hash",
                    "covered_event_ids",
                    "created_at",
                ] {
                    if let Some(v) = c.get(key) {
                        out.insert(key.to_owned(), v.clone());
                    }
                }
                Value::Object(out)
            })
            .collect();
        Ok(json!({
            "sessionId": a.session_id, "branchId": a.branch_id,
            "lastSavedEventId": events.last().map(|e| &e.event_id),
            "turns": turns, "transcript": transcript, "checkpoints": checkpoints,
        }))
    }

    fn session_new(&self, key: &str) -> R<Value> {
        let open = self.open()?;
        let written = session::start(
            &open.vault,
            &enouia_memory_vault::service::SessionStart {
                owner: open.owner.clone(),
                surface: ClientSurface::WindowsApp,
                sensitivity: Sensitivity::Private,
                policy_id: self.policy(&open)?,
            },
            format!("session\n{key}").as_bytes(),
        )?;
        Ok(json!({"sessionId": written.id.0, "branchId": written.id.1, "saved": true}))
    }

    fn compiled(
        &self,
        open: &Open,
        query: &str,
        request: RequestId,
        sid: Option<&enouia_memory_contract::ids::SessionId>,
        bid: Option<&enouia_memory_contract::ids::BranchId>,
    ) -> R<enouia_memory_context::Compiled> {
        let mut input = CompileInput::local(query, open.owner.clone(), request);
        input.output_tokens = OUTPUT_TOKENS;
        input.session_id = sid.cloned();
        input.branch_id = bid.cloned();
        self.with_index(open, |index| Ok(compile(&open.vault, index, &input)?))
    }

    /// Input first, then compile, then the local Mock. A failure after the
    /// input is saved leaves the turn pending with no fabricated reply.
    fn session_ask(&self, a: &wire::AskArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let request = request_id_for(key, "session_ask");
        let input = session::save_input(
            &open.vault,
            &open.owner,
            &a.session_id,
            &a.branch_id,
            &a.text,
            &request,
            format!("ask-input\n{key}").as_bytes(),
        )?
        .id;
        let compiled = self.compiled(
            &open,
            &a.text,
            request,
            Some(&a.session_id),
            Some(&a.branch_id),
        )?;
        let answer = answer_saved(
            &open.vault,
            &open.owner,
            &compiled.capsule.capsule_id,
            OUTPUT_TOKENS,
            Some(&input),
        )?;
        Ok(json!({
            "inputEventId": input,
            "capsuleId": compiled.capsule.capsule_id,
            "inspectionId": compiled.inspection.inspection_id,
            "dispatchId": answer.dispatch_id,
            "status": answer.status,
            "statements": answer.statements,
            "memories": answer.memories,
            "sources": answer.sources,
            "requestHash": answer.request_hash,
            "destination": "local_mock",
        }))
    }

    fn session_checkpoint(&self, a: &wire::CheckpointArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let written = session::checkpoint(
            &open.vault,
            &open.owner,
            &a.session_id,
            &a.branch_id,
            &session::CheckpointInput {
                summary: a.summary.clone(),
                decisions: vec![],
                open_loops: vec![],
            },
            format!("checkpoint\n{key}").as_bytes(),
        )?;
        Ok(json!({"checkpointId": written.id, "status": "provisional", "saved": true}))
    }

    // ----- context inspector --------------------------------------------

    fn context_preview(&self, a: &wire::ContextPreviewArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        let compiled = self.compiled(
            &open,
            &a.query,
            request_id_for(key, "context_preview"),
            a.session_id.as_ref(),
            a.branch_id.as_ref(),
        )?;
        let c = &compiled.capsule;
        Ok(json!({
            "capsuleId": c.capsule_id, "inspectionId": compiled.inspection.inspection_id,
            "memoryCount": c.memory_items().count(), "budget": c.budget,
            "completeness": c.completeness, "state": "saved_preview",
        }))
    }

    fn dispatches_of(
        &self,
        open: &Open,
        pin: &CommitPin,
        capsule: &CapsuleId,
    ) -> R<Vec<DispatchRecord>> {
        Ok(
            latest_records::<DispatchRecord>(&open.vault, pin, RecordKind::Dispatch)?
                .into_iter()
                .filter(|d| &d.capsule_id == capsule)
                .collect(),
        )
    }

    fn context_inspect(&self, id: &CapsuleId) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let capsule: ContextCapsule = parse(&open.vault.read_record(
            &pin,
            &RecordRef::new(RecordKind::Capsule, id.as_str(), one()),
        )?)?;
        // The same current deletion/policy gate as an actual dispatch.
        enouia_memory_context::compiler::validate_saved(&open.vault, &open.owner, &capsule)?;
        let inspection =
            latest_records::<ContextInspection>(&open.vault, &pin, RecordKind::Inspection)?
                .into_iter()
                .find(|i| &i.capsule_id == id)
                .ok_or_else(|| fail(MemoryErrorCode::NotFound, "context.inspection_missing"))?;
        let dispatches: Vec<Value> = self
            .dispatches_of(&open, &pin, id)?
            .iter()
            .map(|d| {
                json!({
                    "dispatchId": d.dispatch_id, "state": d.state, "destination": d.destination,
                    "preparedAt": d.prepared_at, "sentAt": d.sent_at, "completedAt": d.completed_at,
                    "requestHash": d.request_hash,
                })
            })
            .collect();
        let delivery = if dispatches.is_empty() {
            "preview_not_sent"
        } else {
            "dispatched"
        };
        Ok(
            json!({"capsule": capsule, "inspection": inspection, "dispatches": dispatches, "delivery": delivery}),
        )
    }

    fn dispatch_inspect(&self, id: &DispatchId) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let request = enouia_memory_context::mock::inspect_request(&open.vault, &open.owner, id)?;
        let record: DispatchRecord = parse(&open.vault.read_record(
            &pin,
            &RecordRef::new(RecordKind::Dispatch, id.as_str(), one()),
        )?)?;
        Ok(json!({
            "dispatchId": id, "capsuleId": request.capsule_id, "destination": request.destination,
            "state": record.state, "preparedAt": record.prepared_at, "sentAt": record.sent_at,
            "completedAt": record.completed_at,
            "messages": request.messages.iter().map(|m| json!({"role": m.role, "text": m.text})).collect::<Vec<_>>(),
            "tools": record.tools.len(), "output": request.output,
            "requestHash": request.payload_hash(), "verified": request.payload_hash() == record.request_hash,
        }))
    }

    // ----- recovery and status ------------------------------------------

    fn index_rebuild(&self) -> R<Done> {
        let open = self.open()?;
        Ok(self.spawn("index_rebuild", move |ticket| {
            let mut guard = open.index.lock().unwrap_or_else(|poisoned| {
                open.index.clear_poison();
                poisoned.into_inner()
            });
            *guard = None;
            ticket.set_total(open.vault.pin_current().ok().map(|p| p.sequence));
            let cancel = || {
                ticket.advance();
                ticket.cancelled()
            };
            let (index, report) = Index::rebuild_with(&open.vault, &cancel)?;
            let sequence = index.watermark()?.map(|w| w.sequence);
            *guard = Some(index);
            Ok(json!({
                "revisionsIndexed": report.revisions_indexed,
                "commitsApplied": report.commits_applied,
                "reachedHead": report.reached_head,
                "watermarkSequence": sequence,
                "cancelled": !report.reached_head && ticket.cancelled(),
            }))
        }))
    }

    fn vault_verify(&self) -> R<Done> {
        let open = self.open()?;
        Ok(self.spawn("vault_verify", move |_| {
            let pin = open.vault.pin_current()?;
            let report = open.vault.verify(&pin)?;
            Ok(json!({
                "commitId": pin.commit_id, "clean": report.is_clean(),
                "recordsChecked": report.records_checked, "objectsChecked": report.objects_checked,
                "missingRecords": report.missing_records.len(), "corruptRecords": report.corrupt_records.len(),
                "missingObjects": report.missing_objects.len(), "corruptObjects": report.corrupt_objects.len(),
                "damagedSegments": report.damaged_segments.len(),
            }))
        }))
    }

    fn backup_export(&self, token: &str) -> R<Done> {
        let open = self.open()?;
        self.with_pick(token, PickKind::BackupDestination, true, |destination| {
            let verified =
                verify_data_root(&destination, &RootPolicy::default()).map_err(root_rejected)?;
            let name = base_name(&destination);
            let done = self.spawn("backup_export", move |_| {
                let pin = open.vault.pin_current()?;
                let export = export_pinned(&open.vault, &pin, &verified)?;
                Ok(json!({
                    "commitId": export.commit_id, "sequence": export.sequence,
                    "files": export.files.len(), "destinationName": name,
                }))
            });
            *self.last_backup.lock().expect("backup") = done.operation_id.clone();
            Ok(done)
        })
    }

    fn restore_preview(&self, token: &str) -> R<Value> {
        self.with_pick(token, PickKind::ExportFolder, true, |path| {
        let export = verify_export(&path)?;
        let same = self
            .open()
            .ok()
            .map(|o| o.vault.vault_id() == &export.vault_id);
        Ok(json!({
            "valid": true, "vaultId": export.vault_id, "commitId": export.commit_id,
            "sequence": export.sequence, "files": export.files.len(), "createdAt": export.created_at,
            "policyEpoch": export.policy_epoch, "deletionEpoch": export.deletion_epoch,
            "sameVaultAsOpen": same,
            "restoreHow": "cli_restore_into_empty_target",
        }))
        })
    }
}

/// The idempotency key of the file purge that follows a confirmed purge plan.
pub fn purge_key(nonce: &str) -> Vec<u8> {
    format!(
        "complete-purge
{nonce}"
    )
    .into_bytes()
}

/// The exact evidence text of a source: the owner's or agent's words, or the
/// bytes an import locator selects from the raw object.
fn source_text(vault: &Vault, pin: &CommitPin, source: &SourceRecord) -> R<String> {
    if let Some(manual) = &source.manual_assertion {
        return Ok(manual.input_text.clone());
    }
    if let Some(agent) = &source.agent_submission {
        return Ok(agent.submitted_text.clone());
    }
    let raw = source
        .raw_object_hash
        .as_ref()
        .ok_or_else(|| fail(MemoryErrorCode::BrokenProvenance, "source.no_object"))?;
    let bytes = vault.read_object(pin, raw)?;
    let selected = enouia_memory_import::locate::resolve(&bytes, &source.locator)
        .map_err(|_| fail(MemoryErrorCode::BrokenProvenance, "source.locator"))?;
    if sha256(&selected) != source.content_hash {
        return Err(fail(
            MemoryErrorCode::BrokenProvenance,
            "source.content_hash",
        ));
    }
    Ok(String::from_utf8_lossy(&selected).into_owned())
}

fn import_outcome(m: &ImportManifest, ticket: &Ticket) -> Value {
    let mut row = import_row(m);
    let stopped =
        ticket.cancelled() && m.status == enouia_memory_contract::import::ImportStatus::Parsing;
    row["cancelled"] = json!(stopped);
    row["resumable"] = json!(stopped);
    row
}

/// A handled command: its result and, for long work, the operation ID.
struct Done {
    result: Value,
    operation_id: Option<OperationId>,
}

impl Done {
    fn of(result: Value) -> Self {
        Self {
            result,
            operation_id: None,
        }
    }
}

#[cfg(test)]
mod tests;
