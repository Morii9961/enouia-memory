# MV-8.2d · Agent local current-memory context

Date: 2026-10-23 (Asia/Shanghai). Baseline `8c1cea5`, branch
`codex/mv8-mcp-boundary`; within the explicit MV-8.2 local synthetic authorization.
[ADR-MEM-83](../adr/083-host-agent-local-context.md) freezes the limited selection.

Later same-day follow-up: [MV-8.2e](MV-8-host-agent-checkpoints.md) completes the
in-process checkpoint binding. The remaining-exit statements below describe
this earlier slice; broader Context and external Agent enrollment remain open.

## Delivered selection

The existing Host can now accept ContextRead and execute context_get through
the deterministic compiler on its existing Vault. Native selection is fixed to
LocalMock, current commit, no session/branch, answer or inspection_preview.
Project hints, asOf and token ceiling are honored. knownAt, session/branch,
other purposes, non-LocalMock destinations and provider binding names are
explicit invalid_request errors before compilation; they are not ignored or
converted to supported inputs. No Provider is constructed, invoked or registered.

The native compiler entry accepts only an Agent, records client_surface as
mcp_client, and binds native surface/purpose in a separate durable requestId
namespace. The old compiler paths retain their CLI surface, key namespace and
payloads. Capsule/inspection artifacts may be committed by context_get; this
does not produce Canonical memory or an approved user/source message.

Current compiler/index policies still filter resources. ContextRead does not
expose memory_read or memory_source. Default owner-only policies return an
empty capsule with incomplete/no-supported-memory metadata, never invented
evidence. A permitted result returns the validated capsule and opaque inspection
ID, without restricted inspection decisions. Current and cited metadata,
provenance, sensitivity, policy/deletion epochs and authority are checked through
delivery; output is bounded to the Host's existing result cap.

Admission and final audit records contain IDs/enums/codes, not query or content.
Audit/storage uncertainty after compilation ends the session with the original
packet/requestId; the write key stays null. No automatic recompile occurs. A
fresh explicit native admission can replay the original requestId to retrieve
the same capsule/inspection, subject to current policy/deletion barriers.

## Actual checks

| Check | Result |
| --- | --- |
| Five new actual Host Context cases | Passed: no-data default, native provenance and no fabricated original; current policy/exact purpose/token budget; unsupported selection refusal before writes; requestId replay/payload conflict/revocation; actual witness-published error and explicit receipt recovery |
| `cargo test -p enouia-memory-host -p enouia-memory-context --locked -- --test-threads=4` | **Host 55 + Context 15 passed**, zero failures; doctests zero; includes prior read/propose and existing native lifecycle/pipe/fault behavior |
| `cargo clippy -p enouia-memory-host -p enouia-memory-context --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface regeneration/check and four Rust guards | Passed; same 55 Workspace commands; aggregate `0a45c28462149b7e117f958e7d9c470f14ec57a344fc525c13a1161a7215c828` |

The new direct Host→Context path edge already existed in the resolved Workspace
dependency closure. No external package version, schema, storage format or
transport version changed. Existing Context tests preserve saved-capsule,
receipt, source/role, budget, provisional-checkpoint and offline Mock behavior;
Mock evidence is not a real Provider request. Only new owned synthetic roots and
existing owned child-process checks run; no window or Computer Use is used.

## Remaining exits and Runtime statement

Actual Host binding now covers search/read/source/propose and this limited
current-memory Context selection. Historical/session Context and
session_checkpoint binding remain unfinished. Native external Agent
authentication/enrollment, real-client interoperability, installation and owner
acceptance remain pending; no runnable external MCP server is activated.

Runtime's existing embedded product integration and independent history-result
pin remain `9d68d3d8707749ad3b0e2a36cfc97d71a27e712f` / **55 Workspace commands** /
`c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`.
This additive native compiler/Host surface does not require Runtime to change
pin, UI or transport. No private data, real-client enrollment, cloud, migration,
manual status refresh, push, merge or deployment occurs.
