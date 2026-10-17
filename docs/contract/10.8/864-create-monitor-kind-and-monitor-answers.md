- **`crates/knobas-source/src/**` and the IPC schema — issue #864 (2026-10-16): `CreateMonitor`
  gains a kind, and the Monitors reads carry what the new shell's two tabs draw.** Spec #808
  stories 153 and 154, milestone TF4, requirements R-11.3 and R-11.4. No migration, no event, no
  grammar key, no new command, no new module in `commands/` or `ipc/`.

  **`WriteOp::CreateMonitor` gains `kind: MonitorKind`** (`http | tcp | ping | docker`, the
  lowercase word on the wire, `#[serde(default)]` to `http`). The #453 entry left this open in
  so many words: "the day knobas offers a ping or a TCP check is the day the op grows the field
  that says which". R-11.4 is that day: *Create monitor* is prefilled by asset type, HTTP for a
  route or a service, TCP for a database, Docker for a container, Ping for a VM. The field `url`
  keeps its name and now holds what the check watches, spelled as the kind reads it: an http(s)
  URL, `host:port`, a host, or a container's name. Keeping the name is what lets a create queued
  before this change still decode, as the HTTP check it was
  (`a_create_queued_before_kinds_reads_as_an_http_check`). The kind is in knobas' words, not
  Kuma's: a TCP check is Kuma's `port`. A closed set, so a payload naming a check no adapter
  makes fails at `submit_write`. The TypeScript mirror is `MonitorKind` and an optional `kind` on
  `WriteOpPayload.CreateMonitor`.

  **The Uptime Kuma adapter** sends `type: port` with `hostname` and `port`, `type: ping` with
  `hostname`, and `type: docker` with `docker_container` and `docker_host`; the HTTP document is
  the seed's own, unchanged. A Docker check needs one of the account's Docker hosts, and Kuma
  says which exist only by pushing `dockerHostList` during the login (2.5.3's `afterLogin`
  awaits it before the login's ack). The socket reads that push and the create takes the lowest
  id; an account with none is refused before `add` is sent, since a Docker monitor with no host
  is created and then down for ever. Witnessed live by
  `a_tcp_check_is_published_as_a_port_monitor` (`just kuma-live`): a TCP check published as
  Kuma's `port`. The ping is witnessed at the unit seam only, to keep the
  suite inside Kuma's twenty logins a minute. The direction "a Docker host is chosen" is
  witnessed live since #978: the seed gives the test Kuma a Docker host (a read-only socket
  proxy), and `a_docker_check_is_published_with_the_seeded_docker_host` creates a Docker check
  of a testenv container and sees it published as `docker`. "No Docker host is refused" is now
  the unit seam's (`create::tests`), since the test Kuma has one.

  **`assets::MonitoredAsset` gains `type_label: String`**, the asset type's label resolved by the
  backend from the one type table, for the *Watches* column ("knobas-jira Container").
  **`assets::UnmonitoredAsset` gains `environment: Option<String>` and `owner:
  Option<String>`**, both in force (#431's inheritance, the same recursive walk
  `dependencies::ESTATE` makes), for the *Env* and *Owner* columns of *Assets nothing watches*.
  Additive: no field changes meaning and every consumer decodes unchanged. Pinned by the mirror
  tests in `commands/assets.rs` and by
  `the_roster_carries_the_environment_and_owner_in_force` and
  `a_monitor_lists_the_assets_it_watches_with_their_path` in `tests/it/assets_ipc.rs`.
