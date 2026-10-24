# Previously local Provider hardening: current-main integration

2026-10-24. Owner-authorized merge review, synthetic/local scope.
Base main: 162c189a5f5934ae7162fd7be48c44d33175de61.
Historical branch: f4dad99285e30eb2f52edf01fa86baee39d2ec86.

Preserve both histories. Reconcile four conflicts by retaining current history
modules and crate-visible extraction source admission, adding the existing
current-source helper, combining compatibility history and regenerating the
current manifest. No UI, dependency version, wire/storage schema or Runtime pin
changes. Current Workspace catalog remains 58 commands; aggregate:
26c1b75989eb61dcf22d11257abadab38b0f1d970cdefd26e4688220e0698df0.

The old fixes reject ambiguous JSON and input-budget routing, enforce exact
invocation terminal event pairing and source revisions before admission/output,
preserve validated failure/progress usage and refuse malformed native credential
shapes before driver access. The original reports remain historical evidence.

New merge evidence:
- Provider library: 10 passed. Provider integration: 103 passed. Existing tests
  include saved-history review/semantic cases, fake transport/credentials and
  source/budget/publication/replay boundaries.
- Workspace Core first run: 80 passed, six history cases failed because their
  shared positive OpenAI fixture omitted id/object/error. Only those fixture
  fields were added; production validation was not weakened and no tests added.
- Focused history rerun after that fix: 20 passed, 66 filtered. Unaffected Core
  results are reused; the entire Core suite was not rerun after the fixture edit.
- Contract extraction: 2 passed; invocation: 1 passed; integration surface:
  4 passed. Manifest/log consistency and staged whitespace/public-content checks
  passed.
- Strict Clippy for the four affected crates' lib/tests passed; affected-crate
  formatting passed.
- Logs in target/provider-hardening-integration/: providers.txt, workspace.txt
  (the initial six failures are retained), history-after.txt, contracts.txt and
  clippy.txt. These ignored files are local evidence, not public PR attachments.

Independent Python/jsonschema was unavailable in the checked environment and
was not installed. No independent schema cross-check, full Rust workspace,
frontend, GUI build/package, smoke or performance rerun is claimed.

Runtime still pins 08892702 / 57 commands; this merge does not adopt the new
combined security revision in Runtime. Native/real HTTP/account, credential-store,
physical UI and owner acceptance remain separate. No app install/run, personal
data/credential access, actual Provider request or Host/MCP/collector activation
occurred. Concurrent MCP/Core worktrees, all frontend redesign refs/worktrees
and manual project-status snapshots were not changed.
