# ADR-MEM-59 — Opt-in analysis input in private quality bundles

Date: 2026-10-14. Scope: MV-7-H2j, synthetic/offline native helper.

H2i adds selected-source lineage to the model's immutable analysis input. The
existing quality bundle retains its hash but omits the input itself, requiring
separate native inspection. Add history_quality::build_with_analysis_input as an
opt-in private assessment helper; keep build and its suite bytes unchanged.

After the existing saved-response/source/owner validation, read the saved input
at the validated pin, retain the exact UTF-8 text in native_task_input_text, and
verify its original native_task_input_sha256. Enforce the existing 64 KiB task
and 256 KiB whole-bundle budgets, then recheck source freshness and lock state.
This returns memory only: explicit file saving/export remains a host action.
It does not add wire commands, stored records, dependencies or model calls.

The offline evaluator checks the optional text's exact UTF-8 hash, bounded JSON
shape and agreement with the selected source refs/spans/text/role/context in the
assessment scope. It rejects duplicate JSON keys and changed inputs. It does not
authenticate a builder, query a Vault, validate actual parent edges or re-run
native request/response checks. Metadata remains a submitter declaration;
native_validation stays not_rerun. Raw task text is inspectable only in the
private suite, not repeated in the metrics report.

The original prompt, suggestions and lineage metadata remain untrusted data.
Including them never fills judgments, infers important units, publishes a
candidate or approves memory. Pending/unknown and overall_model_quality=null
remain. Old suites without input text remain readable and explicitly report
analysis_input_inspection=not_included. New text reports
included_hash_and_scope_matched, which is structural evidence, not faithfulness.

Runtime may opt in at a new pin and must treat the larger private payload as an
explicit selection/save decision. Real model fidelity/omissions, product
adoption, owner acceptance and later MV stages remain separate.
