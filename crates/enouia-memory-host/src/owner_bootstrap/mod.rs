//! Trusted local empty-root provisioning, not an Owner authentication endpoint
//! for MCP/page callers. OS evidence is consumed only by the authorized native
//! command's explicit current-owner-self enrollment. No wire identity overrides.
mod json;
use crate::windows::CurrentProcessSession;
use enouia_memory_contract::{
    common::{ActorRef, ActorType},
    foundation::Clock,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::canonical_bytes,
    next::*,
    ports::IdSource,
};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, VerifiedRoot,
    automation::segmented::SegmentedAutomationVault, fault::Faults, fs::ManagedRoot, platform,
    verify_data_root,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

const META: &str = ".enouia-owner-data.json";
const AREA: &str = ".enouia-owner-bootstrap";
const JOURNAL: &str = "sync-state/registration.json";
const MAX: usize = 65536;
#[derive(Debug)]
pub enum BootstrapFault {
    Invalid,
    Unauthenticated,
    UnsafeHome,
    Busy,
    Conflict,
    OutcomeUnknown,
    NotRegistered,
}
type Result<T> = std::result::Result<T, BootstrapFault>;
fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|_| BootstrapFault::UnsafeHome)
}
fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    canonical_bytes(v).map_err(|_| BootstrapFault::Invalid)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    NativeLocalCliOwnerSelf,
    SyntheticTest,
}
enum Identity {
    Native(CurrentProcessSession),
    #[cfg(test)]
    Test(Sha256Hex),
}
impl Identity {
    fn account(&self) -> &Sha256Hex {
        match self {
            Self::Native(v) => v.account(),
            #[cfg(test)]
            Self::Test(v) => v,
        }
    }
    fn verify(&self) -> Result<()> {
        match self {
            Self::Native(v) => v.verify().map_err(|_| BootstrapFault::Unauthenticated),
            #[cfg(test)]
            Self::Test(_) => Ok(()),
        }
    }
    fn origin(&self) -> Origin {
        match self {
            Self::Native(_) => Origin::NativeLocalCliOwnerSelf,
            #[cfg(test)]
            Self::Test(_) => Origin::SyntheticTest,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Registered,
    Creating,
    Verified,
    Recorded,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    version: u8,
    canonical_home: String,
    origin: Origin,
    operation: OperationId,
    account: Sha256Hex,
    descriptor: AutomationDescriptor,
    descriptor_hash: Sha256Hex,
    root_relative: String,
    baseline_hash: Sha256Hex,
    baseline_raw_hash: Sha256Hex,
    baseline: Value,
    phase: Phase,
    snapshot: Option<SnapshotBinding>,
}
struct Bootstrap {
    home: VerifiedRoot,
    identity: Identity,
    clock: Arc<dyn Clock + Send + Sync>,
    #[cfg(test)]
    storage_faults: Faults,
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let m = io(fs::symlink_metadata(path))?;
    if !m.is_file() || platform::is_reparse_point(&m) || m.len() > MAX as u64 {
        return Err(BootstrapFault::UnsafeHome);
    }
    let mut b = Vec::new();
    io(io(File::open(path))?
        .take((MAX + 1) as u64)
        .read_to_end(&mut b))?;
    if b.len() > MAX {
        return Err(BootstrapFault::Invalid);
    }
    Ok(b)
}
impl Bootstrap {
    fn home_key(&self) -> Result<&str> {
        self.home.path().to_str().ok_or(BootstrapFault::UnsafeHome)
    }
    fn home(path: &Path, identity: Identity, clock: Arc<dyn Clock + Send + Sync>) -> Result<Self> {
        identity.verify()?;
        let home = verify_data_root(path, &RootPolicy::default())
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        if identity.origin() == Origin::NativeLocalCliOwnerSelf
            && home.path().starts_with(std::env::temp_dir())
        {
            return Err(BootstrapFault::UnsafeHome);
        }
        let this = Self {
            home,
            identity,
            clock,
            #[cfg(test)]
            storage_faults: Faults::none(),
        };
        this.check_home()?;
        Ok(this)
    }
    fn check_home(&self) -> Result<(Vec<u8>, Value)> {
        self.identity.verify()?;
        verify_data_root(self.home.path(), &RootPolicy::default())
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        if !io(platform::inspect_acl(self.home.path()))?.is_owner_only() {
            return Err(BootstrapFault::UnsafeHome);
        }
        bounded(&self.home.path().join("DO_NOT_DELETE.USER_DATA"))?;
        let b = bounded(&self.home.path().join(META))?;
        let v = json::parse(&b).map_err(|_| BootstrapFault::Invalid)?;
        if v["format"] != "enouia-owner-data-home/1"
            || v["allow_development_cleanup"] != false
            || v["allow_git_upload"] != false
            || !v.is_object()
        {
            return Err(BootstrapFault::UnsafeHome);
        }
        if self.identity.origin() == Origin::NativeLocalCliOwnerSelf
            && v["storage_class"] != "owner_data"
        {
            return Err(BootstrapFault::UnsafeHome);
        }
        Ok((b, v))
    }
    fn store(&self, create: bool) -> Result<(ManagedRoot, File)> {
        self.check_home()?;
        let path = self.home.path().join(AREA);
        if !path.exists() {
            if !create {
                return Err(BootstrapFault::NotRegistered);
            }
            io(fs::create_dir(&path))?;
            io(platform::restrict_to_owner(&path))?;
        }
        let root = verify_data_root(&path, &RootPolicy::default())
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        if !io(platform::inspect_acl(root.path()))?.is_owner_only() {
            return Err(BootstrapFault::UnsafeHome);
        }
        let store = ManagedRoot::new(root.path(), Faults::none());
        let lock = store
            .open_lock_file("sync-state/bootstrap.lock")
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        lock.try_lock().map_err(|_| BootstrapFault::Busy)?;
        Ok((store, lock))
    }
    fn load(&self, store: &ManagedRoot) -> Result<Registration> {
        if io(platform::inspect_acl(&store.root().join(JOURNAL)))?.broad_grants() != 0 {
            return Err(BootstrapFault::Unauthenticated);
        }
        let b = store
            .read_bounded(JOURNAL, MAX)
            .map_err(|_| BootstrapFault::Invalid)?
            .ok_or(BootstrapFault::NotRegistered)?;
        let value = json::parse(&b).map_err(|_| BootstrapFault::Invalid)?;
        let r: Registration = serde_json::from_value(value).map_err(|_| BootstrapFault::Invalid)?;
        if r.version != 1
            || r.canonical_home != self.home_key()?
            || r.origin != self.identity.origin()
            || &r.account != self.identity.account()
            || !r.descriptor.validate().is_empty()
            || r.descriptor.layout != AutomationLayout::SegmentedOwnerV1
            || sha256(&bytes(&r.descriptor)?) != r.descriptor_hash
            || r.root_relative != format!("vaults/{}", r.descriptor.vault_id)
            || sha256(&bytes(&r.baseline)?) != r.baseline_hash
            || (matches!(r.phase, Phase::Verified | Phase::Recorded) != r.snapshot.is_some())
            || r.snapshot
                .as_ref()
                .is_some_and(|s| s.vault_id != r.descriptor.vault_id)
        {
            return Err(BootstrapFault::Unauthenticated);
        }
        self.identity.verify()?;
        Ok(r)
    }
    fn save(&self, store: &ManagedRoot, r: &Registration) -> Result<()> {
        self.identity.verify()?;
        self.check_home()?;
        let b = bytes(r)?;
        if b.len() > MAX {
            return Err(BootstrapFault::Invalid);
        }
        store
            .write_atomic(JOURNAL, &b)
            .map_err(|_| BootstrapFault::OutcomeUnknown)
    }
    fn prepare(&self) -> Result<Value> {
        let (store, _lock) = self.store(true)?;
        if store.exists(JOURNAL).map_err(|_| BootstrapFault::Invalid)? {
            return Ok(self.result(&self.load(&store)?));
        }
        let (original, baseline) = self.check_home()?;
        if baseline["production_vault_created"] != false || baseline["active_vault"] != Value::Null
        {
            return Err(BootstrapFault::Conflict);
        }
        let ids = OsIdSource;
        let descriptor = AutomationDescriptor {
            schema_version: NextVersion,
            kind: DescriptorKind::AutomationVault,
            layout: AutomationLayout::SegmentedOwnerV1,
            vault_id: VaultId::from_random(ids.random_16()),
            host_id: DeviceId::from_random(ids.random_16()),
            created_by: ActorRef {
                actor_id: PrincipalId::from_random(ids.random_16()),
                actor_type: ActorType::Owner,
            },
            system_actor: ActorRef {
                actor_id: PrincipalId::from_random(ids.random_16()),
                actor_type: ActorType::System,
            },
            owner_subject_id: Some(SubjectId::from_random(ids.random_16())),
        };
        let r = Registration {
            version: 1,
            canonical_home: self.home_key()?.to_owned(),
            origin: self.identity.origin(),
            operation: OperationId::from_random(ids.random_16()),
            account: self.identity.account().clone(),
            root_relative: format!("vaults/{}", descriptor.vault_id),
            descriptor_hash: sha256(&bytes(&descriptor)?),
            descriptor,
            baseline_hash: sha256(&bytes(&baseline)?),
            baseline_raw_hash: sha256(&original),
            baseline,
            phase: Phase::Registered,
            snapshot: None,
        };
        store
            .write_new(JOURNAL, &bytes(&r)?)
            .map_err(|_| BootstrapFault::OutcomeUnknown)?;
        Ok(self.result(&r))
    }
    fn result(&self, r: &Registration) -> Value {
        json!({"operation_id":r.operation,"root":self.home.path().join(&r.root_relative),"descriptor":r.descriptor,
            "descriptor_hash":r.descriptor_hash,"enrollment":r.origin,"subject_binding":"authenticated_owner_self",
            "phase":r.phase,"snapshot_at_creation":r.snapshot,"empty_at_creation":r.snapshot.is_some(),
            "runtime_adopted":false,"locator_semantics":"verified_creation_not_runtime_selection"})
    }
    fn expected_metadata(&self, r: &Registration) -> Value {
        let mut v = r.baseline.clone();
        v["production_vault_created"] = json!(true);
        v["active_vault"] = json!(r.root_relative);
        v["vault_creation"] = json!({"operation_id":r.operation,"descriptor_hash":r.descriptor_hash,"runtime_adopted":false,"locator_semantics":"verified_creation_not_runtime_selection"});
        v
    }
    fn commit(&self, operation: &OperationId, recover: bool) -> Result<Value> {
        let (store, _lock) = self.store(false)?;
        let mut r = self.load(&store)?;
        if &r.operation != operation {
            return Err(BootstrapFault::Conflict);
        }
        if r.phase == Phase::Recorded {
            return Ok(self.result(&r));
        }
        let (original, current) = self.check_home()?;
        let expected = self.expected_metadata(&r);
        if current != r.baseline && current != expected {
            return Err(BootstrapFault::Conflict);
        }
        if current == r.baseline && sha256(&original) != r.baseline_raw_hash {
            return Err(BootstrapFault::Conflict);
        }
        let parent = self.home.path().join("vaults");
        if !parent.exists() {
            io(fs::create_dir(&parent))?;
            io(platform::restrict_to_owner(&parent))?;
        }
        let parent_root = verify_data_root(&parent, &RootPolicy::default())
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        if !io(platform::inspect_acl(parent_root.path()))?.is_owner_only() {
            return Err(BootstrapFault::UnsafeHome);
        }
        let path = self.home.path().join(&r.root_relative);
        if !path.exists() {
            if r.phase != Phase::Registered && r.phase != Phase::Creating {
                return Err(BootstrapFault::OutcomeUnknown);
            }
            r.phase = Phase::Creating;
            self.save(&store, &r)?;
            io(fs::create_dir(&path))?;
        }
        let root = verify_data_root(&path, &RootPolicy::default())
            .map_err(|_| BootstrapFault::UnsafeHome)?;
        if io(fs::read_dir(root.path()))?.next().is_none() {
            if r.phase != Phase::Registered && r.phase != Phase::Creating {
                return Err(BootstrapFault::OutcomeUnknown);
            }
            r.phase = Phase::Creating;
            self.save(&store, &r)?;
            let vault = SegmentedAutomationVault::create(
                &root,
                r.descriptor.clone(),
                self.clock.clone(),
                Arc::new(OsIdSource),
                self.storage_faults(),
            )
            .map_err(|_| BootstrapFault::OutcomeUnknown)?;
            drop(vault);
        }
        let actual = bounded(&path.join(enouia_memory_contract::layout::vault_descriptor()))?;
        let descriptor: AutomationDescriptor =
            serde_json::from_value(json::parse(&actual).map_err(|_| BootstrapFault::Invalid)?)
                .map_err(|_| BootstrapFault::Invalid)?;
        if descriptor != r.descriptor {
            return Err(BootstrapFault::Conflict);
        }
        self.identity.verify()?;
        let opened = SegmentedAutomationVault::open(
            &root,
            r.descriptor.host_id.clone(),
            self.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        );
        let vault = match opened {
            Ok(v) => v,
            Err(enouia_memory_vault::automation::AutomationFault::RecoveryRequired) if recover => {
                SegmentedAutomationVault::recover(
                    &root,
                    r.descriptor.host_id.clone(),
                    &r.descriptor.system_actor,
                    self.clock.clone(),
                    Arc::new(OsIdSource),
                    Faults::none(),
                )
                .map_err(|_| BootstrapFault::OutcomeUnknown)?
            }
            Err(_) => return Err(BootstrapFault::OutcomeUnknown),
        };
        r.snapshot = Some(
            vault
                .empty_owner_genesis_snapshot(&r.descriptor.system_actor)
                .map_err(|_| BootstrapFault::Conflict)?,
        );
        drop(vault);
        r.phase = Phase::Verified;
        self.save(&store, &r)?;
        self.publish_metadata(&r, &expected)?;
        r.phase = Phase::Recorded;
        self.save(&store, &r)?;
        Ok(self.result(&r))
    }
    fn publish_metadata(&self, r: &Registration, wanted: &Value) -> Result<()> {
        self.identity.verify()?;
        let (original, current) = self.check_home()?;
        if &current == wanted {
            return Ok(());
        }
        if current != r.baseline || sha256(&original) != r.baseline_raw_hash {
            return Err(BootstrapFault::Conflict);
        }
        let target = self.home.path().join(META);
        let acl = io(platform::inspect_acl(&target))?;
        if acl.broad_grants() != 0 {
            return Err(BootstrapFault::UnsafeHome);
        }
        let tmp = self.home.path().join(format!(
            ".enouia-owner-data-bootstrap-{}",
            format_uuid_v4(OsIdSource.random_16())
        ));
        let mut file = io(OpenOptions::new().write(true).create_new(true).open(&tmp))?;
        io(file.write_all(&bytes(wanted)?))?;
        io(file.sync_all())?;
        drop(file);
        self.identity.verify()?;
        if bounded(&target)? != original {
            return Err(BootstrapFault::Conflict);
        }
        io(platform::replace_durable(&tmp, &target)).map_err(|_| BootstrapFault::OutcomeUnknown)
    }
    fn storage_faults(&self) -> Faults {
        #[cfg(test)]
        {
            self.storage_faults.clone()
        }
        #[cfg(not(test))]
        {
            Faults::none()
        }
    }
}
/// Only the approved trusted local native command may call this entry. Parsed
/// flags express scope, not authentication. No MCP/page route invokes it.
pub fn run_cli(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or(BootstrapFault::Invalid)?;
    let valid = match command {
        "prepare" => args.len() == 3 && args[2] == "--bind-current-owner-self",
        "commit" | "recover" => args.len() == 4 && args[3] == "--bind-current-owner-self",
        "status" => args.len() == 2,
        _ => false,
    };
    if !valid {
        return Err(BootstrapFault::Invalid);
    }
    let identity = Identity::Native(
        CurrentProcessSession::capture().map_err(|_| BootstrapFault::Unauthenticated)?,
    );
    let boot = Bootstrap::home(Path::new(&args[1]), identity, Arc::new(SystemClock))?;
    match command {
        "prepare" => boot.prepare(),
        "status" => {
            let (store, _lock) = boot.store(false)?;
            Ok(boot.result(&boot.load(&store)?))
        }
        _ => boot.commit(
            &OperationId::parse(&args[2]).map_err(|_| BootstrapFault::Invalid)?,
            command == "recover",
        ),
    }
}
#[cfg(test)]
mod tests;
