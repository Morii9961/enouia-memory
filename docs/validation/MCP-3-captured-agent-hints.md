# Captured analysis: bounded optional Agent hints

2026-11-03. Base `c6ff0cd`. [ADR117](../adr/117-captured-analysis-agent-hints.md)
and [analysis v3](../contracts/CAPTURE_ANALYSIS_V3.md).

The first captured analysis receives saved Agent hints for that exact source
revision and current policy. Selection uses the same locked native lookup as
source authorization, in stable note-key order, capped at eight and trimmed to
64 KiB of canonical JSON. The journal saves those immutable keys before handoff.
Notes do not become source evidence or Owner authority. Later notes neither
change a pending analysis nor dispatch a terminal job again.

The native CaptureAnalyzer signature explicitly adds the note slice; all seven
existing fake implementations adopt it. Journal v1 adds default/skip-empty
note_keys, retaining old jobs and original receipts without creating another
journal. Nonempty keys occur only on roots whose ADR116 note deltas already
exclude old binaries. Producer wire DTOs and states remain unchanged.

Actual focused evidence: **19 initializer formation tests, 16 Host capture/worker
tests and 6 real Owner pipe tests passed**, no failures. New cases exercise both
immediate and pending fake analysis: its start callback reads the journal and
confirms selected keys are already durable; exact source/Agent role/class are
preserved; a later hint cannot replace that set; the original source forms one
memory and later turns continue; reopen never resends. Other-source/deleted and
old-policy hints are excluded. Count and serialized-byte caps are independent,
including JSON-escaped control characters that exceed the byte budget before
the count cap.

Existing unknown/panic/timeout, cancellation, deleted-source settlement,
publication recovery, foreground/backoff, concurrent policy/apply and actual
pipe status tests pass in that same focused run. Host tests compile against the
new explicit signature. Only synthetic roots, fake analyzers and owned processes
were used; no real model, private Vault, client configuration or Runtime edit.

Final fmt, strict workspace/all-target Clippy (including every adapter/test
signature), Runtime surface and whitespace checks passed. Aggregate:
`3857a33e4d15293c334b529208f9e04c87048181bad5e5ad827566045a07d46f`.

The 887-test workspace and 65-schema evidence from the preceding slice remains
the baseline. This localized worker change does not rerun unrelated workspace
test targets or the schema cross-check; schemas/validators/dependencies did not
change. Owned files are formation_worker.rs, the three affected fake/test files,
ADR117/register, analysis contract, captured queue/worker contracts, this report,
Runtime log and regenerated manifest. System hint consumption is now exercised;
an explicit MCP write tool and real model quality remain separate work.
