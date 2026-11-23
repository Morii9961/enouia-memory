#![cfg(windows)]
//! ADR-MEM-169: several instances of one pipe name and bounded busy waits.
use enouia_memory_contract::{hash::sha256, ports::IdSource};
use enouia_memory_host::windows::Pipe;
use enouia_memory_vault::OsIdSource;
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn shared_instances_serve_parallel_clients_independently() {
    let id = sha256(&OsIdSource.random_16());
    let mut servers: Vec<Pipe> = (0..3).map(|_| Pipe::listen_shared(&id).unwrap()).collect();
    let mut clients: Vec<Pipe> = (0..3).map(|_| Pipe::connect(&id).unwrap()).collect();
    for server in &mut servers {
        server.accept(Duration::from_secs(5)).unwrap();
        assert_eq!(server.client_pid().unwrap(), std::process::id());
    }
    for (n, client) in clients.iter_mut().enumerate() {
        assert_eq!(client.server_pid().unwrap(), std::process::id());
        client.write_all(&[n as u8; 4], deadline()).unwrap();
    }
    // Each client reached exactly one instance; the bytes come back per pair.
    let mut seen = Vec::new();
    for server in &mut servers {
        let mut bytes = [0; 4];
        server.read_exact(&mut bytes, deadline()).unwrap();
        assert!(bytes.iter().all(|b| *b == bytes[0]));
        seen.push(bytes[0]);
        server.write_all(&bytes, deadline()).unwrap();
    }
    seen.sort();
    assert_eq!(seen, vec![0, 1, 2]);
    for (n, client) in clients.iter_mut().enumerate() {
        let mut bytes = [0; 4];
        client.read_exact(&mut bytes, deadline()).unwrap();
        assert_eq!(bytes, [n as u8; 4]);
    }
    // A sole-instance listener cannot join a shared name.
    assert!(Pipe::listen(&id).is_err());
}

#[test]
fn a_busy_name_makes_the_client_wait_for_the_next_instance() {
    let id = sha256(&OsIdSource.random_16());
    let mut first = Pipe::listen_shared(&id).unwrap();
    let _held = Pipe::connect(&id).unwrap();
    first.accept(Duration::from_secs(5)).unwrap();
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| {
            let started = Instant::now();
            let pipe = Pipe::connect(&id);
            (pipe, started.elapsed())
        });
        std::thread::sleep(Duration::from_millis(300));
        let mut second = Pipe::listen_shared(&id).unwrap();
        // The client is returned, not dropped, so it is still open at accept.
        let (client, waited) = waiting.join().unwrap();
        let _client = client.unwrap();
        second.accept(Duration::from_secs(5)).unwrap();
        assert!(waited >= Duration::from_millis(250), "{waited:?}");
        assert!(waited < Duration::from_secs(5), "{waited:?}");
    });
}

#[test]
fn a_sole_listener_still_reserves_its_name() {
    let id = sha256(&OsIdSource.random_16());
    let _sole = Pipe::listen(&id).unwrap();
    assert!(Pipe::listen(&id).is_err());
    assert!(Pipe::listen_shared(&id).is_err());
}
