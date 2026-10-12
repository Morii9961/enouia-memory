# ADR-MEM-52 — Source-linked topic review briefs

Date: 2026-10-12. Scope: MV-7-H2c, deterministic native review navigation.

Extend read-only history review with per-run coverage: saved response hash,
application state/cursor, selected Source revisions/spans/hashes, exact selected
context and the response indices that cite each slice. Re-resolve every selected
slice under current SourceRead policy, including slices with no suggestions.
Require the resulting source/snippet hashes to match the extraction job.

Coverage describes explicitly selected completed responses only. A zero-candidate
response or an uncited source is a visible review gap, not proof of irrelevance,
completeness or deletion. Count repeated source spans separately from unique
spans; do not claim all received history was processed. Metadata for source time,
capture time, speaker, evidence class and branch remains distinct.

Produce an optional bounded Chinese Markdown brief in memory, retaining coverage
even when the owner filters by an exact TopicKey or no topic matches. Each topic
contains original suggestions/citations, separate current candidate edits/status
and H2b comparison hints. All imported/model/owner text is serialized as JSON in
code blocks; none becomes a report heading, HTML or an instruction. This is a
navigation dossier, not a generated summary or a Canonical source.

Default output cap is 128 KiB, hard maximum 2 MiB. Use a bounded serialization
writer; overflow refuses the whole result without truncation or persistence.
Return the base commit and content hash only after head/refs and native enabled
flag are rechecked. No network, file write, automatic acceptance, new stored
schema, wire command or Runtime product UI. Later semantic synthesis and real
model/full-history quality remain pending.

Multiline synthetic source tests exposed duplicate serialization: the extraction
JSON query was also embedded as a recent Turn's JSON string, producing escaped
backslashes that the existing Provider scan refused as a UNC path. For Extraction
purpose only, rendered recent Turn text now uses a fixed placeholder while
retaining event IDs/roles and source provenance. Stored capsule Turn text remains
intact for exact-input validation and freshness dependencies. Exact source
content remains in query, supplied once as the user request. Other purposes
retain full rendered turn text. Source selection and secret/path checks are unchanged; this does not
relax the scan or authorize content from unrelated turns.
