# Explicit Agent-note MCP server boundary

2026-11-03. Base `277165e`. [ADR118](../adr/118-agent-note-mcp-server.md)
and [protocol contract](../contracts/AGENT_NOTE_MCP_V1.md).

The new opt-in server exposes five existing initialized reads plus memory_note,
using one session's initialization/request-ID/count gate. It pairs separately
delegated native reader and note writer for the same Agent and exact leased
Vault instance. Independent scope hashes remain separate. Old initialized
servers still list five read tools and reject memory_note. Read dispatch and
tool metadata are reused without changing their public packet/output schemas.

Native note success returns only the independent hint_saved receipt; known
refusals contain no source/note body, native scope or memory ID. Native unknown
publication emits no tool result, closes the session and retains the complete
original MCP Value. Transport must actually disconnect before writing a Reply;
this server-only slice does not yet provide pipe-6, stdio or client setup.

Actual checks: **2 contract corpus tests, 9 Host note tests and 8 existing actual
initialized-pipe tests passed**, no final failures. Coverage includes six vs.
five tool advertisement; same-Agent but another native instance with the same
synthetic Vault ID refused; real read forwarding; closed versions/identity/scope
fields; known source-quote refusal with a still-live delegation; original-key
receipt replay; duplicate MCP ID refusal; and storage faults before/after CURRENT
that retain the entire original MCP message without a fabricated result.
Both advertised note schemas are bundled to local references only, and actual
saved/refused response data validates against the closed schema.

Independent pinned Python jsonschema 4.26.0 passed **66 schemas**, including
15 new MCP packet/response cases. Fmt, strict workspace/all-target Clippy,
Runtime surface and whitespace checks passed. Aggregate:
`aa0c2a128dcef56fbbe3ac9c4ddc224a38233cfd5f021eee5610b36013434818`.
No full workspace test rerun was made for this localized protocol addition; the
887-test baseline and preceding 41 worker tests remain separately recorded.

An intermediate test incorrectly expected deletion of one source to close the
entire still-valid source delegation. It was corrected: prepare/submit refuses
that source, while other delegated sources can remain live. The final 9-test
note target passed. An unused test import was removed before strict Clippy.

Owned files: Vault Agent note live gate; Host writer binding/getters/live gate;
MCP initialized metadata/dispatch reuse and new note server/export; note MCP
schema/corpus, Rust/Python cross-checks; ADR118/register, protocol contract, this
report, Runtime log and generated surface. Synthetic roots and owned processes
only. No real model, private data, Runtime edit or MCP configuration was used.
