//! Data root verification (MV-1.1, V07). A Vault is only enabled on a root
//! that is an absolute, canonical, local fixed-disk directory on NTFS/ReFS,
//! outside cloud-sync folders, source repositories, and system locations,
//! with no reparse point anywhere on its path and enough free space.
//!
//! These checks read metadata only. They never create, move, or open files
//! under the candidate root, and they do not contact any sync provider.

use crate::platform::{self, DriveKind};

use std::path::{Component, Path, PathBuf, Prefix};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootRejection {
    NotAbsolute,
    /// UNC, device (`\\.\`), or mapped network path.
    NetworkPath,
    Missing,
    NotADirectory,
    /// The path is not its own canonical form (8.3 short name, `subst`, …).
    NotCanonical,
    /// A directory on the path is a symlink, junction, or other reparse point.
    ReparsePoint,
    NotLocalFixedDisk,
    UnsupportedFilesystem,
    /// Inside a cloud-sync folder or carrying cloud-file attributes.
    CloudSyncFolder,
    /// Inside a Git working tree (a source repository is never a Vault).
    InsideRepository,
    /// Under Program Files or the Windows directory.
    SystemLocation,
    InsufficientSpace,
    /// Metadata could not be read.
    Unreadable,
}

#[derive(Clone, Debug)]
pub struct RootPolicy {
    pub min_free_bytes: u64,
    /// Accepted filesystems (case-insensitive). Empty means any (non-Windows).
    pub filesystems: Vec<String>,
    /// Folders whose contents a sync client uploads. Defaults come from the
    /// OneDrive environment variables; tests may add synthetic ones.
    pub sync_roots: Vec<PathBuf>,
}

impl Default for RootPolicy {
    fn default() -> Self {
        let sync_roots = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
            .iter()
            .filter_map(std::env::var_os)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .collect();
        Self {
            min_free_bytes: 256 * 1024 * 1024,
            filesystems: if cfg!(windows) {
                vec!["NTFS".to_owned(), "ReFS".to_owned()]
            } else {
                Vec::new()
            },
            sync_roots,
        }
    }
}

/// Directory names that sync clients create; matched case-insensitively
/// against each path component (prefix match covers "OneDrive - Org").
const SYNC_NAME_PREFIXES: &[&str] = &[
    "onedrive",
    "dropbox",
    "google drive",
    "googledrive",
    "icloud drive",
    "iclouddrive",
    "box sync",
    "nutstore",
    "坚果云",
    "baidunetdisk",
    "百度网盘",
];

#[derive(Clone, Debug)]
pub struct VerifiedRoot {
    path: PathBuf,
    pub filesystem: String,
    pub free_bytes: u64,
}

impl VerifiedRoot {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Strip the `\\?\` verbatim prefix that `canonicalize` adds on Windows.
fn strip_verbatim(path: &Path) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        None => Some(path.to_path_buf()),
        Some(rest) => {
            let bytes = rest.as_bytes();
            let disk = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
            disk.then(|| PathBuf::from(rest))
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let text = p.to_string_lossy().replace('/', "\\");
        let text = text.trim_end_matches('\\').to_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    norm(a) == norm(b)
}

fn is_network_or_device(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        }
        _ => false,
    }
}

fn under(path: &Path, base: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    let (p, b) = (lower(path), lower(base));
    let b = b.trim_end_matches('\\');
    !b.is_empty() && (p == b || p.starts_with(&format!("{b}\\")))
}

pub fn verify_data_root(path: &Path, policy: &RootPolicy) -> Result<VerifiedRoot, RootRejection> {
    if is_network_or_device(path) {
        return Err(RootRejection::NetworkPath);
    }
    if !path.is_absolute() {
        return Err(RootRejection::NotAbsolute);
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| RootRejection::Missing)?;
    if platform::is_reparse_point(&meta) {
        return Err(RootRejection::ReparsePoint);
    }
    if !meta.is_dir() {
        return Err(RootRejection::NotADirectory);
    }
    // Every ancestor: no reparse point (junction, symlink, cloud placeholder
    // folder) and no cloud-file attributes.
    for ancestor in path.ancestors().skip(1) {
        if ancestor.parent().is_none() {
            break; // the volume root itself
        }
        let meta = std::fs::symlink_metadata(ancestor).map_err(|_| RootRejection::Unreadable)?;
        if platform::is_reparse_point(&meta) {
            return Err(RootRejection::ReparsePoint);
        }
        if platform::has_cloud_attributes(&meta) {
            return Err(RootRejection::CloudSyncFolder);
        }
    }
    if platform::has_cloud_attributes(&meta) {
        return Err(RootRejection::CloudSyncFolder);
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| RootRejection::Unreadable)?;
    let canonical = strip_verbatim(&canonical).ok_or(RootRejection::NetworkPath)?;
    if !same_path(&canonical, path) {
        return Err(RootRejection::NotCanonical);
    }
    for component in canonical.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy().to_lowercase();
            if SYNC_NAME_PREFIXES.iter().any(|p| name.starts_with(p)) {
                return Err(RootRejection::CloudSyncFolder);
            }
        }
    }
    if policy.sync_roots.iter().any(|root| under(&canonical, root)) {
        return Err(RootRejection::CloudSyncFolder);
    }
    for var in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
        "windir",
    ] {
        if let Some(base) = std::env::var_os(var)
            && under(&canonical, Path::new(&base))
        {
            return Err(RootRejection::SystemLocation);
        }
    }
    if canonical.ancestors().any(|a| a.join(".git").exists()) {
        return Err(RootRejection::InsideRepository);
    }
    let volume = platform::volume_info(&canonical).map_err(|_| RootRejection::Unreadable)?;
    match volume.drive {
        DriveKind::Remote => return Err(RootRejection::NetworkPath),
        DriveKind::Fixed => {}
        DriveKind::Other if !cfg!(windows) => {}
        _ => return Err(RootRejection::NotLocalFixedDisk),
    }
    if !policy.filesystems.is_empty()
        && !policy
            .filesystems
            .iter()
            .any(|fs| fs.eq_ignore_ascii_case(&volume.filesystem))
    {
        return Err(RootRejection::UnsupportedFilesystem);
    }
    if volume.free_bytes < policy.min_free_bytes {
        return Err(RootRejection::InsufficientSpace);
    }
    Ok(VerifiedRoot {
        path: canonical,
        filesystem: volume.filesystem,
        free_bytes: volume.free_bytes,
    })
}
