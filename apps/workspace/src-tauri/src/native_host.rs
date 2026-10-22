//! Explicit native launch for the reference shell, never a default/fallback.
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::session::ClientHello;
use enouia_memory_contract::ids::{DeviceId, VaultId};
use enouia_memory_contract::json::SchemaVersion;
use enouia_memory_contract::ports::IdSource;
use enouia_memory_host::broker::{Broker, Fault, Ticket};
use enouia_memory_host::windows::{Pipe, ProcessPeer};
use enouia_memory_vault::OsIdSource;
use enouia_memory_workspace::HostSurface;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);
const FAILED: &str = "native_host_start_failed";
struct Launch {
    executable: PathBuf,
    root: PathBuf,
    identity: DeviceId,
    vault: VaultId,
}
impl Launch {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = BTreeMap::new();
        let mut args = args.iter().skip(1);
        while let Some(key) = args.next() {
            if ![
                "--host-executable",
                "--vault",
                "--host-id",
                "--host-vault-id",
                "--hotkey-key",
            ]
            .contains(&key.as_str())
            {
                return Err(FAILED.to_owned());
            }
            let value = args.next().ok_or(FAILED)?;
            if value.starts_with("--") || options.insert(key.as_str(), value.as_str()).is_some() {
                return Err(FAILED.to_owned());
            }
        }
        let get = |key| options.get(key).copied().ok_or_else(|| FAILED.to_owned());
        let executable = PathBuf::from(get("--host-executable")?);
        let root = PathBuf::from(get("--vault")?);
        if !executable.is_absolute() || !root.is_absolute() {
            return Err(FAILED.to_owned());
        }
        Ok(Self {
            executable,
            root,
            identity: DeviceId::parse(get("--host-id")?).map_err(|_| FAILED.to_owned())?,
            vault: VaultId::parse(get("--host-vault-id")?).map_err(|_| FAILED.to_owned())?,
        })
    }
}
struct Session {
    broker: Broker,
    child: Child,
}
pub struct NativeHost {
    session: Mutex<Session>,
    pipe: Sha256Hex,
    hello: ClientHello,
}
fn fault(e: Fault) -> String {
    match e {
        Fault::Stale => "host_stale_connection",
        Fault::NotReady => "host_not_ready",
        Fault::ScopeDenied => "permission_denied",
        Fault::InvalidPacket => "invalid_request",
        Fault::Transport(_) => "host_disconnected",
        Fault::UncertainCall(_) => "host_outcome_unknown",
    }
    .to_owned()
}
impl NativeHost {
    pub fn start(args: &[String]) -> Result<Self, String> {
        let launch = Launch::parse(args)?;
        let pipe = sha256(&OsIdSource.random_16());
        let mut child = Command::new(&launch.executable)
            .arg("--vault")
            .arg(&launch.root)
            .arg("--host-id")
            .arg(launch.identity.as_str())
            .arg("--pipe")
            .arg(pipe.as_str())
            .arg("--client-pid")
            .arg(std::process::id().to_string())
            .args(["--surface", "workspace"])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| FAILED.to_owned())?;
        let startup = (|| {
            let stdout = child.stdout.take().ok_or(FAILED)?;
            let (tx, rx) = mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let mut line = Vec::new();
                let read = BufReader::new(stdout)
                    .take(4097)
                    .read_until(b'\n', &mut line);
                let _ = tx.send((read, line));
            });
            let (read, line) = rx.recv_timeout(TIMEOUT).map_err(|_| FAILED)?;
            read.map_err(|_| FAILED)?;
            if line.len() > 4096
                || line.last() != Some(&b'\n')
                || serde_json::from_slice::<Value>(&line).map_err(|_| FAILED)?
                    != json!({"ready":true,"pid":child.id()})
            {
                return Err(FAILED);
            }
            Ok(())
        })();
        if startup.is_err() {
            // Only our just-created child, before any page work was admitted.
            let _ = child.kill();
            let _ = child.wait();
            return Err(FAILED.to_owned());
        }
        let host = Self {
            session: Mutex::new(Session {
                broker: Broker::new(HostSurface::Workspace),
                child,
            }),
            pipe,
            hello: ClientHello {
                schema_version: SchemaVersion,
                protocol_version: 1,
                workspace_version: 1,
                vault_id: launch.vault,
                compatibility_hash: enouia_memory_host::compatibility_hash(),
            },
        };
        if host.reconnect(false).is_err() {
            if let Ok(mut session) = host.session.lock() {
                session.broker.disconnect();
                let _ = session.child.kill();
                let _ = session.child.wait();
            }
            return Err(FAILED.to_owned());
        }
        Ok(host)
    }
    pub fn ticket(&self) -> Result<Ticket, String> {
        // Called on native admission before queuing a blocking worker. Never
        // block the window thread behind an in-flight transport exchange.
        self.session
            .try_lock()
            .map(|s| s.broker.ticket())
            .map_err(|_| "host_busy".to_owned())
    }
    pub fn call(
        &self,
        ticket: &Ticket,
        window: HostSurface,
        request: &Value,
    ) -> Result<Value, String> {
        self.session
            .lock()
            .map_err(|_| "host_unavailable".to_owned())?
            .broker
            .call(ticket, window, request)
            .map_err(fault)
    }
    pub fn lock(&self) -> Result<(), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "host_unavailable".to_owned())?;
        let ticket = session.broker.ticket();
        session
            .broker
            .lock(&ticket, HostSurface::Workspace)
            .map_err(fault)
    }
    pub fn reconnect(&self, unlock: bool) -> Result<(), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "host_unavailable".to_owned())?;
        session.broker.disconnect();
        if session
            .child
            .try_wait()
            .map_err(|_| "host_unavailable")?
            .is_some()
        {
            return Err("host_exited_restart_required".to_owned());
        }
        let peer = ProcessPeer::enroll(session.child.id(), HostSurface::Workspace)
            .map_err(|_| "host_unavailable")?;
        let deadline = Instant::now() + TIMEOUT;
        // This explicit native action retries endpoint establishment only.
        let pipe = loop {
            match Pipe::connect(&self.pipe) {
                Ok(pipe) => break pipe,
                Err(e)
                    if matches!(e.raw_os_error(), Some(231 | 2)) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(_) => return Err("host_disconnected".to_owned()),
            }
        };
        session
            .broker
            .open(pipe, peer, self.hello.clone(), TIMEOUT)
            .map_err(fault)?;
        let ticket = session.broker.ticket();
        if unlock {
            session
                .broker
                .unlock(&ticket, HostSurface::Workspace)
                .map_err(fault)?;
        } else {
            session.broker.handshake(&ticket).map_err(fault)?;
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        if let Ok(mut session) = self.session.lock() {
            let ticket = session.broker.ticket();
            let _ = session.broker.lock(&ticket, HostSurface::Workspace);
            session.broker.disconnect();
            // Do not kill an active Host. It observes our retained process's
            // exit and drops its service/lease when the shell process ends.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
    use enouia_memory_contract::foundation::Clock;
    use enouia_memory_contract::ids::PrincipalId;
    use enouia_memory_contract::time::Timestamp;
    use enouia_memory_vault::host_transaction::HostCoordinator;
    use enouia_memory_vault::service::new_genesis;
    use enouia_memory_vault::{RootPolicy, SystemClock, Vault, VaultOptions, verify_data_root};
    use std::sync::Arc;
    fn args() -> Vec<String> {
        [
            "reference-shell",
            "--host-executable",
            r"C:\synthetic\host.exe",
            "--vault",
            r"C:\synthetic\vault",
            "--host-id",
            "dev_00000001-0000-4000-8000-000000000001",
            "--host-vault-id",
            "vlt_00000001-0000-4000-8000-000000000001",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    #[test]
    fn native_launch_requires_all_explicit_fields_and_rejects_ambiguity() {
        let valid = args();
        Launch::parse(&valid).unwrap();
        for n in (1..valid.len()).step_by(2) {
            let mut missing = valid.clone();
            missing.drain(n..n + 2);
            assert!(Launch::parse(&missing).is_err());
        }
        for suffix in [
            vec!["--autostart"],
            vec!["--host-id", "dev_duplicate"],
            vec!["--unknown", "value"],
        ] {
            let mut invalid = valid.clone();
            invalid.extend(suffix.into_iter().map(str::to_owned));
            assert!(Launch::parse(&invalid).is_err());
        }
        let mut relative = valid;
        relative[2] = "host.exe".to_owned();
        assert!(Launch::parse(&relative).is_err());
        for arg in ["--host-executable=wrong", "--host-executabl"] {
            let invalid = vec!["reference-shell".to_owned(), arg.to_owned()];
            assert!(super::super::Backend::from_args(&invalid).is_err());
        }
    }

    #[test]
    fn explicit_reference_backend_runs_its_own_synthetic_host_child() {
        // Dedicated native acceptance supplies a freshly built repository Host.
        // Ordinary tests need not discover or build another executable.
        let Some(executable) = std::env::var_os("ENOUIA_MEMORY_HOST_TEST_EXE") else {
            return;
        };
        let base = std::env::temp_dir().join("enouia-reference-host-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let root_path = base.join(sha256(&OsIdSource.random_16()).as_str());
        std::fs::create_dir(&root_path).unwrap();
        struct RootCleanup(PathBuf);
        impl Drop for RootCleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = RootCleanup(root_path.clone());
        let root = verify_data_root(&root_path, &RootPolicy::default()).unwrap();
        let owner = ActorRef {
            actor_type: ActorType::Owner,
            actor_id: PrincipalId::from_random(OsIdSource.random_16()),
        };
        let now = Timestamp::from_unix_ms(SystemClock.now_unix_ms()).unwrap();
        let vault = Vault::create(
            &root,
            new_genesis(&OsIdSource, owner, TrustedSurface::TrustedWindowsApp, &now).unwrap(),
            Arc::new(SystemClock),
            Arc::new(OsIdSource),
            VaultOptions::default(),
        )
        .unwrap();
        enouia_memory_vault::platform::restrict_to_owner(root.path()).unwrap();
        let identity = DeviceId::from_random(OsIdSource.random_16());
        drop(HostCoordinator::bootstrap(&vault, &root, identity.clone()).unwrap());
        let launch_args = vec![
            "reference-shell".to_owned(),
            "--host-executable".to_owned(),
            executable.to_string_lossy().into_owned(),
            "--vault".to_owned(),
            root_path.to_string_lossy().into_owned(),
            "--host-id".to_owned(),
            identity.as_str().to_owned(),
            "--host-vault-id".to_owned(),
            vault.vault_id().as_str().to_owned(),
        ];
        struct ChildCleanup(super::super::Backend);
        impl Drop for ChildCleanup {
            fn drop(&mut self) {
                self.0.shutdown();
                if let super::super::Backend::Native(host) = &self.0 {
                    // Only this test-created child, after the synthetic drill.
                    if let Ok(mut session) = host.session.lock() {
                        let _ = session.child.kill();
                        let _ = session.child.wait();
                    }
                }
            }
        }
        let backend = ChildCleanup(super::super::Backend::from_args(&launch_args).unwrap());
        assert!(backend.0.embedded().is_none());
        let request = json!({"schemaVersion":1,"requestId":"req_00000001-0000-4000-8000-000000000001","command":"remember","idempotencyKey":"reference-native-original-key","arguments":{"text":"合成：参考壳连接独立 Host。","claimKey":"reference.native"}});
        assert_eq!(
            backend
                .0
                .call(
                    backend.0.pending().unwrap(),
                    HostSurface::QuickSearch,
                    &request
                )
                .unwrap_err(),
            "permission_denied"
        );
        let reply = backend
            .0
            .call(
                backend.0.pending().unwrap(),
                HostSurface::Workspace,
                &request,
            )
            .unwrap();
        assert!(reply["error"].is_null());
        let queued = backend.0.pending().unwrap();
        backend.0.lock().unwrap();
        assert!(HostCoordinator::open(&vault, &root, identity.clone()).is_err());
        backend.0.native_action(true).unwrap();
        assert_eq!(
            backend
                .0
                .call(queued, HostSurface::Workspace, &request)
                .unwrap_err(),
            "host_stale_connection"
        );
        let before = std::fs::read(root_path.join("vault/CURRENT")).unwrap();
        let replay = backend
            .0
            .call(
                backend.0.pending().unwrap(),
                HostSurface::Workspace,
                &request,
            )
            .unwrap();
        assert_eq!(
            replay["result"]["candidateId"],
            reply["result"]["candidateId"]
        );
        assert_eq!(
            std::fs::read(root_path.join("vault/CURRENT")).unwrap(),
            before
        );
        backend.0.native_action(false).unwrap();
        drop(backend);
        assert!(HostCoordinator::open(&vault, &root, identity).is_ok());
    }
}
