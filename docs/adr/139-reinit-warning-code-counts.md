# ADR-MEM-139 — Count re-initialization preflight warnings by code

2026-11-11. The read-only legacy snapshot plan reports 732 warnings but only a
total. Before any real run, that total needs an inspectable explanation without
printing private source text, messages, titles or locators.

## Decision

Add `planned.warnings_by_code` to `enouia-memory-reinit plan`, counting the
existing parser-generated warning codes on the built sources. Use sorted keys
and retain `planned.warnings`; the category counts must sum to that total. Do not
change parsing, batch packing, formation policy, evidence, admission or `run`.

A synthetic CLI-level fixture must show nonempty categories and matching totals
without creating a Vault. Validate the real selected snapshot only with `plan`,
reporting code counts; no real root or data writes are implied. No Runtime
surface, schema, dependency, new MV stage or long-term read policy changes.
