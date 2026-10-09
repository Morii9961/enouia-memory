# ADR-MEM-47 — Explicit text Provider dispatch

Status: Adopted for the MV-7 implementation; real-account activation and acceptance remain pending.
Date: 2026-10-08

The owner authorized MV-7 functionality and deferred real smoke calls. This adds `enouia-memory-provider` in Memory. Runtime remains the local product UI owner; workspace IPC and the reference shell continue to use Mock. No implicit Provider switch, credential discovery, or subscription-based capability inference is added.

## Native flow

1. The native owner inspects and confirms a scoped standing policy with `policy::plan_grant` / `confirm`. Each destination fixes an API and model. Revocation follows the same plan/hash confirmation and advances the policy epoch.
2. Governance `Decision::AcceptWithEgress` explicitly binds resulting canonical memory/identity to an existing grant in the displayed, hash-bound review plan. Ordinary approval leaves external use disabled. An existing memory can be bound through an owner revision proposal. A proposal cannot set egress metadata itself.
3. Save the user input first. Compile a saved external capsule with `compile_for_destination`. Bind the actual API/model and retain source references and exclusion reasons. No external request is created by compilation.
4. `VaultAdapter::prepare_saved` renders this capsule into the exact canonical HTTP body, with separate logical and wire hashes. The trusted surface displays this body, destination, output limit and reservation. Private data additionally needs a short-lived, single-use Egress approval for the exact request/resources. Highly sensitive content is denied.
5. `Client::send` rechecks current authorization, lock/restore gates and audit publication; reads one named native secret; atomically claims the owning turn before one HTTP attempt. Provider errors, cancellation, EOF and unknown outcomes never authorize automatic retry/fallback. A further explicit user turn may choose another Provider.

Only text is enabled. OpenAI Responses uses `store:false` and `truncation:disabled`; Anthropic Messages uses its version header and leading system blocks. Tools, image input and hidden reasoning storage are unsupported. Fixed HTTPS origins, certificate validation, bounded bodies, total/connect timeouts, cancellation, disabled redirects/proxy inference/retries apply at the native transport. Raw error bodies and secret headers are discarded.

## Durable state and deletion

The exact message bodies and full HTTP body are ordinary private SessionContent objects, included in existing backup/purge handling. No authorization header is archived. The immutable Dispatch describes an admitted attempt as `outcome_unknown`; the owning Session's optional `provider_invocations` ledger stores wire hash/size, token/cost reservations, terminal event, usage and outcome. Admission and body archival are one optimistic Vault transaction. Every stream text delta is published before further delivery. Output sensitivity inherits the strictest included evidence. Output publication checks the original policy/deletion epochs against the writer's pinned head, and final delivery rechecks authorization.

Native terminal events/content and exact outcome ledger metadata now publish together through transport-neutral `session::append_invocation_output` in one optimistic Vault transaction. Terminal replay binds state, usage and errors as well as text; changed metadata cannot reuse the same key. A length-limited response is a failed/incomplete turn with preserved text and precise `length` ledger state. `recover_local_outcome` still repairs older separately saved terminal receipts without HTTP; legacy length repair can conservatively retain failed/unknown usage. Without a terminal receipt the attempt remains unknown and cannot be resent. Existing Mock append hashes and wire/storage shapes are unchanged. Reservations include unknown/failed attempts and are not refunded automatically. Quotas apply per Session; prices are explicit caller-supplied rates, never inferred. Token counting is an explicitly declared estimate using full wire UTF-8 size plus a safety margin, not a model tokenizer guarantee. Unknown/unverified capabilities are refused.

Deletion adds wire archives and response events to capsule/dispatch dependents. Non-content admission ledger tombstones may remain to prevent resending; purged private objects are inaccessible. Restore network gates stay closed until existing adoption/deletion reconciliation succeeds.

An owner may close a locally unfinished unknown call through a read-only `plan_interruption` and exact-hash `confirm_interruption`, after stopping submissions/cancelling/joining its worker. The opaque ten-minute plan is scoped to the owner, trusted surface, local Vault/root and original admission/input. Confirmation atomically saves a local cancelled turn/ledger, preserving partial text, nullable usage, immutable admitted Dispatch and reservation floors. It cannot replace a saved legacy terminal/known result or permit old-input resend. A subsequent explicit user input may continue the Session. Local cancellation is not remote cancellation or billing evidence; host UI adoption remains separate.

## Compatibility and remaining gates

`SessionRecord.provider_invocations` and `extraction_jobs` are optional and omitted when empty, preserving existing Mock session serialization. Once populated, an older deny-unknown-fields Session reader cannot open them. Runtime must adopt the new Memory revision before enabling these native calls and must own cancellation/disable/lifecycle/UI integration. This change adds no page credential or HTTP command. There is no product Provider UI in this repository and no Runtime adoption evidence.

Windows Credential Manager reads/writes only `Enouia.Memory.Provider.openai` or `.anthropic` through native functions. No real credential has been read/written during implementation. No actual account/model capability, tokenizer, price, HTTP endpoint, cancellation against a live server, or subscription/API billing compatibility is claimed. Optional model extraction is implemented but remains inactive in the host. MV-8 is not started.

## Explicit optional extraction

The native owner selects exact source revisions/UTF-8 snippets, subject and budgets. `ExtractionJob` persists immutable selection hashes, Provider binding, versioned prompt, immutable token/optional monetary ceilings and a resumable cursor in its Session. Creation/preparation never calls HTTP. A source-only extraction capsule avoids unrelated memory retrieval; ordinary egress controls and one-attempt admission remain mandatory. Admission rechecks the job’s binding, purpose and paused state; its saved budgets further constrain Session quotas. A monetary ceiling requires explicit rates, and token/price reservations include the input safety margin. Complete saved JSON is validated before any proposal, including literal source quotations. Facts/preferences become uncertain pending candidates, never automatically canonical memories. Evidence classes/sensitivity come from the actual source.

Local application advances one candidate at a time. A paused job cannot publish against its pause snapshot; later changes fail the optimistic head precondition. Published proposal receipts survive a missing cursor update and are recovered without consuming backlog again or invoking HTTP. Identical previously reviewed claims against unchanged source bytes/evidence class are suppressed; new evidence may be reviewed. Source deletion with dependents erases even an undispatched extraction input. Large raw objects are refused before allocation. Native lifecycle cancellation and UI adoption remain the host's responsibility.

New jobs use `extract-text-2`, carrying source occurrence/capture time, precision, known/unknown branch and selected offsets without inventing time or branch relationships. Version-1 jobs remain valid and keep their saved input. The native read-only preview carries complete selected text and literal quote offsets, keeping model proposals separate from current owner-edited/reviewed candidates. It never approves, writes or sends. Qualification/quotation containment does not prove semantic truth.

See [native usage](../integration/PROVIDERS.md), [implementation evidence](../validation/MV-7.md) and [Runtime compatibility](../integration/RUNTIME.md).
