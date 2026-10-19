# Windows crash-injection termination

Date: 2026-10-19 (Asia/Shanghai). Baseline `56bc5eb`. Scope is `FaultAction::Abort` in the opt-in Vault fault injector; no normal storage, record, schema, dependency or recovery rule changes.

The required workspace test run reached `enouia-memory-vault/tests/crash.rs` and then waited indefinitely for four owned child processes. Their executable paths, parent IDs and child-test arguments were verified before stopping the owned Cargo/test tree. That run exited incomplete and is not passing evidence. A bounded attempt with inherited process error-mode flags also timed out and was stopped; it does not establish that a specific reporting dialog caused the wait.

Two independent minimal processes, without a Vault, isolated the termination primitive: the Rust `std::process::abort()` probe did not exit within four seconds; the Windows self-termination probe exited immediately. This is an observation of this Windows/GNU environment, not a general claim that Rust abort always hangs. The probes and the hung test children were the only stopped processes; the running Runtime was untouched.

On Windows the fault injector now uses `TerminateProcess(GetCurrentProcess(), 3)` at the armed boundary. It can target only itself, never a caller-supplied process, and preserves `std::process::abort()` on other platforms/as an unexpected-failure fallback. The normal `Faults::none()` path and injected I/O errors are unchanged. Microsoft's [CRT abort documentation](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/set-abort-behavior?view=msvc-170) distinguishes reporting/DLL cleanup from immediate termination; [TerminateProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess) documents self-termination without return. Existing pinned `windows-sys =0.61.2` and Threading feature supply the API; no dependency is added.

`cargo test -p enouia-memory-vault --test crash --locked` passed all five existing tests in 11.90 seconds after compilation. They independently verify unpublished-state recovery, published-but-lost-response replay, changed-payload conflict, partial write and pre-CURRENT rename boundaries. They still invoke the fault at each declared persistence boundary; externally killing children is not used as a test result.

The subsequent full run also reached a direct `std::process::abort()` in the writer-lock child. That fixture now uses the same armed `FaultAction::Abort` after acquiring its lock and signaling readiness. All five existing `writer_lock` tests then passed in 0.63 seconds, including acquiring the lock after the child's abnormal exit. No assertion or persistence boundary is skipped.

The complete final workspace gate is recorded separately with the history integration evidence. These are process crashes with surviving OS caches, not power-loss/OS-crash durability, production acceptance or permission to terminate user programs.
