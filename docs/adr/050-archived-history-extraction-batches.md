# ADR-MEM-50 — Archived history to resumable extraction batches

Date: 2026-10-12. Scope: MV-7-H2a, offline orchestration. No Host/MCP or cloud activation.

## Decision

Bind an explicitly selected list of H1 batch indices to one completed import.
Re-read its archived object with the existing size bound, re-probe the adapter,
and require the observed adapter/schema, source locator, content hash, role,
time and current source revision to match. Imported originals, not caller text,
define the snippets. Each batch includes its preceding same-path message as
context; all selected snippets pass the existing extraction access/size/secret
checks before a coordinator is saved. An unsafe or oversized selection fails
without silently skipping messages.

The immutable coordinator is an ordinary owner Session input containing
canonical `history-extraction-plan-1` metadata: import/raw hash, adapter/schema,
H1 planner version/limits, exact ordered batch indices, source revision and
snippet hashes, destination/subject and per-job bounds. Its source_refs contain
every selected dependency. It is provenance-linked content, not a new record,
approval, external wire contract, or instruction to the model. No permissions
are read from this metadata. Normal source deletion therefore also reaches the
coordinator event and content.

The owner key binds the coordinator Session/input through existing idempotency.
Changed plan/selection/model/bounds under that key is a conflict. Each child
uses a distinct deterministic key derived from owner key hash, coordinator
input hash and batch index, and is an existing ExtractionJob in its own Session.
After partial publication or reopening, materialization replays these receipts;
it never sends, refunds an attempt, or replaces a failed/unknown invocation.
The caller must retain its original nonsecret idempotency key and options.

Progress is reconstructed from the child jobs and invocations. The first
unfinished selected batch is navigation, not authority to send. Completed,
paused and ready jobs remain distinct from completed, failed, cancelled and
unknown Provider invocations. Per-job limits do not become a new global account
budget; exact request inspection, grants, existing host quota and explicit
one-attempt sending still apply separately to every child.

## Bounds and remaining work

At most 32 explicitly selected batches, 1–15 messages per H1 batch plus one
preceding context message, existing 32 KiB per snippet / 64 KiB per extraction,
and at most 128 KiB coordinator metadata. Reject duplicate or invalid indices,
empty source text, oversized input, changed/missing sources, bad graph batches,
unsupported adapter changes and non-owner callers. Cancel between child jobs;
partial coordinator/child commits are recoverable, not a failed all-or-nothing
transaction. No automatic retry of model requests follows recovery.

H2a proves source binding and offline restart/cancellation/replay only. Semantic
change proposals, topic reviews, real models, three-year scale/cost and Runtime
entry points remain later H2 work. The workspace IPC is unchanged; the watched
Provider source surface is regenerated and logged for Runtime pin adoption.
