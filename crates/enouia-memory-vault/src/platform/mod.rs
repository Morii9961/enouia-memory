//! Operating-system primitives behind a small, testable surface.

#[cfg(not(windows))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use other as imp;
#[cfg(windows)]
use windows as imp;

use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Remote,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VolumeInfo {
    pub filesystem: String,
    pub drive: DriveKind,
    pub free_bytes: u64,
}

/// Principal classes only: SIDs themselves are never reported or logged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AclPrincipal {
    CurrentUser,
    System,
    Administrators,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AclEntry {
    pub principal: AclPrincipal,
    pub allow: bool,
    pub inherited: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclReport {
    /// Inheritance from parent directories is blocked.
    pub protected: bool,
    pub entries: Vec<AclEntry>,
}

impl AclReport {
    /// Only the current user, SYSTEM, and Administrators are granted access,
    /// and parents cannot add grants later.
    pub fn is_owner_only(&self) -> bool {
        self.protected
            && self
                .entries
                .iter()
                .all(|e| !e.allow || e.principal != AclPrincipal::Other)
            && self
                .entries
                .iter()
                .any(|e| e.allow && e.principal == AclPrincipal::CurrentUser)
    }

    /// Allow entries for principals other than the user, SYSTEM, Administrators.
    pub fn broad_grants(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.allow && e.principal == AclPrincipal::Other)
            .count()
    }
}

/// Cryptographically secure random bytes from the OS.
pub fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    imp::fill_random(&mut bytes)?;
    Ok(bytes)
}

/// Atomically replace `to` with `from` (same directory, hence same volume),
/// durable when this returns.
pub fn replace_durable(from: &Path, to: &Path) -> io::Result<()> {
    imp::replace_durable(from, to)
}

/// Make directory entries created in `dir` durable.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    imp::sync_dir(dir)
}

pub fn volume_info(path: &Path) -> io::Result<VolumeInfo> {
    imp::volume_info(path)
}

pub fn inspect_acl(path: &Path) -> io::Result<AclReport> {
    imp::inspect_acl(path)
}

pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    imp::restrict_to_owner(path)
}

pub fn has_cloud_attributes(metadata: &std::fs::Metadata) -> bool {
    imp::has_cloud_attributes(metadata)
}

pub fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    imp::is_reparse_point(metadata)
}
