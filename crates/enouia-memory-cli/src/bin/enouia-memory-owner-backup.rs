//! Explicit native Owner surface; never called from MCP or Runtime IPC.
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, VerifiedRoot,
    automation::segmented::backup::{activate_owner, export_owner, restore_owner, verify_owner},
    verify_data_root,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

const USAGE: &str = "usage: enouia-memory-owner-backup export <stopped-root> <empty-export> | verify <export> | restore <export> <empty-root> --confirm-restore | activate <restored-root> <current-export> --confirm-current-backup";
fn root(path: &str) -> Result<VerifiedRoot, String> {
    verify_data_root(Path::new(path), &RootPolicy::default()).map_err(|e| format!("root: {e:?}"))
}
fn run(args: &[String]) -> Result<Value, String> {
    let export = match args {
        [c, a, b] if c == "export" => export_owner(
            &root(a)?,
            &root(b)?,
            Arc::new(SystemClock),
            Arc::new(OsIdSource),
        ),
        [c, a] if c == "verify" => verify_owner(&root(a)?),
        [c, a, b, flag] if c == "restore" && flag == "--confirm-restore" => {
            restore_owner(&root(a)?, &root(b)?, &SystemClock)
        }
        [c, a, b, flag] if c == "activate" && flag == "--confirm-current-backup" => {
            activate_owner(&root(a)?, &root(b)?, &SystemClock)
                .map_err(|e| format!("activate: {e:?}"))?;
            return Ok(json!({"activated": true}));
        }
        _ => return Err(USAGE.into()),
    }
    .map_err(|e| format!("backup: {e:?}"))?;
    Ok(
        json!({"commit_id": export.head.commit_id, "sequence": export.head.sequence, "files": export.files.len(), "valid": true, "activation_required": args[0] == "restore"}),
    )
}
fn main() {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("enouia-memory-owner-backup: {error}");
            std::process::exit(1);
        }
    }
}
