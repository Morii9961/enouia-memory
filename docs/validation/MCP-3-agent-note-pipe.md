# Explicit Agent note pipe and stdio

2026-11-03. Base `06b0e3e`. [ADR119](../adr/119-agent-note-local-transport.md)
and [pipe contract](../contracts/AGENT_NOTE_PIPE_V1.md).

memory-agent-pipe-6 binds an existing initialized reader and an independent note
writer to the retained OS Agent peer and exact hello. It reuses kernel peer,
frame and unique-JSON checks. Every exit revokes both grants and disconnects
the actual borrowed pipe. Native unknown publication exits before Reply, letting
the existing Client retain its whole sent MCP Value and refuse further sends.

The explicit launcher mode automation-notes-v1 attaches to a selected native
Host/pipe/Vault without discovery, root opening, enrollment or configuration.
Its dedicated stdio mode returns the complete MCP Value in error.originalRequest;
older bridges retain arguments-only output. Stdout failure also preserves the
full original and closes the Client. No original JSON formatting is claimed.

Actual focused results: **7 new pipe/stdio tests, 9 note/server tests and 8 old
initialized actual-pipe tests passed**, no final failures. New cases include:

- Owned Agent process sees six tools, saves and explicitly replays an independent
  hint_saved receipt with no duplicate commit. The same captured source is then
  explicitly queued by the native Runtime producer; a fake System analyzer sees
  that Agent hint exactly once and forms one real source-backed memory, retaining
  original UserStatement evidence. Note submission itself created no source/job.
- Old pipe-5, wrong hash, forged Owner hello field, missing SourceRead, old
  MemoryPropose scope and a different actual kernel peer refuse before writing.
- Writer revoke, Host lock, fake-clock expiry, enrollment revoke and policy
  withdrawal close an active pipe before note submission without a new commit.
- StagingWritten and AfterCurrent faults give the real Client OutcomeUnknown,
  preserve the complete original call, block a changed subsequent call, then
  recover and explicitly replay the original key to one receipt.
- Stdio unknown error echoes the complete call rather than only arguments.
  Stdout failure after a successful Host commit retains that call and blocks
  sends; fresh native lookup recovers the original saved receipt.
- The actual launcher executable attaches using the new explicit protocol flag
  and returns a genuine saved-hint receipt over stdio.

The prior native fixture was moved to a shared test module for both consumers;
the 9-test note target was rerun after that move. Initial test compile attempts
used nonexistent Pipe::create and initializer-only helper names. They were
corrected to existing Pipe::listen and the Host's explicit fake record fields;
no product compatibility layer was added. Final behavior tests passed.

No schema/validator/dependency changed, so the preceding 66-schema evidence is
reused. No full workspace test rerun was made for this localized transport; the
887-test baseline, 41 worker tests and 19 server checks remain distinct saved
evidence. No real Agent configuration, private Vault, model, Provider, Runtime
pin or production setup was opened or activated.

Owned files: notes native transport/export, v2 explicit full-packet stdio path,
launcher flag, note pipe tests/shared fixture, existing note test imports,
ADR119/register, protocol contracts, this report, Runtime log and generated
surface. All processes and roots were newly owned synthetic test resources.

Final fmt, strict workspace/all-target Clippy, Runtime surface and whitespace
checks passed. Aggregate:
`1ad8b056b153d4ca6960a65f0108bae7190423621317ec6d82b1091dae6af37c`.
