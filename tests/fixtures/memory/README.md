# Synthetic Memory fixtures (MV-0)

Everything here is synthetic test data. None of it is Morii's memory, chat history, or project state, and it asserts nothing about the real MoriMeta or Moriium. Every ID is a readable, syntactically valid UUID v4. Every hash is a SHA-256 of a synthetic label or of the canonical bytes of the record it names. MV-0 does not recompute commit catalog hashes; MV-1 will.

| File | Purpose |
|---|---|
| `records/*.json` | One valid record per kind or variant, including all five memory types, stored in canonical bytes |
| `records-manifest.json` | Valid records with their schema, and 90 invalid single-rule mutations. `schema: accept` marks a constraint that JSON Schema cannot express but Rust must reject |
| `sets/morimeta-confirmed.json` | CONTEXT_MODEL §9 story. F-A: an unreviewed model suggestion. F-B: the user explicitly chooses Professional Darkroom, and the owner reviews it. F-C: a provisional checkpoint saying "prepare to implement". F-D: an unrelated Moriium decision and a highly sensitive record. A malicious imported document yields a rejected candidate |
| `sets/morimeta-insufficient-evidence.json` | Same story without F-B. No canonical record names a chosen design, and the capsule abstains (`no_supported_memory`) |
| `sets/lifecycle.json` | Future-effective supersession, live and expired facts, a conflict group, preference, episode, a reviewed checkpoint memory, Identity, and a logical delete |
| `sets-manifest.json` | The consistent sets plus 46 cross-record mutations, each naming the violation it must produce |
| `ipc-manifest.json` | Valid requests for all 12 operations, valid responses, and invalid requests and responses |
| `expectations/*.json` | Expected valid-time results and MoriMeta capsule outcomes. MV-5 must reproduce them from a real compiler |

The fixtures were produced once by a throwaway generator and are maintained as the source of truth. Edit them directly, and keep the canonical byte form: sorted keys, two-space indent, LF, trailing LF.
