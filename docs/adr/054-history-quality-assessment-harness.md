# ADR-MEM-54 — Offline history quality assessment harness

Date: 2026-10-12. Scope: MV-7-H2e, evaluation preparation with synthetic calibration.

Exact citations do not establish entailment, summary faithfulness or important
information coverage. Add a standard-library-only offline harness that calculates
metrics from explicit human judgments, never infers those judgments from wording.
Bind judgments to canonical JSON hashes of the selected source scope and response
value; these are not native task/wire hashes. Keep native task hash separately
nullable for synthetic cases, required for owner-selected samples.

Review change statements, explanations and summary points independently as
supported, contradicted, unsupported or uncertain. Missing item judgments remain
unreviewed. Required semantic units are observed, missing or unknown; observed
units must link to positively reviewed response paths. Missing/unknown are never
silently treated as irrelevant. Exact reviewer evidence quotes must belong to the
selected source messages, but the harness does not rerun native validation.

Report counts, denominator definitions, explicit unknowns and per-case review
status. No global model quality/acceptance percentage. Exit zero means a report
was generated, not that model quality passed. Synthetic positive/negative/pending
cases calibrate the evaluator only; actual model quality remains pending.

Only read the explicitly selected suite and its bounded relative source fixture
inside the suite directory. No credential/Provider use, private-history discovery,
background tracking, Runtime API/UI change, Canonical write or later MV activation.
