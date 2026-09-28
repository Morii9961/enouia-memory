# Synthetic Memory fixtures (MV-0R)

Everything here is synthetic test data. None of it is Morii's memory, chat history, or project state, and it asserts nothing about the real MoriMeta or Moriium or any real Provider. Every ID is a readable, syntactically valid UUID v4. Hashes are SHA-256 of synthetic labels, of exact synthetic message text, or of the canonical bytes of the record they name.

| File | Purpose |
|---|---|
| `records/*.json` | 51 valid records covering all 20 record kinds and all five memory types, in canonical bytes |
| `records-manifest.json` | The valid records with their schema, plus 108 invalid single-rule mutations. `schema: accept` marks a constraint JSON Schema cannot express but Rust must reject |
| `sets/morimeta-confirmed.json` | CONTEXT_MODEL §9 story. F-A: an unreviewed model suggestion. F-B: the user explicitly chooses Professional Darkroom and the owner reviews it. F-C: a provisional "prepare to implement" checkpoint. F-D: an unrelated Moriium decision and a highly sensitive record. A malicious imported document yields a rejected candidate |
| `sets/morimeta-insufficient-evidence.json` | Same story without F-B. No canonical record names a chosen design; the capsule abstains (`no_supported_memory`) |
| `sets/morimeta-external-egress.json` | Confirmed story plus an owner grant (MoriMeta → example-cloud/example-model) and an external request carrying the private decision under an exact, single-use egress approval |
| `sets/lifecycle.json` | Future-effective supersession, live and expired facts, a conflict group, preference, episode, a reviewed checkpoint memory, Identity, an owner declassification, and a bound logical delete |
| `sets-manifest.json` | The four consistent sets plus 67 cross-record mutations, each naming the violation it must produce (review findings F1–F6 included) |
| `ipc-manifest.json` | Valid requests for all 12 operations, valid responses, invalid requests and responses |
| `expectations/*.json` | Expected valid-time results, MoriMeta capsule outcomes, and the exact synthetic request text behind each Dispatch digest |

Fixtures are produced by `tools/fixture-gen` (Python standard library only). The committed files remain the source of truth: regenerate after a contract change, then review the diff. Keep the canonical byte form (sorted keys, two-space indent, LF, trailing LF).
