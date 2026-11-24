# ADR-MEM-201 — The loopback driver answers `Expect: 100-continue`

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The loopback HTTP/1.1 driver (ADR-MEM-130/176) ignored `Expect:
100-continue`. A client or reverse proxy that sends this header waits for a
`100 Continue` before it sends the body. The driver, meanwhile, waits for the
body. Typical clients give up waiting after a timeout, often one second, and
send the body anyway. Every conversation submission is well over 1 KiB, so
each one could pay that delay, and a client without such a timeout would
stall until the 10-second I/O timeout.

## Decision

The driver sends `HTTP/1.1 100 Continue` when all of the following hold:

- the request's head carries `Expect: 100-continue`;
- the head is accepted (framing valid, size within `MAX_INPUT_BYTES`);
- a body is still to be read (a `Content-Length` above zero, or chunked).

A request refused on its head (for example 411 or 413) gets only its final
status, as RFC 9110 §10.1.1 allows. The interim response may still be sent
when the body has already arrived. HTTP/1.1 clients ignore an unexpected
`1xx`.

## Verification

`remote_http1::expect_continue_gets_an_interim_response` covers three cases:

- **Without the header.** A request gets only `200 OK`.
- **With the header.** The output is `100 Continue` and then `200 OK`.
- **Over a real loopback socket.** The client sends its head, waits for the
  `100`, and then sends the body. It then receives `200 OK`.

The other HTTP and remote suites pass unchanged.
