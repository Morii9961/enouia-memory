# MV-6 correction drafts on acknowledgment and retry

Date: 2026-10-04. Base: `a18f76b`. Scope: correction proposal UI; approved-memory/review behavior is unchanged.

The deferred production MemoryDetail page passed **3/5** targeted checks. It already retained the original memory ID, revision, text and idempotency key on retry, but an acknowledged old request cleared newer unsent correction text. Editing while the initial request was pending lost the draft too.

Acknowledgment now clears the correction field only if it still matches the submitted text. Newer unsent input remains available. A correction still creates a candidate; it does not replace the approved display or bypass review confirmation. The proposal's captured target revision and original key remain unchanged on retry.

The independent fixture passed **76/76**, including five correction checks and the preceding 71 cases. Its simulated proposals prove UI behavior, not real persistence or approval. Production and separate fixtures built successfully. The rebuilt final app passed **52/52** on a fresh synthetic Vault, including actual correction proposal/review, request-hash inspection, original-key retry and acknowledged-record recovery after a renderer crash. Its final installer passed **8/8** and the ownership template passed **14/14**. Fixture APIs/markers remain excluded from production.

Current-day pinned offline format, **201 Rust tests**, Clippy and independent **34-schema** validation remain applicable to unchanged Rust/contracts. Synthetic data only; no Runtime, Activity or real Provider capability is added. MV-7 and the established manual/Host/signing/power-loss acceptance gaps remain unstarted or pending.
