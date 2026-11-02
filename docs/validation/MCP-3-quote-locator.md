# MCP-3 checkpoint 3: original-text quote location

2026-10-28. Branch codex/mcp-quote-locator, base
ddf7da5749778ec3115f2241f13433b91aac9e6a. ADR-MEM-102 and
[MCP_QUOTE_LOCATOR_V1](../contracts/MCP_QUOTE_LOCATOR_V1.md).

## Delivered and compatibility

Pure memory-quote-nfkd-1 returns original captured text and Unicode scalar
offsets after bounded NFKD/whitespace/explicit quote folding. It preserves
normalization provenance, refuses ambiguous repeated spans, partial ligature
expansions and noncontiguous reordered-mark matches. Every accepted original
slice is independently normalized back to the complete hint.

| Consumer | Support / refusal |
| --- | --- |
| New native text helper | Explicit bounded location, no authority or IO |
| Existing QuotedEvidence validator | Exact text, revision/hash and actual speaker/class remain required |
| Existing MCP six tools / newer read tools | No added tool or layout adoption |
| Native Runtime capture pipe | Existing source-only v1 unchanged |
| Agent memory_note / correction | Not delivered; current-policy binding and shared live intake pending |

ICU4X icu_normalizer 2.3.0 already existed in Cargo.lock; this slice adds the
explicit exact Host dependency with compiled data only. No new package versions
are resolved. No System read port or Owner action is exposed.

## Actual evidence

Eight focused Rust tests passed. They exercise full/half-width, whitespace and
Japanese/curly quotes, emoji scalar offsets, half-width voiced kana, Hangul,
canonical combining order, compatibility expansions, repeated/overlapping
ambiguity, preserved case/punctuation, missing quotes and input/evidence limits.
A 4,096-case mixed Unicode corpus checks mapped ordering against ICU's whole
NFKD implementation. Located original text passes unchanged strict evidence;
normalized replacement text, spoofed speaker/class and changed source revision
fail that validator. These are synthetic pure helper tests, not a real MCP
memory_note or model-quality result.

Strict offline/locked workspace clippy passed. Initial full testing failed
during compilation because the disk filled; no pass is claimed for that run.
Only old, regenerable test executables inside this worktree's verified target
were removed after checking no owned test process was active. Evidence and all
source/root files were retained.

The disk-space retry then failed one existing native Agent policy/source test
at its authorized-read assertion. Exact focused reruns passed (one test each).
No permission/timeout/retry behavior was changed. The owned native Agent test
helper now reports a content-free transport failure class instead of silently
turning a failed exchange into JSON null; the authorized-read assertion also
keeps its actual synthetic response for diagnosis. A further complete run with
those diagnostics passed the Host group but was interrupted later in Workspace
without a final exit result. It is not counted as a completed pass. A one-shot
local helper now records the independent final rerun and its exit status. This
is not a claimed permission bug fix.

The final one-shot offline/locked workspace run completed with exit 0 and full
doctest tail: 752 passed, zero failed, one existing ignored benchmark. Evidence:
ignored target/mcp-segmented-validation/quote-workspace-final-once.txt and its
exit file. The one-shot helper exited; no scheduled/recurring task was created.
Rust 1.98.1 fmt, strict all-target workspace clippy (including the diagnostic
change), Runtime surface/log and public owned-file checks passed. The earlier
failure and interrupted attempts remain separate; they are not added together
to manufacture the final count.

No JSON Schema or schema-validator change is made; no new independent schema pass is
claimed for this checkpoint. Runtime surface is regenerated/logged at
3a3055e9f59468e108f1107c39ed7166fceff94b1e79e2ae74023644187cbeb9.

## Requirements 03 differences and next binding

This supplies the normalization building block from section 3.1. The tool must
still obtain already captured sources through independent current Agent gates,
bind actual provenance and submit only optional Agent hints to the same ordered
initialization/live formation entry. No Agent hint becomes Owner confirmation.
The published shared initialization entry has not been adopted here, and the
normalizer is not advertised as a working memory_note tool. Real local MCP
capture-to-formation/quote acceptance and UI field coverage remain pending.

Owned files: Host Cargo/lock, new quote helper/tests and module export, minimal
diagnostics in the existing owned native Agent test helper;
ADR102/contract/report plus Runtime surface/log. Initialization, owner actions,
strict evidence validation, AGENTS and manual project state are untouched.
No private Vault/export, real Provider/embedding/client configuration, Activity
input, Runtime edit, remote listener, deployment or main merge.
