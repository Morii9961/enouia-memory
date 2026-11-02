# MCP-3 checkpoint 1: native Runtime source capture

2026-10-27. Base `9149d279f4355a9d4c80caba26673d6648599b60`, branch
`codex/mcp-runtime-capture`. ADR-MEM-102 and RUNTIME_CAPTURE_NATIVE_V1 govern
this native building block. It is not the completed MCP-3 daily formation path.

## Change and version/capability matrix

| Root / capability | Agent reads | New native Runtime capture |
| --- | --- | --- |
| schema 1 / host-current-v2 / six-tools-1 | Existing six-tool meaning | Unsupported |
| schema 2 / automation-host-v2 / agent-read-1 | Existing five reads | Unsupported |
| schema 2 / automation-host-v3 / agent-record-read-1 | Existing complete record reads | Unsupported |
| schema 2 / automation-segmented-host-v1 / agent-catalog-read-1 | Existing scoped catalog reads | Explicit memory-runtime-capture-native-1 only |
| Unknown/newer root or self-claimed producer | No implicit upgrade | Refuse |

The new API accepts an existing leased segmented Vault and a retained OS
Runtime process selected by trusted native setup. It does not open a second
writer, forward Workspace, use System read permission as Agent permission or
accept principal/scope/PID from a packet. Client is the registered producer role;
Agent, Owner and System producer claims refuse. Old Agent tools and legacy
proposal/checkpoint semantics are unchanged. No Runtime repository was modified.

Exact policy revision, selected source pool, sensitivity, account alias, native
configuration hash, instance identity, wall/monotonic expiry and rollback checks
bound a sealed non-deserializable permit. Pool capacity is finite, not a wildcard
or per-memory Owner approval queue. Later policy revisions require new native
admission. This is not automatic policy/source-pool provisioning.

Capture preserves raw UTF-8 turns, original IDs/time/timezone, role, model,
language and bounded attachment descriptors. Backend source/conversation/branch
identities bind them in one existing System delta with witness. User roles remain
user statements; machine roles are model claims with no attributed subject and
no manual confirmation. Missing event dates stay unknown. Missing original
timezone stays null while runtime.capture records Asia/Shanghai, owner_default
and native configuration hash. Attachment descriptors stay metadata only; no
blob or managed attachment reference is fabricated.

Preflight shares the actual commit admission/size/chain checks and stops before
publication. Its prospective source references are not a capture or job receipt.
Actual capture returns the current witnessed snapshot and source references;
formation_scheduled is false. Same-key same-payload replay verifies current
policy, exact provenance and source deletion before returning. Producer request
keys cannot be changed by selecting another source pool. Reusing a message with
a different request key conflicts rather than silently coalescing.

## Evidence

All roots were newly created synthetic temporary directories. Only owned test
processes were enrolled; kernel process-object liveness and SID/logon session
checks are reused. These tests do not claim a Runtime hook or capture pipe.

New tests exercise raw-text equality, unknown dates, default/source timezone
provenance, machine-role attribution, metadata-only attachments, byte-identical
business/authority-file preflight, legal witnessed capture and reopen, original-key replay and changed
packet conflict, narrowed source pools, capacity, cross-instance denial,
expiry/clock rollback/revocation, policy revision/source deletion, real process
exit/reaping and original packet retention after a lost successful response.
Injected AfterCurrent failure returns outcome_unknown, ordinary open refuses
RecoveryRequired, and only explicit native-primary recovery reconciles the
unchanged packet. No automatic resend, replacement key or recovery was added.

Contract corpus: 12 synthetic cases, including forged identity/scope/confirmation,
wrong major/capability, missing nullable metadata, duplicate messages and role
disagreement. JSON Schema checks shape; native validation additionally enforces
UTF-8 byte budgets, uniqueness and existing language/timezone constraints.

The first complete run found a new test-helper mistake: reading the active
native lifetime lock returned Windows byte-range-lock error 33. The comparison
now excludes only indexes/host.lock and vault/LOCK, the two declared native
synchronization files; all business/authority files remain byte-compared. No
ACL, lock lifetime, product admission or unknown-outcome rule was weakened.
All four focused Vault cases and strict workspace clippy passed after this
test-only repair. A later complete attempt exposed another pre-existing direct
status unwrap in continuation_cases. Its four concurrent worker observations
now reuse the existing bounded observed_status helper, which retries only
content-free Busy reads. The worker-free restart assertion stays unchanged. Product
generation/publication is never retried. The initially misqualified exact test
filter matched zero tests and is not counted as evidence. The correct no-policy
test then passed three times; all 12 continuation cases also passed.

Final checks from the repository root, pinned toolchain, offline/locked:

- cargo fmt --all -- --check: passed.
- cargo test --workspace --locked -- --test-threads=1: **735 passed, zero failed,
  one existing ignored benchmark**; complete doc-test tail and exit code 0.
- cargo clippy --workspace --all-targets --locked -- -D warnings: passed.
- Python 3.12 / python-jsonschema 4.26.0 independent cross-check: **56 schemas**,
  including all **12 Runtime capture corpus cases**, zero disagreements.
- Runtime manifest regenerated and logged; aggregate
  `1c5124c281348297d55dce0458887fce96210bf30e7a2eea2aa0eff813ef553f`.
- Owned staged whitespace and public-content checks passed. No AGENTS or manual
  status artifacts were staged. Next-checkpoint drafts remain ignored build-area
  files and are not part of this delivery.

Final local test evidence is retained under the ignored validation directory:
runtime-capture-workspace-final.txt/.exit and runtime-capture-clippy-final.txt/.exit.
Earlier failed attempts remain distinct and are not counted as passing runs.

## Differences from requirements 03 and next checkpoint

Morii selected Runtime hooks only, superseding the document's optional Agent
host capture. Agent memory_note remains an optional hint in a later binding.
This checkpoint is synchronous native source capture, not conversation_capture:
no public MCP write advertisement, local Runtime wire, durable live queue,
same-conversation job ordering, backfill priority, generation or formed memory
is supplied here. No job ID or final field-coverage claim is fabricated.
Normalized quote lookup and Agent hints are still pending. Runtime hook/UI/pin
adoption and the parallel projection/owner-layout contracts remain separate.

Next: bind a versioned Runtime-only protected local transport, preserving the
same native producer and unknown-outcome boundary; then connect durable live
jobs to the same System formation rules and frozen fields used for initialization.
Uncertain model results remain automatic provisional/re-evaluation work, not a
mandatory queue for Morii. Missing private initialization is not a blocker.
No real Provider/embedding, private Vault/export/API credentials, real client
configuration, Activity/Usage input, remote listener, deployment or main merge.

Owned files: new runtime_capture contract/Vault/Host modules, schema/corpus/tests,
ADR102/contract/report and minimal module registration; private commit dry-run
extraction, schema cross-check registration, design/ADR references and regenerated
Runtime manifest/log. No AGENTS, manual status snapshots or parallel state,
catalog, projection, process or owner-action files were edited.
Four concurrent status observations in the diagnosed continuation test file
are also owned for this fix.
