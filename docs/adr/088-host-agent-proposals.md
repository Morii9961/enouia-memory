# ADR-MEM-88 — Host Agent proposals and keyed receipts

Date: 2026-10-23. Status: accepted within the explicitly authorized MV-8.2 local synthetic development.

Extend the in-process Host binding with memory_propose and the existing
unadvertised memory_update alias. Keep mcp_read_server read-only and add a
mcp_agent_server constructor accepting the implemented read/propose ceilings.
Context/session tools remain unavailable. The native principal is always an
Agent; owner Workspace, reviews, identity changes, deletion and Provider send
are not forwarded. All successful writes are pending AgentProposal candidates.
No caller text becomes a manual assertion or fabricated original source.

Authorize current and cited source revisions with memory:propose, plus the
current target memory for revise/supersede/archive. All resources come from
verified records, current policies and tombstones. Source records lack project
metadata and need an all-projects source grant; a memory project never supplies
one implicitly. Propose permission does not return raw source text or imply a
read scope. New project names/details remain untrusted candidate suggestions;
only a later owner review can create or change Canonical/project records.

Use governance's existing bound proposal path over the same Host writer. Bind
the native principal, canonical memory_propose operation and unchanged caller
key to canonical arguments (excluding requestId, generated IDs and clock).
Replay checks current authorization/deletion before delivering the original
candidate revision, commit and operation receipt. memory_update shares that
namespace. Different arguments conflict; a new session never automatically
replays. A terminal or deleted candidate is refused rather than represented as
currently pending. No storage, schema or transport version changes are needed.

An Agent-specific governance entry point limits pending dedupe to the same
native principal, preventing a success/refusal oracle for other principals'
candidate contents. Existing owner/Workspace global dedupe stays unchanged.
The existing governance dedupe result has no durable receipt for the new key.
This adapter therefore returns nonretryable revision_conflict for that no-write
case, without disclosing the pre-existing candidate ID or fabricating an
operation ID. It does not advertise a successful durable duplicate outcome.
Only an actual stored/replayed receipt produces candidate_proposed.

Append a content-free admission audit before publication; failure is a known
refusal. After entering publication, ambiguous storage/authority or final audit/
freshness failure disconnects and returns outcome_unknown with the exact packet
and key. Do not retry, reconnect or choose another key. A new explicit native
admission may submit the original packet to inspect its durable receipt, subject
to current policy. This is synthetic library evidence, not external Agent
authentication, installation, private-data consent or personal acceptance.
