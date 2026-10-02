fn main() {
    // Only the app's own commands exist; no plugin is registered.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "workspace_call",
            "pick",
            "show_main",
            "hide_window",
            "exit_app",
        ]),
    ))
    .expect("tauri build");
}
