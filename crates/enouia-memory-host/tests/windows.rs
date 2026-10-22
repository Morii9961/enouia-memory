//! Actual local pipe/process tests, never production roots or external peers.
#![cfg(windows)]
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::workspace::HostSurface;
use enouia_memory_host::windows::{Pipe, ProcessPeer};
use std::io;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);
fn id() -> Sha256Hex {
    sha256(
        format!(
            "synthetic-host-pipe-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        )
        .as_bytes(),
    )
}
fn peer(pid: u32) -> ProcessPeer {
    ProcessPeer::enroll(pid, HostSurface::Workspace).unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn pipe_reserves_one_instance_and_handles_client_before_accept() {
    let id = id();
    let mut server = Pipe::listen(&id).unwrap();
    assert!(Pipe::listen(&id).is_err());
    let enrolled = peer(std::process::id());
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let client = scope.spawn(|| {
            let mut client = Pipe::connect(&id).unwrap();
            enrolled.verify_server(&client).unwrap();
            ready_tx.send(()).unwrap();
            client.write_all(b"hello", deadline()).unwrap();
            let mut reply = [0; 5];
            client.read_exact(&mut reply, deadline()).unwrap();
            assert_eq!(&reply, b"world");
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        server.accept(Duration::from_secs(5)).unwrap();
        enrolled.verify_client(&server).unwrap();
        let mut request = [0; 5];
        server.read_exact(&mut request, deadline()).unwrap();
        assert_eq!(&request, b"hello");
        server.write_all(b"world", deadline()).unwrap();
        client.join().unwrap();
    });
    server.disconnect().unwrap();
}

#[test]
fn pending_read_timeout_drains_cancellation_before_the_next_operation() {
    let id = id();
    let mut server = Pipe::listen(&id).unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let client = scope.spawn(move || {
            let mut client = Pipe::connect(&id).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            client.write_all(b"x", deadline()).unwrap();
        });
        server.accept(Duration::from_secs(5)).unwrap();
        let mut byte = [0];
        let error = server
            .read_exact(&mut byte, Instant::now() + Duration::from_millis(25))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        release_tx.send(()).unwrap();
        server.read_exact(&mut byte, deadline()).unwrap();
        assert_eq!(byte, [b'x']);
        client.join().unwrap();
    });
}

struct OwnChild(Child);
impl Drop for OwnChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn kernel_peer_pid_rejects_another_process_and_dead_enrollment_stays_revoked() {
    let id = id();
    let mut server = Pipe::listen(&id).unwrap();
    let mut child = OwnChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "pipe_client_child", "--nocapture"])
            .env("ENOUIA_HOST_PIPE_TEST_ID", id.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let enrolled = peer(child.0.id());
    server.accept(Duration::from_secs(5)).unwrap();
    let wrong = peer(std::process::id());
    assert_eq!(
        wrong.verify_client(&server).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    enrolled.verify_client(&server).unwrap();
    let mut byte = [0];
    server.read_exact(&mut byte, deadline()).unwrap();
    assert_eq!(byte, [b'x']);
    server.write_all(b"y", deadline()).unwrap();
    let until = deadline();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < until,
            "only our synthetic child may be terminated by cleanup"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        enrolled.live().unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(enrolled.verify_client(&server).is_err());
}

#[test]
fn pipe_client_child() {
    let Some(id) = std::env::var_os("ENOUIA_HOST_PIPE_TEST_ID") else {
        return;
    };
    let id = Sha256Hex::parse(&id.to_string_lossy()).unwrap();
    let mut pipe = Pipe::connect(&id).unwrap();
    pipe.write_all(b"x", deadline()).unwrap();
    let mut byte = [0];
    pipe.read_exact(&mut byte, deadline()).unwrap();
    assert_eq!(byte, [b'y']);
}
