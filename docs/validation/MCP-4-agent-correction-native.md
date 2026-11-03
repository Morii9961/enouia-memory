# MCP-4 native correction-report preparation

2026-11-03. Base `cb1a5e6`. [ADR120](../adr/120-native-agent-correction-drafts.md)
and [native contract](../contracts/AGENT_CORRECTION_NATIVE_V1.md).

Independent native correction delegation prepares a report against one exact
currently authorized target revision and one independently authorized Runtime
user statement. The existing full Agent target qualification and pinned quote
locator are reused. The Vault checks strict original evidence under the same
current-state lock. No durable report or corrected-memory result is claimed.

Owned files: Vault segmented agent API/exports, Host qualified_agent API,
tests/agent_correction.rs, ADR120/register, native correction contract, this
report, Runtime log and regenerated surface. Manual project status is unchanged.

`cargo test -p enouia-memory-host --test agent_correction --test agent_note --locked`
passed **12 tests, 0 failures** (3 new correction tests and 9 note regressions).
Pinned Rust 1.98.1, offline/locked dependencies and serial Windows tests used
unique owned synthetic roots. Windows ACL fixtures ran outside the restricted
sandbox. No real Vault, Provider, credentials or client configuration was opened.

Coverage: normalized full-width original quote and exact scalar offsets; real
UserStatement provenance and authenticated Agent identity; exact current target
and snapshot; deterministic request keys versus changed request hashes; other
Agent key separation; unchanged chain/memory and no Owner action/queue effect.
Independent source and target-source exclusions refuse. Wrong target hash or
revision, unknown target, wrong source revision, missing/oversized/empty quote or
interpretation, non-Runtime evidence, Assistant text, fabricated confirmation and
foreign-instance permits refuse. Native Owner/initialized/source-read setup is
required. Revoke, lock/unlock generation, policy change, expiry, source deletion
and target deletion invalidate preparation.

This is native draft preparation only. Durable report intake, original-key
receipts/recovery, System revision evaluation, supersession, forget semantics
and advertised MCP transport remain subsequent work. No schema or validator
changed, so no Python schema cross-check or full workspace suite was run.

Final `cargo fmt --all -- --check`, strict Clippy for the Vault and Host packages
with all targets, Runtime surface and whitespace checks passed. Aggregate:
`d74ae1000024e580d04bbacd4824cd996984d9856f4d3770fb1b46c7e6e498a7`.
