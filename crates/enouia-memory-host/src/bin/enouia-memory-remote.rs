//! Remote MCP service for a ChatGPT connector (ADR-MEM-133).
//!
//! `enouia-memory-remote serve <config.json>` serves one initialized Owner
//! Vault on a loopback address behind a TLS reverse proxy.
//! `enouia-memory-remote approve <config.json>` prints a one-time approval
//! code for the authorize page. Run both as the OS account owning the Vault.
use enouia_memory_host::mcp::remote::{http1, service};
use enouia_memory_vault::automation::segmented::SegmentedAutomationVault;
use enouia_memory_vault::{OsIdSource, RootPolicy, SystemClock, fault::Faults, verify_data_root};
use std::sync::Arc;

fn run(args: &[String]) -> Result<(), String> {
    let [command, path] = args else {
        return Err("usage: enouia-memory-remote <serve|approve> <config.json>".into());
    };
    let bytes = std::fs::read(path).map_err(|e| format!("config: {e}"))?;
    let config: service::ServiceConfig =
        serde_json::from_slice(&bytes).map_err(|e| format!("config: {e}"))?;
    match command.as_str() {
        "approve" => {
            let code = service::approve(&config, &OsIdSource, &SystemClock)?;
            println!("{code}");
            Ok(())
        }
        "serve" => {
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
        _ => Err("usage: enouia-memory-remote <serve|approve> <config.json>".into()),
    }
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("enouia-memory-remote: {error}");
        std::process::exit(1);
    }
}
