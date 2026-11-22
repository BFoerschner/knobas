# `testenv/pleasant` — Pleasant Password Server: the real 20, a stand-in for volume, the contract between them

Pleasant Password Server is Windows only, and its free (unregistered) edition holds **20 passwords and 3 users**
(https://pleasantpasswords.com/download). There's no trial licence to be had (Björn, 2026-11-22: never contact
Pleasant). So, by the licence exception in ADR-0013:

- **The real server at 20 entries is the witness** for how Pleasant behaves, and the adapter's certificate.
  It runs in a disposable Windows Server 2025 VM on the WSL box (below).
- **`standin.mjs` stands in for volume alone:** 2,000 entries, a tree 8 levels deep, three folders of 500+. Its
  timings are knobas' timings, not Pleasant's.
- **`contract.mjs` holds the stand-in to the real server.** It sends one request list (`requests.json`) to both,
  each holding the same 20 entries, and fails on any difference: names, casing, types, nulls, arrays and their order,
  status codes, error bodies, and the headers the adapter reads. `recorded/` holds the real server's answers, so
  the stand-in is checked against them in `just witness-unit` with no VM up.

What the real server does only above 20 entries (its speed, its audit log at volume, anything a paid edition adds) is
witnessed by nothing.

## Files

| File | What |
|---|---|
| `plan.mjs` | The estate, deterministic. `plan(20)` is the real server's; `plan(2000)` starts with those 20 and adds the generated rest. `node plan.mjs 2000` prints a summary. |
| `tidewater.json` | The clickdummy's Pleasant entries (`mockups/passwords-2/data.js`), made up. |
| `seed.mjs` | Writes `plan(20)` to a real server. Idempotent against the server itself. |
| `standin.mjs` | The stand-in: the API v5 routes the adapter uses, from `plan(n)`. |
| `requests.json` | The one request list, error cases included. |
| `contract.mjs` | `record` (real → `recorded/`), `--live` (real against a fresh stand-in), `--recorded` (stand-in against `recorded/`). |
| `recorded/` | The real server's answers, normalised (ids as plan names, times as their format, secrets masked). |
| `test/` | `node --test` cases: the contract against `recorded/`, and the stand-in at 2,000. |

## The stand-in

```sh
node testenv/pleasant/standin.mjs --listen 8297 --entries 2000        # the app-facing instance
node testenv/pleasant/standin.mjs --listen 0 --entries 20 --control   # what the contract spawns
```

It listens on `127.0.0.1` only, over plain HTTP (the real server answers HTTPS on 10001 with a self-signed
certificate). Port 8297 is a host socket outside the README's port table, like the canary's 8299 and `stall.sh`'s
8298, so `check-ports.sh` asserts nothing about it. Users are `admin`, `tomasz.wojcik` (may write) and
`mara.lindqvist` (reads only), each with password `knobas-dev`, unless `--user name:password:role` names others.
`--delay <ms>` holds every answer that long. `--log` prints each request with its status and time. `--control` adds
`POST /__standin/clock` (`{"advanceSeconds": n}`), so the contract can expire a token without waiting.

## The real server

The VM lives on the WSL box under `~/scratch-pleasant/`. It loads the whole PC, so **start it only while you hold
the pc ticket**, and stop it when you release:

```sh
ssh knobas-wsl '~/scratch-pleasant/vm.sh start'    # boots in about a minute; Pleasant answers on WSL's 127.0.0.1:24801
ssh knobas-wsl '~/scratch-pleasant/vm.sh stop'
```

The three users' names and passwords are in `~/scratch-pleasant/credentials.env` there (mode 600), exported as
`KNOBAS_PLEASANT_*` by `just pleasant-contract`.

```sh
just pleasant-contract            # real server vs a fresh stand-in; fails on any difference
just pleasant-contract record     # rewrite recorded/ from the real server (then commit the change)
```

### Setting up a fresh VM (what the recordings assume)

`vm.sh start` on an empty `storage/` installs Windows Server 2025 (eval) unattended in about 15 minutes. Its OEM
script installs Pleasant with `PleasantPassServer.exe /quiet` (the stable channel, 9.5.4.0, signature checked) and
turns sshd on. Then:

1. In the web UI (https://127.0.0.1:24801), sign in as `admin`/`admin`; it asks for a new password at once.
2. Users › Add New User: `tomasz.wojcik` and `mara.lindqvist`, in the default role "Users". The form makes
   client-side keys, so this is UI-only.
3. Settings › General › "Allow User Access to be modified via API". Then
   `POST /api/v5/rest/folders/{Tidewater}/useraccess` gives the writer *Full* and the reader *Read-only*
   (`GET accesslevels` lists the ids).
4. `node testenv/pleasant/seed.mjs` writes `plan(20)` (20 entries, 31 folders, 3 attachments, one native TOTP).
5. Advanced › Comment Triggers: add "View password" (View Entry Password). In the web client, on
   `Tidewater/Payments/Prod`: Comment Requirements › Add, that trigger, role "Users".

## What the real server answers (Pleasant 9.5.4.0, recorded 2026-11-22)

The audit log, read from the admin UI's Logged Events under default settings (all 189 event types on; there is no
"entry viewed" type):

| Call | Audit line, under the calling user |
|---|---|
| `POST /OAuth2/Token` | Identity Verified (a wrong password: Identity Not Verified and Sign-in Failed) |
| `GET folders`, `GET folders/{id}`, `GET entries/{id}`, attachments, search, `GetServerInfo`, `IsCommentRequired` | none |
| `GET`/`POST entries/{id}/password` | Password Fetched (with the usage comment, if one was sent) |
| `PATCH`/`PUT`/`POST`/`DELETE` an entry | Entry Updated / Created / Deleted |
| the OfflinePackage endpoint | Offline Sync: never call it |

Shapes the docs don't show, all in `recorded/` and `shapes.mjs`:
- entries carry `Attachments` (metadata only), `HasViewEntryPasswordAccess`, `HasViewTOTPAccess`,
  `HasModifyTOTPAccess`, and Pleasant's own TOTP: `TOTPIssuer`, `TOTPSecret` (plain), `TOTPDigits`, `TOTPPeriod`;
- `GET entries/{id}` answers `Password: null`; only `…/password` returns it;
- custom fields come back in plain text in every listing, and no field is marked protected;
- the comment rule is inherited and entries carry it, but folder objects carry it only in the whole-tree `GET folders`;
- the API's errors are `text/plain` bodies holding `{"error_message": …}`, for example `400 {"error_message":"Comment required."}`;
  authorization failures, an expired token among them, are `401` JSON with `WWW-Authenticate: Bearer`;
- a token lasts 12 hours (`expires_in: 43199`).

Not in REST at all (web client only, cookie session): access requests, comment rules, an entry's history.
