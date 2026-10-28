# MCP quote locator v1

2026-10-28. MCP-3 preparatory native helper, ADR-MEM-102. This does not advertise
memory_note or change existing QuotedEvidence::resolves.

memory-quote-nfkd-1 accepts one exact captured message (at most 16 KiB UTF-8)
and one nonempty hint (at most 500 Unicode scalars). It uses ICU4X
icu_normalizer 2.3.0 compiled data, already present in the repository lockfile,
now an explicit pinned Host dependency. No IO, Provider, fuzzy matching,
case folding, text editing or authority is involved.

Compare compatibility-decomposed, canonically ordered Unicode text (NFKD),
collapse Unicode whitespace runs to one ASCII space and trim hint edge spaces.
Map U+2018..201B to ASCII single quote, U+201C..201F and U+300C..300F to ASCII
double quote. NFKD also handles full/half-width forms. No other punctuation is
ignored. The normative normalization algorithm is specified by
[Unicode UAX #15](https://www.unicode.org/reports/tr15/); data tables stay pinned
to the crate, without claiming a newer Unicode version.

Each normalized scalar retains its original scalar span. Canonical combining
order is stable within nonstarter runs. Verify the entire mapped NFKD string
against ICU's whole-message NFKD before matching. Candidate spans must be
contiguous, at most 500 original scalars, and their exact original slice must
independently normalize to the entire hint. This refuses partial compatibility
expansions and reordered marks that would require omitted intervening text.
The search uses a linear prefix matcher; normalization and candidate checks are
bounded by the source/hint budgets. Normalized expansion is capped at 32 times
the source byte budget; exceeding it fails closed.

Return one exact original quote and Unicode scalar [start,end), carrying the
algorithm name. Distinct matching source spans return Ambiguous; no matches
return NotFound; invalid inputs and failed normalization consistency are
separate content-free failures. Never choose the first repeated occurrence or
invent offsets. These outcomes do not create a mandatory owner review queue.

Before using this helper, the caller must independently resolve the exact source
revision/hash under the current native Agent read/write/source policy. It must
bind the returned original quote to the actual speaker/evidence class and run
the unchanged strict evidence validator. A match grants no authorization,
user_statement classification or Owner confirmation. Agent-provided hints stay
Agent provenance. Source deletion/revocation requires fresh authorization;
the helper stores no text/cache and has no publication side effect.

| Surface | Capability after this checkpoint |
| --- | --- |
| Existing strict evidence validation | Exact original bytes/scalar offsets unchanged |
| Native quote helper | Explicit bounded normalization only |
| Existing legacy/new-format MCP tools | No added tool, write or layout support |
| memory_note / correction reports | Pending current-authority binding and shared intake |
| Runtime capture pipe | Existing source-only contract unchanged |

This helper alone does not complete the requirements 03 quote acceptance over
real local MCP transport. That evidence belongs to the later bound memory_note
tool and shared formation pipeline. Runtime UI and private initialization remain
outside this checkpoint.
