//! Remote MCP service for a ChatGPT connector (ADR-MEM-133).
//!
//! `enouia-memory-remote serve <config.json>` serves one initialized Owner
//! Vault on a loopback address behind a TLS reverse proxy.
//! `enouia-memory-remote approve <config.json>` prints a one-time approval
//! code for the authorize page. Run both as the OS account owning the Vault.
use enouia_memory_contract::{
    ids::PolicyId, json::Revision, next::OwnerReadGrant, time::Timestamp,
};
use enouia_memory_host::mcp::remote::{http1, service};
use enouia_memory_vault::automation::segmented::{OwnerReadGrantPlan, SegmentedAutomationVault};
use enouia_memory_vault::{OsIdSource, RootPolicy, SystemClock, fault::Faults, verify_data_root};
use std::sync::Arc;

fn run(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: enouia-memory-remote <serve|approve|read-status|read-plan|read-set|read-renew-plan|read-revoke-plan|oauth-status|oauth-revoke> <config.json> [arguments]";
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
        "serve" | "read-status" | "read-plan" | "read-set" | "read-renew-plan"
        | "read-revoke-plan" => {
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
