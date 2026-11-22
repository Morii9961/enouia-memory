//! ADR-MEM-166: the explicit native launcher and stdio bridge as a library
//! function, so an embedding application (Runtime) can ship its own
//! `enouia-memory-mcp` executable at the same pinned revision.
//! No discovery, client configuration or Vault IO.
use std::ffi::OsString;

/// Any launch, enrollment or transport failure; deliberately without detail.
#[derive(Debug)]
pub struct BridgeFailure;

/// Runs the bridge with `args` (without the program name). The caller reports
/// a failure and exits nonzero.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), BridgeFailure> {
    run_inner(args).map_err(|()| BridgeFailure)
}
fn run_inner(args: impl Iterator<Item = OsString>) -> Result<(), ()> {
    use crate::mcp::native::Client;
    use crate::windows::{Pipe, ProcessPeer};
    use enouia_memory_contract::hash::sha256;
    use enouia_memory_contract::ids::VaultId;
    use enouia_memory_contract::ports::IdSource;
    use enouia_memory_contract::workspace::HostSurface;
    use enouia_memory_vault::OsIdSource;
    use std::io::{BufRead, BufReader, Read};
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            // Client pipe drops first; allow the owned Host to shut down before
            // bounded forced cleanup. Never locate/terminate unrelated processes.
            let until = std::time::Instant::now() + Duration::from_secs(1);
            while self.0.try_wait().ok().flatten().is_none() {
                if std::time::Instant::now() >= until {
                    let _ = self.0.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = self.0.wait();
        }
    }
    let mut args = args;
    let mut options = std::collections::BTreeMap::new();
    while let Some(key) = args.next() {
        let key = key.into_string().map_err(|_| ())?;
        if ![
            "--host-executable",
            "--vault",
            "--vault-id",
            "--host-id",
            "--agent-id",
            "--agent-scopes",
            "--protocol",
            "--pipe",
            "--host-pid",
        ]
        .contains(&key.as_str())
        {
            return Err(());
        }
        if options.insert(key, args.next().ok_or(())?).is_some() {
            return Err(());
        }
    }
    if options.contains_key("--protocol")
        || options.contains_key("--pipe")
        || options.contains_key("--host-pid")
    {
        // ADR-MEM-165: `--host-pid` is optional; without it the bridge enrolls
        // the server end of the connected pipe.
        if options.len() != 3 + usize::from(options.contains_key("--host-pid"))
            || !["--protocol", "--pipe", "--vault-id"]
                .iter()
                .all(|key| options.contains_key(*key))
        {
            return Err(());
        }
        let vault = VaultId::parse(options["--vault-id"].to_str().ok_or(())?).map_err(|_| ())?;
        let pipe =
            enouia_memory_contract::hash::Sha256Hex::parse(options["--pipe"].to_str().ok_or(())?)
                .ok_or(())?;
        let pipe = Pipe::connect(&pipe).map_err(|_| ())?;
        let pid: u32 = match options.get("--host-pid") {
            Some(pid) => pid.to_str().ok_or(())?.parse().map_err(|_| ())?,
            None => pipe.server_pid().map_err(|_| ())?,
        };
        let peer = ProcessPeer::enroll(pid, HostSurface::QuickSearch).map_err(|_| ())?;
        return match options["--protocol"].to_str().ok_or(())? {
            "automation-agent-v1" => {
                let mut client = crate::mcp::agent::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-context-v1" => {
                let mut client = crate::mcp::opening::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-corrections-v1" => {
                let mut client = crate::mcp::corrections::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-supersedes-v1" => {
                let mut client = crate::mcp::supersedes::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-forget-requests-v1" => {
                let mut client = crate::mcp::forget_requests::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-notes-v1" => {
                let mut client = crate::mcp::notes::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-initialized-v1" => {
                let mut client = crate::mcp::initialized::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-qualified-v1" => {
                let mut client = crate::mcp::qualified::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-catalog-v1" => {
                let mut client = crate::mcp::catalog::native::Client::connect(
                    pipe,
                    peer,
                    &vault,
                    Duration::from_secs(10),
                )
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "automation-v2" | "automation-records-v1" => {
                let mut client = if options["--protocol"] == "automation-records-v1" {
                    crate::mcp::v2::native::Client::connect_records(
                        pipe,
                        peer,
                        &vault,
                        Duration::from_secs(10),
                    )
                } else {
                    crate::mcp::v2::native::Client::connect(
                        pipe,
                        peer,
                        &vault,
                        Duration::from_secs(10),
                    )
                }
                .map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            "legacy-v1" => {
                let mut client =
                    Client::connect(pipe, peer, &vault, Duration::from_secs(10)).map_err(|_| ())?;
                client
                    .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
                    .map_err(|_| ())
            }
            _ => Err(()),
        };
    }
    if options.len() != 6 {
        return Err(());
    }
    let vault = VaultId::parse(options["--vault-id"].to_str().ok_or(())?).map_err(|_| ())?;
    let id = sha256(&OsIdSource.random_16());
    let mut command = Command::new(&options["--host-executable"]);
    for key in ["--vault", "--host-id", "--agent-id", "--agent-scopes"] {
        command.arg(key).arg(&options[key]);
    }
    let mut child = Owned(
        command
            .arg("--pipe")
            .arg(id.as_str())
            .arg("--client-pid")
            .arg(std::process::id().to_string())
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ())?,
    );
    let stdout = child.0.stdout.take().ok_or(())?;
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout)
            .take(4097)
            .read_line(&mut line)
            .map(|_| line);
        let _ = tx.send(result);
    });
    let ready = rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| ())?
        .map_err(|_| ())?;
    reader.join().map_err(|_| ())?;
    let ready: serde_json::Value = serde_json::from_str(&ready).map_err(|_| ())?;
    if ready != serde_json::json!({"ready":true,"pid":child.0.id()}) {
        return Err(());
    }
    let host = ProcessPeer::enroll(child.0.id(), HostSurface::QuickSearch).map_err(|_| ())?;
    let mut client = Client::connect(
        Pipe::connect(&id).map_err(|_| ())?,
        host,
        &vault,
        Duration::from_secs(10),
    )
    .map_err(|_| ())?;
    client
        .serve_stdio(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
        .map_err(|_| ())
}
