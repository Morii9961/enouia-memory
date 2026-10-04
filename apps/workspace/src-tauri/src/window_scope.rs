//! The window identity is injected by Tauri, never supplied by the page.
//! Each window maps to one Core surface; the scope rule itself is shared
//! with every host (`HostSurface`, ADR-MEM-45).
use enouia_memory_workspace::HostSurface;
use serde_json::Value;

fn surface(window: &str) -> Option<HostSurface> {
    match window {
        "main" => Some(HostSurface::Workspace),
        "overlay" => Some(HostSurface::QuickSearch),
        _ => None,
    }
}

pub fn allows_workspace_call(window: &str, request: &Value) -> bool {
    surface(window).is_some_and(|surface| surface.allows(request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn main_keeps_its_core_channel() {
        for command in ["memory_search", "remember", "review_confirm", "vault_lock"] {
            assert!(allows_workspace_call("main", &json!({"command": command})));
        }
    }

    #[test]
    fn overlay_allows_only_search() {
        assert!(allows_workspace_call(
            "overlay",
            &json!({"command":"memory_search"})
        ));
        for command in [
            "remember",
            "review_plan",
            "review_confirm",
            "forget_plan",
            "vault_lock",
            "session_ask",
            "import_start",
            "source_excerpt",
            "memory_list",
        ] {
            assert!(!allows_workspace_call(
                "overlay",
                &json!({"command":command})
            ));
        }
    }

    #[test]
    fn malformed_and_lookalike_commands_are_refused() {
        for packet in [
            json!(null),
            json!([]),
            json!({}),
            json!({"command":42}),
            json!({"command":"Memory_search"}),
            json!({"command":"memory_search "}),
        ] {
            assert!(!allows_workspace_call("overlay", &packet));
        }
    }

    #[test]
    fn request_cannot_spoof_the_native_window() {
        assert!(!allows_workspace_call(
            "overlay",
            &json!({"command":"remember","window":"main","principal":"owner"})
        ));
    }

    #[test]
    fn unknown_windows_are_refused() {
        for window in ["", "Main", "main ", "foreign"] {
            assert!(!allows_workspace_call(
                window,
                &json!({"command":"memory_search"})
            ));
        }
    }
}
