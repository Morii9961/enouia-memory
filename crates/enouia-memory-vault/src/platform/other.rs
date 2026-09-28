//! Non-Windows fallbacks so the crate builds and its logic can be tested
//! elsewhere. The supported production platform is Windows (NTFS).

use super::{AclReport, DriveKind, VolumeInfo};
use std::io::{self, Read};
use std::path::Path;

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

pub fn volume_info(_path: &Path) -> io::Result<VolumeInfo> {
    Ok(VolumeInfo {
        filesystem: "unknown".to_owned(),
        drive: DriveKind::Other,
        free_bytes: u64::MAX,
    })
}

pub fn inspect_acl(_path: &Path) -> io::Result<AclReport> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "ACL inspection is Windows-only",
    ))
}

pub fn restrict_to_owner(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "ACL changes are Windows-only",
    ))
}

pub fn has_cloud_attributes(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
