# ADR-MEM-229 — The ChatGPT connector has its own local semantic index

Date: 2026-11-27. Status: Accepted (Morii's decision of 2026-11-27: the
connector uses vectors too). Extends ADR-MEM-220..223 and ADR-MEM-228. Until
now only Runtime attached a `SemanticIndex`, so `enouia-memory-remote`
recalled by words only.

## Context

Runtime embeds memory text with a local Ollama (bge-m3) and keeps its own
vector cache. The ChatGPT connector runs in its own process, under an owner
read grant that may cover fewer memories than Runtime shows. ADR-MEM-228
showed on the real library that words-and-meaning matches are the reliable
ones, and that meaning finds memories worded differently. ADR-MEM-220 said
that Memory never runs a model or opens a connection; that stays true for
the readers and the contract, not for this one service option.

## Decision

- **Option.** The service file gains `"semantic": {"ollama": <address>,
  "model": <name>}`. Absent, nothing changes. The address must be loopback
  (127.0.0.1 or ::1), checked when the service starts: memory text never
  leaves the machine.
- **What is embedded.** `QualifiedAgentReader::semantic_texts` returns the
  memories the connector's Agent may read, with the same text as the System
  reader's `semantic_texts` (content, a newline, the title). Nothing outside
  the grant is sent to the model.
- **When.** At the start of every permit generation (at most every four
  minutes) the service embeds the memories the cache lacks, in batches of
  16 through Ollama's `/api/embed` (`keep_alive` 30 minutes), and drops the
  vectors of memories no longer given (withdrawn, forgotten, deleted, out
  of scope). The first start embeds the whole library and delays serving
  until it is done. There is no background thread.
- **Cache.** `semantic-cache.json` in the service's owner-only `state_dir`,
  keyed by the digest of each memory's `TargetRef`, written durably. It is
  separate from Runtime's cache, so neither drops the other's vectors.
  A cache of another model is ignored.
- **Query.** One embedding per recall or `session_start`, waiting at most
  3 seconds; on any failure the recall is lexical, as before.
- **Related.** With an index, the Agent's `related` lists semantic
  neighbors (ADR-MEM-220) and its `unavailable` is `["same_conversation"]`;
  the qualified and initialized read schemas allow both values.
- **Readiness.** `read-check` reports `semantic`: the address, the model,
  whether the model answers (`reachable`) and how many vectors are cached.
- The HTTP client is a minimal HTTP/1.1 POST over a loopback `TcpStream`.
  It reads until the body ends (`Content-Length` or chunked), because Ollama
  may keep the connection open.

## Verification

- `the_connector_index_embeds_what_its_agent_may_read` (host, fake Ollama):
  a non-loopback address is refused; only the Agent's memories are sent;
  a wider scope embeds only what is new; the cache survives a reopen
  without new requests; the vectors cue the Agent's recall; `related`
  reports semantic neighbors; a narrower scope drops vectors from the
  file; with Ollama unreachable the refresh fails and the cache stays.
- `a_local_ollama_answers_the_connector_index` (ignored; run on Morii's
  machine against bge-m3 with synthetic sentences): 1024 dimensions, and a
  sentence about green tea is closer to another about green tea than to
  one about rain.
- Remote service, OAuth, HTTP and connector tests are unchanged.
