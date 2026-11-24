//! Remote MCP service for a ChatGPT connector (ADR-MEM-133).
//!
//! `enouia-memory-remote serve <config.json>` serves one initialized Owner
//! Vault on a loopback address behind a TLS reverse proxy.
//! `enouia-memory-remote approve <config.json>` prints a one-time approval
//! code for the authorize page. Run both as the OS account owning the Vault.
//! `enouia-memory-remote backup <config.json> <backup-dir>` exports the
//! stopped Vault into a new UTC-stamped directory and verifies it
//! (ADR-MEM-188).
use enouia_memory_contract::{
    common::{ActorRef, ActorType},
    foundation::Clock,
    ids::PolicyId,
    json::Revision,
    next::OwnerReadGrant,
    ports::IdSource,
    time::Timestamp,
};
use enouia_memory_host::mcp::remote::{http1, service};
use enouia_memory_vault::automation::segmented::{
    OwnerReadGrantPlan, SegmentedAutomationVault,
    backup::{export_owner, verify_owner},
};
use enouia_memory_vault::{OsIdSource, RootPolicy, SystemClock, fault::Faults, verify_data_root};
use std::sync::Arc;

fn run(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: enouia-memory-remote <serve|approve|backup|read-check|read-status|read-draft|read-plan|read-set|read-renew-plan|read-revoke-plan|read-refresh-plan|oauth-status|oauth-revoke> <config.json> [arguments]";
    let [command, path, rest @ ..] = args else {
        return Err(USAGE.into());
    };
    let bytes = std::fs::read(path).map_err(|e| format!("config: {e}"))?;
    let config: service::ServiceConfig =
        serde_json::from_slice(&bytes).map_err(|e| format!("config: {e}"))?;
    match command.as_str() {
        "approve" if rest.is_empty() => {
            let code = service::approve(&config, &OsIdSource, &SystemClock)?;
            println!("{code}");
            Ok(())
        }
        "backup" => {
            let [dir] = rest else {
                return Err("backup <config> <backup-dir>".into());
            };
            let root = verify_data_root(&config.vault_root, &RootPolicy::default())
                .map_err(|e| format!("vault root: {e:?}"))?;
            let now = Timestamp::from_unix_ms(SystemClock.now_unix_ms())
                .ok_or("clock out of range")?
                .to_string();
            let name: String = now[..19]
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == 'T')
                .chain(['Z'])
                .collect();
            let path = std::path::Path::new(dir).join(name);
            std::fs::create_dir(&path).map_err(|e| format!("backup dir: {e}"))?;
            let export = verify_data_root(&path, &RootPolicy::default())
                .map_err(|e| format!("backup dir: {e:?}"))?;
            let done = export_owner(&root, &export, Arc::new(SystemClock), Arc::new(OsIdSource))
                .map_err(|e| {
                    // Only an empty directory is removed; a partial export stays.
                    let _ = std::fs::remove_dir(&path);
                    format!(
                        "export: {e:?}; Busy means the service or another process holds the Vault"
                    )
                })?;
            let checked = verify_owner(&export).map_err(|e| format!("verify: {e:?}"))?;
            if checked.head != done.head {
                return Err("verify: the export head differs from the Vault head".into());
            }
            println!(
                "{}",
                serde_json::json!({
                    "export": path,
                    "commit_id": done.head.commit_id,
                    "sequence": done.head.sequence,
                    "files": done.files.len(),
                    "verified": true,
                })
            );
            Ok(())
        }
        "oauth-status" | "oauth-revoke" => {
            let client = match (command.as_str(), rest) {
                ("oauth-status", []) => None,
                ("oauth-revoke", [id, confirm]) if confirm == "--confirm-revoke-client" => Some(id.as_str()),
                _ => return Err("oauth-status <config>; oauth-revoke <config> <client-id> --confirm-revoke-client".into()),
            };
            println!(
                "{}",
                service::oauth_admin(&config, client, &OsIdSource, &SystemClock)?
            );
            Ok(())
        }
        "serve" | "read-check" | "read-status" | "read-draft" | "read-plan" | "read-set"
        | "read-renew-plan" | "read-revoke-plan" | "read-refresh-plan" => {
            let root = verify_data_root(&config.vault_root, &RootPolicy::default())
                .map_err(|e| format!("vault root: {e:?}"))?;
            let vault = SegmentedAutomationVault::open(
                &root,
                config.host_id.clone(),
                Arc::new(SystemClock),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .map_err(|e| format!("vault: {e:?}"))?;
            if command == "read-check" && rest.is_empty() {
                let result = service::read_check(Arc::new(vault), &config, &SystemClock)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
                );
                return Ok(());
            }
            let owner = vault.descriptor().created_by.clone();
            let output = match (command.as_str(), rest) {
                ("read-status", []) => Some(
                    serde_json::to_value(
                        vault
                            .owner_read_status(&owner)
                            .map_err(|e| format!("read status: {e:?}"))?,
                    )
                    .map_err(|e| e.to_string())?,
                ),
                // ADR-MEM-176: draft the connector's following grant from this
                // config, so Morii reviews one plan instead of writing JSON.
                ("read-draft", [until, out]) => {
                    let status = vault
                        .owner_read_status(&owner)
                        .map_err(|e| format!("read status: {e:?}"))?;
                    let until = Timestamp::parse(until).map_err(|_| "invalid expiry")?;
                    let grant = OwnerReadGrant {
                        grant_id: PolicyId::from_random(OsIdSource.random_16()),
                        revision: Revision::new(1).ok_or("revision")?,
                        vault_id: vault.descriptor().vault_id.clone(),
                        principal: ActorRef {
                            actor_type: ActorType::Agent,
                            actor_id: config.agent_id.clone(),
                        },
                        subject_id: config.subject_id.clone(),
                        project_id: config.project_id.clone(),
                        // Every recorded revision of the listed policies; the
                        // grant follows the subject's other and later windows.
                        policies: status
                            .policy_refs
                            .into_iter()
                            .filter(|r| config.policy_ids.contains(&r.policy_id))
                            .collect(),
                        source_ids: config.source_ids.clone(),
                        max_sensitivity: config.max_sensitivity,
                        source_read: config.source_read,
                        active: true,
                        valid_from: status.now,
                        valid_until: until,
                        // ADR-MEM-184: correction and forget requests,
                        // confirmed by Morii in Runtime; they need source text.
                        allow_requests: config.source_read,
                        follow_new_sources: true,
                        purpose: Some("chatgpt_connector".into()),
                    };
                    let id = grant.grant_id.clone();
                    let plan = vault
                        .plan_owner_read_grant(&owner, grant)
                        .map_err(|e| format!("read plan: {e:?}"))?;
                    // Written as UTF-8 by this command, never through a shell
                    // redirect that may change the encoding; never overwritten.
                    let bytes = serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?;
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(out)
                        .and_then(|mut f| std::io::Write::write_all(&mut f, &bytes))
                        .map_err(|e| format!("plan file: {e}"))?;
                    println!("{}", serde_json::json!({"plan_file": out, "grant_id": id}));
                    return Ok(());
                }
                ("read-plan", [file]) => {
                    let grant: OwnerReadGrant =
                        serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    Some(
                        serde_json::to_value(
                            vault
                                .plan_owner_read_grant(&owner, grant)
                                .map_err(|e| format!("read plan: {e:?}"))?,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }
                ("read-renew-plan", [id, until]) | ("read-revoke-plan", [id, until])
                    if command == "read-renew-plan" || until == "--revoke" =>
                {
                    let id = PolicyId::parse(id).map_err(|e| e.to_string())?;
                    let mut grant = vault
                        .owner_read_grant(&owner, &id)
                        .map_err(|e| format!("read grant: {e:?}"))?;
                    grant.revision =
                        Revision::new(grant.revision.get() + 1).ok_or("revision limit")?;
                    if command == "read-renew-plan" {
                        if !grant.active {
                            return Err("a revoked grant requires a new explicit read-plan".into());
                        }
                        let until = Timestamp::parse(until).map_err(|_| "invalid expiry")?;
                        if until <= grant.valid_until {
                            return Err("renewal must extend the expiry".into());
                        }
                        grant.valid_until = until;
                    } else {
                        grant.active = false;
                    }
                    Some(
                        serde_json::to_value(
                            vault
                                .plan_owner_read_grant(&owner, grant)
                                .map_err(|e| format!("read plan: {e:?}"))?,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }
                // ADR-MEM-168: show what the grant misses and plan the refresh;
                // `read-set` with the plan file still needs Morii's confirmation.
                ("read-refresh-plan", [id]) => {
                    let id = PolicyId::parse(id).map_err(|e| e.to_string())?;
                    let coverage = vault
                        .owner_read_grant_coverage(&owner, &id)
                        .map_err(|e| format!("read coverage: {e:?}"))?;
                    eprintln!(
                        "uncovered policies: {}, uncovered sources: {}",
                        coverage.uncovered_policies.len(),
                        coverage.uncovered_sources.len()
                    );
                    let grant = coverage
                        .refreshed
                        .ok_or("nothing to refresh, or the grant is revoked or at its limits")?;
                    Some(
                        serde_json::to_value(
                            vault
                                .plan_owner_read_grant(&owner, grant)
                                .map_err(|e| format!("read plan: {e:?}"))?,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }
                ("read-set", [file, confirm]) if confirm == "--confirm-owner-read" => {
                    let plan: OwnerReadGrantPlan =
                        serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    Some(serde_json::to_value(vault.commit_owner_read_grant(&owner, &plan).map_err(|e| format!("read set: {e:?}; unknown publication requires native recovery and exact revision lookup, never a new plan"))?).map_err(|e| e.to_string())?)
                }
                ("serve", []) => None,
                _ => return Err(USAGE.into()),
            };
            if let Some(output) = output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
                );
                return Ok(());
            }
            let listener = http1::bind_loopback(config.bind).map_err(|e| format!("bind: {e}"))?;
            service::serve(
                Arc::new(vault),
                &config,
                &listener,
                &SystemClock,
                &OsIdSource,
                || false,
            )
        }
        _ => Err(USAGE.into()),
    }
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("enouia-memory-remote: {error}");
        std::process::exit(1);
    }
}
