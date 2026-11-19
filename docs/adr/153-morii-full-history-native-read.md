# ADR-MEM-153 — Morii's own native reads are not limited by formation windows

Date: 2026-11-19. Status: Accepted (Morii's decision); synthetic validation.

## Context

Every memory is formed under an `AutomationPolicy` whose `valid_from`/`valid_until`
bound formation. The native System read (`SegmentedAutomationVault::read`,
`read_process`) also required a policy that is active and inside its window. Fresh
initialization uses seven-day formation policies. When they end, the native System
reader that Runtime uses on the registered read-only entry loses almost every
memory, except those formed under Morii's own permanent policy.

ADR-MEM-141 kept that rule for existing readers and added explicit, time-bounded
read grants for Agent and remote use. Morii decided on 2026-11-19 that this is his
own software and he must always be able to read everything: "这个软件就是我自用的
东西，必须保证我自己有完整的访问权限，可以随时看到所有的内容".

## Decision

Add `SystemReadScope` with two values:

- `CurrentPolicy` is the existing behaviour.
- `MoriiFullHistory` authorizes a native System read against every committed
  policy revision (`policy_history`), regardless of `active` or the formation
  window.

`read`/`read_process` keep `CurrentPolicy`. New `read_scoped`/`read_process_scoped`
take the scope. In `MoriiFullHistory`:

- Each memory is read under its own recorded policy revision, the same
  independent-read rule ADR-MEM-141 uses for grants.
- Process reads (activity, history and change cursors) use the same rule.
  Their permission hash binds to the full policy history.
- Still enforced: source deletion, memory deletion, withdrawal and forget
  barriers, subject/project/access/egress matching against the memory's policy,
  and the caller's sensitivity ceiling.

`SegmentedMemoryReader::from_authenticated_system_full_history` selects this scope.
Its identity check is the same as `from_authenticated_system`. It is meant only
for Morii's own native process after the registered bootstrap authentication.
Agent hosts, MCP, remote service and read grants are unchanged. No grant is
created and no formation authority is extended. Expired policies still cannot
form, revise or capture anything.

## Consequences

- Runtime's registered read-only entry should construct the full-history reader
  so its catalog, detail, source, process and recall views stay complete after
  formation policies end. Its sensitivity ceiling (`HighlySensitive` since
  Runtime #211) remains its own choice.
- No persisted format, commit type, IPC command or wire change. The Runtime
  surface digests of `catalog.rs`, `segmented_agent.rs` and the vault read path
  change; Workspace stays at 61 commands.
- ADR-MEM-141's "existing entries keep requiring the current formation policy"
  no longer applies to this Morii-only reader.

## Verification

Synthetic segmented Vault with 12 records and fixture policies ending at
2026-10-25T00:00Z, clock moved one day past expiry:

- The current-policy reader shows 0 records.
- The full-history reader shows all records except one deleted beforehand.
  Stats, the activity feed and the recall preview also work, and the Public
  ceiling still hides Private records.
- Locking still refuses reads.

Existing vault and host read-path tests pass unchanged.
