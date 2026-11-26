# ADR-MEM-225 — A word the segmenter finds is not a chance pairing

Date: 2026-11-27. Status: Candidate (route: CJK recall), awaiting
Runtime's real-library comparison with 76ef079. Refines ADR-MEM-214 and
ADR-MEM-219.

## Context

Since ADR-MEM-214, a two-character piece of a CJK query is a word, and not
a chance pairing, only when it holds more than a quarter of the memories
of its rarer character. On Runtime's real library, "我的大学在哪里" did
not cue memories whose titles hold 大学. The answer (天津理工计科) was out
for another reason (`not_adopted`), but five memories about the university
were `no_cue`. The likely cause: 学 is common there (many 学习 memories),
so 大学 holds too few of 学's memories and is judged a chance pairing. A
chance pairing only orders ties, so these memories had no strong term.

## Decision

- `RecallExtras` gains `words`: the query's words of two or more
  characters, as the host's segmenter cut them.
- The host passes `query_words::query_words` (jieba cut) for every recall.
- A two-character piece equal to one of these words is a word, whatever its
  characters' frequency elsewhere. It keeps its weight (ADR-MEM-213) and
  becomes strong like any other word piece when that weight reaches the bar.
- Only exact two-character pieces are affected. 影集 inside a segmented
  摄影集 stays a chance pairing (ADR-MEM-215).

Without `words` nothing changes. The contract still depends on serde alone.

## Verification

- `a_segmented_word_is_not_a_chance_pairing` (host) uses a library where 学
  and 大 are common (学习, 大家) and one rare question character sets the
  bar. "我的大学在哪里" recalls all three 大学 memories with the
  segmenter's words and none without them.
- The other jieba, attention, catalog and Agent recall tests pass
  unchanged.
- Pending: Runtime's comparison on the question set at 16K and 64K.
