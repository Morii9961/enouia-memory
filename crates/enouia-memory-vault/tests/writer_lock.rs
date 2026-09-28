//! V01: one writer per Vault across processes. A second writer gets `busy`
//! within the configured wait; a writer that crashes releases the lock
//! (the OS drops it with the handle); two concurrent writers of the same
//! request produce exactly one commit and one replay.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_vault::fault::Faults;
use enouia_memory_vault::fs::ManagedRoot;
use enouia_memory_vault::{Fault, lock};
use std::path::PathBuf;
use std::time::Duration;
use support::{Harness, child_env, open_at, spawn_child, wait_for};

const CHILD: &str = "child_process_entry";

/// Runs only inside a child process started by the tests below.
#[test]
fn child_process_entry() {
    let Some(mode) = child_env("MODE") else {
        return;
    };
    let root = PathBuf::from(child_env("ROOT").unwrap());
    let signal = PathBuf::from(child_env("SIGNAL").unwrap_or_default());
    match mode.as_str() {
        "hold" | "hold-and-abort" => {
            let managed = ManagedRoot::new(&root, Faults::none());
            let _guard = lock::acquire(&managed, Duration::from_secs(10)).unwrap();
            std::fs::write(signal.join("ready"), b"1").unwrap();
            if mode == "hold-and-abort" {
                std::thread::sleep(Duration::from_millis(100));
                std::process::abort();
            }
            wait_for(&signal.join("release"), 30);
        }
        "commit" => {
            let (vault, lifecycle) = open_at(&root, 1, Faults::none(), 10_000);
            wait_for(&signal.join("go"), 30);
            let code = match vault.commit(lifecycle.request(1)) {
                Ok(CommitOutcome::Committed { .. }) => 10,
                Ok(CommitOutcome::Replayed { .. }) => 11,
                Err(_) => 12,
            };
            std::process::exit(code);
        }
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn a_second_writer_in_the_same_process_gets_busy() {
    let harness = Harness::with_commits("lock-local", 0);
    let guard = lock::acquire(harness.vault.managed_root(), Duration::ZERO).unwrap();
    harness.clock.set(harness.lifecycle.time(1));
    let error = harness
        .vault
        .commit(harness.lifecycle.request(1))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::Busy);
    assert_eq!(error.fault, Fault::WriterBusy);
    drop(guard);
    harness.apply(1);
}

#[test]
fn a_writer_in_another_process_blocks_until_it_releases() {
    let harness = Harness::with_commits("lock-process", 0);
    let signal = harness.root.child("signal-dir");
    let mut child = spawn_child(
        CHILD,
        &[
            ("MODE", "hold".into()),
            ("ROOT", harness.root.path().display().to_string()),
            ("SIGNAL", signal.display().to_string()),
        ],
    );
    wait_for(&signal.join("ready"), 30);
    harness.clock.set(harness.lifecycle.time(1));
    let error = harness
        .vault
        .commit(harness.lifecycle.request(1))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::Busy);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 1);
    std::fs::write(signal.join("release"), b"1").unwrap();
    assert!(child.wait().unwrap().success());
    harness.apply(1);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 2);
}

#[test]
fn a_crashed_writer_never_leaves_a_stale_lock() {
    let harness = Harness::with_commits("lock-crash", 0);
    let signal = harness.root.child("signal-dir");
    let mut child = spawn_child(
        CHILD,
        &[
            ("MODE", "hold-and-abort".into()),
            ("ROOT", harness.root.path().display().to_string()),
            ("SIGNAL", signal.display().to_string()),
        ],
    );
    wait_for(&signal.join("ready"), 30);
    assert!(!child.wait().unwrap().success(), "child aborted");
    // The LOCK file still exists; ownership died with the process.
    assert!(harness.root.path().join("vault/LOCK").exists());
    harness.apply(1);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 2);
}

#[test]
fn two_processes_racing_one_request_commit_once() {
    let harness = Harness::with_commits("lock-race", 0);
    let signal = harness.root.child("signal-dir");
    let envs = [
        ("MODE", "commit".to_owned()),
        ("ROOT", harness.root.path().display().to_string()),
        ("SIGNAL", signal.display().to_string()),
    ];
    let mut a = spawn_child(CHILD, &envs);
    let mut b = spawn_child(CHILD, &envs);
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(signal.join("go"), b"1").unwrap();
    let mut codes = [
        a.wait().unwrap().code().unwrap(),
        b.wait().unwrap().code().unwrap(),
    ];
    codes.sort();
    assert_eq!(codes, [10, 11], "one commit and one replay");
    let pin = harness.vault.pin_current().unwrap();
    assert_eq!(pin.sequence, 2);
    assert!(harness.vault.verify(&pin).unwrap().is_clean());
}
