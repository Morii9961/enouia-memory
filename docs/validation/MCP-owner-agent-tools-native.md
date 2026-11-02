# Owner-root native Agent read methods: checkpoint 3

2026-10-29. Branch `codex/mcp-owner-agent-detail`, base
`85911a2a86a63708b65a82de3957905a794ba20b`. ADR-MEM-105 checkpoint 3.

## Change and capability matrix

The existing sealed `memory-agent-qualified-native-read-1` delegation now has
detail, source, related and recall methods in addition to the full-field page.
Every method enters the same current Agent policy/source/instance/lifecycle
gate. No System reader or Owner action port is forwarded. Arguments cannot
select identity or authority. This is trusted native embedding, not OS enrollment.

Detail returns the qualified current item and original complete revision fields
in history, with safe scoped receipts. Historical revisions do not acquire
current Owner badges. Current source-read permission controls original source
metadata/text, independent of memory evidence access. UTF-8 excerpts and whole
responses are bounded. History/source counts and incomplete coverage remain
explicit when a response budget removes material.

Relations use currently permitted endpoints and recomputed Owner entity overlays.
Recorded lineage/entity mentions are separate from algorithm theme suggestions;
neither is independent factual verification or a causal claim. Candidate/retained
counts describe the supported relations in the delegated view. Same-conversation
relations require their own Agent qualification and remain explicitly unavailable,
as do semantic neighbors. No broad conversation metadata is borrowed.

Recall supplies catalog-4's qualified attention states to a new pure native
selector. Old selectors keep their age-based behavior. Owner protection cannot
bypass adoption, conditions, validity, deletion, source authorization or budgets.
The whole permitted candidate set is ranked without retaining every full rendered
record; only selected full records/context survive. Exclusion reason counts are
exact and examples bounded to 128. Context returns the actual same response and
trace, checking current scope/snapshot before access; it never silently reruns it.
These calls do not reinforce attention or write business files.

| Consumer / root | Result |
| --- | --- |
| Existing schema-1 / Host V2/V3 / segmented MCP profiles | Unchanged |
| Qualified native profile / automation-segmented-owner-v1 | Page, detail, source, related, recall and same Context response |
| Qualified profile / older or unknown root | Refuse at existing descriptor boundary |
| Owner root / old Agent or Runtime capture profile | Existing early refusal retained |
| Source text without source-read | Refuse; memory evidence access is unchanged |
| Unselected/unknown memory or source ID | Absence within a live permitted read |
| New-root MCP tools/pipe/output schemas | Not advertised by this checkpoint |
| Initializer import/attachment extension | Not adopted here |

## Actual verification

Five new synthetic native tests and five existing qualified-page tests passed.
They use newly created temporary Owner roots, real legal formation receipts and
native Owner confirmations. They cover independent source permission, Chinese
UTF-8 boundaries, indistinguishable unselected/unknown sources, 100 detail reads
with all business bytes unchanged, original history fields, actual-byte truncation,
qualified Owner protection with unchanged knowledge, exact same Context response,
deletion and lock invalidation, scoped relation endpoints/counts, no global receipt
sequence, old selector behavior, exact attention-input length and budget refusal.
The protected staged candidate remains excluded as not adopted.

The complete pinned offline/locked workspace run exited 0 with its full doctest
tail: 785 passed, zero failed, four existing ignored scale/example tests. It
includes the native Owner-consent compile-fail case. Workspace all-target strict
clippy, fmt, Runtime manifest/log and exact nine-file public allowlist checks
passed. Pinned Python 3.12 / python-jsonschema 4.26.0 passed all 60 schemas and
their corpora, accepted six fresh actual Agent exchanges and two Runtime
preflight/capture exchanges collected during that workspace run, and rejected
one forged output in each transcript. These are regression evidence for existing
pipe profiles, not new-root MCP transport acceptance. The final one-shot helper
finished; log/exit markers and synthetic transcripts remain under ignored
`target/mcp-segmented-validation/owner-tools-*`.

The Runtime surface was generated and logged before workspace verification:
`835ab41a912f0685d678725acd936e93396d003fe7eeee10c1047c3ccbb9552c`.

## Remaining Runtime 03 requirements

This native checkpoint still needs an explicitly negotiated new-root MCP transport,
five tool output schemas/annotations and actual independent-process acceptance.
Same-conversation relations need separate qualification. Initializer carrier
adoption, Runtime-only capture-to-live formation, correction/supersede/forget
requests, session_start and resource links remain separate work. No Runtime
checkout, real hook/client configuration, private root, Provider/embedding,
deployment or main merge is touched.

The initialization session confirmed its published d11b7a0 is not an Owner-root
compatibility candidate. Its exact 14125f5 integration is still uncommitted and
not adopted here. It also has no separate enqueue-from-already-captured-refs
entry yet. Future live integration must preserve original SourceRevisionRef and
capture keys rather than recapture Runtime material as an import.
