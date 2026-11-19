# ADR-MEM-151 — Preserve source identity across fresh plans

Date: 2026-11-18. Status: Proposed; synthetic validation only.

The authored packer may place statements that cite the same original message
in different bounded plans. Previously each plan allocated a separate SourceId.
The native capture transaction correctly rejected the second capture for the
same exact import locator, even though both plans passed pure admission.

For an overlapping fresh run, keep the first SourceId and capture the exact
source once. Later plans retain their full evidence and context closure but bind
those references to the first source revision, with all ranges and Raw anchors
preserved. Check repeated-source metadata before reuse. The native locator
uniqueness rule stays in force. Distinct original messages remain distinct,
including those in the same ADR-MEM-149 support group.

The run uses one access policy identity so the same source revision can qualify
under several bounded formation policies. Each policy still lists only its own
exact source IDs, original formation interval, subject, project, sensitivity and
allowed forms. The existing egress policy identities and policy checks apply.
No read grant is created. Fresh runs with disjoint plans keep their existing
representation.

Each catalog fragment includes only its newly captured turns. If a turn's
parent was captured in another fragment, its source record and Raw retain the
parent edge. The catalog turn uses a fragment-local parent when available. If a
batch contains only reused sources, it can form against the existing sources
when its catalog adds no entities. A fresh plan that needs a catalog-only write
is refused before enrollment under the existing native Capture contract.

## Continue an already enrolled run

Use `reconcile-source-overlap` through the trusted registered Morii CLI. It
requires the original manifest, the recorded bootstrap operation, the exact
current commit ID and the exact compact journal SHA-256. It applies to one
specific stop shape. The first batch must have captured its sources and
committed every record, the current commit must be the original second-batch
policy at revision 1, and every remaining work item must still be queued with
no request, response, binding or receipt. Native replay and exact source,
request, response and receipt checks verify the published first batch. All
formation policies must still be in their original valid intervals.

Reconciliation retains the original run hash and first plan state byte for
byte. It replaces only the queued plans, with the original second policy ID
advanced to revision 2 and the remaining policy IDs retained. It recomputes
capacity using the retained packets and the full remaining lifecycle before
archiving. Original plan objects and the original journal are kept in Raw.
The new plan retains the prior journal hash and commit so an export also
preserves the exact reconciliation basis.

One atomic journal write selects the replacement plans. The operation does
not publish a policy, capture a source or form a record. After close and reopen,
ordinary `run` resumes the original input and publishes the revised policy
through the normal capture path. Stale inputs, repeated reconciliation and
any prepared or unresolved work refuse. A failed publication with unknown
outcome requires inspection of the same journal before another action.

The operation lives in the standalone fresh initializer and keeps compact
journal version 2. Morii backup and Runtime readers use their existing formats.
