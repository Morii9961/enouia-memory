# MV-6 context and actual-request display binding

Date: 2026-10-04. Base: `1fe9d0d`. Scope: context inspection UI; no Core, provider or IPC change.

The deferred-client baseline passed **3/9** context checks. Older actual-request responses replaced newer ones, obsolete errors appeared after success, a failed read kept the previous request, and old request content could appear underneath a different capsule's preview. The old inspection also remained visible while a new inspection was loading or failed.

Capsule inspection and dispatch inspection now have independent latest-read generations. Beginning a new preview or selecting a new capsule clears the prior inspection/request and invalidates pending dispatch reads. Selecting a dispatch clears the previous request and shows only the matching result, labeled with its dispatch ID. Current read failures are visible and retry the captured capsule/dispatch; obsolete failures cannot update the current view. Preview remains a local write with its original action and idempotency key on retry. Busy reads are announced.

The independent fixture passed **54/54**, including all nine context cases and the preceding 45 Explorer/session/overlay/status checks. Those clients simulate reads and writes; they do not prove dispatch persistence or provider behavior. The rebuilt actual app passed **52/52** on a fresh synthetic Vault, including saved Mock request hash verification, preview-versus-dispatched distinction, original-key retry and acknowledged-record recovery after a renderer crash. Its final installer passed **8/8**. Production builds exclude fixture APIs and markers.

Required current-day pinned offline format, **201 Rust tests**, Clippy and independent **34-schema** validation remain valid for unchanged Rust/contracts. All inputs are synthetic; logs, bundles and Vaults remain ignored. This does not start MV-7, authenticate MV-8 clients, or establish manual login/Narrator/interactive upgrade, signing or power-loss acceptance.
