# Explicit fresh visible-text view — synthetic validation

2026-11-16. [ADR146](../adr/146-explicit-visible-text-view.md), based on merged
fresh-driver main `442b22fa16881aae70f6f17c37b0c41464777ad6` (PR109).

## Behavior

`SourceInput.visible_text_only` defaults to false. Explicit selection keeps the
mixed ChatGPT source `Partial` and every present/missing/external attachment state.
It adds the closed `memory-init.visible-text-view` declaration for all string
parts, not a completeness upgrade or an attachment-content assertion. Native
import verifies the original Raw hash, exact content locator, complete newline-
joined string parts, source/conversation identities and speaker before capture.
Image-only/empty views, changed/truncated text and forged scope/identity refuse.
Unselected Partial still fails the ordinary native admission check.

Fresh manifests additionally accept:

- `context_quotes`: per-statement lists of `{source_index, quote, start, end}`.
  Indexes refer to the explicit source table; offsets count Unicode scalars.
  The exact range must resolve against a declared dependency. It is retained in
  the plan's analysis artifact, outside factual support. A different range from
  that same source can separately remain direct evidence without promoting this
  contextual range. Packing remaps source indexes and retains statement positions.
- `authored_records`: an explicitly named local JSONL input with its SHA-256,
  plus `record_origins`, one explicit line index per statement. Selected rows must
  have ID/revision and unchanged content/title. The exact selected file bytes are
  archived once per hash as normal Raw analysis material; the plan retains hash,
  line index, record digest, original ID and revision. Original provenance, layer,
  cautions, time basis and attachment-review metadata remain available unchanged.
  This is neither an imported database nor a substitute for witnessed formation.

The complete selected analysis file, including any unselected lines, is retained.
Neither it nor selected Raw conversations are redacted by source-table filtering.
All these inputs remain data, with no Owner confirmation, Provider invocation,
attachment reading permission or long-term read grant inferred from them.

Existing manifests without new selections retain default refusal and the prior
packing watermark shape. New input bytes, ranges, metadata or source-view choice
remain bound to the complete run. Journal limits and receipt-only uncertainty
barriers are unchanged; no completed history is pruned.

## Evidence

Focused new synthetic checks:

- `visible_text_view_opt_in_preserves_partial_and_native_raw_proof_refuses_forgery`:
  default Partial refusal; explicit complete text selection; native formation,
  close/reopen and exact evidence resolution; unchanged Partial and external/
  present attachment states. Forged text, message/conversation identity, speaker,
  locator, scope and missing Raw are rejected without a new Vault commit. Actual
  journal size stays within the complete lifecycle bound.
- `precise_context_and_original_authored_records_are_bound_and_archived_without_promotion`:
  exact context ranges both on a context-only dependency and a separately quoted
  source; unchanged direct evidence count; exact archived original analysis bytes,
  explicit reversed origin indexes; bad context anchors, changed content and
  changed original provenance conflict/refuse. Original unknown model remains null.

Final checks actually run (all test input, roots and metadata synthetic):

- `cargo fmt --all -- --check`: **passed**.
- `cargo test --workspace --locked`: **1,082 passed, 0 failed, 6 ignored**,
  across 142 test/doc-test targets. The driver target contains **8 passed,
  1 ignored** including both new native cases and the close/reopen unchanged-run
  assertion for context/original-record metadata.
- The ignored `trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run`
  test was explicitly run with a selected durable synthetic parent: **1 passed**.
  Actual Windows-account bootstrap prepare/commit and the production fresh CLI
  form two synthetic records including the selected mixed text view, reopen,
  repeat without multiplication, explicitly recover and refuse changed analysis.
  The uniquely allocated protected synthetic home is retained; no real owner
  inputs/home/Vault are used. Its private local location is omitted here.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: **passed**.
- Existing repository virtual-environment Python schema cross-check:
  **79 schemas and fixture corpora passed**, using python-jsonschema 4.26.0.
  System Python lacked jsonschema; no dependency was installed. Wire/schema
  fields are unchanged; the native source-view validator is independently tested.
- Runtime surface regeneration/check and `git diff --check`: **passed**.

The first full run stopped at the driver CLI subprocess with `fresh.manifest_shape`
while the in-process loader accepted the same fixture. A concurrent independent
worktree explicitly shared the target directory and replaced the normal CLI with
the older manifest implementation. After that build ended, the current CLI was
rebuilt, the exact failed check passed, and the complete workspace rerun above
passed. This is not evidence that the initial failed invocation passed.

Review also found that hashing the retained native context SourceIds would change
on plan rebuilding. The final implementation hashes authored indexes/ranges and
origin bindings while retaining the original native references in the saved plan;
the new reopen/resume test proves no false conflict or duplicate work. The original
reserved namespace attempt was refused; the final declaration follows the existing
`memory-init` extension namespace without weakening namespace validation.

## Scope limits

Native Raw proof establishes the complete selected text view, not semantic
entailment, visual understanding, file authenticity beyond the explicit hashes,
or human acceptance. Raw attachment bytes are not analyzed by this entry.
Runtime needs a compatible new pin before replaying these memories; its completed
previous adoption remains completed. Workspace remains at 61 commands with no
UI/IPC change, and no Runtime checkout was modified.

New code is used only with synthetic fixtures before merge. This PR does not
create/write a real Owner Vault or complete corpus-wide native capacity,
formation, reopen/query/source acceptance, independent backup or product acceptance.
It does not deploy, delete protected data, refresh manual status snapshots or
start a new MV stage. Private actual-intake mapping findings remain outside Git.
