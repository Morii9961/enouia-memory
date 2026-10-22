//! Reference backend selection. Native failure never creates embedded storage.
use enouia_memory_workspace::{Config, HostSurface, PickKind, Workspace};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

#[cfg(windows)]
#[path = "native_host.rs"]
mod native_host;

pub enum Backend {
    Embedded(Arc<Workspace>),
    #[cfg(windows)]
    Native(Box<native_host::NativeHost>),
}
pub enum Pending {
    Embedded,
    #[cfg(windows)]
    Native(enouia_memory_host::broker::Ticket),
}
impl Backend {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let native = args.iter().skip(1).any(|arg| arg.starts_with("--host-"));
        if native {
            #[cfg(windows)]
            return native_host::NativeHost::start(args)
                .map(Box::new)
                .map(Self::Native);
            #[cfg(not(windows))]
            return Err("native_host_unsupported".to_owned());
        }
        let ws = Arc::new(Workspace::new(Config::default()));
        if let Some(root) = args.windows(2).find(|w| w[0] == "--vault") {
            // Keep the embedded shell's existing rejected-root behavior.
            let _ = ws.open_root(Path::new(&root[1]));
        }
        Ok(Self::Embedded(ws))
    }
    pub fn pending(&self) -> Result<Pending, String> {
        match self {
            Self::Embedded(_) => Ok(Pending::Embedded),
            #[cfg(windows)]
            Self::Native(host) => host.ticket().map(Pending::Native),
        }
    }
    pub fn call(
        &self,
        pending: Pending,
        window: HostSurface,
        request: &Value,
    ) -> Result<Value, String> {
        match (self, pending) {
            (Self::Embedded(ws), Pending::Embedded) => {
                if !window.allows(request) {
                    return Err("permission_denied".to_owned());
                }
                Ok(ws.call(request))
            }
            #[cfg(windows)]
            (Self::Native(host), Pending::Native(ticket)) => host.call(&ticket, window, request),
            #[cfg(windows)]
            _ => Err("host_stale_connection".to_owned()),
        }
    }
    pub fn embedded(&self) -> Option<&Arc<Workspace>> {
        match self {
            Self::Embedded(ws) => Some(ws),
            #[cfg(windows)]
            Self::Native(_) => None,
        }
    }
    pub fn register_pick(&self, kind: PickKind, path: &Path) -> Value {
        match self {
            Self::Embedded(ws) => match ws.register_pick(kind, path) {
                Ok(p) => {
                    serde_json::json!({"token":p.token,"displayName":p.display_name,"bytes":p.bytes})
                }
                Err(e) => serde_json::json!({"error":e}),
            },
            #[cfg(windows)]
            Self::Native(_) => {
                serde_json::json!({"error":{"code":"native_host_picker_unavailable"}})
            }
        }
    }
    pub fn set_companion(&self, value: Value) {
        if let Some(ws) = self.embedded() {
            ws.set_companion(value);
        }
    }
    pub fn lock(&self) -> Result<(), String> {
        match self {
            Self::Embedded(ws) => {
                let reply = ws.call(&serde_json::json!({"schemaVersion":1,"requestId":"req_00000000-0000-4000-8000-000000000001","command":"vault_lock","idempotencyKey":null,"arguments":{}}));
                if !reply["error"].is_null() {
                    return Err("vault_lock_failed".to_owned());
                }
                Ok(())
            }
            #[cfg(windows)]
            Self::Native(host) => host.lock(),
        }
    }
    pub fn shutdown(&self) {
        match self {
            Self::Embedded(ws) => ws.shutdown(),
            #[cfg(windows)]
            Self::Native(host) => host.shutdown(),
        }
    }
    pub fn native_action(&self, unlock: bool) -> Result<(), String> {
        match self {
            #[cfg(windows)]
            Self::Native(host) => host.reconnect(unlock),
            Self::Embedded(_) => {
                let _ = unlock;
                Err("native_host_inactive".to_owned())
            }
        }
    }
    pub fn queue_lock(core: Arc<Self>) -> tauri::async_runtime::JoinHandle<Value> {
        if let Some(ws) = core.embedded() {
            return crate::lifecycle::queue_lock(ws.clone());
        }
        tauri::async_runtime::spawn_blocking(move || match core.lock() {
            Ok(()) => serde_json::json!({"locked":true}),
            Err(code) => {
                rfd::MessageDialog::new()
                    .set_title("Enouia Memory")
                    .set_description(format!("Host 锁定未获确认：{code}。请检查连接状态。"))
                    .show();
                serde_json::json!({"error":{"code":code}})
            }
        })
    }
    pub fn queue_shutdown(core: Arc<Self>) -> tauri::async_runtime::JoinHandle<()> {
        if let Some(ws) = core.embedded() {
            return crate::lifecycle::queue_shutdown(ws.clone());
        }
        Self::queue_exit(core, || {})
    }
    pub fn queue_exit(
        core: Arc<Self>,
        after: impl FnOnce() + Send + 'static,
    ) -> tauri::async_runtime::JoinHandle<()> {
        if let Some(ws) = core.embedded() {
            return crate::lifecycle::queue_exit(ws.clone(), after);
        }
        tauri::async_runtime::spawn_blocking(move || {
            core.shutdown();
            after();
        })
    }
}
