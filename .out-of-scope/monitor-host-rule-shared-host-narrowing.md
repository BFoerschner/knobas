# Narrowing `monitor_url_host` for a host several assets share

The `monitor_url_host` suggestion rule compares hosts and never ports. A
monitor watching a host that several assets share is proposed to every one of
them, and knobas does not narrow that.

## Why this is out of scope

The rule reads one fact — *knobas knows this asset by that name* — from two
places an asset states it: its `hostname` property and the host of a route.
Neither side carries a port it could compare on. A hostname property has no
port at all, and #451's first criterion requires the monitor side to drop its
port so that `http://gitea:3000` matches an asset whose hostname is `gitea`.
Comparing ports on the route arm alone would make one rule fail two ways:
the hostname arm would stay wide while the route arm narrowed, and a reader
could not predict from the reason which behaviour they were looking at.

The cost is real and named in the rule's own docstring. Eight routes in
`testenv/hetzner/estate.json` carry `127.0.0.1` and land on seven different
assets. A Kuma running on the notebook rather than in a container would watch
`http://127.0.0.1:8111/` and be proposed to all seven, six of them wrong.

Three things make that survivable rather than worth a mechanism:

- Nothing fires on it today. The seeded Kuma runs in a container and reaches
  the notebook as `host.docker.internal`, which no route states.
- A proposal is dismissible and a dismissal is remembered, so a wrong
  proposal costs one click once and never returns.
- The suggestion engine's suppression already drops any pair that is a link,
  and the estate's monitors are attached by the names kept on the assets, so
  the right pair is drawn before the rule can propose the wrong ones.

The reconsideration trigger is a real Kuma that reaches the notebook by
`127.0.0.1`, or an estate in which a loopback route is what a monitor watches.
Then delete this file and decide the port question with a negative control
for a shared host, which does not exist today.

## Prior requests

- #479: "monitor_url_host reads only a monitor's URL, so the estate's ping checks and its shared hosts are out of reach" (part 2 of three; part 1 proceeded as a widening of the rule)
