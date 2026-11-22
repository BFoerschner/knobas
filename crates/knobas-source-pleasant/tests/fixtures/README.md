# Pleasant answers the adapter's tests replay

**The real server's answers** are in `testenv/pleasant/recorded/`: Pleasant
9.5.4, recorded by the server agent on the free edition's 20 entries, masked
(`S/pleasant-server.md` §6). `tests/it/recorded.rs` serves them as they were
answered, status, content type and body, with each mask made concrete: an id
becomes a fixed GUID, a time a fixed instant, a code seed RFC 6238's. The sync,
the password reads and their refusals, the sign-in refusals, the expired token,
the version and the folder walk's comment rule are tested over them.

`tree.json` is the older **stand-in**, written from the vendor's documented
shapes before the server was recorded. It stays for what the 20 do not hold
in one tree: every code-key format side by side, a key knobas cannot read, a
field the listing sends empty. Where it and the recordings disagree, the
recordings are right.
