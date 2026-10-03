# MV-6 write retry and operation recovery follow-up

Date: 2026-10-03. The production action hook now creates one key per submission and retains both that key and the original action closure for its retry. Write call sites pass the key through to the IPC client. Distinct submissions obtain new keys, and busy write controls are disabled. Long-operation polling no longer overlaps interval calls, stops on unmount, and exposes status/cancellation failures with retry controls.

The separately built `e2e/retry-fixture.tsx` imports the actual production action hook, error component, and IPC client. It calls the real Core on a synthetic Vault, then deliberately substitutes a retryable error after the first successful commit. **3/3 targeted checks passed**: the first response loss follows exactly one new candidate, the visible retry returns the same key and candidate without another write, and a new submission with different synthetic content uses a new key and creates one additional candidate.

This is a deliberate retryable-error simulation after a real local commit. It is not a network disconnect, a crash during storage, a Provider request, or proof that every kind of transport failure is retryable. The first attempt to override Tauri's immutable invocation property was ineffective and was abandoned.

`npm run test:fixtures` typechecks the fixture and produces its separate test bundle under ignored `.local/`; the normal entry point does not import it. The production frontend typecheck/build passed, and its output contains neither the fault marker nor the fixture API. The fixture is optionally supplied as the final argument to the real-app smoke harness. The expanded keyboard run passed these retry checks but separately exposed two accessibility failures, which are being fixed in the accessibility follow-up; no aggregate pass is claimed for that run.

The required offline root checks passed: 196 Rust workspace tests, formatting, Clippy with warnings denied, and independent Python validation of 34 schemas. Real-app import/rebuild/verification checks exercise normal operation polling. Error-path polling and actual transport disconnect remain separate limitations.
