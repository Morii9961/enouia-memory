//! Linux primitives for the VPS server (ADR-MEM-131). "Owner-only" means POSIX
//! mode bits with no group or other access, owned by the effective user, and
//! no POSIX ACL extension. Children are protected by a 0700 directory; the
//! service should still run with umask 077.

use super::{AclEntry, AclPrincipal, AclReport, DriveKind, VolumeInfo};
use std::ffi::CString;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

const ACL_ACCESS: &[u8] = b"system.posix_acl_access\0";
const ACL_DEFAULT: &[u8] = b"system.posix_acl_default\0";

pub fn fill_random(buffer: &mut [u8]) -> io::Result<()> {
    std::fs::File::open("/dev/urandom")?.read_exact(buffer)
}

pub fn replace_durable(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)?;
    if let Some(parent) = to.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

pub fn sync_dir(dir: &Path) -> io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
}

pub fn volume_info(path: &Path) -> io::Result<VolumeInfo> {
    let c = c_path(path)?;
    // SAFETY: `c` is a valid NUL-terminated path; `stat` is plain old data.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut stat) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // Magic numbers from linux/magic.h; network and FUSE mounts are remote.
    let (filesystem, drive) = match (stat.f_type as u64) & 0xffff_ffff {
        0xEF53 => ("ext4", DriveKind::Fixed),
        0x5846_5342 => ("xfs", DriveKind::Fixed),
        0x9123_683E => ("btrfs", DriveKind::Fixed),
        0x2FC1_2FC1 => ("zfs", DriveKind::Fixed),
        0xF2F5_2010 => ("f2fs", DriveKind::Fixed),
        0x0102_1994 => ("tmpfs", DriveKind::Other),
        0x794C_7630 => ("overlay", DriveKind::Other),
        0x6969 | 0xFF53_4D42 | 0xFE53_4D42 | 0x517B | 0x6573_5546 | 0x0BD0_0BD0 => {
            ("network", DriveKind::Remote)
        }
        _ => ("unknown", DriveKind::Other),
    };
    Ok(VolumeInfo {
        filesystem: filesystem.to_owned(),
        drive,
        free_bytes: (stat.f_bavail as u64).saturating_mul(stat.f_bsize as u64),
    })
}

fn has_xattr(path: &CString, name: &[u8]) -> io::Result<bool> {
    // SAFETY: both strings are NUL-terminated; a null buffer of size 0 only
    // asks for the attribute's length.
    let size =
        unsafe { libc::getxattr(path.as_ptr(), name.as_ptr().cast(), std::ptr::null_mut(), 0) };
    if size >= 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENODATA) | Some(libc::ENOTSUP) => Ok(false),
        _ => Err(error),
    }
}

pub fn inspect_acl(path: &Path) -> io::Result<AclReport> {
    let meta = std::fs::metadata(path)?;
    let c = c_path(path)?;
    let mode = meta.mode();
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let owner = match meta.uid() {
        uid if uid == euid => AclPrincipal::CurrentUser,
        0 => AclPrincipal::System,
        _ => AclPrincipal::Other,
    };
    let mut entries = vec![AclEntry {
        principal: owner,
        allow: mode & 0o700 != 0,
        inherited: false,
    }];
    let other = |allow: bool| AclEntry {
        principal: AclPrincipal::Other,
        allow,
        inherited: false,
    };
    entries.push(other(mode & 0o070 != 0));
    entries.push(other(mode & 0o007 != 0));
    // Any POSIX ACL extension can grant named users; treat it as a broad grant.
    entries.push(other(
        has_xattr(&c, ACL_ACCESS)? || (meta.is_dir() && has_xattr(&c, ACL_DEFAULT)?),
    ));
    Ok(AclReport {
        protected: true,
        entries,
    })
}

pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    let meta = std::fs::metadata(path)?;
    // SAFETY: geteuid has no preconditions and cannot fail.
    if meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "owned by another user",
        ));
    }
    let c = c_path(path)?;
    for name in [ACL_ACCESS, ACL_DEFAULT] {
        // SAFETY: both strings are NUL-terminated.
        if unsafe { libc::removexattr(c.as_ptr(), name.as_ptr().cast()) } != 0 {
            let error = io::Error::last_os_error();
            if !matches!(
                error.raw_os_error(),
                Some(libc::ENODATA) | Some(libc::ENOTSUP)
            ) {
                return Err(error);
            }
        }
    }
    let mode = if meta.is_dir() { 0o700 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    if !inspect_acl(path)?.is_owner_only() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "could not restrict to owner",
        ));
    }
    Ok(())
}

pub fn has_cloud_attributes(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enouia-vault-linux-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn restrict_makes_directories_and_files_owner_only() {
        let dir = temp("restrict");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let report = inspect_acl(&dir).unwrap();
        assert!(!report.is_owner_only());
        assert_eq!(report.broad_grants(), 2);
        restrict_to_owner(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        assert!(inspect_acl(&dir).unwrap().is_owner_only());
        let file = dir.join("f");
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(inspect_acl(&file).unwrap().broad_grants(), 2);
        restrict_to_owner(&file).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
        assert!(inspect_acl(&file).unwrap().is_owner_only());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn group_or_other_bits_are_broad_grants() {
        let dir = temp("bits");
        for (mode, grants) in [(0o700, 0), (0o750, 1), (0o701, 1), (0o777, 2), (0o000, 0)] {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
            let report = inspect_acl(&dir).unwrap();
            assert_eq!(report.broad_grants(), grants, "{mode:o}");
            assert_eq!(report.is_owner_only(), mode == 0o700, "{mode:o}");
        }
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn volume_reports_real_free_space_and_a_known_kind() {
        let dir = temp("volume");
        let volume = volume_info(&dir).unwrap();
        assert!(volume.free_bytes > 0 && volume.free_bytes < u64::MAX);
        assert_ne!(volume.drive, DriveKind::Remote);
        assert!(volume_info(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
