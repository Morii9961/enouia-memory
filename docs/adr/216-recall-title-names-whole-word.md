# ADR-MEM-216 — A title naming the whole query word earns its bonus again

Date: 2026-11-25. Status: Accepted (route: ChatGPT daily use and Runtime
recall); synthetic validation, awaiting the Runtime session's real-library
check. Refines ADR-MEM-214 (adopted by Runtime #258); ADR-MEM-215 had no
effect on the real library.

## Context

ADR-MEM-215 left every real-library order unchanged. "摄影集" still ranked
the broader memory first, 21403 against 21203. The Runtime session then
checked the actual counts, read-only:

- **Library.** "摄影集" occurs in 6 memories, "摄影" in 28.
- **The photo-book memory.**
  - Its body holds 摄影集, 摄影 and 集 once each.
  - Its title contains 摄影集.
- **The broader memory.**
  - Its body also holds 摄影集 once, plus 摄影 six times and 集 once.
  - Its title does not contain 摄影集.

So both memories hold the whole word and get the whole-run bonus. Both reach
the two-mention cap on 摄影:

- the photo book, from body 1 plus its title counting 2;
- the broader memory, from its body.

影集 is a chance pairing in this library, because 集 is common, so it adds
nothing. The tie-break then goes to the broader memory's six repeats of
摄影. ADR-MEM-215's change to character coverage could not alter this.

## Decision

When a memory's title contains a whole CJK run of three or more characters
from the query, the whole-run bonus (the run's heaviest piece) is added a
second time. The bonus sits outside the two-mention cap. A title that names
the whole query word says what the memory is about. Repeats of a sub-piece
stay a tie-break.

Nothing else changes. Queries without CJK are unchanged.

## Verification

`enouia-memory-contract`
`attention::a_title_naming_the_whole_word_beats_repeats_of_a_part`
reproduces the reported counts:

- The photo book has 摄影集 in its title and once in its body.
- The broader memory has 摄影 in its title and six times in its body, plus
  one 摄影集.
- Four other memories hold 摄影.
- Ten everyday memories make 集 common, so 影集 is a chance pairing.

On 3a4b115 the broader memory leads, which reproduces the real order. With
this change the photo book leads. All 16 attention tests pass, as do the
contract, host and init suites.
