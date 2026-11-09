//! Runnable remote service (ADR-MEM-133): one initialized Owner Vault, the
//! OAuth authority with a locked on-disk state file, and the read-only opening
//! profile behind the loopback HTTP/1.1 driver. The process runs as the OS
//! account that owns the Vault root; that ownership is the Owner's
//! authentication on the server.
use super::oauth::{Authority, OAuthConfig, State};
use super::{Config, Endpoint, Handler, Request, Response, Router, http1};
use crate::automation::qualified_agent::QualifiedAgentHost;
use crate::mcp::opening;
use enouia_memory_contract::{
    common::{ActorRef, ActorType, Sensitivity},
    foundation::Clock,
    ids::{DeviceId, PolicyId, PrincipalId, ProjectId, SourceId, SubjectId},
    next::AttentionProfile,
    ports::IdSource,
    time::Timestamp,
};
use enouia_memory_vault::automation::segmented::{SegmentedAgentScope, SegmentedAutomationVault};
use enouia_memory_vault::platform;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Vault permits last at most five minutes; renew a minute early.
pub const PERMIT_MS: i64 = 300_000;
pub const RENEW_MS: i64 = 240_000;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
const STATE: &str = "oauth-state.json";
const STATE_TMP: &str = "oauth-state.json.tmp";
const LOCK: &str = "oauth.lock";

/// The owner-written service file. Every field is required; nothing is
/// guessed, and the exposure scope is exactly what the owner lists.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    pub vault_root: PathBuf,
    pub host_id: DeviceId,
    pub state_dir: PathBuf,
    pub bind: SocketAddr,
    pub resource: String,
    pub issuer: String,
    pub redirect_prefixes: Vec<String>,
    pub origins: Vec<String>,
    pub agent_id: PrincipalId,
    pub subject_id: SubjectId,
    pub project_id: Option<ProjectId>,
    pub policy_ids: Vec<PolicyId>,
    pub source_ids: Vec<SourceId>,
    pub max_sensitivity: Sensitivity,
    pub source_read: bool,
    pub profile: AttentionProfile,
}

impl ServiceConfig {
    pub fn oauth(&self) -> Option<OAuthConfig> {
        let prefixes: Vec<&str> = self.redirect_prefixes.iter().map(String::as_str).collect();
        OAuthConfig::new(&self.issuer, &self.resource, &prefixes)
    }
    pub fn endpoint(&self) -> Option<Config> {
        let origins: Vec<&str> = self.origins.iter().map(String::as_str).collect();
        Config::new(&self.resource, &[self.issuer.as_str()], &origins)
    }
    fn scope(&self, valid_until: Timestamp) -> SegmentedAgentScope {
        SegmentedAgentScope {
            subject_id: self.subject_id.clone(),
            project_id: self.project_id.clone(),
            policy_ids: self.policy_ids.clone(),
            source_ids: self.source_ids.clone(),
            max_sensitivity: self.max_sensitivity,
            source_read: self.source_read,
            valid_until,
        }
    }
}

/// OAuth state in an owner-only directory beside (never inside) the Vault.
/// Every read-modify-write holds an exclusive lock, so the approval command
/// and the server can run at the same time.
pub struct StateFile {
    dir: PathBuf,
}

impl StateFile {
    pub fn open(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        platform::restrict_to_owner(dir)?;
        Ok(Self {
            dir: dir.to_owned(),
        })
    }
    pub fn update<T>(&self, f: impl FnOnce(&mut State) -> T) -> io::Result<T> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(LOCK))?;
        lock.lock()?;
        let path = self.dir.join(STATE);
        let mut state = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(e),
        };
        let before = state.clone();
        let out = f(&mut state);
        if state != before {
            let tmp = self.dir.join(STATE_TMP);
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&serde_json::to_vec(&state).map_err(io::Error::other)?)?;
            file.sync_all()?;
            drop(file);
            platform::replace_durable(&tmp, &path)?;
        }
        Ok(out)
    }
}

/// Load the persisted state before each request and save it after.
struct Persisted<'a, H> {
    inner: H,
    authority: &'a Authority<'a>,
    file: &'a StateFile,
}

impl<H: Handler> Handler for Persisted<'_, H> {
    fn respond(&mut self, request: &Request) -> Response {
        let result = self.file.update(|state| {
            self.authority.replace_state(std::mem::take(state));
            let response = self.inner.respond(request);
            *state = self.authority.state();
            response
        });
        result.unwrap_or_else(|_| Response::new(503))
    }
}

/// Issue a fresh approval code for the owner and save its digest.
pub fn approve(
    config: &ServiceConfig,
    ids: &dyn IdSource,
    clock: &dyn Clock,
) -> Result<String, String> {
    let oauth = config
        .oauth()
        .ok_or("invalid issuer, resource or redirect prefixes")?;
    let file = StateFile::open(&config.state_dir).map_err(|e| format!("state directory: {e}"))?;
    file.update(|state| {
        let authority = Authority::new(oauth, ids, clock, std::mem::take(state));
        let code = authority.issue_approval_code();
        *state = authority.state();
        code
    })
    .map_err(|e| format!("state file: {e}"))
}

/// Serve until `stop` says so. One Vault permit per generation; every
/// generation lasts at most `RENEW_MS`, then sessions end and clients
/// reinitialize. An unknown outcome stops the service for operator recovery.
pub fn serve(
    vault: Arc<SegmentedAutomationVault>,
    config: &ServiceConfig,
    listener: &TcpListener,
    clock: &dyn Clock,
    ids: &dyn IdSource,
    mut stop: impl FnMut() -> bool,
) -> Result<(), String> {
    let oauth = config
        .oauth()
        .ok_or("invalid issuer, resource or redirect prefixes")?;
    if config.endpoint().is_none() {
        return Err("invalid resource or origins".into());
    }
    let file = StateFile::open(&config.state_dir).map_err(|e| format!("state directory: {e}"))?;
    if file.dir.starts_with(&config.vault_root) {
        return Err("the state directory must not be inside the Vault".into());
    }
    let owner = vault.descriptor().created_by.clone();
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(vault, owner.clone())
        .map_err(|e| format!("owner host: {e:?}"))?;
    let principal = ActorRef {
        actor_type: ActorType::Agent,
        actor_id: config.agent_id.clone(),
    };
    let authority = Authority::new(oauth, ids, clock, State::default());
    while !stop() {
        let now = clock.now_unix_ms();
        let until = Timestamp::from_unix_ms(now + PERMIT_MS).ok_or("clock out of range")?;
        let reader = host
            .reader(&owner, principal.clone(), config.scope(until))
            .map_err(|e| format!("agent permit: {e:?}"))?;
        let endpoint = Endpoint::new(config.endpoint().expect("checked"), &authority, ids, |_| {
            opening::Server::new(&reader, &config.profile)
        });
        let mut handler = Persisted {
            inner: Router {
                authority: &authority,
                endpoint,
            },
            authority: &authority,
            file: &file,
        };
        let renew_at = now + RENEW_MS;
        http1::serve(listener, &mut handler, IO_TIMEOUT, |h, _| {
            h.inner.endpoint.halted().is_none() && clock.now_unix_ms() < renew_at && !stop()
        })
        .map_err(|e| format!("listener: {e}"))?;
        let halted = handler.inner.endpoint.halted().is_some();
        drop(handler);
        reader.revoke();
        if halted {
            return Err("an outcome is unknown; recover the Vault before serving again".into());
        }
    }
    Ok(())
}
