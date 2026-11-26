# ADR-MEM-221 — Semantic similarity is judged against its own distribution

Date: 2026-11-26. Status: Accepted (route: semantic recall); synthetic
validation, awaiting the Runtime session's real-library rerun. Refines the
thresholds of ADR-MEM-220.

## Context

The Runtime session wired its bge-m3 index (Runtime #268) and measured the
real library: 351 memories and ten everyday questions, 3510 pairs, read-only.

- **Distribution.** Similarity (cosine ×1000) had median 385, p90 488, p99
  560 and maximum 648. No pair reached the fixed cue of 650, so semantic
  similarity never cued a memory alone. Score was added only between 550 and
  648.
- **Overlap.** Relevant pairs mostly fell between 540 and 650. Irrelevant top
  pairs reached 550–560:
  - a question with no answer in the library (pets) peaked at 556;
  - 东京Day4 scored 550 for the Kyoto question;
  - a visual-design record scored 560 for "Enouia迁移".

  No fixed threshold separates the two groups.
- **Clusters.** Relevant results cluster at the top of their question:
  - Kyoto: 648, 640, 594, 584;
  - learning: 603, 570, 570, ...

  Irrelevant top hits stand alone (pets: 556, then 531).

## Decision

Similarity is judged against its own distribution. For recall, that is the
query's similarities to every authorized candidate that has a vector. For
`related`, it is the anchor's similarities to every memory in scope.

- **Statistics.** `SemanticStats::of` computes the count, mean and standard
  deviation in integers (integer square root), so every platform agrees.
- **Score and cue.** With at least `SEMANTIC_MIN_SAMPLE` (30) similarities
  and some spread, a similarity's standing is z, in hundredths of a
  deviation above the mean:
  - z from `SEMANTIC_BONUS_Z100` (2.0) adds
    `(z − 2.0) × SEMANTIC_Z_SCALE` (60 per hundredth), so 3.0 adds 6000;
  - z from `SEMANTIC_CUE_Z100` (3.0) cues the memory alone and makes a
    semantic neighbor;
  - below `SEMANTIC_GUARD_MILLI` (500) nothing counts, however far the
    similarity stands out.
- **Fallback.** With fewer similarities or no spread, the absolute
  thresholds of ADR-MEM-220 still apply: floor 550, cue 650, scale 40.

Everything else is unchanged: authorization first, reasons, the trait, and
lexical scoring. Without an index nothing changes.

## Expected on the real library

With deviation about 70:

- **Kyoto.** The Kyoto cluster (648, 640) is cued alone; 594 and 584 add
  score.
- **Pets.** The isolated pets hit (556, z about 2.4) adds a little and is no
  cue.

Runtime will confirm with the same set at 16 KB and 64 KB, and report the
z values of relevant and irrelevant top hits.

## Verification

`enouia-memory-contract` `attention`:

- **Statistics.** `semantic_similarity_is_judged_against_its_own_distribution`
  uses 300 background similarities with mean 385 and deviation about 70.
  - 648 and 640 cue; 584 adds score without cueing; 480 adds nothing.
  - An isolated 556 is no cue.
  - A far outlier below the guard counts for nothing.
  - Three similarities use the absolute thresholds.
- **Recall.** `recall_cues_the_memory_that_stands_out`: of forty memories
  with no word in common, only the one at 648 is selected.
- **Earlier tests.** The semantic tests of ADR-MEM-220 still pass. Their
  small samples or identical vectors use the fallback.
