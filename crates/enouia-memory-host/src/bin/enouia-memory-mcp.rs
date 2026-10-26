//! Explicit native launcher/stdio bridge. No discovery, client config or Vault IO.
#[cfg(not(windows))]
fn main() {
    eprintln!("agent_platform_unsupported");
    std::process::exit(2);
}
#[cfg(windows)]
fn main() {
    if run().is_err() {
        eprintln!("agent_launch_or_transport_failed");
        std::process::exit(2);
    }
}
#[cfg(windows)]
fn run() -> Result<(), ()> {
    use enouia_memory_contract::hash::sha256;
    use enouia_memory_contract::ids::VaultId;
    use enouia_memory_contract::ports::IdSource;
    use enouia_memory_contract::workspace::HostSurface;
    use enouia_memory_host::mcp::native::Client;
    use enouia_memory_host::windows::{Pipe, ProcessPeer};
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
    let mut args = std::env::args_os().skip(1);
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
        ]
        .contains(&key.as_str())
        {
            return Err(());
        }
        if options.insert(key, args.next().ok_or(())?).is_some() {
            return Err(());
        }
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
