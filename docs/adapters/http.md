# HTTP

An external adapter's one way out is `wasi:http` 0.2's outgoing handler. There is no socket, no DNS of its own and no file system. The host decides, request by request, whether a request leaves, what it carries and how fast it goes. This page says what the host does, so the adapter does not do it twice or fight it.

Source: the module docs of `crates/knobas-wasm-host/src/net.rs`, `allow.rs` and `redact.rs`. A working client is the XWiki adapter's [`adapter/src/http.rs`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/adapter/src/http.rs), about 150 lines over the `wasip2` crate (`wasip2 = "=1.0.4"`, the copy Rust's std for `wasm32-wasip2` resolves).

## Which hosts a request may reach

The manifest's `hosts` list is the allowlist. Each entry is one of:

- **a template**, the whole entry `{key}`: the value of `base_url` (the source's address) or of a property the descriptor's `config_schema` declares. A URL value allows its own scheme, host and port; a bare host allows `http` and `https` on the default ports.
- **a fixed host**, `api.example.com` or `api.example.com:8443`: that host on that port, or on the scheme's default port.

No paths, no wildcards, no `user@`, no template inside a longer entry. Matching is on the resolved host after URL parsing, never on text: `api.example.com` does not allow `api.example.com.evil.test`. A template that resolves to nothing usable allows nothing.

A request to any other host fails with `connection-refused` before a socket opens. The host also refuses a fixed **name** whose addresses are not public (loopback, private ranges), on every connection, so the owner of a declared domain cannot point it at the reader's machine. A template's value is one the reader typed, so `{base_url}` may be `http://127.0.0.1:8099`.

Most adapters need `["{base_url}"]` and nothing else. The reader sees the resolved list before confirming an install.

## The credential

The component never sees the secret. The host attaches it to every request whose origin (scheme, host and port) is the source's address, and to no other:

| `auth_methods` entry the reader picked | What the host sends |
|---|---|
| `Pat`, `ApiToken` | `Authorization: Bearer <secret>` |
| `UserPassword` | `Authorization: Basic`, from the config's `username` and the secret |
| `OAuth` | nothing: the source fails to build |

An `Authorization` header the component set on such a request is replaced. A `UserPassword` source whose config has no `username` fails to build, so declare `username` in `config_schema` when you offer that method. A product that wants the token elsewhere (a custom header, a query parameter) cannot be served by an external adapter today.

## What comes back

- **Redacted.** Every response from the source's address reaches the component with each spelling of the secret replaced by `[redacted]`, in the body and in the headers, and a header holding one dropped whole. A server that echoes the request cannot hand the secret back. The replacement is textual: when the battery's test server for the [quickstart](quickstart.md) used the token `t`, every `t` in its JSON became `[redacted]` and no memo decoded. Use realistic secrets in tests.
- **Decoded.** The component's `Accept-Encoding` is dropped from requests to the source's address, so the body arrives uncompressed and the redactor can read it.
- **No redirects followed.** A `3xx` reaches the component as it is. Follow it yourself if you want to; the next request is held to the allowlist like any other. XWiki answers an unauthenticated `bin/` URL with a `302` to its login page: that is an answer to classify, not a page to fetch.
- **No cookies kept.** Each request stands alone, so a product that needs a session cookie needs the adapter to carry it between requests within one call.

## Pacing and time

Requests to the source's address wait on the instance's own rate limit, shared with every built-in client for the same origin: five a second, a burst of ten. A naive adapter that pages as fast as it can sends no more than a built-in one would. Requests to other declared hosts are not paced. A `Retry-After` is yours to read and honour.

The call's deadline (ten minutes by default) holds everything, every wait included. Within it the host honours the component's `request-options` timeouts. An outgoing body is buffered whole before it leaves, so it counts against the memory cap.

The pacing sets a floor on run time. The XWiki adapter makes four requests per page (record, comments, tags, rendering), so a full sync of a thousand pages takes at least 800 s at five a second, past the default deadline. Its manifest asks for `deadline_secs: 900`; a wiki that large needs more, up to the maximum of 3600 s, or fewer requests per item.

## Errors from the transport

A transport failure reaches the component as a bare `wasi:http` error code, with no text quoting the request: `ConnectionRefused`, `DnsError`, `ConnectionTimeout` and so on. A request the allowlist refused looks like `ConnectionRefused`. Map them as [sync.md](sync.md#errors) says, with one difference for writes:

- a write whose connection never opened (`ConnectionRefused`, a DNS failure, a connect timeout, TLS) did nothing: `Unreachable`, and the write queue sends it again;
- a write that left and got no answer may have landed: `Unconfirmed`, and the queue holds it for a person to check. [writes.md](writes.md).

## Checking who you are

A product that serves a failed login as a guest gives no status to classify. Find the response that names the authenticated user and compare it with the configured one. For XWiki it is the `XWiki-User` header, present on every authenticated response and on none other. The cheapest request that carries it is the adapter's who-am-I. A guest answer is `Unauthorized`.

Be careful with wrong passwords in tests. XWiki locks an account behind a captcha after three failed logins in five minutes, and basic auth cannot answer a captcha: every later request is the guest's, the right password included. The XWiki adapter's check runs the battery's unauthorized case as a user that does not exist, so it never locks out the real one.
