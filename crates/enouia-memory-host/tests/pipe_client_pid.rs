//! ADR-MEM-161 addendum: the server end reads the connected client's PID.
#![cfg(windows)]
use enouia_memory_contract::{hash::sha256, ids::format_uuid_v4, ports::IdSource};
use enouia_memory_host::windows::Pipe;
use enouia_memory_vault::OsIdSource;
use std::time::Duration;

#[test]
fn server_end_reads_the_connected_client_pid_and_client_end_refuses() {
    let id = sha256(format_uuid_v4(OsIdSource.random_16()).as_bytes());
    let mut server = Pipe::listen(&id).unwrap();
    let client = std::thread::spawn(move || Pipe::connect(&id).unwrap());
    server.accept(Duration::from_secs(10)).unwrap();
    let client = client.join().unwrap();
    assert_eq!(server.client_pid().unwrap(), std::process::id());
    assert!(client.client_pid().is_err());
}
