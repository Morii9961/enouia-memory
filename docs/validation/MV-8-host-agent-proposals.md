# MV-8.2c · Host Agent proposals and durable receipts

Date: 2026-10-23 (Asia/Shanghai). Baseline `3ccc5c0`, branch
`codex/mv8-mcp-boundary`; continues Morii's explicit MV-8.2 local synthetic
authorization. [ADR-MEM-82](../adr/082-host-agent-proposals.md) fixes the write
boundary. No external native Agent transport or real client is activated.

## Delivered behavior

`HostService::mcp_agent_server` now accepts the implemented memory:read,
source:read and memory:propose ceilings over the same Host Vault/lifecycle gate.
The original `mcp_read_server` still rejects write grants; the public ReadPort
name remains an alias of HostPort. Advertised tools follow the implemented
ceilings. context_get/session_checkpoint remain unavailable on this binding.

memory_propose and the unadvertised existing memory_update alias resolve current
and cited source metadata and any current target memory from the verified Vault.
Current persisted memory:propose policy must authorize every dependency. Propose
does not imply source text or memory read. Identity/checkpoint types, identity
changes and deletion are refused. Proposed text is stored as AgentProposal,
with the authenticated native Agent and actual source-derived evidence class
and sensitivity. No source/manual assertion is invented, and Canonical records
cannot change without a separate owner review.

The governance Agent entry point isolates pending dedupe by native principal;
existing owner/Workspace global dedupe is unchanged. The regression exercises
the old global result against an owner's pending candidate, then verifies that
the Agent path publishes its own candidate without revealing that private ID.
Same-principal duplicates have no receipt for a new key, so this adapter returns
nonretryable revision_conflict with no candidate ID/operation ID. It does not
fabricate a successful duplicate receipt or change storage to obtain one.

Successful writes use the existing Host-bound witnessed publication and bound
governance receipts. Native principal + canonical operation + caller key bind
canonical arguments; requestId/generated IDs/time are excluded. memory_update
shares memory_propose's key namespace. Explicit new-session replay returns the
original candidate revision, commit and operation ID; changed arguments conflict.
Current policy/deletion are checked even for receipt-only replay. Terminal
candidates are refused rather than mislabeled pending. No automatic reconnect,
resend, replacement keys, Canonical acceptance or Provider send occurs.

Content-free admission audit precedes publication. Failed admission audit is a
known refusal. Ambiguous publication/storage or post-publication audit/freshness
failure closes admission and preserves the exact original packet/key as
outcome_unknown. Output failure after a real synthetic commit ends stdio before
the next packet; explicit original-key replay is verified separately.

## Actual checks

| Check | Result |
| --- | --- |
| Six new Host proposal cases in the main run | Passed: persisted permission/pending provenance; session-independent receipt/payload conflict/unreceipted duplicate refusal; canonical alias; current revocation/terminal state; real commit then lost output with no second publication; cross-principal dedupe isolation |
| `cargo test -p enouia-memory-host --locked -- --test-threads=4` | **49 passed**, zero failures; includes the prior read cases and native service/session/pipe/fault cases; doctests zero |
| Added actual-publication-fault test, run separately with `--lib scoped_agent_proposal_actual_publication_fault` | **1 passed**, exercising both existing AfterCurrent and HostWitnessPublished fault points. Both retain the exact packet/key and close the old session. Incomplete witness refuses new admission; complete witness permits explicit original-key receipt recovery without another commit. No automatic recovery is invoked. |
| `cargo test -p enouia-memory-govern --test review --locked -- --test-threads=4` | **9 passed**, including existing global dedupe, bound publication race, native Host writing and exact owner review |
| `cargo clippy -p enouia-memory-host -p enouia-memory-govern --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface regeneration/check and four Rust surface guards | Passed; same 55 Workspace commands; aggregate `927ee5344e6cb1bbf0a43ee60fd73fc2b780d4b6b82ccc6ce36a6d1d3c17af6a` |

Normal-permission tests use only freshly owned synthetic temporary roots and
processes; owner ACLs remain intact. The tests create no personal consent or
real-data approval. No external dependency, lockfile, schema, storage version or
owner pipe change occurs in this slice. Checks are scoped to the modified Host/
governance behavior and integration surface; no full workspace suite, graphical
shell, real Provider, history quality, Runtime or Activity check is run.

## Remaining exits and Runtime statement

Actual in-process Host binding now covers search/read/source/propose plus the
legacy update alias. Context and checkpoint domain binding, native external
Agent authentication/enrollment, real-client compatibility and personal
acceptance remain open. The generic six-tool fake-port evidence does not close
these exits. This slice does not install or expose an MCP server process.

Runtime's established embedded integration and historical-results pin remain
unchanged: `9d68d3d8707749ad3b0e2a36cfc97d71a27e712f`, **55 Workspace commands**,
aggregate `c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`.
No Runtime pin/transport/UI adoption is required by this additive Host API.
No private Vault, real client, cloud, migration, manual status refresh, push,
merge or deployment occurs.
