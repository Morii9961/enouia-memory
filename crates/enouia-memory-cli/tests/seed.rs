//! Synthetic installed-CLI bridge and ordinary owner review. No personal data.
use enouia_memory_contract::hash::sha256;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn cli(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_enouia-memory"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr))),
    )
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Fixture, String, String, Value) {
    let root = std::env::temp_dir().join(format!("enouia-seed-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let root = PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\"));
    let vault = root.join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let text = "# Synthetic\nI prefer paper books.\n";
    std::fs::write(root.join("source.md"), text).unwrap();
    let package = json!({"format":"enouia.native-seed-stage/1","account_alias":"synthetic","inputs":[
        {"name":"source.md","sha256":sha256(text.as_bytes()),"sensitivity":"private"}],"seeds":[{
        "seed_id":"synthetic-preference","revision":1,"proposed_type":"preference","content":"Synthetic: prefers paper books.",
        "details":{"title":"Synthetic preference","subject_ids":["sub_00000001-0000-4000-8000-000000000001"],"scope":"synthetic.reading","strength":"explicit"},
        "evidence":[{"input":"source.md","locator":{"kind":"byte_range","start":0,"end":text.len()},"content_hash":sha256(text.as_bytes()),
            "evidence_class":"unknown","speaker_role":"unknown"}],"context":[],"sensitivity":"private","reason":"Synthetic agent seed; pending owner review."}]});
    let file = root.join("package.json");
    std::fs::write(&file, serde_json::to_vec(&package).unwrap()).unwrap();
    (
        Fixture(root),
        vault.display().to_string(),
        file.display().to_string(),
        package,
    )
}

#[test]
fn seed_stages_once_replays_after_review_and_refuses_changed_inputs() {
    let (_fixture, vault, file, mut package) = fixture();
    let (ok, preview) = cli(&["seed-preview", &file]);
    assert!(ok, "{preview}");
    assert_eq!(preview["canonical_writes"], 0);
    let (ok, initialized) = cli(&["init", &vault, "--confirm-new-vault"]);
    assert!(ok, "{initialized}");
    let hash = preview["package_sha256"].as_str().unwrap();
    assert!(
        !cli(&[
            "seed-stage",
            &vault,
            &file,
            "--sha256",
            &"0".repeat(64),
            "--confirm-stage"
        ])
        .0
    );
    let args = [
        "seed-stage",
        &vault,
        &file,
        "--sha256",
        hash,
        "--confirm-stage",
    ];
    let (ok, staged) = cli(&args);
    assert!(ok, "{staged}");
    let (ok, replayed) = cli(&args);
    assert!(ok, "{replayed}");
    assert_eq!(staged, replayed);
    assert_eq!(
        cli(&["candidates", &vault]).1["pending"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        cli(&["memories", &vault]).1["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let checked = cli(&["seed-review-check", &vault]).1;
    assert_eq!(checked["checked"], 1, "{checked}");
    assert!(checked["failed"].as_array().unwrap().is_empty());
    let id = staged["candidates"][0]["candidate_id"].as_str().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_enouia-memory"))
        .args(["review", &vault, id, "1", "accept"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let plan: Value = serde_json::from_str(&line).unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        plan["confirm_code"].as_str().unwrap()
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(cli(&args).0);
    assert_eq!(
        cli(&["memories", &vault]).1["memories"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        cli(&["search", &vault, "paper"]).1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    package["seeds"][0]["content"] = json!("Changed choice must conflict.");
    std::fs::write(&file, serde_json::to_vec(&package).unwrap()).unwrap();
    let changed = cli(&["seed-preview", &file]).1;
    let (ok, error) = cli(&[
        "seed-stage",
        &vault,
        &file,
        "--sha256",
        changed["package_sha256"].as_str().unwrap(),
        "--confirm-stage",
    ]);
    assert!(!ok, "{error}");
    assert_eq!(error["error"], "idempotency_conflict");
    package["seeds"][0]["details"]["subject_ids"][0] =
        json!("sub_00000001-0000-5000-8000-000000000001");
    std::fs::write(&file, serde_json::to_vec(&package).unwrap()).unwrap();
    assert!(
        !cli(&["seed-preview", &file]).0,
        "v5 identifiers must fail before any import"
    );
    std::fs::write(
        Path::new(&file).parent().unwrap().join("source.md"),
        "changed bytes",
    )
    .unwrap();
    assert!(!cli(&["seed-preview", &file]).0);
    assert_eq!(cli(&["verify", &vault]).1["clean"], true);
}
