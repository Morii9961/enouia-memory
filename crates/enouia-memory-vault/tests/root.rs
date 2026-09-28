//! V07: an unverified data root is never enabled, and managed writes cannot
//! leave the verified root through a junction or other reparse point.
//! Every directory here is a synthetic temporary directory; no real sync
//! client, network share, or user folder is touched.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_vault::root::RootRejection;
use enouia_memory_vault::{Fault, RootPolicy, verify_data_root};
use std::path::{Path, PathBuf};
use std::process::Command;
use support::{Harness, TempRoot};

fn verify(path: &Path) -> Result<(), RootRejection> {
    verify_data_root(path, &RootPolicy::default()).map(|_| ())
}

fn junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("cmd available");
    assert!(status.status.success(), "mklink /J failed");
}

#[test]
fn a_plain_local_temp_directory_is_accepted() {
    let root = TempRoot::new("root-ok");
    let verified = verify_data_root(root.path(), &RootPolicy::default()).unwrap();
    assert!(verified.filesystem.eq_ignore_ascii_case("NTFS") || verified.filesystem == "ReFS");
    assert!(verified.free_bytes > 0);
}

#[test]
fn malformed_remote_and_missing_roots_are_rejected() {
    let root = TempRoot::new("root-shapes");
    assert_eq!(
        verify(Path::new(r"relative\vault")),
        Err(RootRejection::NotAbsolute)
    );
    assert_eq!(
        verify(Path::new(r"\\server\share\vault")),
        Err(RootRejection::NetworkPath)
    );
    assert_eq!(
        verify(Path::new(r"\\.\C:\vault")),
        Err(RootRejection::NetworkPath)
    );
    assert_eq!(
        verify(Path::new(r"\\?\UNC\server\share\v")),
        Err(RootRejection::NetworkPath)
    );
    assert_eq!(
        verify(&root.path().join("absent")),
        Err(RootRejection::Missing)
    );
    let file = root.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(verify(&file), Err(RootRejection::NotADirectory));
    let dir = root.child("a");
    let indirect = dir.join("..").join("a");
    assert_eq!(verify(&indirect), Err(RootRejection::NotCanonical));
}

#[test]
fn junctions_anywhere_on_the_path_are_rejected() {
    let root = TempRoot::new("root-junction");
    let target = root.child("target");
    std::fs::create_dir_all(target.join("inner")).unwrap();
    let link = root.path().join("link");
    junction(&link, &target);
    assert_eq!(verify(&link), Err(RootRejection::ReparsePoint));
    assert_eq!(
        verify(&link.join("inner")),
        Err(RootRejection::ReparsePoint)
    );
    assert!(verify(&target.join("inner")).is_ok());
}

#[test]
fn sync_folders_repositories_system_locations_and_small_volumes_are_rejected() {
    let root = TempRoot::new("root-policy");
    let synced = root.child("OneDrive - Synthetic").join("vault");
    std::fs::create_dir_all(&synced).unwrap();
    assert_eq!(verify(&synced), Err(RootRejection::CloudSyncFolder));
    for name in ["Dropbox", "坚果云"] {
        let dir = root.child(name);
        assert_eq!(verify(&dir), Err(RootRejection::CloudSyncFolder), "{name}");
    }
    let custom = root.child("synced-by-policy");
    let policy = RootPolicy {
        sync_roots: vec![custom.clone()],
        ..RootPolicy::default()
    };
    let inside: PathBuf = custom.join("vault");
    std::fs::create_dir_all(&inside).unwrap();
    assert_eq!(
        verify_data_root(&inside, &policy).map(|_| ()),
        Err(RootRejection::CloudSyncFolder)
    );
    let repo = root.child("checkout");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let in_repo = repo.join("data");
    std::fs::create_dir_all(&in_repo).unwrap();
    assert_eq!(verify(&in_repo), Err(RootRejection::InsideRepository));
    let this_repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let this_repo = PathBuf::from(
        std::fs::canonicalize(this_repo)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches(r"\\?\"),
    );
    assert_eq!(verify(&this_repo), Err(RootRejection::InsideRepository));
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        assert_eq!(
            verify(Path::new(&program_files)),
            Err(RootRejection::SystemLocation)
        );
    }
    let greedy = RootPolicy {
        min_free_bytes: u64::MAX,
        ..RootPolicy::default()
    };
    assert_eq!(
        verify_data_root(root.path(), &greedy).map(|_| ()),
        Err(RootRejection::InsufficientSpace)
    );
    let fat_only = RootPolicy {
        filesystems: vec!["FAT32".to_owned()],
        ..RootPolicy::default()
    };
    assert_eq!(
        verify_data_root(root.path(), &fat_only).map(|_| ()),
        Err(RootRejection::UnsupportedFilesystem)
    );
}

#[test]
fn a_junction_inside_the_vault_cannot_redirect_a_commit() {
    let harness = Harness::with_commits("root-escape", 0);
    let outside = TempRoot::new("root-escape-outside");
    let records = harness.root.path().join("vault").join("records");
    std::fs::create_dir_all(&records).unwrap();
    junction(&records.join("source"), outside.path());
    harness.clock.set(harness.lifecycle.time(1));
    let error = harness
        .vault
        .commit(harness.lifecycle.request(1))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::PermissionDenied);
    assert_eq!(error.fault, Fault::UnsafePath("reparse point"));
    assert_eq!(
        std::fs::read_dir(outside.path()).unwrap().count(),
        0,
        "nothing was written through the junction"
    );
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 1);
    // Removing the junction (the link only) lets the same request commit.
    std::fs::remove_dir(records.join("source")).unwrap();
    harness.apply(1);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 2);
}
