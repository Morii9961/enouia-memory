# ADR-MEM-217 — Agent host creation waits briefly for the writer lock

Date: 2026-11-25. Status: Accepted (Runtime report); synthetic validation.

## Context

ADR-MEM-175 let read-only paths and permit issuance wait up to
`BRIEF_LOCK_WAIT` (two seconds) for the writer lock, so parallel sessions of
one Agent no longer lose with `Busy`. Commits still try once.

Two read-only calls were missed. Each Agent connection that Runtime serves
creates a `QualifiedAgentHost`, and that creation calls both:

- `SegmentedAutomationVault::snapshot()`;
- `check_original_agent_carrier()`, for the initialized capability.

Both still tried the lock once (`Duration::ZERO`). While session A held the
writer lock to issue its permits, session B's host creation returned
`Vault(Busy)`, and Runtime dropped the connection. The client saw
`BrokenPipe` during the handshake.

A Runtime session traced this with temporary logging. Its
`concurrent_agent_sessions_all_connect` test failed intermittently in
single-threaded full runs, and the only error was
`from_authenticated_initialized_owner -> Vault(Busy)`. The test had been
flaky since Runtime dropped its own Busy retry when it adopted ADR-MEM-175.

## Decision

`snapshot()` and `check_original_agent_carrier()` wait up to
`BRIEF_LOCK_WAIT`, like the other reads of ADR-MEM-175. Both are reads, so
the rule that commits try once still holds.

Other read-only calls that try once (for example `capsules`,
`capture_windows`, `agent_requests`, `read_scoped`) are left as they are.
None has a reported failure, and changing them is a separate decision.

## Verification

`owner_read_grant::grant_purpose_labels_are_closed_and_owner_queries_wait_briefly_for_the_lock`:

- **Setup.** Another thread holds the writer lock for 300 ms inside an Agent
  read.
- **Result.** `snapshot()`, `check_original_agent_carrier()` and
  `QualifiedAgentHost::from_authenticated_initialized_owner` all succeed
  after waiting.
- **Before the change.** `snapshot()` failed at once in this test.

The vault and host suites pass. Runtime will recheck its full suite three
times single-threaded and three times in parallel at the new pin.
