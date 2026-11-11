//! ADR-MEM-136: offline portable Owner exports. No source discovery or network.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MANIFEST: &str = "owner-export.json";
const RESTORE: &str = "config/segmented-restore-1.json";
const JOURNALS: [&str; 2] = [
    "sync-state/initialization-owner-1.json",
    "sync-state/captured-formation-1.json",
];
const LEASES: [&str; 2] = [
    "sync-state/initialization-owner-1.lock",
    "sync-state/captured-formation-1.lock",
];
const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_MANIFEST: usize = 32 * 1024 * 1024;
const FORMAT: &str = "enouia-segmented-owner-export/1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub path: String,
    pub sha256: Sha256Hex,
    pub size_bytes: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerExport {
    pub format: String,
    pub descriptor_hash: Sha256Hex,
    pub head: AutomationCurrent,
    pub files: Vec<ExportFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreReceipt {
    version: u32,
    descriptor_hash: Sha256Hex,
    head: AutomationCurrent,
    export_hash: Sha256Hex,
    restored_at: Timestamp,
    activated_at: Option<Timestamp>,
}

fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|_| AutomationFault::StorageFailed)
}
fn empty(root: &VerifiedRoot) -> Result<()> {
    if io(std::fs::read_dir(root.path()))?.next().is_some() {
        return Err(AutomationFault::Conflict);
    }
    Ok(())
}
fn separate(a: &VerifiedRoot, b: &VerifiedRoot) -> Result<()> {
    if a.path().starts_with(b.path()) || b.path().starts_with(a.path()) {
        return Err(AutomationFault::Denied);
    }
    Ok(())
}
fn read(root: &ManagedRoot, path: &str, limit: usize) -> Result<Vec<u8>> {
    root.read_bounded(path, limit)?
        .ok_or(AutomationFault::Unavailable)
}
/// Exact file inventory; never follow symlinks, junctions or special files.
fn inventory(base: &Path, dir: &Path, paths: &mut BTreeSet<String>) -> Result<()> {
    for entry in io(std::fs::read_dir(dir))? {
        let path = io(entry)?.path();
        let meta = io(std::fs::symlink_metadata(&path))?;
        if platform::is_reparse_point(&meta) {
            return Err(AutomationFault::Denied);
        }
        if meta.is_dir() {
            inventory(base, &path, paths)?;
        } else if meta.is_file() {
            paths.insert(
                path.strip_prefix(base)
                    .map_err(|_| AutomationFault::Denied)?
                    .to_str()
                    .ok_or(AutomationFault::Denied)?
                    .replace('\\', "/"),
            );
        } else {
            return Err(AutomationFault::Denied);
        }
    }
    Ok(())
}
fn allowed(path: &str) -> bool {
    if !crate::fs::is_managed_path(path) {
        return false;
    }
    path == layout::vault_descriptor()
        || path == layout::current_pointer()
        || path == AUTOMATION_INTENT
        || path == AUTOMATION_WITNESS
        || JOURNALS.contains(&path)
        || path
            .strip_prefix("vault/automation-segments/")
            .and_then(|v| v.strip_suffix(".json"))
            .is_some_and(|v| Sha256Hex::parse(v).is_some())
        || path
            .strip_prefix("vault/raw/objects/")
            .is_some_and(|v| Sha256Hex::parse(v).is_some())
}

/// Full semantic replay shared by export, independent verification and activation.
fn verify_contents(root: &ManagedRoot, export: &OwnerExport) -> Result<AutomationDescriptor> {
    if export.format != FORMAT || export.files.len() > MAX_SEGMENTED_COMMITS * 3 {
        return Err(AutomationFault::Denied);
    }
    let mut files = BTreeMap::new();
    for file in &export.files {
        if !allowed(&file.path)
            || file.size_bytes > MAX_FILE as u64
            || files.insert(file.path.as_str(), file).is_some()
        {
            return Err(AutomationFault::Denied);
        }
        let bytes = read(root, &file.path, MAX_FILE)?;
        if bytes.len() as u64 != file.size_bytes || sha256(&bytes) != file.sha256 {
            return Err(AutomationFault::Unavailable);
        }
        if let Some(hash) = file.path.strip_prefix("vault/raw/objects/")
            && file.sha256.as_str() != hash
        {
            return Err(AutomationFault::Unavailable);
        }
    }
    let required = |path: &str| -> Result<()> {
        files
            .contains_key(path)
            .then_some(())
            .ok_or(AutomationFault::Unavailable)
    };
    required(&layout::vault_descriptor())?;
    let descriptor: AutomationDescriptor =
        parse_document(&read(root, &layout::vault_descriptor(), 16 * 1024)?)?;
    if descriptor.layout != AutomationLayout::SegmentedOwnerV1
        || digest(&descriptor) != export.descriptor_hash
        || descriptor.vault_id != export.head.vault_id
    {
        return Err(AutomationFault::Denied);
    }
    for path in [layout::current_pointer().as_str(), AUTOMATION_WITNESS] {
        required(path)?;
        if parse_document::<AutomationCurrent>(&read(root, path, 16 * 1024)?)? != export.head {
            return Err(AutomationFault::RecoveryRequired);
        }
    }
    required(AUTOMATION_INTENT)?;
    let intent: AutomationPublication = parse_document(&read(root, AUTOMATION_INTENT, 32 * 1024)?)?;
    if intent.after != export.head {
        return Err(AutomationFault::RecoveryRequired);
    }
    let (mut next, mut chain, mut total) = (Some(export.head.clone()), Vec::new(), 0usize);
    let mut reachable = BTreeSet::new();
    while let Some(pointer) = next {
        if chain.len() >= MAX_SEGMENTED_COMMITS {
            return Err(AutomationFault::Capacity);
        }
        let path = SegmentedAutomationVault::path(&pointer.manifest_sha256);
        required(&path)?;
        reachable.insert(path);
        let bytes = read(
            root,
            &SegmentedAutomationVault::path(&pointer.manifest_sha256),
            MAX_SEGMENT_BYTES,
        )?;
        total += bytes.len();
        if total > MAX_SEGMENTED_CHAIN_BYTES || sha256(&bytes) != pointer.manifest_sha256 {
            return Err(AutomationFault::Capacity);
        }
        let commit: SegmentedCommit = parse_document(&bytes)?;
        if canonical_bytes(&commit)? != bytes || commit.current() != pointer {
            return Err(AutomationFault::Unavailable);
        }
        next = commit.parent.clone();
        chain.push((commit, bytes.len()));
    }
    let mut state = State::default();
    for (commit, size) in chain.into_iter().rev() {
        let admitted = state.check(&descriptor, &commit)?;
        state.apply(commit, admitted, size);
    }
    if state
        .head
        .as_ref()
        .is_none_or(|h| h.parent != intent.before)
        || files
            .keys()
            .any(|p| p.starts_with("vault/automation-segments/") && !reachable.contains(*p))
    {
        return Err(AutomationFault::RecoveryRequired);
    }
    // Raw closure includes every revision, including logically deleted records.
    // Their immutable history remains intact; this slice does not implement purge.
    for source in state.sources.values().flatten() {
        if let Some(hash) = &source.record.raw_object_hash {
            required(&layout::raw_object(hash))?;
        }
        for attachment in &source.attachments {
            if let Some(hash) = &attachment.object_hash {
                let path = layout::raw_object(hash);
                required(&path)?;
                if Some(files[path.as_str()].size_bytes) != attachment.size_bytes {
                    return Err(AutomationFault::Unavailable);
                }
            }
        }
    }
    Ok(descriptor)
}

/// Offline export: an active native Host or journal owner makes this fail busy.
pub fn export_owner(
    source: &VerifiedRoot,
    destination: &VerifiedRoot,
    clock: Arc<dyn Clock + Send + Sync>,
    ids: Arc<dyn IdSource + Send + Sync>,
) -> Result<OwnerExport> {
    separate(source, destination)?;
    empty(destination)?;
    let managed = ManagedRoot::new(source.path(), Faults::none());
    let descriptor: AutomationDescriptor =
        parse_document(&read(&managed, &layout::vault_descriptor(), 16 * 1024)?)?;
    if descriptor.layout != AutomationLayout::SegmentedOwnerV1 {
        return Err(AutomationFault::Denied);
    }
    let vault = SegmentedAutomationVault::open(
        source,
        descriptor.host_id.clone(),
        clock,
        ids,
        Faults::none(),
    )?;
    let mut leases = Vec::new();
    for path in LEASES {
        let lease = managed.open_lock_file(path)?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        leases.push(lease);
    }
    let _writer = lock::acquire(&managed, Duration::ZERO)?;
    let state = vault
        .state
        .lock()
        .map_err(|_| AutomationFault::Unavailable)?;
    vault.current(&state)?;
    let head = state
        .head
        .as_ref()
        .ok_or(AutomationFault::Unavailable)?
        .current();
    let mut paths = BTreeSet::from([
        layout::vault_descriptor(),
        layout::current_pointer(),
        AUTOMATION_INTENT.into(),
        AUTOMATION_WITNESS.into(),
    ]);
    let mut next = Some(head.clone());
    while let Some(pointer) = next {
        let path = SegmentedAutomationVault::path(&pointer.manifest_sha256);
        let commit: SegmentedCommit = parse_document(&read(&managed, &path, MAX_SEGMENT_BYTES)?)?;
        next = commit.parent;
        paths.insert(path);
    }
    for name in managed.list("vault/raw/objects")? {
        paths.insert(format!("vault/raw/objects/{name}"));
    }
    for name in managed.list("sync-state")? {
        let path = format!("sync-state/{name}");
        if JOURNALS.contains(&path.as_str()) {
            paths.insert(path);
        } else if !LEASES.contains(&path.as_str()) {
            return Err(AutomationFault::Denied);
        }
    }
    io(platform::restrict_to_owner(destination.path()))?;
    let out = ManagedRoot::new(destination.path(), Faults::none());
    let mut export = OwnerExport {
        format: FORMAT.into(),
        descriptor_hash: digest(&descriptor),
        head,
        files: vec![],
    };
    for path in paths {
        if !allowed(&path) {
            return Err(AutomationFault::Denied);
        }
        let bytes = read(&managed, &path, MAX_FILE)?;
        out.write_new(&path, &bytes)?;
        export.files.push(ExportFile {
            path,
            sha256: sha256(&bytes),
            size_bytes: bytes.len() as u64,
        });
    }
    verify_contents(&out, &export)?;
    let bytes = canonical_bytes(&export)?;
    if bytes.len() > MAX_MANIFEST {
        return Err(AutomationFault::Capacity);
    }
    let mut manifest = io(std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.path().join(MANIFEST)))?;
    use std::io::Write;
    io(manifest.write_all(&bytes))?;
    io(manifest.sync_all())?;
    io(platform::sync_dir(destination.path()))?;
    Ok(export)
}

/// Independent, read-only verification: no source root, leases or indexes needed.
pub fn verify_owner(export_dir: &VerifiedRoot) -> Result<OwnerExport> {
    let mut present = BTreeSet::new();
    inventory(export_dir.path(), export_dir.path(), &mut present)?;
    let file = io(std::fs::File::open(export_dir.path().join(MANIFEST)))?;
    if io(file.metadata())?.len() > MAX_MANIFEST as u64 {
        return Err(AutomationFault::Capacity);
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    io(file.take((MAX_MANIFEST + 1) as u64).read_to_end(&mut bytes))?;
    if bytes.len() > MAX_MANIFEST {
        return Err(AutomationFault::Capacity);
    }
    let export: OwnerExport =
        serde_json::from_slice(&bytes).map_err(|_| AutomationFault::Denied)?;
    if canonical_bytes(&export)? != bytes {
        return Err(AutomationFault::Denied);
    }
    let listed: BTreeSet<_> = export
        .files
        .iter()
        .map(|f| f.path.clone())
        .chain([MANIFEST.into()])
        .collect();
    if listed != present {
        return Err(AutomationFault::Unavailable);
    }
    verify_contents(
        &ManagedRoot::new(export_dir.path(), Faults::none()),
        &export,
    )?;
    Ok(export)
}

/// Verification completes before any target write. Partial restores stay gated.
pub fn restore_owner(
    export_dir: &VerifiedRoot,
    target: &VerifiedRoot,
    clock: &dyn Clock,
) -> Result<OwnerExport> {
    separate(export_dir, target)?;
    let export = verify_owner(export_dir)?;
    empty(target)?;
    io(platform::restrict_to_owner(target.path()))?;
    let out = ManagedRoot::new(target.path(), Faults::none());
    let receipt = RestoreReceipt {
        version: 1,
        descriptor_hash: export.descriptor_hash.clone(),
        head: export.head.clone(),
        export_hash: digest(&export),
        restored_at: Timestamp::from_unix_ms(clock.now_unix_ms())
            .ok_or(AutomationFault::Unavailable)?,
        activated_at: None,
    };
    out.write_new(RESTORE, &canonical_bytes(&receipt)?)?;
    let source = ManagedRoot::new(export_dir.path(), Faults::none());
    for file in export
        .files
        .iter()
        .filter(|f| f.path != layout::current_pointer())
    {
        let bytes = read(&source, &file.path, MAX_FILE)?;
        if sha256(&bytes) != file.sha256 {
            return Err(AutomationFault::Unavailable);
        }
        out.write_new(&file.path, &bytes)?;
    }
    out.write_new(&layout::current_pointer(), &canonical_bytes(&export.head)?)?;
    verify_contents(&out, &export)?;
    Ok(export)
}

/// Called on every normal open/recovery. A malformed receipt also fails closed.
pub(super) fn require_activated(
    root: &ManagedRoot,
    descriptor: &AutomationDescriptor,
) -> Result<()> {
    if let Some(bytes) = root.read_bounded(RESTORE, 16 * 1024)? {
        let receipt: RestoreReceipt =
            serde_json::from_slice(&bytes).map_err(|_| AutomationFault::Denied)?;
        if receipt.version != 1
            || receipt.descriptor_hash != digest(descriptor)
            || receipt.activated_at.is_none()
        {
            return Err(AutomationFault::Denied);
        }
    }
    Ok(())
}

/// Trusted native Owner assertion: the caller established that this backup is
/// current, with no newer deletion/withdrawal/authority state, and stopped the old
/// Host. A different head requires a new restore, never an implicit merge.
pub fn activate_owner(
    target: &VerifiedRoot,
    current_export: &VerifiedRoot,
    clock: &dyn Clock,
) -> Result<()> {
    separate(target, current_export)?;
    let export = verify_owner(current_export)?;
    let root = ManagedRoot::new(target.path(), Faults::none());
    if !io(platform::inspect_acl(target.path()))?.is_owner_only() {
        return Err(AutomationFault::Denied);
    }
    let lease = root.open_lock_file(&layout::host_lock_file())?;
    lease.try_lock().map_err(|_| AutomationFault::Busy)?;
    let _writer = lock::acquire(&root, Duration::ZERO)?;
    let mut receipt: RestoreReceipt = serde_json::from_slice(&read(&root, RESTORE, 16 * 1024)?)
        .map_err(|_| AutomationFault::Denied)?;
    if receipt.version != 1
        || receipt.head != export.head
        || receipt.descriptor_hash != export.descriptor_hash
        || receipt.export_hash != digest(&export)
    {
        return Err(AutomationFault::Conflict);
    }
    verify_contents(&root, &export)?;
    receipt.activated_at =
        Some(Timestamp::from_unix_ms(clock.now_unix_ms()).ok_or(AutomationFault::Unavailable)?);
    root.write_atomic(RESTORE, &canonical_bytes(&receipt)?)?;
    Ok(())
}
