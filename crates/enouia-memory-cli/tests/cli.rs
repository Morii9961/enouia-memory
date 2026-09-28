//! D03 behavior and the MV-1 local entry point end to end. The built binary
//! is copied out of the repository into a temporary "install" directory and
//! run with a cleared environment (only the Windows system directory on
//! PATH, working directory outside any checkout). It must initialize,
//! write, verify, export, restore, and recover a synthetic Vault alone.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join("enouia-memory-cli-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let path = base.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn dir(&self, name: &str) -> String {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path.display().to_string()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Installed {
    exe: PathBuf,
    cwd: PathBuf,
}

impl Installed {
    fn run(&self, args: &[&str]) -> (i32, Value) {
        let mut command = Command::new(&self.exe);
        command.env_clear().current_dir(&self.cwd).args(args);
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", &root);
            command.env("PATH", Path::new(&root).join("System32"));
        }
        let output = command.output().expect("installed binary starts");
        let text = String::from_utf8_lossy(&output.stdout);
        let value = serde_json::from_str(text.trim()).unwrap_or(Value::Null);
        (output.status.code().unwrap_or(-1), value)
    }
}

#[test]
fn the_installed_binary_runs_the_whole_local_lifecycle_alone() {
    let temp = Temp::new("lifecycle");
    let install = PathBuf::from(temp.dir("install"));
    let exe = install.join("enouia-memory.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_enouia-memory"), &exe).unwrap();
    let cli = Installed {
        exe,
        cwd: install.clone(),
    };

    // A source checkout is never accepted as a data root.
    let (code, out) = cli.run(&["check-root", env!("CARGO_MANIFEST_DIR")]);
    assert_eq!(code, 2);
    assert_eq!(out["reason"], "InsideRepository");

    let root = temp.dir("vault-root");
    let (code, _) = cli.run(&["init", &root]);
    assert_eq!(code, 1, "init needs an explicit confirmation");
    let (code, init) = cli.run(&["init", &root, "--confirm-new-vault"]);
    assert_eq!(code, 0, "{init}");
    assert_eq!(init["sequence"], 1);
    let (_, status) = cli.run(&["status", &root]);
    assert_eq!(status["state"], "healthy");
    assert_eq!(status["network_allowed"], true);

    let text = "（合成）我用命令行记下这句话。";
    let (code, _) = cli.run(&[
        "assert",
        &root,
        "--text",
        text,
        "--confirm-text",
        "不同的文字",
    ]);
    assert_eq!(code, 1, "confirmation must repeat the exact text");
    let (code, first) = cli.run(&[
        "assert",
        &root,
        "--text",
        text,
        "--confirm-text",
        text,
        "--key",
        "k1",
    ]);
    assert_eq!(code, 0, "{first}");
    let (_, again) = cli.run(&[
        "assert",
        &root,
        "--text",
        text,
        "--confirm-text",
        text,
        "--key",
        "k1",
    ]);
    assert_eq!(again["source_id"], first["source_id"]);
    assert_eq!(again["replayed"], true);
    let (_, verify) = cli.run(&["verify", &root]);
    assert_eq!(verify["clean"], true);
    assert_eq!(verify["records_checked"], 2);

    let export = temp.dir("export");
    let (code, exported) = cli.run(&["export", &root, &export]);
    assert_eq!(code, 0, "{exported}");
    let (_, checked) = cli.run(&["verify-export", &export]);
    assert_eq!(checked["valid"], true);
    let restored = temp.dir("restored");
    let (code, out) = cli.run(&["restore", &export, &restored, "--confirm-restore"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["network_disabled_until_reconciled"], true);
    let (_, status) = cli.run(&["status", &restored]);
    assert_eq!(status["state"], "healthy");
    assert_eq!(status["network_allowed"], false);
    assert_eq!(status["head_sequence"], 2);

    // Damage CURRENT: status says recovering; the owner adopts the newest
    // journal-evidenced commit explicitly.
    std::fs::write(Path::new(&root).join("vault").join("CURRENT"), b"{").unwrap();
    let (_, status) = cli.run(&["status", &root]);
    assert_eq!(status["state"], "recovering");
    let (_, recovery) = cli.run(&["recovery", &root]);
    let newest = recovery["candidates"][0]["commit_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(recovery["candidates"][0]["evidence"], "publish_journal");
    let (code, _) = cli.run(&["adopt", &root, &newest]);
    assert_eq!(code, 1, "adoption needs the commit ID repeated");
    let (code, adopted) = cli.run(&["adopt", &root, &newest, "--confirm", &newest]);
    assert_eq!(code, 0, "{adopted}");
    let (_, status) = cli.run(&["status", &root]);
    assert_eq!(status["state"], "healthy");

    // Owner-only ACL on the root.
    let (_, protect) = cli.run(&["protect", &root, "--confirm-owner-only"]);
    assert_eq!(protect["owner_only"], true);
    let (_, acl) = cli.run(&["acl", &root]);
    assert_eq!(acl["broad_grants"], 0);
}

#[test]
fn output_never_echoes_record_text() {
    let temp = Temp::new("echo");
    let install = PathBuf::from(temp.dir("install"));
    let exe = install.join("enouia-memory.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_enouia-memory"), &exe).unwrap();
    let cli = Installed { exe, cwd: install };
    let root = temp.dir("root");
    cli.run(&["init", &root, "--confirm-new-vault"]);
    let text = "SENTINEL-CLI-合成-不回显";
    let (_, out) = cli.run(&["assert", &root, "--text", text, "--confirm-text", text]);
    for command in [
        vec!["status", &root],
        vec!["verify", &root],
        vec!["recovery", &root],
    ] {
        let (_, value) = cli.run(&command);
        assert!(!value.to_string().contains("SENTINEL"), "{command:?}");
    }
    assert!(!out.to_string().contains("SENTINEL"));
}
