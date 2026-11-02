# MCP-2c checkpoint 1: segmented native Agent gate

2026-10-26. Base MCP UI-record reads: `a93d972`; exact published segmented
dependency: `b2c5c1a7e806fa5aeda9b74e35241dfbefd26acb`. Main is not changed.

## Scope and ownership

Adds `vault/automation/segmented/agent.rs` and its two module registration
lines. Existing System storage, read and projection functions remain the
published dependency's implementation. No parallel uncommitted process or
projection work was copied. The MCP read-session extraction preserves the
existing HostV2/3 initialization, unique request IDs, closure and 256-request
limit; it grants no authority. AGENTS and manual status files remain untouched.

The sealed permit has no deserialize/clone/serialize constructor. Trusted native
code must independently authenticate Owner registration and Agent process
enrollment. This gate binds the live Vault instance, Agent principal, subject,
project, exact current policy refs, selected source IDs, sensitivity/source-read
ceilings and at most five minutes on both injected and monotonic clocks.
Client parameters cannot create this authority. Expiry, clock rollback,
revocation or policy replacement permanently invalidate a permit.

Reads verify current storage authority and policies, then apply the independent
Agent predicate before returning borrowed data, labels or countable rows. Exact
source revisions, entity-name/alias evidence and recursive derivation/reversion
closures must fit each record's policy and delegated sources. Deleted upstream
memories hide dependents. History requires a currently visible identity and
independent revision checks. The borrowed carrier cannot be serialized as a
System response. Source text must still be gated by source-read at the future
tool projection boundary.

## Version and capability matrix

| Root | Existing Agent MCP | This checkpoint | Writer |
| --- | --- | --- | --- |
| schema 1 / host-current-v2 | Existing six tools | Unchanged | Existing proposal/checkpoint semantics |
| schema 2 / automation-host-v2 | memory-agent-read-1, five reads | Unchanged | System automatic pipeline only |
| schema 2 / automation-host-v3 | memory-agent-record-read-1, five reads with frozen fields | Unchanged | System automatic pipeline only |
| schema 2 / automation-segmented-host-v1 | Not exposed | Native sealed Agent delegation only | Existing single leased segmented System writer |

The planned `memory-agent-catalog-read-1` is not advertised on MCP yet. Old
clients still reject unsupported roots at their compatibility handshake, before
business decoding, queries or counts. System `memory-native-read-2` remains a
distinct native capability. A pin upgrade grants no new permission.

## Evidence and remaining work

Synthetic fixtures create new temporary roots and legally capture, prepare,
evaluate and apply witnessed decisions; receipts are never fabricated. Focused
tests cover 180 rows across three policies, narrow and public-only delegation,
cross-Vault refusal, duplicate/empty policy/source scopes, excessive sensitivity
and TTL, subject impersonation, explicit revocation, expiry/clock rollback,
policy withdrawal/reactivation, source deletion, lineage deletion, hidden
history and 100 reads with an unchanged business snapshot.

Final checks actually run with the pinned offline/locked toolchain:

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --locked -- --test-threads=1`: 718 passed, zero failed,
  including the four new gate tests and existing real-pipe Agent regressions.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
  (`CARGO_INCREMENTAL=0` avoids filesystem cache-link warnings).
- Python 3.12 / python-jsonschema 4.26.0 independent cross-check: 54 schemas,
  including 33 catalog cases, 50 UI-record cases and both Agent read suites;
  zero disagreements. Run because the exact dependency adds storage/schema.
- `python tools/integration/runtime_surface.py`: aggregate
  `5f5bb358ddf2e01e1aff9daaaa4cdfe3352c3c6fedac22922213fe1183665df8`, logged.
- Staged whitespace and public-content checks: passed. No AGENTS/manual status
  changes. Published dependency functions retain their original implementation;
  bridge files combine the existing MCP additions with the published modules.

No private root, real Provider/embedding, Runtime checkout, real client
configuration, remote listener or production initialization was used.

Relative to Runtime requirements 03 section 6, segmented real-transport tool
evidence, complete typed outputs, scalable recall trace and Runtime Context
binding remain undelivered. Native gate tests do not count as those acceptance
rows. Continue with that read-only binding before Runtime-only capture, notes,
correction/forget requests and session resources. Missing private initialization
does not block any synthetic backend work.
