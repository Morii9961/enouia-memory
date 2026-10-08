# MV-6 Core — Vault lifecycle isolation

Date: 2026-10-05. Base: `f37d77f`. End: the feature commit containing this report (see Git history). Scope: the embedded workspace Core and its Runtime handoff, within MV-6 / ADR-MEM-46. MV-7 remains unstarted.

## Reproduced problem

On the base implementation, two new lifecycle tests failed: `review_confirm` returned a cached successful receipt after `vault_lock`, and `operation_list` still exposed the completed old Vault backup (including its commit and destination name). A third test confirmed that changing the diff hash of an already confirmed plan still returned success. Close cleared pending plans and import replay entries, but left confirmed replies, operation history, `lastBackup` and unconsumed data picker tokens. The confirmation cache was consulted before checking for an open Vault or the original diff.

## Change

Close now detaches the Vault, requests cancellation and joins workers, then clears its operation history, pending/confirmed plans, import replay entries, last backup and import/backup/export picker tokens. This applies to lock, root switch, failed open after close, and shutdown. Native root picker tokens keep their existing expiry and success-only consumption so a rejected open/create can still be retried. Progress remains readable while workers stop. Once close returns, old operation IDs are unknown and old plans cannot replay, including after unlocking the same Vault.

Confirmed replies retain their exact diff hash, and replay first requires an open Vault. Same-diff retries within the same open Vault return the original reply without another commit; a changed hash returns `revision_conflict` / `workspace.diff_hash_mismatch`. Persisted memories and backup files are not removed. The reference shell labels last backup as since this Vault was opened.

Runtime must update its UI state handling together with its pinned revision, as recorded in the [compatibility log](../integration/RUNTIME.md). No Runtime checkout is used or modified here.

## Evidence

The new synthetic Core regressions cover:

- `a_confirmed_retry_still_checks_the_diff`: same-diff replay, changed-diff refusal and unchanged Vault head.
- `closing_a_vault_forgets_confirmed_replies`: lock, switch, failed open and shutdown; unlocking preserves the stored memory but invalidates its old confirmation reply.
- `closing_a_vault_forgets_operations_backups_and_picks`: the same four close paths, actual synthetic backup export, old task reads/cancels, expired import/backup/export tokens, reset backup status and a fresh native pick.
- `closing_keeps_progress_until_the_worker_has_joined`: a channel-controlled worker keeps readable progress during shutdown; the task disappears only after it is released and joined.

Final repository-root checks passed on the pinned Rust 1.98.1 GNU toolchain with `CARGO_NET_OFFLINE=true` and the MinGW compiler ahead of the Rust toolchain in `PATH`:

- `cargo fmt --all -- --check`.
- `cargo test --workspace --locked`: **220 passed**, including **18 workspace Core tests** and the four new regressions above.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `python tools/schema-check/check_schemas.py` using Python 3.12.10 and python-jsonschema 4.26.0: **34 schemas**, 53 valid records, 117 record cases, 4,913 set records, 51 IPC messages, 63 workspace messages and 46 store documents agree.
- `python tools/integration/runtime_surface.py`: aggregate `65d7ae8c115ad4d38c1702243df7f2ba4ff4f4593356dd0b486a74aa71dcbf36`, recorded and logged.
- `git diff --check` and review of the wording-only reference UI diff.

The initial sandboxed full run stopped at the existing CLI owner-only ACL check. That unchanged test passed outside the sandbox, then the complete workspace suite and Clippy passed outside the sandbox. No ACL or CLI behavior was patched. Local verification logs remain ignored.

## Limits

This is local synthetic Core evidence. The reference UI change is wording only, inspected in the diff; Windows application/installer suites were not rerun. Runtime pin adoption and its UI lifecycle acceptance remain pending in Runtime. This does not establish real export acceptance, Provider calls, Narrator, actual login startup, interactive upgrade, signing, OS-crash or power-loss durability. Existing stage-report gaps remain pending.
