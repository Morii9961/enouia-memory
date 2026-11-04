# MCP-5 opening/resources — synthetic evidence

2026-11-04; branch codex/mcp5-opening-resources, based on MCP-4 4aa1605.
Morii explicitly authorized MCP-5 synthetic development. ADR126 and
MCP_OPENING_RESOURCES_V1 were written before implementation. This report covers
the opening/resources slice only, not an entire real product acceptance.

## Implemented

Explicit initialized read-only Server, memory-agent-pipe-10 /
memory-agent-initialized-context-1 / automation-context-v1 attach. Five existing
tools, one bounded memory_start prompt, static guide and memory/recall templates.
Every projection uses the same qualified native reader. No write permit, source
text resource, model call, capture or durable context publication. Same-reader
actual last recall only; stale context refuses instead of reranking. Prompt
role=user explicitly does not authenticate Runtime evidence or Owner consent.

Native read metadata polling now checks the instance's recovery latch. The new
context poll additionally checks current delegated policy versions; pipe-5 keeps
its previous refusal mechanism. OutcomeUnknown and its subsequent native
RecoveryRequired emit no new-profile result; full first MCP Value is retained,
transport actually disconnects, grants revoke and client sends stay poisoned.

## Checks actually run

Pinned Rust 1.98.1, locked/offline, incremental disabled, one test thread. Windows
synthetic roots use ordinary required OS ACL access. No private material read.

- Host targeted run: **61 passed / 0 failed**, seven targets:
  agent_correction 14, agent_forget 10, agent_supersede 16,
  initialized_agent_native 2, initialized_mcp 8, opening_mcp 6, opening_pipe 5.
  Log: target/mcp5-opening-tests.log. After the final new-profile constructor
  check, opening_mcp + opening_pipe reran: **11 passed / 0 failed**;
  target/mcp5-opening-final-tests.log. Other relevant source unchanged.
- Contract mcp_opening: **1 passed / 0 failed**, shared positive/negative corpus.
  Python jsonschema cross-check: **73 schemas**, **13 new opening cases**, all
  existing registered corpora also agree. No package installed.
- Scoped strict Clippy: Host lib, enouia-memory-mcp binary, opening_mcp,
  opening_pipe, initialized_agent_native, initialized_mcp; also Vault lib.
  Host log: target/mcp5-opening-clippy.log. No warnings.
- cargo fmt --all / final fmt --check, whitespace check, public-content scan
  (22 owned files, no private path/credential-pattern matches) and Runtime
  surface consistency check passed. Surface aggregate:
  345d086989f4d5326fb40c4ea88a4490365eb5fc278d9ab99f3e4894cf900a28.

Checks prove closed arguments/no authority injection; bounded actual recall and
its same-connection cache; current memory/no original source text; no state
publication from reads; unknown/not-found URI refusal without filesystem/network
resolution; policy/expiry/lock/deletion refusal; old capabilities stay tools-only.
Actual own-process pipes test incompatible profiles/hash/identity/write scopes
and wrong kernel peers; explicit launcher attach/EOF; full original packet on
failed stdout; no replacement packet after poison; actual EOF/no Reply when a
separately authorized synthetic report makes the Vault unknown.

First iteration's unknown regression failed because metadata polling omitted the
native recovery latch. A follow-up identified that this latch returns
RecoveryRequired after the original OutcomeUnknown, and treats both as blocked
uncertainty. Moving immediate policy refusal into the shared old poll also
changed an existing pipe-5 test's exit result; the final dedicated context poll
restores that behavior. All final targeted runs above passed.

## Evidence boundary

No full workspace rerun was warranted for this localized read/profile change;
neither 913 at f42f3b1 nor the earlier 948 run is evidence for this source tree.
No real model/Provider/embedding, private Vault, external capture hook, Runtime
repository/pin/Activity, real client configuration or Owner acceptance was used.
No forget-hide, manual status snapshot, deployment, migration or main merge.
No promise of automatic Agent prompt use, real recall quality or product adoption.
The draft depends on unmerged MCP-4 PR #84; merge requires Morii's separate order.

## Follow-up: connection isolation, source authority and budget acceptance

Implementation c34a428 is pushed as draft
[PR #85](https://github.com/Morii9961/enouia-memory/pull/85), based on PR #84's
branch. This follow-up changes tests/documentation only. It adds same-Agent
different-reader cache isolation, empty source metadata without SourceRead,
successful real MemoryRead-only pipe, and rejection of ContextRead as an alias
for this profile's native enrollment. A seven-memory synthetic set proves the
four-item bound, a one-item attention ceiling, and an empty/incomplete result
when the rendered budget is one byte. All leave snapshot, capture jobs and
Owner actions unchanged; budget exclusion creates no mandatory work queue.

opening_mcp **7 passed**, opening_pipe **6 passed**, **13 total / 0 failed**;
target/mcp5-opening-acceptance-tests.log. The two added tests and strengthened
existing cases are additional scoped evidence, not a repeated full baseline.
No production source/schema/validator changes: earlier schema, integration and
regression evidence remains applicable. Scoped Clippy and fmt/whitespace checks
passed. The follow-up public-content scan is checked before push.
