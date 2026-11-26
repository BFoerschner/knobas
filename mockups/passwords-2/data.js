/*
  The model under the Passwords clickdummy (second pass). Nothing here is real: Tidewater is the mockups'
  fictional company, and every password, key and code is made up.

  A password manager is a source. Its adapter fills the password role: the minimum is to list entries
  (title, username, address, folder path as a list of segments, when changed) and to read one secret when
  asked. Everything else is optional and declared per adapter in `caps`; the page draws a control only
  where the entry's source declares it.
*/
"use strict";
const DATA = (() => {
  /* What an adapter may declare beyond the minimum, in the words the source's Settings page uses. */
  const CAPS = [
    ["write", "Add and edit entries, change passwords"],
    ["totp", "One-time codes"],
    ["files", "Attachments"],
    ["history", "History"],
    ["expiry", "Expiry dates"],
    ["comments", "Usage comments, where the server requires them"],
    ["audit", "An audit log of who read which password"],
    ["requests", "Access requests"],
  ];

  /* Every adapter *Add a source* offers. `role` marks the ones that fill the password role. */
  const ADAPTERS = {
    jira: { name: "Jira", holds: "Tickets, saved views", secret: "Token", user: true },
    confluence: { name: "Confluence", holds: "Pages", secret: "Token", user: true },
    gitea: { name: "Gitea", holds: "Repositories, pull requests, branches, commits", secret: "Token" },
    teamcity: { name: "TeamCity", holds: "Builds", secret: "Token" },
    kuma: { name: "Uptime Kuma", holds: "Monitors", secret: "Token" },
    pleasant: {
      name: "Pleasant Password Server", holds: "Passwords", role: true, secret: "Password", user: true,
      caps: { write: 1, totp: 1, files: 1, history: "log", expiry: 1, comments: 1, audit: 1, requests: 1 },
      /* It syncs only by hand unless the reader says otherwise, so nothing is read from it in the background. */
      everyDefault: 0,
      latency: 1300,
      placeholder: "https://passwords.tidewater.internal:10001",
      fields: [{ k: "user", l: "User", ph: "mara.lindqvist" }],
      kept: "Stored in macOS Keychain. The adapter signs in with it at every sync and every read.",
    },
    bitwarden: {
      name: "Bitwarden or Vaultwarden", holds: "Passwords", role: true, secret: "Master password",
      caps: { write: 1, totp: 1, files: 1, history: "earlier" },
      everyDefault: 900,
      latency: 260,
      placeholder: "https://vault.tidewater.example (empty for bitwarden.com)",
      fields: [
        { k: "client_id", l: "API key client_id", ph: "user.4f1c9a…" },
        { k: "client_secret", l: "API key client_secret", ph: "", secret: true },
      ],
      kept: "Stored in macOS Keychain with the API key. The adapter signs in with the key and decrypts with the master password, on this Mac, at every sync and every read.",
    },
  };

  /* The four places a KeePass TOTP plugin keeps its key. The Pleasant adapter reads all of them. */
  const TOTP_FORMATS = [
    { id: "builtin", plugin: "KeePass 2.47+, built in", field: "TimeOtp-Secret-Base32", format: "The key in Base32" },
    { id: "traytotp", plugin: "KeeTrayTOTP", field: "TOTP Seed, TOTP Settings", format: "Key and settings in two fields" },
    { id: "keeotp", plugin: "KeeOtp", field: "otp", format: "key=…&step=30&size=6" },
    { id: "xc", plugin: "KeePassXC style", field: "otp", format: "An otpauth:// link" },
  ];

  /* Settings › Sources, in the app's order. `every` is seconds; 0 is "only when I press Sync now". */
  const SOURCES = [
    { id: "jira", adapter: "jira", name: "Tidewater Jira", address: "https://jira.tidewater.example", user: "mara", holds: "1,284 tickets", every: 300, at: "10:20" },
    { id: "wiki", adapter: "confluence", name: "Tidewater Confluence", address: "https://wiki.tidewater.example", user: "mara", holds: "3,912 pages", every: 900, at: "10:15" },
    { id: "gitea", adapter: "gitea", name: "Tidewater Gitea", address: "https://git.tidewater.internal", holds: "2,406 items", every: 60, at: "10:23" },
    { id: "teamcity", adapter: "teamcity", name: "Tidewater TeamCity", address: "https://ci.tidewater.internal", holds: "312 builds", every: 30, at: "10:23" },
    { id: "kuma", adapter: "kuma", name: "Tidewater Uptime Kuma", address: "https://status.tidewater.internal", holds: "18 monitors", every: 20, at: "10:23" },
    {
      id: "team", adapter: "pleasant", name: "Team passwords", address: "https://passwords.tidewater.internal:10001", user: "mara.lindqvist",
      every: 0, at: "10:22", synced: true, totpFormat: "builtin", commentFolders: [],
      /* A folder with nothing in it still has a place in the tree. */
      folders: [["Tidewater", "Office", "Archive"]],
      runs: [["today 10:22", "6.4 s", "32 entries in 26 folders, 1 changed. 4 code keys kept in the keychain."], ["2026-11-20 16:05", "7.1 s", "32 entries in 26 folders, none changed."], ["2026-11-19 09:12", "6.8 s", "32 entries in 26 folders, 3 changed."]],
    },
    {
      id: "org", adapter: "bitwarden", name: "Tidewater org", address: "https://vault.tidewater.example", user: "mara.lindqvist@tidewater.example",
      every: 900, at: "10:20", synced: true, folders: [],
      runs: [["today 10:20", "0.9 s", "12 entries, none changed."], ["today 10:05", "0.8 s", "12 entries, none changed."], ["today 09:50", "1.1 s", "12 entries, 1 changed."]],
    },
  ];

  /* A source to add when both of the above are already there. */
  const EXTRA = {
    pleasant: { id: "dr", name: "DR passwords", address: "https://passwords-dr.tidewater.internal:10001", user: "mara.lindqvist", totpFormat: "keeotp", folders: [] },
    bitwarden: { id: "personal", name: "Personal vault", address: "", user: "mara@lindqvist.example", folders: [] },
  };

  const E = (o) => ({ user: "", url: "", notes: "", fields: [], files: [], totp: false, expires: null, approver: null, readonly: false, log: [], earlier: [], by: "", ...o });
  const T = ["Tidewater"];
  const agents = [1, 2, 3, 4, 5, 6, 7].map((n) => E({
    id: `pl-agent-${n}`, src: "team", path: [...T, "Platform", "CI", "Agents"], title: `build-agent-0${n} root`, user: "root",
    url: `ssh://10.20.8.1${n}`, pw: `bA0${n}!rt-${["Kq7x", "Wm2p", "Zd9c", "Hv4s", "Lp6n", "Xr3j", "Tg8w"][n - 1]}#92`, changed: n < 5 ? "2026-09-08" : "2026-10-14", by: "Tomasz Wójcik",
  }));

  /* `src` and `path` are the source's own; the page shows a path through `shownPath` and never the source's id.
     Pleasant's folders all sit under one root, `Tidewater`, which the page skips. */
  const ENTRIES = [
    E({ id: "pl-payout-db", src: "team", path: [...T, "Payments", "Prod", "Databases"], title: "payout-service database user", user: "payout_app",
      url: "postgres://vm-db-01.tidewater.internal:5432/payments", pw: "Tq8#mVx2-Lr7!pZeW", changed: "2026-08-30", by: "Priya Nair",
      notes: "Read and write on payments.*, no DDL.\nThe service reads it from PAYOUT_DB_PASSWORD.",
      fields: [["Secret path", "kv/payments/prod/payout-db"]],
      log: [["2026-11-12 08:41", "Tomasz Wójcik", "copied the password", "OPS-77 rotation check"], ["2026-10-03 14:20", "Jonas Becker", "copied the password", "PAY-219"], ["2026-08-30 16:02", "Priya Nair", "changed the password", ""]] }),
    E({ id: "pl-ledger-db", src: "team", path: [...T, "Payments", "Prod", "Databases"], title: "ledger-api database user", user: "ledger_app",
      url: "postgres://vm-db-01.tidewater.internal:5432/ledger", pw: "Lg3!rA-pi9Qv#x2M", changed: "2026-06-11", by: "Priya Nair" }),
    E({ id: "pl-pg-prod", src: "team", path: [...T, "Payments", "Prod", "Databases"], title: "pg-payments-prod superuser", user: "postgres",
      url: "postgres://vm-db-01.tidewater.internal:5432", pw: "pG-prod!Sup3r#Wk81", changed: "2026-02-03", by: "Tomasz Wójcik", expires: "2026-11-26",
      notes: "Break glass only. Use the service users for everything else." }),
    E({ id: "pl-ledger-key", src: "team", path: [...T, "Payments", "Prod", "Keys"], title: "ledger-api signing key", pw: "corvid-Ankle-71-fennel", changed: "2026-08-14", by: "Priya Nair",
      notes: "The password is the PEM's passphrase.",
      fields: [["Key id", "led-2026-08"], ["Algorithm", "Ed25519"], ["Recovery PIN", "4471-9026", true]],
      files: [{ name: "ledger-signing.pem", size: "3.2 KB", kind: "PEM" }],
      log: [["2026-08-14 11:20", "Priya Nair", "added the entry", ""]] }),
    E({ id: "pl-webhook", src: "team", path: [...T, "Payments", "Prod", "Keys"], title: "PSP webhook signing secret", pw: "whsec_T1dewaterQ2vW8nR4kX0", url: "https://dashboard.psp.example/webhooks", changed: "2026-09-22", by: "Mara Lindqvist" }),
    E({ id: "pl-psp-live", src: "team", path: [...T, "Payments", "Prod", "PSP"], title: "PSP live secret key", pw: "sk_live_51Hx9TidewaterQ2vW8", url: "https://dashboard.psp.example", changed: "2026-08-18", by: "Mara Lindqvist", approver: "Priya Nair",
      notes: "Moves money. Use the sandbox key for anything you can." }),
    E({ id: "pl-psp-dash", src: "team", path: [...T, "Payments", "Prod", "PSP"], title: "PSP dashboard, finance sign-in", user: "finance@tidewater.example", pw: "Fin-dash!72xKe#Lo", url: "https://dashboard.psp.example", totp: true, changed: "2026-07-02", by: "Lena Hoffmann" }),
    E({ id: "pl-pg-stage", src: "team", path: [...T, "Payments", "Stage", "Databases"], title: "pg-payments-stage superuser", user: "postgres", pw: "stage-Wl9!kd2Q", url: "postgres://vm-pay-stage-01:5432/payments", changed: "2026-10-02", by: "Tomasz Wójcik" }),
    E({ id: "pl-payout-stage", src: "team", path: [...T, "Payments", "Stage", "Databases"], title: "payout-service stage database user", user: "payout_app", pw: "Ry4-stage-Hq88", url: "postgres://vm-pay-stage-01:5432/payments", changed: "2026-10-02", by: "Tomasz Wójcik" }),
    E({ id: "pl-psp-sandbox", src: "team", path: [...T, "Payments", "Stage", "PSP"], title: "PSP sandbox key", pw: "sk_test_4eC39HqLyjWDarjtT1zd", url: "https://sandbox.psp.example", changed: "2026-05-19", by: "Mara Lindqvist" }),
    E({ id: "pl-teamcity", src: "team", path: [...T, "Platform", "CI", "Service accounts"], title: "TeamCity service account", user: "svc-knobas", pw: "Jb7!vQ3x-Nc5$Ke1", url: "https://ci.tidewater.internal", changed: "2026-11-17", by: "Tomasz Wójcik" }),
    E({ id: "pl-deploy-bot", src: "team", path: [...T, "Platform", "CI", "Service accounts"], title: "Gitea deploy bot", user: "deploy-bot", pw: "gta_d3pl0y-9f2c7a11be", url: "https://git.tidewater.internal", changed: "2026-09-08", by: "Tomasz Wójcik" }),
    ...agents,
    E({ id: "pl-kuma", src: "team", path: [...T, "Platform", "Monitoring"], title: "Uptime Kuma admin", user: "admin", pw: "kum4-Upt!me9", url: "https://status.tidewater.internal", changed: "2026-10-06", by: "Tomasz Wójcik" }),
    E({ id: "pl-grafana", src: "team", path: [...T, "Platform", "Monitoring"], title: "Grafana admin", user: "admin", pw: "gr4f-Ana!77Xp", url: "https://grafana.tidewater.internal", totp: true, changed: "2026-10-28", by: "Tomasz Wójcik" }),
    E({ id: "pl-ops-mail", src: "team", path: [...T, "Platform", "Mail"], title: "ops@ shared mailbox", user: "ops@tidewater.example", pw: "0ps-Mailbox!4qRt", url: "https://mail.tidewater.example", totp: true, changed: "2026-07-21", by: "Priya Nair" }),
    E({ id: "pl-smtp", src: "team", path: [...T, "Platform", "Mail"], title: "SMTP relay", user: "relay@tidewater.example", pw: "smtp-Re1ay#58vN", url: "smtp://mail.tidewater.example:587", changed: "2026-03-30", by: "Priya Nair" }),
    E({ id: "pl-edge", src: "team", path: [...T, "Platform", "Network", "Edge"], title: "vm-edge-01 root", user: "root", pw: "Edg3!r00t-Vn5w", url: "ssh://10.20.1.5", changed: "2026-08-22", by: "Tomasz Wójcik" }),
    E({ id: "pl-traefik", src: "team", path: [...T, "Platform", "Network", "Edge"], title: "Traefik dashboard", user: "admin", pw: "tr43fik-Dash#1", url: "https://edge.tidewater.internal:8443", changed: "2026-08-22", by: "Tomasz Wójcik" }),
    E({ id: "pl-wg", src: "team", path: [...T, "Platform", "Network", "VPN"], title: "WireGuard hub", pw: "wg-hub-Pr1v!Key-pass", url: "wg://vpn.tidewater.example:51820", changed: "2026-04-15", by: "Tomasz Wójcik",
      files: [{ name: "wg-hub.conf", size: "612 B", kind: "CONF" }] }),
    E({ id: "pl-fr-prod", src: "team", path: [...T, "Integrations", "Flowrun", "Prod"], title: "Flowrun admin (prod)", user: "admin", pw: "Fl0w-prod!x8Rv", url: "https://flows.tidewater.internal", changed: "2026-05-20", by: "Jonas Becker", expires: "2026-12-04" }),
    E({ id: "pl-fr-stage", src: "team", path: [...T, "Integrations", "Flowrun", "Stage"], title: "Flowrun admin (stage)", user: "admin", pw: "Fl0w-stage!k2Pn", url: "https://flows-stage.tidewater.internal", changed: "2026-09-11", by: "Jonas Becker" }),
    E({ id: "pl-ebics", src: "team", path: [...T, "Integrations", "Bank", "SEPA gateway"], title: "EBICS user", user: "TIDEWTR1", pw: "Eb1cs!Tw-2026#qL", url: "https://ebics.bank.example", changed: "2026-01-12", by: "Lena Hoffmann",
      files: [{ name: "ebics-keys.zip", size: "18 KB", kind: "ZIP" }] }),
    E({ id: "pl-sepa-portal", src: "team", path: [...T, "Integrations", "Bank", "SEPA gateway"], title: "SEPA gateway portal", user: "treasury@tidewater.example", pw: "S3pa-Port!al77", url: "https://portal.bank.example", totp: true, changed: "2026-06-30", by: "Lena Hoffmann" }),
    E({ id: "pl-alarm", src: "team", path: [...T, "Office", "Building"], title: "Alarm panel", pw: "4-7-1-9-0-2", changed: "2025-12-01", by: "Lena Hoffmann", notes: "Front door panel. Changes with the facility contract." }),
    E({ id: "pl-wifi", src: "team", path: [...T, "Office", "Building"], title: "Guest Wi-Fi", user: "tidewater-guest", pw: "harbour-lights-2026", changed: "2026-11-02", by: "Lena Hoffmann" }),

    /* Bitwarden: the reader's own folders (nested by naming them Parent/Child) and the organization's collections. */
    E({ id: "bw-atl", src: "org", path: ["Personal", "Accounts"], title: "Atlassian account", user: "mara.lindqvist@tidewater.example", pw: "harbour-Pylon-52-vex", url: "https://tidewater.atlassian.net", totp: true, changed: "2026-03-09", earlier: ["2025-09-14", "2025-03-02"] }),
    E({ id: "bw-slack", src: "org", path: ["Personal", "Accounts"], title: "Slack", user: "mara.lindqvist@tidewater.example", pw: "Sl4ck-quay!Mn08", url: "https://tidewater.slack.com", changed: "2026-01-20" }),
    E({ id: "bw-gitea", src: "org", path: ["Personal", "Tokens"], title: "Gitea token for knobas", user: "mara.lindqvist", pw: "gta_8f3c1e9b04d27a61c5", url: "https://git.tidewater.internal", changed: "2026-09-02" }),
    E({ id: "bw-jira", src: "org", path: ["Personal", "Tokens"], title: "Jira and Confluence API token", user: "mara.lindqvist@tidewater.example", pw: "ATATT3xFfGF0Tdw9mZkQ7", changed: "2025-11-24",
      notes: "Atlassian tokens last a year. Make the new one before this one runs out." }),
    E({ id: "bw-npm", src: "org", path: ["Personal", "Tokens"], title: "npm publish token", user: "mara-tidewater", pw: "npm_Q2x8TideW4terLk91vZ", url: "https://www.npmjs.com", changed: "2026-04-18" }),
    E({ id: "bw-router", src: "org", path: ["Personal", "Home", "Network"], title: "Home router", user: "admin", pw: "r0uter-Kitchen!55", url: "http://192.168.1.1", changed: "2024-08-03" }),
    E({ id: "bw-hetzner", src: "org", path: ["Platform", "Cloud"], title: "Hetzner Cloud console", user: "ops@tidewater.example", pw: "Hz!9qL-2wPe7#Ty", url: "https://console.hetzner.cloud", totp: true, changed: "2026-06-15",
      files: [{ name: "recovery-codes.txt", size: "420 B", kind: "TXT" }], earlier: ["2025-12-01"] }),
    E({ id: "bw-dns", src: "org", path: ["Platform", "Cloud"], title: "DNS registrar", user: "ops@tidewater.example", pw: "Dn5-reg!Str4r#09", url: "https://registrar.example", totp: true, changed: "2026-02-27" }),
    E({ id: "bw-ghcr", src: "org", path: ["Platform", "Registries"], title: "GHCR pull token", user: "tidewater-bot", pw: "ghp_R2d9TideW8xQ1mN4", url: "https://ghcr.io", changed: "2026-07-01", readonly: true,
      notes: "docker login ghcr.io on every payments host." }),
    E({ id: "bw-sentry", src: "org", path: ["Payments", "Observability"], title: "Sentry", user: "payments@tidewater.example", pw: "S3ntry-pay!Lm42", url: "https://sentry.tidewater.example", changed: "2026-05-06" }),
    E({ id: "bw-grafana", src: "org", path: ["Payments", "Observability"], title: "Grafana viewer", user: "viewer", pw: "gr4f-View!19", url: "https://grafana.tidewater.internal", changed: "2026-10-28" }),
    E({ id: "bw-figma", src: "org", path: ["Design"], title: "Figma team admin", user: "design@tidewater.example", pw: "F1gma-Team!adm88", url: "https://www.figma.com", totp: true, changed: "2026-03-17" }),

    /* What an added source brings, when both of the above are already there. */
    E({ id: "dr-db", src: "dr", path: ["DR", "Payments"], title: "payout-service database user (DR)", user: "payout_app", pw: "Dr-Tq8#mVx2-Lr7", url: "postgres://vm-db-dr-01:5432/payments", changed: "2026-08-30" }),
    E({ id: "dr-edge", src: "dr", path: ["DR", "Platform"], title: "vm-edge-dr-01 root", user: "root", pw: "Dr-Edg3!r00t", url: "ssh://10.30.1.5", changed: "2026-08-22" }),
    E({ id: "dr-portal", src: "dr", path: ["DR", "Platform"], title: "DR site portal", user: "ops@tidewater.example", pw: "Dr-p0rtal!Xq4", url: "https://dr.tidewater.example", totp: true, changed: "2026-06-01" }),
    E({ id: "pv-bank", src: "personal", path: ["Banking"], title: "Bank", user: "mara.lindqvist", pw: "b4nk-Lind!qv1st", url: "https://bank.example", totp: true, changed: "2026-01-09" }),
    E({ id: "pv-mail", src: "personal", path: ["Mail"], title: "Private mail", user: "mara@lindqvist.example", pw: "m4il-Priv!ate22", url: "https://mail.lindqvist.example", changed: "2025-10-30" }),
    E({ id: "pv-nas", src: "personal", path: ["Home", "Network"], title: "NAS admin", user: "admin", pw: "n4s-Home!adm9", url: "http://192.168.1.20:5000", changed: "2025-07-14" }),
  ];

  /* What an attachment holds, for the viewer. Made up, and marked so. */
  const FILES = {
    "ledger-signing.pem": "-----BEGIN ENCRYPTED PRIVATE KEY-----\nMIGbMFcGCSqGSIb3DQEFDTBKMCkGCSqGSIb3DQEFDDAcBAhNb3RBUmVhbEtleQIC\nCAAwDAYIKoZIhvcNAgkFADAdBglghkgBZQMEASoEEE1vY2t1cE9ubHlOb3RSZWFs\nBEBUaGlzSXNBTW9ja3VwTm90QVJlYWxLZXlUaWRld2F0ZXJMZWRnZXJTaWduaW5n\nS2V5MjAyNjA4\n-----END ENCRYPTED PRIVATE KEY-----\n",
    "wg-hub.conf": "[Interface]\nAddress = 10.99.0.1/24\nListenPort = 51820\nPrivateKey = bW9ja3VwLW9ubHktbm90LWEtcmVhbC1rZXktMDAwMDA=\n\n[Peer]\n# vm-edge-01\nPublicKey = bW9ja3VwLXBlZXItcHVibGljLWtleS0wMDAwMDAwMDA=\nAllowedIPs = 10.99.0.2/32\n",
    "recovery-codes.txt": "Hetzner Cloud recovery codes, each works once\n\n4f2k-91qx   p0d3-mm7a\nz81c-ka24   0qwe-77lv\nn3r8-xy5t   c6b1-ud90\n",
  };

  return { CAPS, ADAPTERS, TOTP_FORMATS, SOURCES, EXTRA, ENTRIES, FILES };
})();
