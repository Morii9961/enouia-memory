//! D03/D04 static parts: self-contained dependencies, purity of the contract
//! crate, and Vault paths that never touch the Runtime's Activity data root.
//! Installed-artifact and live-store isolation tests are MV-1 behavior.

use enouia_memory_contract::RecordKind;
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::layout::{self, ACTIVITY_DIR, MANAGED_ROOTS};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line.ends_with("dependencies]");
            continue;
        }
        if in_deps && let Some((name, _)) = line.split_once('=') {
            deps.push(name.trim().trim_end_matches(".workspace").to_owned());
        }
    }
    deps
}

#[test]
fn memory_contract_depends_only_on_serde() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-contract/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(deps, ["serde", "serde_json"]);
}

/// The store adds only the contract crate and, on Windows, `windows-sys`.
#[test]
fn memory_vault_dependencies_are_pinned_and_minimal() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-vault/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-contract",
            "serde",
            "serde_json",
            "windows-sys"
        ]
    );
    assert!(manifest.contains("windows-sys = { version = \"=0.61.2\""));
}

/// The importer adds only pinned, pure-Rust inflate and CRC-32 crates.
#[test]
fn memory_import_dependencies_are_pinned_and_minimal() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-import/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "crc32fast",
            "enouia-memory-contract",
            "enouia-memory-vault",
            "miniz_oxide",
            "serde_json"
        ]
    );
    assert!(manifest.contains("miniz_oxide = { version = \"=0.8.9\""));
    assert!(manifest.contains("crc32fast = { version = \"=1.5.2\""));
}

/// The CLI only composes this repository's crates.
#[test]
fn memory_cli_depends_only_on_this_repository() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-cli/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-context",
            "enouia-memory-contract",
            "enouia-memory-govern",
            "enouia-memory-import",
            "enouia-memory-index",
            "enouia-memory-vault",
            "serde_json"
        ]
    );
}

#[test]
fn memory_context_has_only_local_dependencies() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-context/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-contract",
            "enouia-memory-govern",
            "enouia-memory-index",
            "enouia-memory-vault",
            "serde",
            "serde_json"
        ]
    );
    assert!(!manifest.contains("reqwest") && !manifest.contains("tokio"));
}

/// The index adds only the pinned SQLite binding (bundled source, no
/// default features); governance is a test-only dependency.
#[test]
fn memory_index_dependencies_are_pinned_and_minimal() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-index/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-contract",
            "enouia-memory-govern",
            "enouia-memory-vault",
            "rusqlite",
            "serde_json"
        ]
    );
    assert!(manifest.contains(
        "rusqlite = { version = \"=0.40.2\", default-features = false, features = [\"bundled\"] }"
    ));
    let (normal, dev) = manifest.split_once("[dev-dependencies]").unwrap();
    assert!(!normal.contains("enouia-memory-govern") && dev.contains("enouia-memory-govern"));
}

/// Governance composes the contract and the store only (the importer is a
/// test-only dependency for synthetic sources).
#[test]
fn memory_govern_depends_only_on_contract_and_store() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-govern/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-contract",
            "enouia-memory-import",
            "enouia-memory-vault",
            "serde_json"
        ]
    );
    let (normal, dev) = manifest.split_once("[dev-dependencies]").unwrap();
    assert!(!normal.contains("enouia-memory-import"));
    assert!(dev.contains("enouia-memory-import"));
}

/// The embedded Core that hosts consume (the reference shell, Runtime's
/// adapter, later the MV-8 Host) depends only on this repository's domain
/// crates and Serde. It stays transport-neutral (ADR-MEM-45).
#[test]
fn memory_workspace_depends_only_on_this_repository() {
    let manifest =
        std::fs::read_to_string(repo().join("crates/enouia-memory-workspace/Cargo.toml")).unwrap();
    let mut deps = dependencies(&manifest);
    deps.sort();
    assert_eq!(
        deps,
        [
            "enouia-memory-context",
            "enouia-memory-contract",
            "enouia-memory-govern",
            "enouia-memory-import",
            "enouia-memory-index",
            "enouia-memory-vault",
            "serde",
            "serde_json"
        ]
    );
}

/// Window toolkits and native dialogs belong to host shells, never to a
/// domain crate, so any host can embed the Core (ADR-MEM-45).
#[test]
fn no_domain_crate_depends_on_a_window_toolkit() {
    for entry in std::fs::read_dir(repo().join("crates")).unwrap() {
        let manifest = entry.unwrap().path().join("Cargo.toml");
        let deps = dependencies(&std::fs::read_to_string(&manifest).unwrap());
        for toolkit in ["tauri", "tauri-build", "rfd", "wry", "tao"] {
            assert!(
                !deps.iter().any(|d| d == toolkit),
                "{manifest:?} depends on {toolkit}"
            );
        }
    }
}

/// The repository builds from its own checkout: no manifest may reference a
/// path outside the repository or a Git dependency (e.g. the Runtime repo).
#[test]
fn no_dependency_reaches_outside_this_repository() {
    let root = repo().canonicalize().unwrap();
    let mut manifests = vec![
        repo().join("Cargo.toml"),
        repo().join("apps/workspace/src-tauri/Cargo.toml"),
    ];
    for entry in std::fs::read_dir(repo().join("crates")).unwrap() {
        manifests.push(entry.unwrap().path().join("Cargo.toml"));
    }
    for manifest in manifests {
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(
            !text.contains("git ="),
            "{manifest:?} uses a git dependency"
        );
        assert!(!text.to_lowercase().contains("enouia-runtime") && !text.contains("enouia-common"));
        for line in text.lines().filter(|l| l.contains("path =")) {
            let rel = line
                .split("path =")
                .nth(1)
                .unwrap()
                .trim()
                .trim_matches(|c| c == '"' || c == '}' || c == ' ');
            let target = manifest.parent().unwrap().join(rel.trim_end_matches('"'));
            let target = target
                .canonicalize()
                .unwrap_or_else(|_| panic!("{manifest:?}: {rel} missing"));
            assert!(
                target.starts_with(&root),
                "{manifest:?}: {rel} leaves the repository"
            );
        }
    }
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn contract_source_is_pure_and_repository_independent() {
    let mut files = Vec::new();
    rust_sources(
        &repo().join("crates/enouia-memory-contract/src"),
        &mut files,
    );
    assert!(files.len() >= 20);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for forbidden in [
            "enouia_activity",
            "std::fs",
            "std::net",
            "std::process",
            "std::env",
            "include_str!",
            "include_bytes!",
            "moriium",
            "Moriium/",
            "auth.json",
            "SystemTime",
        ] {
            let code: String = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(!code.contains(forbidden), "{file:?} uses {forbidden}");
        }
    }
}

#[test]
fn vault_layout_stays_inside_memory_roots_and_away_from_activity() {
    let rev = Revision::new(3).unwrap();
    let id = "mem_00000001-0000-4000-8000-000000000001";
    let mut paths = vec![
        layout::current_pointer(),
        layout::commit_manifest("cmt_00000001-0000-4000-8000-000000000001"),
        layout::identity_sidecar("idn_00000001-0000-4000-8000-000000000001", rev),
        layout::session_event("ses_1", "evt_1"),
        layout::staging_dir("op_1"),
        layout::INDEX_FILE.to_owned(),
    ];
    for kind in [
        RecordKind::Memory,
        RecordKind::Identity,
        RecordKind::Source,
        RecordKind::Attachment,
        RecordKind::Project,
        RecordKind::Candidate,
        RecordKind::Session,
        RecordKind::Checkpoint,
        RecordKind::Review,
        RecordKind::Tombstone,
        RecordKind::PurgeReceipt,
        RecordKind::Capsule,
        RecordKind::Inspection,
        RecordKind::Dispatch,
    ] {
        paths.push(layout::record_path(kind, id, rev).expect("stored kind"));
    }
    assert_eq!(
        layout::record_path(RecordKind::ProviderCapabilities, id, rev),
        None
    );
    for path in paths {
        let root = path.split('/').next().unwrap();
        assert!(MANAGED_ROOTS.contains(&root), "{path}");
        assert_ne!(root, ACTIVITY_DIR, "{path}");
        assert!(!path.contains(".."), "{path}");
    }
    assert!(!MANAGED_ROOTS.contains(&ACTIVITY_DIR));
}
