# 2026-11-18: A source typed as plain http clones over http

**Found in the ux19 Windows pass.** The profile reached its Gitea as `http://127.0.0.1:3000` (the WSL box's products on
loopback), and *Clone* refused it: "the source's address '…' is http, and knobas clones over https or ssh only". The
tester cloned the scratch repository with Git for Windows by hand and attached the folder. Self-hosted Gitea and
Forgejo on a LAN over plain http are common among the people knobas is for.

## Why it refused

ADR-0023 allowed https and ssh only, twice: `clone_url` refused any other scheme of the typed address, and git ran with
`-c protocol.allow=never` and only `protocol.https` and `protocol.ssh` allowed. The fetch of a missing commit
(ADR-0041) and a pull request's worktree fetch (ADR-0035) took the same list, `PROTOCOLS`. Nothing in the in-process
readers or the sync refused http: the source's API reads ran over that address, its token in every request.

## Decided (Fable, under Björn's delegation)

Asked whether to keep refusing, allow http anywhere, allow it on loopback only (the plugin-release rule), or allow it
with a note. **Allowed, wherever the address is typed as http, with no note**:

- The refusal protected nothing. An http-only server has no other address, so a person runs the same
  `git clone http://…` by hand, and knobas only loses the click. The source's token already crosses that address in
  clear on every sync, and a credential helper offers only what was stored for `http://` on that host.
- The plugin rule's loopback limit is about code knobas runs. A clone is files knobas never executes, with submodules
  off. Loopback only would have fixed this box and still refused the LAN Gitea.
- A note on *Clone* alone would flag the smaller exposure and leave the token's unsaid. If plain http deserves a
  line, it belongs once at the address field in Settings › Sources, covering sync too: a ticket of its own.

## Taken

- **`admits`** (`knobas_core::checkout::clone`): a scheme is admitted if the protocols name it, or if it is http and
  they name https (the production list). `clone_url` uses it; its refusal now says "…knobas clones over https, ssh
  or http only".
- **`protocol_args`**: the `-c` flags for a command that reaches a URL: every protocol refused, the list allowed, and
  `protocol.http.allow=always` only when the URL itself is plain http. An https or ssh source never gets http, so no
  redirect takes it down. The clone, the fetch and the worktree fetch all build their flags there.
- **An https or ssh address before any http one** (`pick_clone_url`): a source that names several (Gitea's
  `clone_urls`, a plugin's descriptor) clones over https or ssh when it has one, whatever the order.
- ADR-0023, ADR-0035 and ADR-0041 amended.

## Tests

`knobas-core`: `a_source_typed_as_plain_http_clones_over_http` (loopback and a LAN name, the URL and the flags),
`http_is_allowed_only_for_a_url_that_is_http` (no http for an https or ssh URL, a test's own list untouched), and the
refusal's words in `an_address_outside_https_ssh_and_http_is_refused`. `knobas-app`:
`an_https_or_ssh_address_is_taken_before_a_plain_http_one`; `clone_ipc.rs`'s production refusal of a `file://`
address names the three; `clone_live.rs`'s `the_app_itself_clones_the_testenv_http_address` (the testenv Gitea is
http) replaces the test that pinned its refusal.
