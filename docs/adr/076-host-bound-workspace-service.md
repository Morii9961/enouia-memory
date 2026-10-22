# ADR-MEM-76 — Host-bound Workspace service

Date: 2026-10-22. Scope: owner-authorized MV-8.1g native library integration.

The session gate and native writer must guard actual Workspace domain calls.
`HostService` opens only an explicitly supplied, already adopted v2 root,
binds its native writer identity and attaches that same `Arc<Vault>` to
Workspace. No bootstrap, root creation, listener, process spawn, OS
authentication, Provider setup or embedded fallback occurs.

`Workspace::from_host_vault` requires a verified bound writer, retains that
exact root/handle and rechecks it on reopen. Embedded `Workspace::new` keeps
its existing lifecycle. Native attachment retains the Vault lease across
UI lock/close; cancelling and joining workers closes the view, not the Host's
storage ownership. Native owners release the lease when all attached handles
drop. Shutdown documents this distinction explicitly.

`HostService::call` holds the session dispatch gate, validates native scope and
the unchanged Workspace envelope, freshly verifies actual Host publication/
authority, then forwards to Core. Client root open/create/unlock commands are
refused; only trusted native control can reopen the retained root. Opaque
connections stay inside the native transport, never page arguments.

Native owner lock/unlock operations serialize with each other. They revoke
all connections before Core lifecycle work; unlock reopens only the retained
root and re-enables admission after fresh authority verification. Failures
leave admission locked. A returning UI must authenticate/handshake again;
original durable operation keys retain their Core recovery semantics. Dropping
the service revokes admission and cancels/joins Core work before releasing
its native Vault ownership.

NativeAdmission is still an assertion from an authenticated, authorized native
adapter, not proof derived from JSON or same-user access. Existing Core async
cancellation and exact Provider approvals remain required; ordinary connection
loss does not itself authorize resend or new async work. Picker/Provider native
adapters, a protected listener, real OS peer enrollment, executable, reference
reconnect and Runtime adoption are separate work. Physical purge/export stay
fenced under ADR-MEM-74. No Workspace command or schema changes.

See [synthetic integration evidence](../validation/MV-8-host-workspace-service.md).
