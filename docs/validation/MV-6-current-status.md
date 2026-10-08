# MV-6 current status and next steps

Date: 2026-10-08. Scope: the independent Memory repository, within MV-6. MV-7 has not started.

## Implementation and integration

MV-1 through MV-5 provide the local Memory foundation. MV-6's reference Windows shell and acceptance harness are implemented. Runtime remains the product Windows client under ADR-MEM-45.

The existing hardening branch, `codex/mv6-vault-lifecycle-isolation`, ends at `04792f18ee25ce48948d583b831c30c8f427f78e`. Local `main` was fast-forwarded from `3b511b7` to that revision, preserving all 23 existing commits (22 non-merge commits). The other worktree was clean; it was neither edited nor removed. This integration adds no new production behavior or dependencies.

The combined changes cover:

- Vault-scoped operations, plans, confirmation replies, backup status and picker tokens; lifecycle exclusion and worker cleanup.
- Original request binding and published source, proposal, session and Mock result recovery on concurrent retries.
- Selected-file revalidation and bounded double reads before import archival.
- Complete purge result publication before a concurrent confirmation retry, lock or shutdown completes.
- The reference shell and installer fixes already on the former main, retained by the branch's existing merge.

Individual reproductions and limitations are linked from the [documentation index](../README.md). The recorded and logged Runtime surface aggregate is `ef2282cd0abf8aeb5598fda80ec96df5c372bd9b269e3f729a73540494582e77`. Wire/API/storage format and dependencies are unchanged by this integration.

## Verification and publication

Fresh checks from the repository root on the integrated production revision passed on October 8:

| Check | Result |
|---|---|
| Rust 1.98.1 offline `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked` | 271 passed, 0 failed, including 60 Core tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| Python 3.12.10 / python-jsonschema 4.26.0 independent cross-check | 34 schemas; 53 valid records, 117 record cases, 4,913 set records, 51 IPC messages, 63 workspace messages and 46 store documents agree |
| Runtime surface checker | Recorded and logged aggregate matches |
| Reference frontend and both fixture/read harness builds | Passed; these are builds, not actual-app acceptance |
| Installer ownership checks | 11/11 |
| Isolated NSIS drills | Locked-file handling 2/2, startup cleanup 4/4, version guards 7/7 |
| Public-history pattern scan | 22 non-merge commits, 0 credential/personal-path matches, 0 tracked private-data paths |

The required Rust tests were run with native Windows ACL access, using synthetic data and isolated roots. The deliberately aborted-process recovery test took 83.20 seconds and passed. Its evidence concerns process crashes, not OS crashes or power loss. The installer drills do not install the product or enable the real startup value; their non-startup registry keys were removed. Local build and verification artifacts remain ignored.

The previously interrupted, explicitly authorized push was completed. A post-push `git ls-remote` confirmed the public feature branch at `04792f18ee25ce48948d583b831c30c8f427f78e`. With the owner's subsequent merge-and-push authorization, [PR #5](https://github.com/Morii9961/enouia-memory/pull/5) was merged on October 8 as `4df878043e5d089a32f04e82df358a3dbab24295`, preserving the existing feature history. This status document and its index/wording updates form a documentation-only follow-up on `main`. The production source, contracts, dependencies and recorded integration surface are identical to the validated feature revision; the verification evidence above remains applicable. Runtime adoption has not been performed by this merge.

Historical evidence remains in the individual reports; the October 4 actual-app counts do not validate this integrated revision.

## Next work

1. In Runtime's repository, inspect its current Memory pin and adopt the combined Memory revision now merged through PR #5, using Runtime's integration runbook. Update the dependency, lockfile and pin manifest together, then adopt the applicable compatibility-log rows. Memory has not inspected or changed Runtime in this run, so current Runtime adoption is unverified.
2. Re-run Runtime's W01-W05 acceptance on synthetic Vaults, including lock/switch/shutdown state cleanup, responsive status during pending work, identical keyed retries, import cancellation/resume, complete confirmation results and installer behavior. Reference-shell and Core tests do not establish Runtime-host acceptance.
3. Complete the remaining manual Windows checks: actual login startup, Narrator, an actual contrast theme, interactive installation/earlier-release upgrade and signing. Real export acceptance, restic encrypted backup and OS-crash/power-loss evidence remain separate pending items in the stage reports.
4. MV-7 is the next planned implementation stage: Provider capability/key-storage discovery, exact outbound payload inspection, controlled calls to two chosen Providers and optional candidate extraction. It requires an explicit owner request to start the stage. Real calls also need selected accounts, outbound-data approval and usage limits; synthetic adapters alone cannot establish real Provider integration.

New local product UI belongs in Runtime. Memory keeps the transport-neutral Core, contracts, reference harness and future cloud stages. No real memories, exports, credentials or production resources were used in this integration.
