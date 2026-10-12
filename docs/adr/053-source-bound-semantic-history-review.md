# ADR-MEM-53 — Source-bound semantic history review

Date: 2026-10-12. Scope: MV-7-H2d, explicit native semantic review, synthetic activation.

Create an owner-selected exact-topic analysis task from saved completed history
responses. Bind original suggestions, response hashes and all selected source
spans/context; exclude mutable candidate application status/owner edits from the
immutable model input. Limit unique spans to 16, selected text to 64 KiB and the
whole task JSON to 64 KiB. Refuse overflow instead of omitting coverage.

Use ordinary source-linked Session input and the existing native Provider ledger,
Extraction-purpose context compiler and exact-body/grant/host-quota/one-attempt
send path. No new persisted record, ExtractionJob version, wire command or SDK.
Creation/prepare/preview never sends. Reopening with the same owner key/options
returns the same task; failed/unknown invocations do not authorize retry.

The analysis response is a strict versioned JSON object with bounded proposed
changes (new_information, correction, cancellation, conflict, uncertain) and
topic summary points. Each statement has exact selected-source quotes. Validate
the whole response before returning any result. Preserve unknown occurrence time,
speaker/evidence class, full selected text and source branches in resolved cites.

Correction/cancellation require user evidence on both sides and proven directed
source ancestry within the selected graph; otherwise the model must use conflict
or uncertain. They never establish business effective time. For Preference,
non-user-only evidence cannot support a definite new/corrected preference.
Cross-branch comparisons remain review hints, never supersession permission.

Interpretations and summaries are derived model claims, not approval, Canonical
records or owner-accepted truth. This API does not publish candidates. Any later
replacement still uses explicit target/time and trusted owner governance.
Quote/structure validation does not prove entailment or completeness; controlled
fake responses test gates, while actual model quality and important omissions
remain pending. Source/context secrecy checks remain unchanged.

Reload tasks against current source hashes/policy and saved original response
hashes, including uncited slices' latest Source revision. Hosts use
`AnalysisGuard` with Client.guard and VaultAdapter as Client.journal: each guard
pass reloads the task before ordinary egress checks. Empty responses retain
explicit uncited coverage. Return only while pinned head/refs and native enabled
flag remain current.
No private-history discovery, real Provider call, credentials, deployment, Runtime
product UI, automatic acceptance or later MV-stage activation.
