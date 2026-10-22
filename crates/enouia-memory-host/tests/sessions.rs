//! Synthetic native admission, never an OS-authenticated connection claim.
use enouia_memory_contract::common::{ActorRef, ActorType};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::host::PrimaryBinding;
use enouia_memory_contract::host::session::ClientHello;
use enouia_memory_contract::ids::{DeviceId, PrincipalId, VaultId};
use enouia_memory_contract::json::{Revision, SchemaVersion};
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::workspace::HostSurface;
use enouia_memory_host::{
    MAX_CONNECTIONS, NativeAdmission, SESSION_TTL_MS, SessionFault, SessionGate, compatibility_hash,
};
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const NOW: i64 = 1_790_000_000_000;
fn owner(n: u32) -> ActorRef {
    ActorRef {
        actor_id: PrincipalId::parse(&format!("prn_{n:08x}-0000-4000-8000-000000000001")).unwrap(),
        actor_type: ActorType::Owner,
    }
}
fn vault(n: u32) -> VaultId {
    VaultId::parse(&format!("vlt_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn hello() -> ClientHello {
    ClientHello {
        schema_version: SchemaVersion,
        protocol_version: 1,
        workspace_version: 1,
        vault_id: vault(1),
        compatibility_hash: compatibility_hash(),
    }
}
fn gate(clock: &Arc<FakeClock>, seed: u64) -> SessionGate {
    SessionGate::new(
        vault(1),
        owner(1).actor_id,
        PrimaryBinding {
            host_id: DeviceId::parse("dev_00000001-0000-4000-8000-000000000001").unwrap(),
            primary_epoch: Revision::new(1).unwrap(),
        },
        clock.clone(),
        &SequentialIdSource::new(seed),
    )
}
fn native(surface: HostSurface) -> NativeAdmission {
    NativeAdmission::from_authenticated_owner(&owner(1), surface).unwrap()
}
fn packet(command: &str) -> Value {
    json!({"schemaVersion":1,"requestId":"req_00000001-0000-4000-8000-000000000001",
        "command":command,"idempotencyKey":null,"arguments":{}})
}
fn search() -> Value {
    let mut p = packet("memory_search");
    p["arguments"] =
        json!({"query":"synthetic","includeHistorical":false,"cursor":null,"limit":20});
    p
}

#[test]
fn handshake_checks_native_owner_exact_versions_compatibility_and_vault() {
    let clock = Arc::new(FakeClock::new(NOW));
    let gate = gate(&clock, 1);
    let mut agent = owner(1);
    agent.actor_type = ActorType::Agent;
    assert!(matches!(
        NativeAdmission::from_authenticated_owner(&agent, HostSurface::Workspace),
        Err(SessionFault::Unauthenticated)
    ));
    assert!(matches!(
        gate.connect(
            NativeAdmission::from_authenticated_owner(&owner(2), HostSurface::Workspace).unwrap(),
            &hello()
        ),
        Err(SessionFault::Unauthenticated)
    ));
    for change in ["protocol", "workspace", "hash", "vault"] {
        let mut h = hello();
        let expected = match change {
            "protocol" => {
                h.protocol_version = 2;
                SessionFault::Incompatible
            }
            "workspace" => {
                h.workspace_version = 2;
                SessionFault::Incompatible
            }
            "hash" => {
                h.compatibility_hash = sha256(b"other schema");
                SessionFault::Incompatible
            }
            _ => {
                h.vault_id = vault(2);
                SessionFault::WrongVault
            }
        };
        assert!(
            matches!(gate.connect(native(HostSurface::Workspace), &h), Err(e) if e == expected)
        );
    }
    let (welcome, connection) = gate
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    assert_eq!(welcome.expires_at.unix_ms(), NOW + SESSION_TTL_MS);
    assert_eq!(welcome.surface, HostSurface::Workspace);
    assert_eq!(
        gate.dispatch(&connection, &packet("workspace_status"), |s| s),
        Ok(HostSurface::Workspace)
    );
}

#[test]
fn surface_and_packet_validation_happen_before_core_dispatch() {
    let clock = Arc::new(FakeClock::new(NOW));
    let gate = gate(&clock, 2);
    let (_, connection) = gate
        .connect(native(HostSurface::QuickSearch), &hello())
        .unwrap();
    assert_eq!(
        gate.dispatch(&connection, &search(), |s| s),
        Ok(HostSurface::QuickSearch)
    );
    assert_eq!(
        gate.dispatch(&connection, &packet("review_confirm"), |_| panic!(
            "scope bypass"
        )),
        Err::<(), _>(SessionFault::ScopeDenied)
    );
    let mut forged = search();
    forged["surface"] = json!("workspace");
    assert_eq!(
        gate.dispatch(&connection, &forged, |_| panic!("forged field")),
        Err::<(), _>(SessionFault::InvalidPacket)
    );
    let (_, owner_connection) = gate
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    assert_eq!(
        gate.dispatch(
            &owner_connection,
            &json!({"command":"workspace_status"}),
            |_| panic!("malformed packet")
        ),
        Err::<(), _>(SessionFault::InvalidPacket)
    );
}

#[test]
fn lock_revocation_reconnect_and_host_restart_never_revive_old_connections() {
    let clock = Arc::new(FakeClock::new(NOW));
    let first = gate(&clock, 3);
    let (welcome, old) = first
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    let (_, overlay) = first
        .connect(native(HostSurface::QuickSearch), &hello())
        .unwrap();
    // Lock invalidates all handles before invoking Core, even on Core failure.
    assert_eq!(
        first.dispatch(&old, &packet("vault_lock"), |_| Err::<(), _>(
            "synthetic Core error"
        )),
        Ok(Err("synthetic Core error"))
    );
    assert_eq!(
        first.dispatch(&overlay, &search(), |_| ()),
        Err(SessionFault::Locked)
    );
    assert!(matches!(
        first.connect(native(HostSurface::Workspace), &hello()),
        Err(SessionFault::Locked)
    ));
    first.unlock().unwrap();
    assert_eq!(
        first.dispatch(&old, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
    let (_, fresh) = first
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    first.revoke_all().unwrap();
    assert_eq!(
        first.dispatch(&fresh, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
    let restarted = gate(&clock, 4);
    let (new_welcome, _) = restarted
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    assert_ne!(welcome.instance_id, new_welcome.instance_id);
    assert_eq!(
        restarted.dispatch(&old, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
    drop(first);
    assert_eq!(
        restarted.dispatch(&old, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
}

#[test]
fn session_limits_expiry_and_dropped_connections_do_not_evict_active_peers() {
    let clock = Arc::new(FakeClock::new(NOW));
    let gate = gate(&clock, 5);
    let mut connections = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        connections.push(
            gate.connect(native(HostSurface::Workspace), &hello())
                .unwrap()
                .1,
        );
    }
    assert!(matches!(
        gate.connect(native(HostSurface::Workspace), &hello()),
        Err(SessionFault::Capacity)
    ));
    drop(connections.pop());
    let (_, replacement) = gate
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    assert_eq!(
        gate.dispatch(&connections[0], &packet("workspace_status"), |_| ()),
        Ok(())
    );
    clock.set(NOW + SESSION_TTL_MS);
    assert_eq!(
        gate.dispatch(&replacement, &packet("workspace_status"), |_| panic!(
            "expired backend"
        )),
        Err::<(), _>(SessionFault::Expired)
    );
    assert_eq!(
        gate.dispatch(&replacement, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
    assert!(
        gate.connect(native(HostSurface::Workspace), &hello())
            .is_ok()
    );
}

#[test]
fn clock_regression_or_invalid_time_revokes_all_until_native_unlock() {
    for time in [NOW - 1, i64::MAX] {
        let clock = Arc::new(FakeClock::new(NOW));
        let gate = gate(&clock, 6);
        let (_, connection) = gate
            .connect(native(HostSurface::Workspace), &hello())
            .unwrap();
        clock.set(time);
        let expected = if time < NOW {
            SessionFault::ClockRegression
        } else {
            SessionFault::InvalidTime
        };
        assert_eq!(
            gate.dispatch(&connection, &packet("workspace_status"), |_| ()),
            Err(expected)
        );
        clock.set(NOW);
        assert_eq!(
            gate.dispatch(&connection, &packet("workspace_status"), |_| ()),
            Err(SessionFault::Locked)
        );
        gate.unlock().unwrap();
        assert_eq!(
            gate.dispatch(&connection, &packet("workspace_status"), |_| ()),
            Err(SessionFault::Revoked)
        );
        assert!(
            gate.connect(native(HostSurface::Workspace), &hello())
                .is_ok()
        );
    }
}

#[test]
fn revocation_waits_for_synchronous_dispatch_then_old_handle_cannot_enter_again() {
    let clock = Arc::new(FakeClock::new(NOW));
    let gate = Arc::new(gate(&clock, 7));
    let (_, connection) = gate
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let dispatch_gate = gate.clone();
        let connection_ref = &connection;
        let dispatch = scope.spawn(move || {
            dispatch_gate.dispatch(connection_ref, &packet("workspace_status"), |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let gate = gate.clone();
        let revoke = scope.spawn(move || {
            attempt_tx.send(()).unwrap();
            gate.revoke_all().unwrap();
            done_tx.send(()).unwrap();
        });
        attempt_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release_tx.send(()).unwrap();
        assert_eq!(dispatch.join().unwrap(), Ok(()));
        revoke.join().unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    assert_eq!(
        gate.dispatch(&connection, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Revoked)
    );
}

#[test]
fn a_panicking_native_backend_leaves_the_gate_unavailable_without_fallback() {
    let clock = Arc::new(FakeClock::new(NOW));
    let gate = gate(&clock, 8);
    let (_, connection) = gate
        .connect(native(HostSurface::Workspace), &hello())
        .unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), _> = gate.dispatch(&connection, &packet("workspace_status"), |_| {
            panic!("synthetic backend panic")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(
        gate.dispatch(&connection, &packet("workspace_status"), |_| ()),
        Err(SessionFault::Unavailable)
    );
    assert_eq!(gate.unlock(), Err(SessionFault::Unavailable));
}
