# Fresh native driver — synthetic validation

2026-11-15. [ADR145](../adr/145-fresh-native-driver.md), based on merged main
`eb7050961cceaac04d4aec79257524331be6c7b0`. Only synthetic input is processed.

## Interface and retained boundaries

Package `enouia-memory-init` adds `enouia-memory-fresh`:

```text
enouia-memory-fresh plan <manifest.json>
enouia-memory-owner-bootstrap prepare <MORII_DATA_HOME> --bind-current-owner-self
enouia-memory-owner-bootstrap commit <MORII_DATA_HOME> <OPERATION_ID> --bind-current-owner-self
enouia-memory-fresh run <manifest.json> <MORII_DATA_HOME> <OPERATION_ID> --bind-current-owner-self
enouia-memory-fresh recover <manifest.json> <MORII_DATA_HOME> <OPERATION_ID> --bind-current-owner-self
```

The bootstrap binary belongs to `enouia-memory-host`. These are trusted local
native commands, never page/MCP endpoints. `plan` receives no home/root and writes
nothing. Its JSON reports batch preflights, explicit unavailable attachment slots,
whole-journal queued/lifecycle bytes and `ready_to_run`; false capacity readiness
must not be read as success. It proves structural/native-rule admission against
the supplied formation interval, not semantic entailment or human acceptance.

`driver::FreshManifest` is the closed `enouia-fresh-input/1` input shape. The
synthetic builder in `tests/fresh_driver.rs` is an executable complete example:

- `shards`: opaque alias/account, exact file path/hash, exact conversation IDs.
  Relative paths resolve against the manifest directory. No directory discovery.
- `sources`: explicit account/conversation/message, derived Raw envelope hash,
  text hash, sensitivity and every parsed attachment slot. `original` names a
  local original path/hash; null explicitly retains missing/external availability.
  No URL fetching, filename inference or implicit missing-attachment success.
- `analysis`: the existing `AnalyzedBatch`, including truthful actor, prompt
  version and nullable model version. Indexes refer to the source table. Preserve
  all content, direct and context dependencies, entity reality/roles and times.
- `quote_ranges`: one list per statement, primary then supporting quotes, as
  Unicode-scalar `[start,end)` pairs. Exact text, source revision and every range
  must resolve; even repeated wording retains its explicitly selected occurrence.
- `formed_at` / `formation_until`: explicit seven-day formation interval;
  `project_id`: explicit project or null. No long-term read authority is created.

The run hash binds exact manifest bytes; original shard/attachment hashes are
verified before selection. Changing paths, selection, source revision, analysis,
anchors or attachments cannot append another run to the same root. Resume uses
persisted plans/packets/keys. Unknown application resolves only the original
receipt; absent truth remains an error, without a replacement key or resend.
Explicit recovery checks the run binding before native forward recovery. Receipt
resolution does not renew formation authority; new formation remains subject to
the original native policy expiry. Existing receipt lookup also retains its
authorization gates: after policy expiry it can withhold an original receipt.
That case remains `OutcomeUnknown` with the original packet/key/binding; the
driver neither widens reads nor changes the input to resend it.

Selected Raw retains the entire selected conversation, including surrounding
messages and attachment references not represented in the source table. This
driver performs no redaction. Unselected source records do not make Raw private
context disappear. Incomplete sources remain refused by native admission; no
completeness or confirmation is invented to make an input pass.

## Checks actually run

- `cargo test -p enouia-memory-init --test fresh_driver --locked -- --nocapture`:
  **6 passed, 1 ignored** (the explicit durable-home rehearsal below). Covers
  complete no-write CLI preflight; 65 sources in two bounded plans; table-order
  remapping; identical quoted text at distinct exact ranges; entity evidence on a
  later occurrence; context closure/sensitivity; attachment originals/unavailable
  slots; revision/missing/anchor refusals; native formation, close/reopen,
  paginated native catalog/detail/source excerpt reads; no multiplication;
  changed-run refusal; receipt-only uncertain states with and without a receipt.
  Policy-expired lookup remains blocked under existing native authorization.
- The ignored `trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run`
  test was explicitly run with `ENOUIA_SYNTHETIC_DURABLE_PARENT` pointing at a
  selected durable local test parent. **Passed**: actual Windows-account native
  prepare/commit, registered-root lookup, production fresh CLI formation, repeated
  invocation with three unchanged records, close/reopen/exact sources, explicit
  recovery, and changed-analysis recovery refusal without another commit. The
  uniquely allocated protected synthetic home is retained; no existing Morii
  home or real Vault was used. Public reports omit its private local location.
- `cargo test -p enouia-memory-host --lib owner_bootstrap --locked`:
  **15 passed, 54 filtered out**. Includes the added recorded-operation/root
  authentication check; synthetic enrollment cannot pass the production entry.
- `cargo test -p enouia-memory-init --test initialization --test packing
  --test fresh_chatgpt --locked`: **23 + 5 + 4 passed**. Existing authored/source/
  packing/native recovery behavior remains covered by these focused regressions.
- Scoped initializer library/new binary/driver-test Clippy with `--no-deps`
  and `-D warnings`, targeted rustfmt, `git diff --check`, and Runtime surface
  regeneration/check: **passed**. No schema or validator changed.

Measured whole-journal sizes (bytes, every completed entry retained):

| Synthetic case | Queued | Lifecycle bound | Actual completed / result |
| --- | ---: | ---: | ---: |
| 65 statements, 65 sources, 2 plans | 1,005,459 | 1,724,644 | 1,717,486 |
| 256 larger statements, 2 plans | 9,797,862 | 20,294,922 | Refused before archive/capture |

The limit remains **16,777,216 bytes**. The upper bound serializes seeds, requests,
escaped native decisions, bindings and receipts with maximum-width counters and
longest work state. The test checks actual journal size after every execution
step. Passing a queued journal or individual statement budget is insufficient.

Initial fixture iterations exposed the intentional incomplete-source refusal for
image body parts and a catalog page-size assertion that assumed one page held
all 65 items. Fixtures now distinguish complete text with attachment metadata and
follow native cursors. Sandboxed Windows native creation returned `Denied`;
native checks passed under the actual account outside that sandbox. No production
admission, completeness, pagination or ACL rules were weakened for these tests.

## Remaining gates

This is a reviewable synthetic implementation, awaiting Morii's merge. No real
corpus was passed through the driver; its actual aggregate capacity is unknown.
There was no real Vault formation, source acceptance, Provider call, read grant,
backup, deployment, Runtime checkout change, data deletion or status snapshot.
No full-workspace suite or Linux run was performed. Runtime surface change is an
additive Host helper only; the completed previous Runtime adoption remains valid.
