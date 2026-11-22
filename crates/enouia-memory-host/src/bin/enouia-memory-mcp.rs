//! Explicit native launcher/stdio bridge. No discovery, client config or Vault IO.
//! A thin wrapper over `mcp::bridge::run` (ADR-MEM-166).
#[cfg(not(windows))]
fn main() {
    eprintln!("agent_platform_unsupported");
    std::process::exit(2);
}
#[cfg(windows)]
fn main() {
    if enouia_memory_host::mcp::bridge::run(std::env::args_os().skip(1)).is_err() {
        eprintln!("agent_launch_or_transport_failed");
        std::process::exit(2);
    }
}
