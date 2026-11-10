# Legacy library re-initialization (ADR-MEM-135)

2026-11-10, branch `claude/legacy-reinit-initializer`.

## Synthetic evidence

`crates/enouia-memory-init/tests/legacy.rs` (Windows and Linux):

- `legacy_candidates_become_adopted_memories_in_a_bootstrapped_root` covers
  these cases:
  - An old source with no message in the export is reported missing.
  - A rejected candidate and a candidate without matched evidence are skipped
    and counted.
  - A candidate citing two conversations keeps the second as a context
    dependency when both share a batch, and drops and counts it when they do
    not.
  - A message longer than 500 characters is excerpted to the cited sentence.
  - A multi-line quote still gets a valid title.
  - Both memories are adopted in a root the bootstrap created, with no model
    invocations.
  - Adopting the used root again is refused.
- `a_long_user_only_conversation_splits_across_batches`: 40 owner turns of one
  conversation, whose assistant replies were not kept, split 32 + 8 into two
  batches. The Vault accepts the split conversation.

Checks: `cargo test -p enouia-memory-init` (all suites pass), clippy on
`enouia-memory-init` and `enouia-memory-cli` with `-D warnings`, `cargo fmt`
check. The Runtime surface is unchanged (aggregate `31fc7e44`).

## Real dry run (no writes)

Run on a working copy of the owner's v1 library: `legacy-snapshot`, then
`enouia-memory-reinit plan`. No Vault was written. Counts only:

| | |
|---|---|
| old sources / matched in the export | 731 / 731 |
| candidates / statements (self-reports) | 185 / 185 (185) |
| planned batches / sources / statements | 16 / 731 / 185 |
| staged (not adopted) | 0 |
| quotes excerpted to 500 characters | 10 |
| other citations kept / dropped | 262 / 107 |
| unparseable / unsupported inputs | 0 / 0 |

The real run (`enouia-memory-reinit run`) needs a freshly bootstrapped Owner
root. Where it runs is Morii's decision.
