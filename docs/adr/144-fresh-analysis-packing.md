# ADR-MEM-144 — Pack supplied fresh analysis without losing evidence

2026-11-14. Fresh corpus analysis can exceed the initializer's 64-source or
128-statement plan bounds. The legacy packer intentionally omits cross-batch
context and cannot be reused for newly authored multi-source records.

## Decision

Add a pure `packing::pack_analyzed` entry. Take an already prepared source set
and an explicit `AnalyzedBatch`. Group statements in caller order, keeping every
primary quote, supporting quote and context source in the same plan. Remap only
source indexes. Preserve authored content, provenance, source values, exact
locators, original parent references and attachment carriers. Use no analyzer,
filesystem discovery, policy, Owner action or Vault write.

Each statement must fit the existing source bound on its own. Refuse the entire
preparation when a dependency is missing, a source or Raw object is oversized,
or an individual statement cannot fit. The caller must explicitly revise the
semantic units; never truncate a source, silently omit context, concatenate
quotes or raise a contract budget. Statements that revise or derive from other
analysis positions require a separately authored batch; this entry refuses
those links rather than changing their meaning across plans.

Bind the parent selection watermark, all selected source values, and each
batch's source/statement positions into its selection watermark. Authored
analysis remains separately hash-bound by `Plan::build_analyzed`, so changed
wording retains the existing changed-analysis refusal on resume. Unused source
objects are excluded; used Raw objects retain their original exact bytes and
may contain surrounding selected conversation context. This does not redact
Raw or replace explicit source-selection/privacy review. A source used by more
than one plan remains the same upstream origin; it is not independent support.

Keep original intake counts at the caller. Do not multiply preparation-wide
unsupported/duplicate counts by the number of packed plans. The returned
statement-index map supports coverage checks without putting private text into
public reports. Packed plan preflight still runs through ordinary admission.

## Scope and verification

Use synthetic source/statement boundaries, cross-conversation support, all-quote
and context preservation, exact Raw/locator checks, source-scope watermarks,
capacity and linked-statement refusals, and native plan construction. No schema,
wire, dependency or Runtime integration surface change is intended.

Production use waits for Owner merge. Semantic corpus review, explicitly
selected attachment bytes, bounded journal execution, the production fresh-input
driver, native bootstrap/reopen/read acceptance and independent backup remain
separate. This is a lossless plan packer, not completed real initialization, a
Provider call, a read grant, deployment or deletion.
