//! The Runtime integration surface (ADR-MEM-45). Enouia Runtime's Windows
//! client embeds the workspace Core at a pinned revision and hosts Memory's
//! local frontend. Every file it depends on is recorded with its digest in
//! `docs/integration/runtime-surface.json`; the aggregate digest must appear
//! in the compatibility log `docs/integration/RUNTIME.md`. A change to the
//! surface therefore fails here until the manifest is regenerated and the
//! change is logged for Runtime in the same commit.
//!
//! This only forces an in-repository acknowledgment. It never reads the
//! Runtime checkout, and it cannot prove that Runtime has adopted a change.

use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::workspace::COMMANDS;
use serde_json::Value;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "docs/integration/runtime-surface.json";
const LOG: &str = "docs/integration/RUNTIME.md";
const REGENERATE: &str = "run `python tools/integration/runtime_surface.py --write`, then add a \
     row with the new aggregate to docs/integration/RUNTIME.md describing what Runtime must adopt";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(repo().join(MANIFEST)).unwrap()).unwrap()
}

/// SHA-256 over the file bytes with CRLF normalized to LF, so the digest
/// does not depend on the checkout's line endings.
fn digest(rel: &str) -> String {
    let bytes = std::fs::read(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    let mut lf = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        lf.push(bytes[i]);
        i += 1;
    }
    sha256(&lf).as_str().to_owned()
}

fn files_under(rel: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(repo().join(rel)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let child = format!("{rel}/{name}");
        if entry.file_type().unwrap().is_dir() {
            files_under(&child, out);
        } else {
            out.push(child);
        }
    }
}

fn recorded() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = manifest()["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_owned(),
                f["sha256"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    files.sort();
    files
}

/// SHA-256 over `<path>\t<sha256>\n` lines sorted by path.
fn aggregate(files: &[(String, String)]) -> String {
    let mut text = String::new();
    for (path, sha) in files {
        text.push_str(&format!("{path}\t{sha}\n"));
    }
    sha256(text.as_bytes()).as_str().to_owned()
}

#[test]
fn every_surface_file_matches_its_recorded_digest() {
    let changed: Vec<String> = recorded()
        .into_iter()
        .filter(|(path, sha)| digest(path) != *sha)
        .map(|(path, _)| path)
        .collect();
    assert!(
        changed.is_empty(),
        "the Runtime integration surface changed: {changed:?}; {REGENERATE}"
    );
}

#[test]
fn covered_directories_are_recorded_completely() {
    let recorded: Vec<String> = recorded().into_iter().map(|(p, _)| p).collect();
    let mut missing = Vec::new();
    for dir in manifest()["coveredDirectories"].as_array().unwrap() {
        let mut found = Vec::new();
        files_under(dir.as_str().unwrap(), &mut found);
        missing.extend(found.into_iter().filter(|f| !recorded.contains(f)));
    }
    assert!(
        missing.is_empty(),
        "new files in the Runtime integration surface: {missing:?}; {REGENERATE}"
    );
}

#[test]
fn recorded_commands_are_the_workspace_commands() {
    let manifest = manifest();
    let commands: Vec<&str> = manifest["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(commands, COMMANDS, "{REGENERATE}");
}

#[test]
fn the_compatibility_log_records_the_current_surface() {
    let files = recorded();
    let current = aggregate(&files);
    assert_eq!(
        manifest()["aggregate"].as_str(),
        Some(current.as_str()),
        "{REGENERATE}"
    );
    let log = std::fs::read_to_string(repo().join(LOG)).unwrap();
    assert!(
        log.contains(&current),
        "docs/integration/RUNTIME.md has no entry for surface {current}; {REGENERATE}"
    );
}
