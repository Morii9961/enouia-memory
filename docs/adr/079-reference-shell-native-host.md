# ADR-MEM-79 — Opt-in reference shell native Host

Date: 2026-10-22. Status: accepted for the owner-authorized MV-8 continuation.

The reference Tauri shell can explicitly select a native Host backend using an
owner-provided Host executable, existing v2 root, Host device ID and expected
Vault ID. All four are native launch arguments. The shell creates a fresh pipe
name, launches only that executable hidden and enrolls its own live process as
Workspace broker. It checks the child's ready PID, retains that process and
performs the existing kernel-authenticated versioned handshake. Failed native
startup must fail visibly and must never construct an embedded Workspace.
Default launches retain the existing embedded backend.

The backend captures ADR-MEM-78's ticket when accepting a native window call,
before queuing a blocking worker. Main and overlay scope still come from Tauri
window labels. The native backend never maps page input to native control tags.
Page root creation/open/unlock and picker registration are unavailable there;
native tray actions explicitly reconnect or unlock the fixed existing Host.
Transport failures are visible stable error codes, including unknown outcomes.
No operation keys are regenerated and no application request is retried.

Native tray lock and exit run on blocking workers. Exit attempts a native lock,
disconnects and lets the Host observe the broker process exit; it does not
terminate an active Host or another process. Failed startup may clean up only
the child it just created before accepting any page requests. Reconnect uses
that retained child identity and the original native endpoint, never a newly
discovered PID or fallback executable. If the Host exits, a new explicit shell
launch is required.

This reference mode exposes existing Core management/search/review behavior
and native connection actions for synthetic acceptance. File picker forwarding,
Provider adapters, installation/autostart, multi-client enrollment and Runtime
adoption remain separate. It does not adopt or migrate a real Vault, install a
Host, activate MCP or start a new MV stage.
