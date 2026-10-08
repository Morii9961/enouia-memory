//! Managed file operations confined to a verified data root (MV-1.1).
//!
//! Every path is a relative, `/`-separated name built from generated IDs and
//! hashes under one of `layout::MANAGED_ROOTS`. Before touching a file, each
//! existing directory between the root and the target is checked: a reparse
//! point (junction, symlink, mount point) there stops the operation, so a
//! managed write cannot be redirected outside the root.

use crate::error::{Fault, Result, VaultError};
use crate::fault::{FaultPoint, Faults};
use crate::platform;
use enouia_memory_contract::layout::MANAGED_ROOTS;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Reserved device names that Windows resolves anywhere in a path.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A managed relative path: first segment a managed root, every segment of
/// `[A-Za-z0-9_.-]`, not `.`/`..`, not a reserved device name, no trailing dot.
pub fn is_managed_path(rel: &str) -> bool {
    let mut segments = rel.split('/');
    let Some(first) = segments.next() else {
        return false;
    };
    MANAGED_ROOTS.contains(&first)
        && rel.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.ends_with('.')
                && !RESERVED.contains(
                    &segment
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_lowercase()
                        .as_str(),
                )
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}

#[derive(Clone, Debug)]
pub struct ManagedRoot {
    root: PathBuf,
    faults: Faults,
}

impl ManagedRoot {
    pub fn new(root: &Path, faults: Faults) -> Self {
        Self {
            root: root.to_path_buf(),
            faults,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn faults(&self) -> &Faults {
        &self.faults
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf> {
        if !is_managed_path(rel) {
            return Err(VaultError::unsafe_path("unmanaged name"));
        }
        let mut path = self.root.clone();
        path.extend(rel.split('/'));
        Ok(path)
    }

    /// Check every existing component below the root; none may be a reparse
    /// point, and every directory component must be a real directory.
    fn check_components(&self, rel: &str) -> Result<()> {
        let mut path = self.root.clone();
        for segment in rel.split('/') {
            path.push(segment);
            match fs::symlink_metadata(&path) {
                Ok(meta) if platform::is_reparse_point(&meta) => {
                    return Err(VaultError::unsafe_path("reparse point"));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn parent_rel(rel: &str) -> Option<&str> {
        rel.rsplit_once('/').map(|(parent, _)| parent)
    }

    /// Create the parent directories of `rel`, one checked level at a time.
    pub fn ensure_dir(&self, rel: &str) -> Result<()> {
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        match fs::create_dir_all(&path) {
            Ok(()) => {}
            Err(e) => return Err(e.into()),
        }
        self.check_components(rel)?;
        if !fs::symlink_metadata(&path)?.is_dir() {
            return Err(VaultError::unsafe_path("not a directory"));
        }
        Ok(())
    }

    fn ensure_parent(&self, rel: &str) -> Result<()> {
        match Self::parent_rel(rel) {
            Some(parent) => self.ensure_dir(parent),
            None => Ok(()),
        }
    }

    pub fn exists(&self, rel: &str) -> Result<bool> {
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        Ok(fs::symlink_metadata(path).is_ok())
    }

    pub fn read(&self, rel: &str) -> Result<Option<Vec<u8>>> {
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Bound allocation before reading bytes, including a concurrently grown
    /// file. Existing managed-path and reparse checks still apply.
    pub(crate) fn read_bounded(&self, rel: &str, max_bytes: usize) -> Result<Option<Vec<u8>>> {
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let over_budget = || {
            VaultError::new(
                enouia_memory_contract::MemoryErrorCode::BudgetExceeded,
                Fault::Contract(vec!["object.read_budget"]),
            )
        };
        let limit = u64::try_from(max_bytes)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(over_budget)?;
        if file.metadata()?.len() > max_bytes as u64 {
            return Err(over_budget());
        }
        let mut bytes = Vec::new();
        file.take(limit).read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(over_budget());
        }
        Ok(Some(bytes))
    }

    /// Names of the entries of a managed directory (empty if absent).
    pub fn list(&self, rel: &str) -> Result<Vec<String>> {
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    fn write_file(&self, path: &Path, bytes: &[u8], create_new: bool) -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true);
        if create_new {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        let mut file = options.open(path)?;
        self.faults.io(FaultPoint::WriteBytes)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }

    /// Write a file that must not exist yet, flushed before returning.
    pub fn write_new(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        self.ensure_parent(rel)?;
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        self.write_file(&path, bytes, true)?;
        if let Some(parent) = path.parent() {
            platform::sync_dir(parent)?;
        }
        Ok(())
    }

    /// Replace a file atomically: a flushed temporary sibling, then one
    /// same-directory write-through rename. Readers see old or new bytes.
    pub fn write_atomic(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        self.ensure_parent(rel)?;
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        let suffix = hex(&platform::random_bytes::<8>()?);
        let tmp = path.with_file_name(format!(
            "{}.tmp-{suffix}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        let result = self.write_file(&tmp, bytes, true).and_then(|()| {
            self.faults.io(FaultPoint::BeforeRename)?;
            platform::replace_durable(&tmp, &path).map_err(VaultError::from)
        });
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// Append one line and flush it. A torn final line is tolerated by readers.
    pub fn append_line(&self, rel: &str, line: &[u8]) -> Result<()> {
        self.ensure_parent(rel)?;
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut buffer = Vec::with_capacity(line.len() + 1);
        buffer.extend_from_slice(line);
        buffer.push(b'\n');
        file.write_all(&buffer)?;
        file.sync_all()?;
        Ok(())
    }

    /// Move a managed file or directory to another managed name (same volume).
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.ensure_parent(to)?;
        let (src, dst) = (self.resolve(from)?, self.resolve(to)?);
        self.check_components(from)?;
        self.check_components(to)?;
        platform::replace_durable(&src, &dst)?;
        Ok(())
    }

    /// Remove a managed directory tree. Only staging may be removed this way.
    pub fn remove_staging(&self, rel: &str) -> Result<()> {
        if !rel.starts_with("vault/staging/") {
            return Err(VaultError::unsafe_path("not staging"));
        }
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete one file of purged content (MV-3.4). Only record, event, and
    /// object files and quarantined copies may be deleted, and only by the
    /// purge of a published purge tombstone. Returns whether a file was there.
    pub fn remove_purged(&self, rel: &str) -> Result<bool> {
        const PURGEABLE: &[&str] = &[
            "vault/records/",
            "vault/session-events/",
            "vault/raw/objects/",
            "vault/assets/objects/",
            "vault/session-content/objects/",
            "vault/orphans/",
        ];
        if !PURGEABLE.iter().any(|p| rel.starts_with(p)) {
            return Err(VaultError::unsafe_path("not purgeable"));
        }
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        match fs::remove_file(&path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    platform::sync_dir(parent)?;
                }
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Open a managed file for locking (created if absent, never truncated).
    pub fn open_lock_file(&self, rel: &str) -> Result<File> {
        self.ensure_parent(rel)?;
        let path = self.resolve(rel)?;
        self.check_components(rel)?;
        Ok(OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?)
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Map an unsafe-path failure to its fault for callers that only need that.
pub fn is_unsafe(error: &VaultError) -> bool {
    matches!(error.fault, Fault::UnsafePath(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_names_reject_traversal_and_devices() {
        assert!(is_managed_path("vault/records/memory/mem_1/1.json"));
        assert!(is_managed_path("config/restore-state.json"));
        for bad in [
            "",
            "activity/x",
            "vault/../x",
            "vault/./x",
            "vault//x",
            "vault/x.",
            "vault/CON",
            "vault/nul.json",
            "vault/a:b",
            "vault/a\\b",
            "/vault/x",
            "C:/vault",
        ] {
            assert!(!is_managed_path(bad), "{bad}");
        }
    }
}
