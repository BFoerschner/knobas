/*
  The model under the Passwords clickdummy. Password managers are sources like any other: they are added,
  signed in and removed in Settings › Sources, and an adapter implements the password role against one
  interface (list entries, read a secret, and the optional writes). The Passwords page draws that role and
  never says which source an entry came from.
  Nothing leaves the page; a reload starts over. Today is Tuesday 2026-11-17, 10:24. Mara Lindqvist is on PAY-231.
*/
"use strict";
const PW = (() => {
  const NOW = new Date("2026-11-17T10:24:00");
  const DAY = 864e5;

  /* The password role. `list` and `reveal` are its minimum; every other line is optional and declared per adapter.
     The Passwords page asks only these, never the adapter's name. */
  const CAPS = [
    ["list", "List entries and their folders"],
    ["reveal", "Read a password when asked"],
    ["write", "Add, edit, move and change passwords"],
    ["totp", "One-time codes"],
    ["attachments", "Attachments"],
    ["history", "Earlier passwords and changes"],
    ["expiry", "Expiry dates"],
    ["reason", "Usage comments, where the server requires them"],
    ["audit", "Records who read which password"],
    ["requests", "Access requests"],
  ];

  /* Every adapter "Add a source" offers. The password ones fill the password role; the rest are here so the list looks like the app's. */
  const ADAPTERS = {
    jira: { name: "Jira", holds: "Tickets, saved views" },
    confluence: { name: "Confluence", holds: "Pages" },
    gitea: { name: "Gitea", holds: "Repositories, pull requests" },
    teamcity: { name: "TeamCity", holds: "Builds" },
    kuma: { name: "Uptime Kuma", holds: "Monitors" },
    keepass: {
      name: "KeePass file", holds: "Passwords", role: "passwords",
      caps: { list: 1, reveal: 1, write: 1, totp: 1, attachments: 1, history: 1, expiry: 1 },
      address: { l: "File", ph: "~/Sync/ops/platform.kdbx" },
      fields: [{ k: "keyfile", l: "Key file", ph: "Optional" }],
      secret: "Master password", secretNote: "Kept in the macOS keychain with the key file's path, like every source's credential. The adapter opens the file with it at every sync and every read.",
      credential: "Master password and key file path, in the macOS keychain. The adapter opens the file with them at every sync and every read. The keys behind one-time codes are kept there too, so codes work offline.",
      report: "Opened with the master password: KDBX 4, 6 entries in 2 groups",
      sample: { name: "Home lab", address: "~/Sync/home-lab.kdbx", folders: ["Home lab/Network", "Home lab/Servers"] },
    },
    pleasant: {
      name: "Pleasant Password Server", holds: "Passwords", role: "passwords",
      caps: { list: 1, reveal: 1, write: 1, totp: 1, attachments: 1, history: 1, expiry: 1, reason: 1, audit: 1, requests: 1 },
      address: { l: "Address", ph: "https://vault.tidewater.internal" },
      fields: [{ k: "user", l: "Username", ph: "mara.lindqvist" }],
      secret: "Password", secretNote: "Kept in the macOS keychain, like every source's credential.",
      credential: "Password, in the macOS keychain. The keys behind one-time codes are kept there too: the TOTP plugin's fields come with each entry, and the adapter moves them to the keychain instead of the mirror.",
      report: "Signed in as mara.lindqvist (Pleasant 8.2). 4 entries in 2 folders you can read",
      sample: { name: "DR passwords", address: "vault-dr.tidewater.internal", folders: ["DR/Payments", "DR/Platform"] },
    },
    bitwarden: {
      name: "Bitwarden or Vaultwarden", holds: "Passwords", role: "passwords",
      caps: { list: 1, reveal: 1, write: 1, totp: 1, attachments: 1, history: 1 },
      address: { l: "Address", ph: "https://vault.tidewater.example (empty for bitwarden.com)" },
      fields: [{ k: "cid", l: "API key client_id", ph: "user.4f1c…" }, { k: "csec", l: "API key client_secret", ph: "", type: "password" }],
      secret: "Master password", secretNote: "Kept in the macOS keychain with the API key. The adapter decrypts the entries on this machine with it at every sync and every read.",
      credential: "API key and master password, in the macOS keychain. The adapter signs in with the key and decrypts with the master password at every sync and every read. The keys behind one-time codes are kept there too, so codes work offline.",
      report: "Signed in as mara@tidewater.example (Vaultwarden 1.32). Your own entries and 1 organization",
      sample: { name: "Personal", address: "bitwarden.eu", folders: ["Personal/Home", "Personal/Banking"] },
    },
  };


  const d = (s) => new Date(s + "T12:00:00");
  const E = (o) => ({ user: "", url: "", notes: "", fields: [], files: [], history: [], totp: false, expires: null, readonly: false, ...o, changed: d(o.changed) });

  /* `src` says which source holds the entry. Only Settings › Sources and the write path read it. */
  const ENTRIES = [
    E({ id: "pl-payout-db", src: "team", folder: "Tidewater/Payments/Prod/Databases", title: "payout-service database user", user: "payout_app", pw: "Tq8#mVx2-Lr7!pZe", url: "postgres://vm-db-01:5432/payments", changed: "2026-08-30",
      notes: "Read and write on payments.*; no DDL. The service reads it from PAYOUT_DB_PASSWORD.",
      history: [["2026-11-12 08:41", "Tomasz Wójcik", "copied the password"], ["2026-08-30 16:02", "Priya Nair", "changed the password"]] }),
    E({ id: "pl-ledger-key", src: "team", folder: "Tidewater/Payments/Prod/Keys", title: "ledger-api signing key", pw: "corvid-Ankle-71-fennel", changed: "2026-08-14",
      fields: [["Key id", "led-2026-08"], ["Algorithm", "Ed25519"]], files: [["ledger-signing.pem", "3.2 KB"]],
      notes: "The password is the PEM's passphrase.", history: [["2026-08-14 11:20", "Priya Nair", "added the entry"]] }),
    E({ id: "pl-stripe", src: "team", folder: "Tidewater/Payments/Prod/PSP", title: "PSP live secret key", pw: "sk_live_51Hx9TidewaterQ2vW8", changed: "2026-08-18", approval: "Priya Nair",
      history: [["2026-08-18 15:30", "Mara Lindqvist", "changed the password"]] }),
    E({ id: "pl-pg-stage", src: "team", folder: "Tidewater/Payments/Stage/Databases", title: "pg-payments-stage superuser", user: "postgres", pw: "stage-Wl9!kd2Q", url: "postgres://vm-pay-stage-01:5432/payments", changed: "2026-10-02" }),
    E({ id: "pl-payout-stage", src: "team", folder: "Tidewater/Payments/Stage/Databases", title: "payout-service stage database user", user: "payout_app", pw: "Ry4-stage-Hq88", url: "postgres://vm-pay-stage-01:5432/payments", changed: "2026-10-02" }),
    E({ id: "pl-teamcity", src: "team", folder: "Tidewater/Platform/CI/Service accounts", title: "TeamCity service account", user: "svc-knobas", pw: "Jb7!vQ3x-Nc5$Ke1", url: "https://ci.tidewater.internal", changed: "2026-11-17",
      history: [["2026-11-17 09:12", "Tomasz Wójcik", "changed the password"], ["2026-08-17 09:02", "Tomasz Wójcik", "changed the password"]] }),
    E({ id: "pl-fr-prod", src: "team", folder: "Tidewater/Integrations/Flowrun/Prod", title: "Flowrun admin (prod)", user: "admin", pw: "Fl0w-prod!x8Rv", url: "https://flows.tidewater.internal", changed: "2026-05-20", expires: d("2026-11-20") }),
    E({ id: "pl-fr-stage", src: "team", folder: "Tidewater/Integrations/Flowrun/Stage", title: "Flowrun admin (stage)", user: "admin", pw: "Fl0w-stage!k2Pn", url: "https://flows-stage.tidewater.internal", changed: "2026-09-11" }),
    E({ id: "pl-kuma", src: "team", folder: "Tidewater/Platform/Monitoring", title: "Uptime Kuma admin", user: "admin", pw: "kum4-Upt!me9", url: "https://status.tidewater.internal", changed: "2026-10-06" }),
    E({ id: "pl-ops-mail", src: "team", folder: "Tidewater/Platform/Mail", title: "ops@ shared mailbox", user: "ops@tidewater.example", pw: "0ps-Mailbox!4q", url: "https://mail.tidewater.example", totp: true, changed: "2026-07-21" }),

    E({ id: "bw-gitea", src: "org", folder: "My entries", title: "Gitea token for knobas", user: "mara.lindqvist", pw: "gta_8f3c1e9b04d27a61c5", url: "https://git.tidewater.internal", changed: "2026-09-02" }),
    E({ id: "bw-atl", src: "org", folder: "My entries", title: "Atlassian account", user: "mara.lindqvist@tidewater.example", pw: "harbour-Pylon-52-vex", url: "https://tidewater.atlassian.net", totp: true, changed: "2026-03-09" }),
    E({ id: "bw-jira", src: "org", folder: "My entries", title: "Jira and Confluence API token", user: "mara.lindqvist@tidewater.example", pw: "ATATT3xFfGF0Tdw9mZ", changed: "2025-11-24", expires: d("2026-11-24"), expiresBy: "Tidewater Jira reports it for the token it signs in with.",
      notes: "Atlassian tokens last a year. Make the new one before this one runs out." }),
    E({ id: "bw-ghcr", src: "org", folder: "Platform/Registries", title: "GHCR pull token", user: "tidewater-bot", pw: "ghp_R2d9TideW8xQ1mN4", url: "https://ghcr.io", changed: "2026-07-01", readonly: true,
      notes: "docker login ghcr.io on every payments host." }),
    E({ id: "bw-hetzner", src: "org", folder: "Platform/Cloud", title: "Hetzner Cloud console", user: "ops@tidewater.example", pw: "Hz!9qL-2wPe7", url: "https://console.hetzner.cloud", totp: true, changed: "2026-06-15", readonly: true }),
    E({ id: "bw-grafana", src: "org", folder: "Payments/Observability", title: "Grafana admin", user: "admin", pw: "gr4f-Ana!77", url: "https://grafana.tidewater.internal", changed: "2026-10-28" }),

    E({ id: "kp-pay01", src: "servers", folder: "Servers/Prod", title: "vm-pay-01 root", user: "root", pw: "p4Y!01-Wq#Lm9z", url: "ssh://10.20.4.11", changed: "2026-04-28" }),
    E({ id: "kp-db01", src: "servers", folder: "Servers/Prod", title: "vm-db-01 root", user: "root", pw: "dB-01!xT8kQ#vr", url: "ssh://10.20.4.20", changed: "2026-09-19" }),
    E({ id: "kp-stage01", src: "servers", folder: "Servers/Stage", title: "vm-pay-stage-01 deploy", user: "deploy", pw: "st4ge-Dep!oy22", url: "ssh://10.20.5.11", changed: "2026-10-30" }),
    E({ id: "kp-edge", src: "servers", folder: "Servers/Shared", title: "vm-edge-01 root", user: "root", pw: "Edg3!r00t-Vn5w", url: "ssh://10.20.1.5", changed: "2026-08-22" }),
    E({ id: "kp-traefik", src: "servers", folder: "Servers/Shared", title: "Traefik dashboard", user: "admin", pw: "tr43fik-Dash#1", url: "http://vm-edge-01:8080", changed: "2026-08-22" }),
    E({ id: "kp-lowcode", src: "servers", folder: "Servers/Shared", title: "vm-lowcode-01 deploy", user: "deploy", pw: "LowC0de!dPl9", url: "ssh://10.20.6.20", changed: "2026-02-03" }),
    E({ id: "kp-pve02", src: "servers", folder: "Hypervisors/Proxmox", title: "Proxmox pve-02", user: "root@pam", pw: "pve-02!Hyp#Rr4", url: "https://pve-02.tidewater.internal:8006", totp: true, changed: "2026-09-01" }),
  ];

  const S = {
    /* Settings › Sources, in the app's order. State: ok, err or offline. */
    sources: [
      { id: "jira", adapter: "jira", name: "Tidewater Jira", address: "tidewater.atlassian.net", count: "1,284 tickets", state: "ok", line: "Synced 4 min ago" },
      { id: "confluence", adapter: "confluence", name: "Tidewater Wiki", address: "tidewater.atlassian.net/wiki", count: "3,912 pages", state: "ok", line: "Synced 4 min ago" },
      { id: "gitea", adapter: "gitea", name: "git.tidewater.internal", address: "git.tidewater.internal", count: "47 repositories", state: "ok", line: "Synced 2 min ago" },
      { id: "teamcity", adapter: "teamcity", name: "ci.tidewater.internal", address: "ci.tidewater.internal", count: "312 builds", state: "ok", line: "Synced 4 min ago" },
      { id: "kuma", adapter: "kuma", name: "status.tidewater.internal", address: "status.tidewater.internal", count: "18 monitors", state: "ok", line: "Synced 1 min ago" },
      { id: "team", adapter: "pleasant", name: "Team passwords", address: "vault.tidewater.internal", who: "mara.lindqvist", state: "ok", line: "Synced by hand at 10:22", listed: "10:22", reasonFolders: [], manual: true },
      { id: "org", adapter: "bitwarden", name: "Tidewater org", address: "vault.tidewater.example", who: "mara@tidewater.example", state: "ok", line: "Synced 4 min ago", listed: "10:20" },
      { id: "servers", adapter: "keepass", name: "Platform servers", address: "~/Sync/ops/platform.kdbx", state: "ok", line: "Synced 3 min ago", listed: "10:21" },
    ],
    entries: ENTRIES,
    granted: {},      /* entry id → the last usage comment, to fill in next time */
    requested: {},    /* entry id → "pending" | "approved" */
    job: { key: "PAY-231", title: "Retry failed SEPA payouts" },
  };

  const source = (id) => S.sources.find((s) => s.id === id);
  const adapterOf = (s) => ADAPTERS[s.adapter];
  const isPasswords = (s) => adapterOf(s).role === "passwords";
  const can = (e, cap) => !!adapterOf(source(e.src)).caps[cap];
  const srcOf = (e) => source(e.src);
  const visible = (e) => { const s = srcOf(e); return !!s; };
  const days = (a, b = NOW) => Math.round((b - a) / DAY);
  /* Folder settings inherit down the tree: a folder that requires a usage comment requires it for everything under it. */
  const under = (folder, f) => folder === f || folder.startsWith(f + "/");
  const reasonAt = (e) => (srcOf(e).reasonFolders || []).find((f) => under(e.folder, f));
  const needsReason = (e) => !!reasonAt(e);

  /* Why an entry wants attention, worst first; null when it does not. */
  function attention(e) {
    if (!visible(e)) return null;
    if (e.expires) {
      const n = days(NOW, e.expires);
      if (n <= 14) return { tone: n <= 3 ? "err" : "warn", text: n <= 0 ? "Expired" : `Expires in ${n} day${n === 1 ? "" : "s"}` };
    }
    const age = days(e.changed);
    if (age > 180) return { tone: "warn", text: `Unchanged for ${age} days` };
    return null;
  }

  return { NOW, DAY, CAPS, ADAPTERS, S, source, adapterOf, isPasswords, can, srcOf, visible, days, needsReason, reasonAt, under, attention };
})();
