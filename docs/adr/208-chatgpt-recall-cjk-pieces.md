# ADR-MEM-208 — ChatGPT recall splits Chinese questions into two-character pieces

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

Catalog recall (`select_catalog_recall`) splits the query on whitespace. It
drops every memory that contains none of the resulting terms (`NoCue`), and
ranks the rest by how many terms they contain.

Chinese and Japanese are written without spaces. A question such as
"Morii平时喜欢喝什么茶？" is therefore one term, and no memory contains it.
For Morii, who writes in Chinese, `memory_recall` (the tool ChatGPT is told
to call before answering) would return nothing for most natural questions.

`memory_search` uses the same substring rule, but its terms must all match.
Its description already asks for words, so short keywords work there.

## Decision

The ChatGPT short form of `memory_recall` expands the query text before it
reaches recall:

- **Words kept.** The original text is kept as it is.
- **CJK pieces.** For each run of CJK characters (kana, CJK ideographs,
  Hangul), the overlapping two-character pieces are added: "喜欢喝茶" adds
  "喜欢", "欢喝" and "喝茶".
- **Mixed tokens.** In a token such as "Morii平时", the non-CJK part is also
  added when it has at least two letters or digits.
- **No duplicates.** A piece already in the text or added before is skipped.
- **Size.** Pieces stop being added at the 2048 bytes a recall query allows.

Ranking is unchanged: memories that share more pieces rank first. Common
pieces such as "什么" may let unrelated memories in, but they rank below
memories that share more pieces, and the profile's item limit bounds the
result.

The shared recall contract, Runtime's native recall and `memory_search` are
not changed. Runtime's recall would benefit from the same rule; that is a
contract change for Runtime to decide (see [RUNTIME.md](../integration/RUNTIME.md)).

## Verification

`remote_daily_process`, through the real binary:

- **Chinese memory.** A Chinese conversation forms the memory
  "我最喜欢喝乌龙茶".
- **Natural question.** `memory_recall` with "Morii平时喜欢喝什么茶？"
  returns that memory among its selected items.

The English recall in the same flow, and the other remote suites, pass
unchanged.
