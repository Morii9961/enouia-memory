//! Explicit native prototype launcher. Never installs, adopts or picks a root.
#[cfg(not(windows))]
fn main() {
    eprintln!("host_platform_unsupported");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    if run().is_err() {
        // Native failures never include a root, SID, request, secret or record.
        eprintln!("host_start_or_transport_failed");
        std::process::exit(2);
    }
}

#[cfg(windows)]
fn run() -> Result<(), ()> {
    use enouia_memory_contract::hash::Sha256Hex;
    use enouia_memory_contract::ids::DeviceId;
    use enouia_memory_contract::workspace::HostSurface;
    use enouia_memory_host::service::{HostService, ServiceFault};
    use enouia_memory_host::transport::{TransportFault, serve_connection};
    use enouia_memory_host::windows::{Pipe, ProcessPeer};
    use enouia_memory_vault::{RootPolicy, verify_data_root};
    use enouia_memory_workspace::Config;
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::time::Duration;

    let mut options = BTreeMap::new();
    let mut once = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(key) = args.next() {
        if key == "--once" {
            if once {
                return Err(());
            }
            once = true;
            continue;
        }
        let key = key.into_string().map_err(|_| ())?;
        if ![
            "--vault",
            "--host-id",
            "--pipe",
            "--client-pid",
            "--surface",
        ]
        .contains(&key.as_str())
        {
            return Err(());
        }
        let value = args.next().ok_or(())?;
        if options.insert(key, value).is_some() {
            return Err(());
        }
    }
    let root = options.remove("--vault").ok_or(())?;
    let text = |key| -> Result<String, ()> {
        options
            .get(key)
            .ok_or(())?
            .clone()
            .into_string()
            .map_err(|_| ())
    };
    let identity = DeviceId::parse(&text("--host-id")?).map_err(|_| ())?;
    let pipe_id = Sha256Hex::parse(&text("--pipe")?).ok_or(())?;
    let client_pid: u32 = text("--client-pid")?.parse().map_err(|_| ())?;
    let surface = match text("--surface")?.as_str() {
        "workspace" => HostSurface::Workspace,
        "quick_search" => HostSurface::QuickSearch,
        _ => return Err(()),
    };
    // This explicit native launch is the owner enrollment point. No value
    // comes from a page/handshake; the adapter retains the actual process object.
    let peer = ProcessPeer::enroll(client_pid, surface).map_err(|_| ())?;
    let root =
        verify_data_root(std::path::Path::new(&root), &RootPolicy::default()).map_err(|_| ())?;
    let host = HostService::open(&root, identity, Config::default()).map_err(|_| ())?;
    let mut pipe = Pipe::listen(&pipe_id).map_err(|_| ())?;
    println!("{{\"ready\":true,\"pid\":{}}}", std::process::id());
    std::io::stdout().flush().map_err(|_| ())?;
    while peer.live().is_ok() {
        match pipe.accept(Duration::from_millis(500)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => return Err(()),
        }
        let result = serve_connection(&host, &mut pipe, &peer, Duration::from_secs(10));
        let _ = pipe.disconnect();
        if matches!(result, Err(TransportFault::Service(ServiceFault::Vault(_)))) {
            return Err(());
        }
        if once {
            break;
        }
    }
    Ok(())
}
