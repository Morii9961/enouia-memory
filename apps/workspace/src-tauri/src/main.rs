//! Windows Memory Workspace shell (MV-6, ADR-MEM-44): Memory's reference
//! shell and acceptance harness. Enouia Runtime hosts the product client
//! through its own adapter (ADR-MEM-45).
//!
//! The shell forwards the page's typed requests to the embedded Core, opens
//! native file dialogs (the page receives a token, never a path), and owns
//! the window lifecycle: closing hides, the tray offers show/lock/exit, a
//! global hotkey opens the quick-search overlay. It writes no logs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use enouia_memory_workspace::{Config, PickKind, Workspace};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State, WebviewWindow, WindowEvent};

mod lifecycle;
mod startup;
mod window_scope;

#[tauri::command]
fn startup_status() -> Result<Value, String> {
    startup::get()
}

#[tauri::command]
fn startup_set(enabled: bool) -> Result<Value, String> {
    startup::set(enabled)
}

struct Core(Arc<Workspace>);

#[tauri::command]
async fn workspace_call(
    core: State<'_, Core>,
    window: WebviewWindow,
    request: Value,
) -> Result<Value, String> {
    if !window_scope::allows_workspace_call(window.label(), &request) {
        return Err("permission_denied".to_owned());
    }
    let locking = request.get("command").and_then(Value::as_str) == Some("vault_lock");
    let ws = core.0.clone();
    let response = tauri::async_runtime::spawn_blocking(move || ws.call(&request))
        .await
        .map_err(|_| "worker_failed".to_owned())?;
    if locking && let Some(overlay) = window.app_handle().get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    Ok(response)
}

/// Open a native dialog and hand the page a token for the choice.
#[tauri::command]
async fn pick(core: State<'_, Core>, kind: String) -> Result<Value, String> {
    let ws = core.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (kind, title) = match kind.as_str() {
            "import_file" => (PickKind::ImportFile, "选择要导入的导出文件"),
            "vault_root" => (PickKind::VaultRoot, "选择 Vault 数据目录"),
            "backup_destination" => (PickKind::BackupDestination, "选择空的备份目标目录"),
            "export_folder" => (PickKind::ExportFolder, "选择要预览的备份目录"),
            _ => return json!({"error": {"code": "invalid_request", "rules": ["workspace.pick_kind"]}}),
        };
        let dialog = rfd::FileDialog::new().set_title(title);
        let chosen: Option<PathBuf> = match kind {
            PickKind::ImportFile => dialog.pick_file(),
            _ => dialog.pick_folder(),
        };
        let Some(path) = chosen else {
            return json!({"cancelled": true});
        };
        match ws.register_pick(kind, &path) {
            Ok(p) => json!({"token": p.token, "displayName": p.display_name, "bytes": p.bytes}),
            Err(e) => json!({"error": e}),
        }
    })
    .await
    .map_err(|_| "worker_failed".to_owned())
}

fn show(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn show_main(app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    show(&app, "main");
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

/// Exit: cancel and join operations, release the Vault, end the process.
#[tauri::command]
async fn exit_app(app: AppHandle, core: State<'_, Core>) -> Result<(), String> {
    lifecycle::queue_shutdown(core.0.clone())
        .await
        .map_err(|_| "worker_failed".to_owned())?;
    app.exit(0);
    Ok(())
}

fn tray_icon() -> tauri::image::Image<'static> {
    let n = 32u32;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let edge = x.min(y).min(n - 1 - x).min(n - 1 - y);
            let px = if edge < 2 {
                [0x1f, 0x5a, 0x50, 255]
            } else {
                [0x2a, 0x7d, 0x6e, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    tauri::image::Image::new_owned(rgba, n, n)
}

/// Ctrl+Alt+<letter> (M unless `--hotkey-key` says otherwise) on its own thread. A taken combination is reported as a
/// conflict instead of failing silently.
#[cfg(windows)]
fn hotkey(app: AppHandle, ws: Arc<Workspace>, letter: u8) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
    std::thread::spawn(move || {
        let combo = format!("Ctrl+Alt+{}", letter as char);
        // SAFETY: a thread-level hotkey (null window) owned by this thread.
        let ok = unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                1,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                u32::from(letter),
            )
        } != 0;
        ws.set_companion(json!({
            "tray": "present", "overlay": "available",
            "hotkey": {"combo": combo, "state": if ok { "registered" } else { "conflict" }},
        }));
        if !ok {
            return;
        }
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: standard message loop for this thread's queue.
        while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
            if msg.message == WM_HOTKEY {
                show(&app, "overlay");
            }
        }
    });
}

#[cfg(not(windows))]
fn hotkey(_app: AppHandle, ws: Arc<Workspace>, _letter: u8) {
    ws.set_companion(
        json!({"tray": "present", "overlay": "available", "hotkey": {"state": "unsupported"}}),
    );
}

fn main() {
    let ws = Arc::new(Workspace::new(Config::default()));
    let args: Vec<String> = std::env::args().collect();
    let background = args.iter().any(|arg| arg == "--autostart");
    let mut context = tauri::generate_context!();
    if background {
        for window in &mut context.config_mut().app.windows {
            if window.label == "main" {
                window.visible = false;
            }
        }
    }
    if let Some(root) = args
        .windows(2)
        .find(|w| w[0] == "--vault")
        .map(|w| PathBuf::from(&w[1]))
    {
        // A rejected root (or one open in another app) leaves the workspace
        // empty; the owner then opens a Vault from the Vault page, which
        // names the reason.
        let _ = ws.open_root(&root);
    }
    let letter = args
        .windows(2)
        .find(|w| w[0] == "--hotkey-key")
        .and_then(|w| w[1].bytes().next())
        .map(|b| b.to_ascii_uppercase())
        .filter(u8::is_ascii_uppercase)
        .unwrap_or(b'M');
    let core = ws.clone();
    tauri::Builder::default()
        .manage(Core(ws.clone()))
        .invoke_handler(tauri::generate_handler![
            workspace_call,
            pick,
            show_main,
            hide_window,
            exit_app,
            startup_status,
            startup_set
        ])
        .setup(move |app| {
            let show_item =
                MenuItem::with_id(app, "show", "显示 Enouia Memory", true, None::<&str>)?;
            let lock_item = MenuItem::with_id(app, "lock", "锁定 Vault", true, None::<&str>)?;
            let exit_item = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &lock_item, &exit_item])?;
            let tray_core = core.clone();
            TrayIconBuilder::with_id("main")
                .icon(tray_icon())
                .tooltip("Enouia Memory")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => show(app, "main"),
                    "lock" => {
                        if let Some(overlay) = app.get_webview_window("overlay") {
                            let _ = overlay.hide();
                        }
                        // The menu callback returns while Core waits for calls
                        // and joins operations on a blocking worker.
                        lifecycle::queue_lock(tray_core.clone());
                    }
                    "exit" => {
                        let app = app.clone();
                        lifecycle::queue_exit(tray_core.clone(), move || app.exit(0));
                    }
                    _ => {}
                })
                .build(app)?;
            hotkey(app.handle().clone(), core.clone(), letter);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window hides it; operations keep running. Exit is
            // the tray's or the page's explicit action.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(context)
        .expect("workspace shell");
    ws.shutdown();
}
