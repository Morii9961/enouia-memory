# MV-8.1j · Opt-in reference shell native Host

Date: 2026-10-22 (Asia/Shanghai). Baseline
`a8285115f2b14e0b8480769936a3abf6ab235008` (merged PR #47).
Continues owner-authorized MV-8 using synthetic data/processes only.

[ADR-MEM-79](../adr/079-reference-shell-native-host.md) adds a reference backend
selected only through these explicit native arguments:

```text
--host-executable "<absolute matching Host executable>"
--vault "<existing synthetic Host v2 root>"
--host-id "<that root's dev_... identity>"
--host-vault-id "<that root's vlt_... ID>"
```

Use the Host and reference shell from the same pinned revision. Schema/hash
compatibility does not attest executable equality. Optional `--hotkey-key`
keeps the existing shortcut choice. Incomplete, duplicate, relative or unknown
Host arguments fail before any embedded Workspace is constructed. Native mode
does not bootstrap/upgrade a root or install/autostart a Host. Default launches
retain the embedded backend.

The shell launches its explicit child hidden, bounds the ready line/deadline,
checks the actual child PID and authenticates the pipe with retained process
identities. It captures the native queue ticket before scheduling a worker and
injects Tauri's actual main/overlay surface independently of page JSON. Ticket
admission uses try-lock, so it does not block the window behind transport I/O.
The native tray offers explicit reconnect and unlock; page packets keep the
fixed Call tag. Picker/root-switch and native-mode autostart are unavailable.
Native lock errors and native connection actions have native dialog feedback.

The page maps known Host failures to explicit nonretryable errors. An uncertain
reply retains the original request ID, operation key and cloned original
arguments on CallError; later edits cannot replace that saved packet. No
automatic resend is introduced. A caller can inspect the retained request and
use its exact key in a separately explicit recovery flow. There is no new
automatic recovery button or persistence of that packet across renderer exit.

The synthetic reference-backend drill starts the freshly built actual Host
executable, checks no embedded backend exists, rejects overlay writes, retains
the exclusive lease while locked, explicitly reconnects/unlocks, refuses old
queued work and replays to the same candidate without changing CURRENT. It
then reconnects again. Test cleanup ends only its own child; this is not proof
of normal graphical shell exit. Prior process-transport evidence separately
checks normal Host exit after its enrolled broker process exits. Production
exit attempts lock/disconnect and lets that process-exit rule release the Host;
it never kills an active Host. Startup failure may clean up only its newly
created child before page work is admitted.

Final validation:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 490 passed, zero failures; doctest groups passed with zero doctests |
| Strict workspace all-target Clippy | Passed |
| `cargo fmt --all -- --check` | Passed |
| Python and Rust Runtime surface guards | Passed; aggregate `d855683e0879c7887aae819b1345e31587a9a956be5fff5bf31de42ed08478d5`, 55 commands |
| `node --test e2e/host-api.test.mjs` | Four passed, zero failures |
| Frontend typecheck and production build | Passed with project-pinned dependencies |
| Public-content/diff review | 15 owned engineering/documentation/synthetic files; no matching private paths, credential shapes or broken local links; concurrent instructions/status files excluded |

Rust checks use pinned 1.98.1 GNU, offline/locked Cargo and four test threads,
with incremental compilation/dev-test debug information disabled. Schemas and
validators are unchanged; prior independent 43-schema evidence is reused.
Passing this four-thread run does not diagnose the historical default-parallel
ordered-barrier issue retained in earlier reports.

Focused evidence already includes 15 reference-shell Rust tests, with the
optional native executable drill explicitly enabled through
`ENOUIA_MEMORY_HOST_TEST_EXE` pointing to this checkout's freshly built Host.
Ordinary runs without that variable do not execute the process drill. Four
Node tests exercise the production API module with only native invoke replaced:
unknown outcomes/original packet preservation, other nonretryable Host failures,
unchanged valid Core result/retry flags, and unavailable native picker.
Frontend typecheck and production Vite build pass. These are not rendered
Tauri/WebView, tray, native dialog or owner daily-use acceptance.

Saved logs/exit records use ignored `.local/mv8-reference-*` files. Earlier
failures are preserved: wrong synthetic Vault ID prefix, Clippy enum size,
Windows sandbox npm EPERM, uncached offline dependency and a test assuming the
old TypeScript 6 JavaScript API. Fixtures/enum storage/test transform were
corrected; project-pinned dependencies were installed without scripts or
version upgrades. Tests now use the installed Vite 8 transform API checked
against its local published definitions. The earlier quota rejection executed
no install; the explicit retry ran after the account window reset.

Remaining gates: rendered/native-window acceptance, picker/Provider adapters,
enrollment UX/multiple clients and MCP, physical maintenance/independent restore,
Runtime exact-pin adoption and real owner acceptance. No private Vault, real
Provider, Runtime checkout, migration, cloud deployment or manual project-status
snapshot was used or changed. Concurrent user instructions/status edits remain
outside this change.
