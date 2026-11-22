# Pleasant Password Server: the real server holds 20, a stand-in holds 2,000, a contract binds them (2026-11-22)

**The fork.** The Passwords role's first source is Pleasant Password Server, which runs only on Windows. Its free edition holds 20 passwords and 3 users. A trial licence means talking to Pleasant's sales team, and Björn ruled that out ("never contact Pleasant"). His words on the way forward: "do 20 passwords and if you are sure how pleasant works in general, maybe just mock it and just do 2000 entries with a mock? but be really sure you're correctly mirroring their actual api."

**Taken.**
- **The real server.** A disposable Windows Server 2025 VM (dockurr/windows on the WSL box, 3 GB, `--restart no`, run only under a pc ticket) runs Pleasant 9.5.4.0 with `plan(20)` from `testenv/pleasant/`. It's the witness for behaviour.
- **The stand-in.** `testenv/pleasant/standin.mjs` serves the same API v5 routes at 2,000 entries for volume, and only for volume.
- **The contract.** `testenv/pleasant/contract.mjs` sends one request list (49 requests: sign-in failures, an expired token, a missing comment, the entry limit, writes) to both servers and fails on any difference in field names, casing, types, nulls, array order, status, error body, or the headers the adapter reads. `recorded/` keeps the real answers, so `witness-unit` checks the stand-in with no VM up.
- **Where it lives.** A Fable ruling (02:13) chose Node in `testenv/`, not a crate. mockd stays frozen. Port 8297 is a host socket outside the port table, like the canary's. ADR-0013 records it as the licence exception.

**Measured on the real server, and why it matters** (the full table is in `testenv/pleasant/README.md`):
- **The audit log.** Of the calls a sync makes, only a token request writes a line ("Identity Verified", under the user). The tree, listings, an entry by id, attachments and search write nothing; Pleasant 9.5 has no "entry viewed" event to turn on. A password read writes "Password Fetched", with its usage comment. So a by-hand sync costs one line under the user's name, or none while its 12-hour token lasts.
- **Custom fields.** Listings carry every custom field's value in plain text. No field is marked protected; a KeePass-protected PIN comes back as a plain string.
- **Native TOTP.** Pleasant 9 has its own TOTP on each entry (`TOTPSecret` and friends). It comes in listings, next to the four plugin formats in custom fields.
- **Passwords by id.** `GET entries/{id}` answers `Password: null`; the docs' example is out of date.
- **Comment rules.** They're inherited, and entries carry them. Folder objects carry them only in the whole-tree call.

**Reversing it.** A licence that lifts the cap retires the stand-in, and the real server takes the volume. The contract stays as the real server's regression test.
