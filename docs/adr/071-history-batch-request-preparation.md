# ADR-MEM-71 — Prepare one saved history batch for inspection

Date: 2026-10-20. Status: Adopted within MV-7; synthetic verification only.

Add keyed synchronous Workspace command `history_batch_prepare` (53 commands).
The owner selects a saved task ID, one original batch index, an egress policy
ID and output-token ceiling. The Core derives the coordinator, child Session,
run and input from the immutable admission and canonical receipts, revalidates
the full saved selection, and requires a ready child with no prior invocation.
The configured exact Provider/model must match the saved binding. The page
cannot substitute a child, model, source, subject, endpoint or actor.

Reuse extraction preparation and the existing exact-body inspection, temporary
call handle, TTL and `external_send` confirmation. Preparing may persist the
existing Context capsule/inspection, but never reads credentials or transports
bytes. Sending remains a separate explicit owner action; this implementation
is verified with fake ports only. No job is recreated and no candidate is applied.

A successful preparation also records the command key and exact argument hash
through an ordinary SessionAppend receipt while revising the same child Session
header (only revision/updated_at change). This provides durable request binding
without a side database, new storage field or fake invocation. Replay checks
the saved receipt, all live provenance and the job's uninvoked state before
rebuilding a fresh expiring handle. Changed arguments under that key conflict,
including after reopening. A lost/expired handle is not a send authorization.
The first compiled request remains subject to existing input-bound compilation;
incompatible output changes can require a new explicitly selected local task.

Existing job/session quotas, source policies, revocation, deletion, lock and
no-resend rules remain in force. Completed, failed, cancelled or unknown prior
invocations cannot be retried by preparing this batch again. Runtime must adopt
the exact revision and expose distinct prepare/inspect/send states. No Runtime
UI, personal-history use, real account, automatic batch execution, MV-8, MCP or
cloud activation is included.
