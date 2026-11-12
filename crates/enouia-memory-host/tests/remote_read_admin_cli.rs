#![cfg(any(windows, target_os = "linux"))]
#[path = "support/owner_read_grant_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, foundation::Clock, time::Timestamp};
use enouia_memory_vault::SystemClock;
use fixture::*;
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn cli(config: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_enouia-memory-remote"))
        .arg(args[0])
        .arg(config)
        .args(&args[1..])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn native_cli_previews_sets_renews_revokes_and_reports_without_source_text() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let files = Env::new();
    let mut g = read_grant(&e, &p);
    let now = SystemClock.now_unix_ms();
    g.valid_from = Timestamp::from_unix_ms(now - 1000).unwrap();
    g.valid_until = Timestamp::from_unix_ms(now + 86_400_000).unwrap();
    g.source_read = false;
    let config = files.path.join("remote.json");
    std::fs::write(&config, json!({
        "vault_root":e.root.path(),"host_id":e.descriptor.host_id,"state_dir":files.path.join("oauth"),"bind":"127.0.0.1:0",
        "resource":"https://memory.example/mcp","issuer":"https://memory.example","redirect_prefixes":["https://chatgpt.com/"],"origins":[],
        "agent_id":g.principal.actor_id,"subject_id":g.subject_id,"project_id":g.project_id,"policy_ids":[p.policy_id],"source_ids":g.source_ids,
        "max_sensitivity":"private","source_read":false,"read_grant_id":g.grant_id,"profile":value("attention/profile.json")
    }).to_string()).unwrap();
    let grant_path = files.path.join("grant.json");
    let plan_path = files.path.join("reviewed-plan.json");
    std::fs::write(&grant_path, serde_json::to_vec(&g).unwrap()).unwrap();
    let baseline = v.snapshot().unwrap();
    drop(v);
    let before = cli(&config, &["read-status"]);
    assert_eq!(before["snapshot"], json!(baseline));
    let plan = cli(&config, &["read-plan", grant_path.to_str().unwrap()]);
    assert_eq!(
        cli(&config, &["read-status"])["snapshot"],
        before["snapshot"]
    );
    std::fs::write(&plan_path, plan.to_string()).unwrap();
    let refused = Command::new(env!("CARGO_BIN_EXE_enouia-memory-remote"))
        .args([
            "read-set",
            config.to_str().unwrap(),
            plan_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    cli(
        &config,
        &[
            "read-set",
            plan_path.to_str().unwrap(),
            "--confirm-owner-read",
        ],
    );
    let status = cli(&config, &["read-status"]);
    assert_eq!(status["grants"][0], json!(g));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut service_config: Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    service_config["bind"] = json!(listener.local_addr().unwrap().to_string());
    std::fs::write(&config, service_config.to_string()).unwrap();
    let checked = cli(&config, &["read-check"]);
    assert_eq!(checked["read_ready"], true);
    assert_eq!(checked["authority_mode"], "owner_read_grant");
    assert_eq!(checked["snapshot"], status["snapshot"]);
    assert_eq!(checked["scope"]["source_read"], false);
    assert!(!checked.to_string().contains("moonlight"));
    assert!(!files.path.join("oauth").exists());
    assert_eq!(
        cli(&config, &["read-status"])["snapshot"],
        status["snapshot"]
    );
    let until = Timestamp::from_unix_ms(now + 2 * 86_400_000).unwrap();
    let renewed = cli(
        &config,
        &["read-renew-plan", g.grant_id.as_str(), until.as_str()],
    );
    assert_eq!(renewed["grant"]["revision"], 2);
    assert_eq!(renewed["grant"]["source_read"], false);
    std::fs::write(&plan_path, renewed.to_string()).unwrap();
    cli(
        &config,
        &[
            "read-set",
            plan_path.to_str().unwrap(),
            "--confirm-owner-read",
        ],
    );
    let revoked = cli(
        &config,
        &["read-revoke-plan", g.grant_id.as_str(), "--revoke"],
    );
    std::fs::write(&plan_path, revoked.to_string()).unwrap();
    cli(
        &config,
        &[
            "read-set",
            plan_path.to_str().unwrap(),
            "--confirm-owner-read",
        ],
    );
    let final_status = cli(&config, &["read-status"]);
    assert_eq!(final_status["grants"][0]["active"], false);
    assert_eq!(final_status["grants"][0]["revision"], 3);
    let checked = Command::new(env!("CARGO_BIN_EXE_enouia-memory-remote"))
        .args(["read-check", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    let refused = Command::new(env!("CARGO_BIN_EXE_enouia-memory-remote"))
        .args([
            "read-renew-plan",
            config.to_str().unwrap(),
            g.grant_id.as_str(),
            until.as_str(),
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert_eq!(
        cli(&config, &["read-status"])["snapshot"],
        final_status["snapshot"]
    );
    assert!(!final_status.to_string().contains("moonlight"));
    assert_eq!(cli(&config, &["oauth-status"])["clients"], json!([]));
}
