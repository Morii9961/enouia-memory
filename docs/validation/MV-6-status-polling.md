# MV-6 workspace status ordering and failure gate

Date: 2026-10-04. Base: `1c7eb86`. Scope: MV-6 root status reads and content visibility. Core, contracts and native permissions are unchanged.

The existing status hook passed only **1/8** targeted deferred-client checks. Its interval started another read while the previous one was pending, an old `open` response replaced a newer `locked` response, and failures silently kept an old status without a retry.

Automatic polling now waits for the read to finish before scheduling the next one. Explicit refresh after Vault actions may supersede an older read; only the latest generation publishes success or failure. A current failure clears the known status, displays an error and offers retry when the Core marks it retryable. The root removes Vault content and recovery controls while status is unknown, instead of using an earlier `open` state as current evidence. Polling stops on unmount. This is a UI gate; native Core lock rules still enforce access.

The independent fixture passed **45/45**: the existing 36 UI checks plus nine status checks for serial polling, lock ordering, obsolete errors, visible current errors, withdrawal of an open content gate, retry scope/recovery and unmount cleanup. Responses and focus are simulated. Frontend and separate read/retry fixtures built successfully. The rebuilt actual app passed **52/52** on a fresh synthetic Vault, including native/Core lock and unlock, original-key retry and acknowledged-record recovery after a renderer crash. Its final installer passed **8/8** isolated checks.

The current-day pinned offline **201 Rust tests**, format, Clippy and independent **34-schema** validation remain applicable to unchanged Rust/contracts. No model, Runtime or Activity capability is added; MV-7 has not started. Manual login, Narrator, interactive upgrade, signing and power-loss acceptance remain pending.
