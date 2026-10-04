# MV-6 candidate drafts and post-write refresh

Date: 2026-10-04. Base: `3f4b612`. Scope: candidate creation/review UI; Core review approval remains unchanged.

The deferred production Review page passed **3/7** targeted checks. Although its retry already kept the original payload/key, acknowledging that old request cleared newly edited text and claim fields. The nested follow-up list read also lost the outer write's busy state before candidate refresh finished. A partially edited draft lost its changed field too.

Candidate list reads now use a separate latest-read channel. The write waits for its follow-up refresh before releasing busy state, and submissions/review-plan controls remain gated during writes or candidate reads. Each acknowledged field is cleared only when it still equals the submitted value; newer text or claim drafts remain. A list refresh failure has its own retry, which requests the list rather than replaying the acknowledged write. Core plans, confirmation hashes and write idempotency are unchanged.

The shared fixture passed **71/71**, including seven candidate cases and the prior 64 checks. These simulated writes prove UI behavior, not real candidate persistence. Production and separate fixture builds passed. The rebuilt actual app passed **52/52** on a fresh synthetic Vault, including actual candidate creation, exact review-plan confirmation, original-key retry and acknowledged-record recovery after a renderer crash. Its final installer passed **8/8**. Test APIs and markers are absent from the production frontend.

Required current-day pinned offline format, **201 Rust tests**, Clippy and independent **34-schema** checks remain valid for unchanged Rust/contracts. All data is synthetic; private data and local evidence are not committed. MV-7 and the existing manual/Host/provider/power-loss acceptance gaps remain unstarted or pending.
