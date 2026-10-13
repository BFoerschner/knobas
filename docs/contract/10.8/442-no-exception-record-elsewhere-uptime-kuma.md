- **No exception, and the record is elsewhere** — issue #442 (2026-09-07), the Uptime Kuma
  adapter's read half. Listed here because #442's own acceptance criteria ask for "a §10.8 entry
  for the descriptor and the `monitor` kind", and the honest answer is that none is owed: the
  adapter is a new crate, plus the registry row and manifest line §3a licenses, and it touches no
  migration, no IPC schema or layout, neither barrel, nothing in `crates/knobas-source/src/**` and
  nothing in `crates/knobas-http/**` — exactly #284's Confluence shape. The descriptor, the
  `monitor` kind, the single `state` declaration, the digest cursor, the adapter-side tombstone and
  three §4.1 departures are recorded in **§9's *Amendments from the Uptime Kuma adapter, read
  half*** above, with the reference row in **§4.2 E**, together
  with a measured fact that bears on spec #427's story 38. A reader sent here by that criterion
  should read that section; a reader looking for a *changed frozen surface* will not find one,
  which is the point of this line. The Kuma **write** ops and the two-secret keychain envelope are
  a different ticket and will owe a real entry (ADR-0006, spec #427 story 81). **That ticket is
  #452 and its entry is the next one below.**

