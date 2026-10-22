//! Windows Memory Workspace shell (MV-6, ADR-MEM-44): Memory's reference
//! shell and acceptance harness. Enouia Runtime hosts the product client
//! through its own adapter (ADR-MEM-45).
//!
//! The shell forwards typed requests to the selected embedded/native backend, opens
//! native file dialogs (the page receives a token, never a path), and owns
//! the window lifecycle: closing hides, the tray offers show/lock/exit, a
//! global hotkey opens the quick-search overlay. It writes no logs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use backend::Backend;
use enouia_memory_workspace::PickKind;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State, WebviewWindow, WindowEvent};

mod backend;
mod lifecycle;
mod startup;
mod window_scope;

#[tauri::command]
fn startup_status(core: State<'_, Core>) -> Result<Value, String> {
    if core.0.embedded().is_none() {
        return Err("native_host_autostart_unavailable".to_owned());
    }
    startup::get()
}

#[tauri::command]
fn startup_set(core: State<'_, Core>, enabled: bool) -> Result<Value, String> {
    if core.0.embedded().is_none() {
        return Err("native_host_autostart_unavailable".to_owned());
    }
    startup::set(enabled)
}

struct Core(Arc<Backend>);

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
    let pending = ws.pending()?;
    let surface = window_scope::surface(window.label()).ok_or("permission_denied")?;
    let response =
        tauri::async_runtime::spawn_blocking(move || ws.call(pending, surface, &request))
            .await
            .map_err(|_| "worker_failed".to_owned())??;
    if locking && let Some(overlay) = window.app_handle().get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    Ok(response)
}

/// Open a native dialog and hand the page a token for the choice.
#[tauri::command]
async fn pick(core: State<'_, Core>, kind: String) -> Result<Value, String> {
    let ws = core.0.clone();
    if ws.embedded().is_none() {
        return Err("native_host_picker_unavailable".to_owned());
    }
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
        ws.register_pick(kind, &path)
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
    Backend::queue_shutdown(core.0.clone())
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
fn hotkey(app: AppHandle, ws: Arc<Backend>, letter: u8) {
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
fn hotkey(_app: AppHandle, ws: Arc<Backend>, _letter: u8) {
    ws.set_companion(
        json!({"tray": "present", "overlay": "available", "hotkey": {"state": "unsupported"}}),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ws = match Backend::from_args(&args) {
        Ok(backend) => Arc::new(backend),
        Err(code) => {
            rfd::MessageDialog::new()
                .set_title("Enouia Memory")
                .set_description(format!(
                    "原生 Host 启动失败：{code}。请检查显式启动参数和现有 Host v2 数据根。"
                ))
                .show();
            return;
        }
    };
    let background = args.iter().any(|arg| arg == "--autostart");
    let mut context = tauri::generate_context!();
    if background {
        for window in &mut context.config_mut().app.windows {
            if window.label == "main" {
                window.visible = false;
            }
        }
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
            if core.embedded().is_none() {
                menu.append(&MenuItem::with_id(
                    app,
                    "host_reconnect",
                    "原生 Host：重新连接",
                    true,
                    None::<&str>,
                )?)?;
                menu.append(&MenuItem::with_id(
                    app,
                    "host_unlock",
                    "原生 Host：解锁",
                    true,
                    None::<&str>,
                )?)?;
            }
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
                        Backend::queue_lock(tray_core.clone());
                    }
                    "exit" => {
                        let app = app.clone();
                        Backend::queue_exit(tray_core.clone(), move || app.exit(0));
                    }
                    "host_reconnect" | "host_unlock" => {
                        let core = tray_core.clone();
                        let unlock = event.id().as_ref() == "host_unlock";
                        tauri::async_runtime::spawn_blocking(move || {
                            let message = match core.native_action(unlock) {
                                Ok(()) if unlock => "Host 已解锁并建立新会话。".to_owned(),
                                Ok(()) => "Host 已重新连接。".to_owned(),
                                Err(code) => format!("Host 操作未完成：{code}。原请求未自动重发。"),
                            };
                            rfd::MessageDialog::new()
                                .set_title("Enouia Memory")
                                .set_description(message)
                                .show();
                        });
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
