# MV-6 source excerpt identity and read ordering

Date: 2026-10-04. Base: `d68dd39`. Scope: source excerpt UI with the existing Core byte-range contract.

The production Source component passed **4/10** deferred-client checks. Older excerpts replaced newer ones, obsolete errors reappeared, failures retained previous contents, and a new source/revision or withdrawn evidence still displayed an old excerpt. Byte-range retry itself already retained its captured arguments.

Reads now use the latest-generation rule and clear the prior excerpt when starting. Each loaded excerpt retains its source ID and revision and renders only when both match the available evidence. Identity/revision/availability changes invalidate reads and clear transient contents. A current failure therefore shows an error without stale text; retry still requests the original source revision and byte range. Busy source reads are announced. Core slicing and provenance rules are unchanged.

The shared independent fixture passed **64/64**, including ten source cases plus the preceding 54 checks. Controlled evidence changes and deferred responses are simulated; this does not prove live provenance withdrawal by a separate client. Frontend and separate fixtures built successfully. The rebuilt actual app passed **52/52** on a fresh synthetic Vault, including actual source reading, Core/Mock workflows and acknowledged-record recovery after a renderer crash. Its final installer passed **8/8**. No fixture API or marker enters the production frontend.

Current-day pinned offline format, **201 Rust tests**, Clippy and independent **34-schema** validation remain valid for unchanged Rust/contracts. All inputs are synthetic, all Vaults/builds/local evidence remain ignored, and MV-7 is unstarted. Manual accessibility/login/interactive upgrade, signing, Host authentication and power-loss acceptance remain pending.
