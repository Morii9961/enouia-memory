# MV-6 renderer crash and acknowledged-data preservation

Date: 2026-10-03. Scope: explicitly opt-in `--crash` acceptance on a new synthetic Vault. The integrated real-app run passed **43/43 checks**, including four new crash checks.

The harness requests the actual renderer crash through WebView2's debugging protocol and observes `Inspector.targetCrashed`. It then force-stops only its own app process, reopens the same Vault, and checks that the pending-candidate count is unchanged, the saved session transcript is present, and the approved corrected memory is available. Vault verification succeeds after reopening. The debug port is loopback and exists only for this test process.

All writes being checked were acknowledged before the fault. This establishes local recovery of those records after a renderer crash followed by forced app termination. It does not establish safety during an interrupted commit, an OS crash, power loss, disk failure, or a real-device recovery exercise. Every input and record is synthetic; no Provider, Runtime, real exports, or user Vault was used. Evidence remains in ignored local/temporary material.

The crash drill is deliberately opt-in: `node apps/workspace/e2e/smoke.mjs <release-exe> <synthetic-vault> <synthetic-markdown> <temporary-output> K <separately-built-retry-fixture> --crash`. Without the final flag, the existing acceptance run does not request a renderer crash.
