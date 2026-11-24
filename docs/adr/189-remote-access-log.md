# ADR-MEM-189 — Minimal access log for the remote service

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use, MV-9.1 minimal
logging); synthetic validation.

## Context

`enouia-memory-remote serve` wrote nothing while it ran. On a VPS, the only
signs of trouble were ChatGPT errors and the exit message, for example:

- whether ChatGPT reached the service at all;
- which tools it called;
- whether calls were refused;
- when the read grant ends.

MV-9.1 asks for minimal logs, and the public repository rule is that
memories, chat text and credentials never leave the Vault in logs.

## Decision

The service writes plain lines to stderr, which systemd sends to journald.

- **Startup line, once per process.** Time, `resource`, bind address, the read
  grant's `valid_until` (or `none`), and whether writes are on.
- **One line per request,** in this order:
  - UTC time;
  - HTTP method;
  - the path with any query string removed;
  - status;
  - duration in milliseconds;
  - `rpc=<method>` for JSON-RPC bodies;
  - `tool=<name>` for `tools/call`;
  - `error` when the reply has a JSON-RPC error or `isError: true`.

Nothing else from a request or reply is written. Query strings, bodies,
arguments, results, headers, client IDs, approval codes, authorization codes
and tokens are left out. No log file, rotation or level setting is added;
journald keeps and rotates the lines.

Example:

```text
2026-11-24T04:31:02.114Z POST /mcp 200 18ms rpc=tools/call tool=memory_search
2026-11-24T04:31:09.870Z POST /mcp 200 41ms rpc=tools/call tool=memory_request_forget error
```

## Verification

`remote_daily_process` runs on Windows and on Linux. It drains the real
process's stderr and checks the following.

- **Required lines.** The log has:
  - the startup line;
  - `GET /oauth/authorize` from a request that carried a query;
  - `POST /mcp 200`;
  - `rpc=tools/call tool=memory_search`;
  - a refused forget request logged with `error`.
- **Absent values.** The log contains none of:
  - the query value;
  - the conversation text ("Lumen", "corner seat");
  - the approval code, the authorization code or the access token.

The other remote test suites pass unchanged.
