# MV-8.1f · Versioned native session gate

Date: 2026-10-22 (Asia/Shanghai). Baseline
`78014db93e9c8fd570fd86142c2d8aee51f49c84` (merged PR #43).
Continues owner-authorized MV-8; this is a transport-independent library.

[ADR-MEM-75](../adr/075-native-host-session-gate.md) adds closed native
hello/welcome contracts and a native connection gate with exact schema/version/
Vault checks, fixed native scope, bounded capacity/expiry, instance isolation,
synchronous revocation and lock/clock failure behavior. No listener or real
authentication is claimed. NativeAdmission deliberately carries a trusted
adapter assertion, with no Deserialize implementation or page constructor.

Seven synthetic session tests passed: native owner/version/hash/Vault checks;
scope and packet refusal before dispatch; lock/revoke/unlock/reconnect/restart
isolation; capacity/drop/expiry; clock rollback/invalid time; barrier-controlled
synchronous dispatch versus revocation; and backend-panic unavailability.
The concurrency test uses channels and bounded waits rather than timing
sleeps. Its first compilation borrowed a non-Sync receiver; moving that test
receiver into its worker fixed the fixture without a product workaround.

Thirteen pure Host contract tests passed, including hello/welcome fixtures,
unknown claimed identities/surfaces, invalid versions, raw duplicate decoded
fields and 4 KiB budgets. The invalid-calendar fixture initially expected
Schema rejection; the existing common timestamp Schema intentionally checks
format only. Its corrected manifest records Schema acceptance plus Rust
calendar rejection, preserving both validators' actual boundaries.

The first complete workspace attempt failed its Contract purity boundary:
embedding schema text in the Contract crate is forbidden. The implementation
moved that compatibility calculation and schema inclusion to the Host adapter;
the boundary test remains unchanged. Its failed output is preserved in ignored
`.local/mv8-session-workspace-before-pure-contract-fix.log`; the focused final
boundary/Host contract run passed 25 tests. The final workspace run below uses
the corrected source.

The next corrected-source attempt was interrupted with the chat, leaving no
exit record and no live test process. Its partial
`.local/mv8-session-workspace-final.log` is not reported as a passing run;
the resumed complete attempt is recorded separately.

Final checks:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 468 passed, zero failures; doctest groups passed with zero doctests; `.local/mv8-session-workspace-resumed.*` |
| Strict workspace all-target Clippy | Passed on the corrected source |
| `cargo fmt --all -- --check` | Passed |
| Independent Schema cross-check | 43 schemas, 56 valid records, 126 record cases, 5,496 set records, 51 IPC messages, 134 Workspace messages, 49 store documents, 39 Host documents, 8 Provider-invocation and 19 extraction-job cases |
| Runtime surface checker | Passed; recorded/logged `5c5c72be21781ed2195956b832af162bb058a5174d54c3feb4fe1ecc10273009`, 55 commands |
| Public-content/diff review | 23 engineering/documentation/synthetic files; local links resolve, no matching private paths or credential shapes; user instructions and manual status excluded |

Outputs and exit codes are retained in ignored `.local/mv8-session-*.log`/`.exit`.
Rust uses pinned 1.98.1 GNU, offline/locked Cargo and four test threads, with
incremental compilation and dev/test debug information disabled. Independent
Schema checks use pinned Python 3.12.10/python-jsonschema 4.26.0.

The unique process, protected Windows listener/client authentication,
authenticated owner enrollment and native unlock, actual Host-bound Workspace
dispatch, asynchronous-work revocation, reference-shell reconnect and Runtime
adoption remain unimplemented integration gates. The five-minute expiry and
64-connection bound are this v1 gate's fixed policy, not measured client
compatibility. Restricted MCP, real Provider acceptance, independent encrypted
restore, private data migration and cloud activation remain separate. No
credentials, real Provider calls, private material, Runtime checkout or manual
status snapshots were used or changed.
