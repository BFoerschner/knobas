<!--
  A password source's *Passwords* section on its Settings page (ADR-0056): the eight abilities the role allows beyond
  its minimum, each *Yes* with a line of detail or *Not offered*, from the adapter's `passwords` declaration and counts
  over the mirror; and, for a source whose entries name a code-key format, where the keys were found: the four KeePass
  plugins' fields, each with how many entries had it, and the keys it could not read.
-->
<script lang="ts">
  import type { Offers, SourceFacts } from "../../ipc/passwords";
  import { passwordSourceFacts } from "../../ipc/passwords";
  import { keychainName } from "../../shell/secret-store";

  let {
    sourceId,
    name,
    account,
    offers,
    synced,
    facts: read = passwordSourceFacts,
  }: {
    sourceId: string;
    name: string;
    account: string | null;
    offers: Offers;
    synced: boolean;
    facts?: (sourceId: string) => Promise<SourceFacts>;
  } = $props();

  let facts = $state<SourceFacts | null>(null);
  $effect(() => {
    const id = sourceId;
    void synced;
    let live = true;
    void read(id)
      .then((answer) => {
        if (live) facts = answer;
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  });

  const plural = (n: number, one: string, many: string) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  const codes = $derived((facts?.formats ?? []).filter(([format]) => format !== "unreadable").reduce((sum, [, n]) => sum + n, 0));
  const unreadable = $derived((facts?.formats ?? []).find(([format]) => format === "unreadable")?.[1] ?? 0);
  const countOf = (format: string) => (facts?.formats ?? []).find(([f]) => f === format)?.[1] ?? 0;

  const rows = $derived<[string, boolean, string][]>([
    ["Add and edit entries, change passwords", offers.writes.length > 0, "A write is sent at once. A password never waits in the write queue."],
    [
      "One-time codes",
      offers.one_time_codes,
      facts ? `${plural(codes, "entry has", "entries have")} a code. The keys are kept in ${keychainName} and never reach the mirror, so codes work while it is down.` : "",
    ],
    ["Attachments", offers.attachments, "Read when you open or save one."],
    [
      "History",
      offers.history !== null,
      offers.history === "log" ? "Who read and who changed an entry, read when you open it." : "An entry's earlier passwords, read when you open them.",
    ],
    ["Expiry dates", offers.expiry, ""],
    ["Usage comments, where the server requires them", offers.usage_comments, "Asked where the server says a folder or an entry needs one, and nowhere else."],
    ["An audit log of who read which password", offers.audit_log, `Every password read is recorded there${account ? ` under ${account}` : ""}. A sync reads no passwords.`],
    ["Access requests", offers.access_requests, "Asked for on the entry; knobas checks one only when you press Check now."],
  ]);

  /** Where a code's key is kept: the server's own field, then the four places a KeePass plugin keeps it, in the adapter's format ids. */
  const FORMATS: [string, string, string, string][] = [
    ["pleasant", "The server's own", "TOTPSecret", "The key in Base32, on the entry"],
    ["keepass", "KeePass 2.47+, built in", "TimeOtp-Secret-Base32", "The key in Base32"],
    ["tray_totp", "KeeTrayTOTP", "TOTP Seed, TOTP Settings", "Key and settings in two fields"],
    ["keeotp", "KeeOtp", "otp", "key=…&step=30&size=6"],
    ["otpauth", "KeePassXC style", "otp", "An otpauth:// link"],
  ];
</script>

<section class="tf-stg-sec">
  <h2>Passwords</h2>
  <p class="tf-stg-p">
    What {name} offers the Passwords page. A control is drawn there only where its source offers it. Entries sync into the mirror; a password is read only when you show or copy
    it.{#if facts && synced}{" "}It lists {plural(facts.entries, "entry", "entries")} in {plural(facts.folders, "folder", "folders")}.{/if}
  </p>
  {#if !offers.marks_protected_fields}
    <!-- Fable's ruling while Björn was away (2026-11-22): every value is mirrored, and the page says so. -->
    <p class="tf-stg-p tf-pw-fieldsnote">
      Notes and custom fields sync with the entry and are searched. This server sends every custom field's value with its listing and marks none as protected, so knobas
      cannot tell a PIN from a port number. Keep a value that must not rest on this machine in an entry's password.
    </p>
  {/if}
  {#each rows as [label, yes, detail] (label)}
    <div class="tf-stg-row">
      <div class="tf-stg-l">{label}</div>
      <div class="tf-stg-c tf-pw-capc">
        {#if yes}
          <span class="tf-pw-capy"><span class="tf-dot tf-ok"></span>Yes</span><span class="tf-pw-capd">{synced ? detail : ""}</span>
        {:else}
          <span class="tf-pw-capn">Not offered</span>
        {/if}
      </div>
    </div>
  {/each}
</section>

{#if offers.one_time_codes}
  <section class="tf-stg-sec">
    <h2>Where the one-time code keys are</h2>
    <p class="tf-stg-p">
      The server keeps a code's key on the entry itself, or a KeePass plugin keeps it in the entry's custom fields. The adapter reads every format below, the server's own
      first, moves the key to {keychainName} and takes those fields out of the entry before it reaches the mirror.{#if !synced}{" "}It says which one it found after the first sync.{/if}
    </p>
    <div class="tf-pw-tblwrap">
      <table class="tf-tbl tf-pw-formats">
        <thead><tr><th>Plugin</th><th>Field</th><th>Format</th><th>Found</th></tr></thead>
        <tbody>
          {#each FORMATS as [id, plugin, field, format] (id)}
            {@const n = countOf(id)}
            <tr class:tf-pw-found={n > 0}>
              <td>{plugin}</td>
              <td><span class="tf-mono">{field}</span></td>
              <td>{format}</td>
              <td>{#if n > 0}<span class="tf-chip tf-ok">{plural(n, "entry", "entries")}</span>{:else if synced}<span class="tf-dim">none</span>{/if}</td>
            </tr>
          {/each}
          {#if unreadable > 0}
            <tr>
              <td colspan="3">Keys in a form knobas cannot read (Steam codes, HOTP, a key that is not Base32)</td>
              <td><span class="tf-chip tf-warn">{plural(unreadable, "entry", "entries")}</span></td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>
  </section>
{/if}
