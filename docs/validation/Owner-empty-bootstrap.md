# Native empty Owner bootstrap: validation

Date: 2026-10-30. Branch: `codex/owner-vault-bootstrap`, based on published
`3b6e974cc12182859d3645d64c03e5acc534896f`. Scope: the separate trusted native
Host/CLI creation adapter in [ADR110](../adr/110-native-empty-owner-bootstrap.md),
not import, formation, Provider, queue, Runtime UI/pin or Core-8 switching.

The owner explicitly requested an empty formal Vault; the actual human message
was checked in the Runtime conversation. Creation authorization is established.
Synthetic validation and the native OS/owner-self binding are distinct; no test
identity or copied fixture is accepted by the production command.

## Owned changes

- Host lib/windows: kernel current-process account/session evidence retained in
  private live objects, not serialized into resumable capabilities.
- `owner_bootstrap/{mod,json,tests}.rs` and the separate native command binary:
  protected home registration, original-operation recovery, canonical home/root
  binding, baseline publication and scope-limited native CLI arguments.
- Vault segmented module: native System-only empty Owner genesis verification.
- ADR110/index, this report and Runtime surface/log. No frozen record schema,
  Owner action method, MCP route, dependency or other worktree is changed.

## Verified behavior

Fourteen focused cases pass: register/create/drop/reopen/verify; original IDs on
replay; unknown JSON key preservation; genesis published before metadata;
wrong authenticated account; malformed/path-modified binding; metadata conflict;
nonempty unknown directory retained; duplicate JSON and identity override flags;
missing registration; live kernel evidence cannot adopt synthetic home; home
lease exclusion; incomplete CURRENT/witness requires explicit same-operation
recovery; lost metadata response resolves without new IDs; changed baseline bytes
conflict even with equal JSON values; a root with a business policy is not empty;
and pending/recorded registration copied to a different home is refused. Related
assertions are grouped in fourteen test functions.

All roots are newly generated temporary synthetic homes. Production enrollment
has no public Deserialize or test identity constructor. No test launches a real
Owner registration or creation against the private durable data home.

Owner-contract review found one P1 before publication: copied registration could
relocate the original root because the home was not bound. The fix persists the
private verified canonical home and checks it before any Recorded short circuit
or root work. Both pending and Recorded copy-to-another-home regressions pass.
Non-Unicode canonical paths refuse rather than collide through lossy conversion.

`active_vault` is only a verified creation locator. The operation publishes
`runtime_adopted=false`; existing explicit root selection remains mandatory.
Metadata publication coordinates cooperating writers using the home lease and
byte-baseline checks, followed by atomic replacement. It is not an OS atomic CAS
against a non-cooperating manual rename. No root, journal or unknown user data is
deleted on conflict/failure.

## Checks and execution boundary

Focused tests: 14 passed. After the canonical-home fix, pinned Rust 1.98.1,
offline/locked full workspace tests passed: **806 passed, 0 failed, 4 explicitly
ignored**, across 96 test/documentation groups with `RUST_TEST_THREADS=4`.
Fmt check, workspace/all-target strict clippy, independent Python 3.12/jsonschema
4.26.0 (60 schemas) and the logged Runtime surface check passed. The aggregate is
`9fc419e8d0a3366b427a516114e8d947bcfaad33c03784cf179ea4b016353cdb`.
Final logs are `bootstrap-workspace-tests-final.log` and
`bootstrap-clippy-final.log` under ignored local storage. An earlier full run
predated the fix and is not final evidence. The previously documented default
high-parallel Workspace contention limitation is not claimed fixed here.

Owner-contract limited source review closed the cross-home P1 and found no
remaining definite blocker in the reviewed files. It did not execute tests or
any real home operation; the test evidence above belongs to this implementation
session. No claim of Runtime adoption or actual formal creation follows from it.

At candidate publication, this report covers code and synthetic verification.
Actual private creation, binding and root locations belong only in the protected
local operation/adoption record and private session report, never this document
or Git fixtures. Do not interpret synthetic results as proof that a formal root
has already been created.
