# ADR-MEM-220 — Semantic vectors from a local model, scored by Memory

Date: 2026-11-26. Status: Accepted (Morii, 2026-11-26: "那就开始吧", with the
model running on his computer and Memory choosing the approach). Runtime
implements the model side. This decision opens MV-11's semantic neighbors
(B3), which Morii had deferred on 2026-11-24.

## Context

Recall and neighbor suggestions are lexical (ADR-MEM-162, 209–216). Lexical
matching cannot connect memories that mean the same thing in different
words, for example 第一次喝酒 and 首次饮酒.

Morii chose these constraints:

- **The model runs on his computer.** The VPS is too small for one.
- **Memory content never leaves his machines.** No hosted embedding API.

Other projects (Ombre-Brain, mem0, Letta) mix lexical scores with embedding
similarity. They differ mainly in where the model runs.

## Decision

### Split

- **Memory** defines the vector format and the similarity, and merges
  similarity into recall and neighbor suggestions. Memory never calls a
  model and never opens a network connection; the Core stays
  transport-neutral.
- **Runtime** runs the model and keeps a cache of vectors. It does this
  through a `SemanticIndex` it implements and attaches to the hosts it
  creates. Its first implementation uses Ollama's local `/api/embed` with
  `bge-m3`: 1024 dimensions, about 1.2 GB, CPU only.

### Contract

- **`SemanticVector { model, values }`.**
  - `model` names the model and version, 1–128 characters.
  - `values` is the L2-normalised vector scaled to `i16` (×32767), 1–4096
    dimensions.
  - `SemanticVector::from_f32` normalises and quantises model output. It
    refuses a zero, non-finite or oversized vector.
- **Similarity.** `semantic_milli(a, b)` is the cosine in thousandths,
  computed in integers so every platform agrees. Two vectors of different
  models or sizes have no similarity.
- **Recall.** The catalog and qualified selectors take an optional semantic
  input: the query's vector and one optional vector per candidate.
  - A candidate at or above `SEMANTIC_FLOOR_MILLI` adds
    `(similarity - floor) × SEMANTIC_SCALE` to its lexical score, with the
    reason `semantic`.
  - At or above `SEMANTIC_CUE_MILLI` it is cued even with no word in common.
  - Without semantic input, every result is exactly as before.
  - The constants start at floor 550, cue 650 and scale 2. They will be
    tuned on Runtime's real-library checks, as the lexical rules were.
- **Graph.** A new edge kind `semantic_neighbor` is a suggestion, never a
  recorded relation. `related` lists up to five memories at or above the
  cue similarity. Without vectors, the graph still reports semantic
  neighbors as unavailable.

### Host

- **The index trait.** `SemanticIndex` (`Send + Sync`) has:
  - `model() -> &str`;
  - `query(text) -> Option<SemanticVector>`;
  - `memory(&TargetRef) -> Option<SemanticVector>`.

  A `None` answer means lexical only: no model, Ollama not running, or the
  memory not yet indexed. A missing index never fails a read.
- **Attaching it.** `QualifiedAgentHost::with_semantic(index)` and
  `NativeMemoryOwner::with_semantic(index)` attach the index. Recall,
  `session_start` and `related` then use it. Every authorization, sensitivity
  and deletion check still runs first, and vectors are looked up only for
  candidates already authorized.
- **What to embed.** `NativeMemoryOwner::semantic_texts(max)` lists, for
  each current memory in Morii's full-history read, its `TargetRef` and its
  text (content, then title). This is the set Runtime embeds. Vectors are
  keyed by `TargetRef`:
  - a revised memory needs a new vector;
  - a vector whose target is no longer listed belongs to a withdrawn,
    forgotten or deleted memory, and Runtime purges it.

### Privacy

- **Location.** Vectors are derived data. They live in Runtime's private
  data directory on Morii's computer, outside the Vault, its exports and
  every repository. They can be rebuilt at any time.
- **Treatment.** Vectors can partially reveal content, so they are handled
  as sensitive and never sent anywhere.
- **The VPS.** The remote service (ChatGPT) has no index for now. It keeps
  lexical recall, plus the synonyms ChatGPT adds (ADR-MEM-218).

### Later, separately

- **Vectors on the VPS.** Once the Vault moves to the VPS, Runtime may upload
  memory vectors to the remote service. Memory content is already there, and
  the VPS never runs the model. Query vectors are still missing there, so
  ChatGPT recall stays lexical plus synonyms; neighbor suggestions could use
  the uploaded vectors. This belongs to MV-10's local replica work and
  starts only when Morii asks.
- **jieba.** jieba segmentation is the open experiment on branch
  `claude/hybrid-recall`. It is not part of this decision.

## Verification (planned)

- **Contract tests.**
  - Quantisation and integer similarity agree on both platforms.
  - Mismatched models are refused.
  - With semantic input, a memory worded differently but similar in meaning
    is cued and ranked.
  - Without it, the attention goldens are unchanged.
- **Host tests.**
  - A fake index (fixed vectors) changes recall and `related` as expected.
  - A missing or failing index leaves results lexical.
  - `semantic_texts` omits withdrawn and deleted memories.
- **Real library.** Runtime calibrates the constants on its real-library set
  with bge-m3.
