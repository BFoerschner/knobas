# 2026-11-19: Assets has a nav entry again, and says what it is from empty

**Björn ruled this on 2026-11-19, for ux21.** He asked what the empty *Watches* section on Monitors was; told it lists
the assets in the asset tree a monitor is linked to, and that the tree had no nav entry, he said: "can you enable assets
again in ux21?" It binds as his ruling and overrides the sentence of
`docs/decisions/2026-11-05-cut-dependencies-and-proposed-links.md` (and the PR ruling that followed it) that left the
tree with no entry. Dependencies and Proposed links stay cut.

## The entry

**Assets, after Monitors**, as the top bar's seventh entry: Tickets, Inbox, Code, Builds, Docs, Monitors, Assets, Link
sets, Time. After Monitors because Monitors' *Watches* column and its *Assets nothing watches* tab are about what Assets
holds, so the two read as a pair, and Monitors is the one with a badge, used more. Named *Assets*, the word the brief and
`CONTEXT.md` (*Assets view*) use, and short like its neighbours; the page's title on the bare tree, its breadcrumb and
its `>` row say *Assets* too (they said *Asset tree*), so the entry and the page agree. Decided under
`/frontend-design:frontend-design`; no real judgment was left for a Fable agent.

- It lights on `#/assets`, `#/asset/<id>` and `#/route/<id>`, and comes back to where you were as every entry does
  (*A nav entry comes back to where you were*): from another section to the asset you left, pressed inside the section
  to `#/assets`. The nav-place press trip `nav-place-asset` holds it.
- The launcher's `>` menu lists it in NAV order. It was a *Go to* row of its own; now it is one of the menu's.
  `filterActions` ranks a name a word of which starts with the typed words above one that only holds them, so `>set`
  still leads with Link sets and Settings, not Assets.
- `Esc` on `#/assets` does nothing unless a visit opened it (the launcher, a card's row), as on Monitors: with an entry
  it is no longer a place only visited. An asset still goes back to what opened it, else to its holder.
- The crumb bar stays drawn on `#/assets`, where a section's own page has none, because drawn only under an asset it
  came with the first press in a column and moved the tree (F-054's reason).
- Not opened ahead (`ahead.ts`): its first open is within its lines without it (below).

## From empty

Björn has few or no assets, so the first thing he meets is the empty tree and the empty *Watches*.

- **The empty tree** says what an asset is and offers every way in: *No assets yet. Assets are what you run, each
  inside what holds it: machines, VMs, containers, services and databases. A monitor linked to one says what it
  watches.* Then *New asset…* and *Import an estate file…*, and *Or start from a monitor: open one on Monitors and press
  New asset for it.*
- **The open monitor's *Watches*** (it said *No asset. Link one from its asset's page.*, and the asset's page had no
  such control) lists what it watches, or *Nothing yet.*, then two actions, both always drawn so a link that lands
  moves neither:
  - *Link an asset…*: *Move to…*'s picker (`assets/LinkAssetDialog.svelte`). It walks the estate a level at a time,
    and where you have walked to is what *Link to <asset>* links, writing the one `monitored-by` link (`create_link`,
    from the asset to the monitor) an import or *Create monitor for this asset* draws through a name. At the top it
    waits (*Open the asset it watches.*); an asset the monitor already watches is marked and offers *Already linked*;
    an empty estate says so and points at *New asset…*, which stands at the footer's start.
  - *New asset for it…*: the tree's *New asset* dialog at the top of the estate, named after the monitor, its type
    the one the check's kind watches (`assetFor`: a container for a Docker check, a database for a database's, a VM for
    a ping, a service otherwise), subtitled *At the top of the estate, watched by <monitor>*. Created, it is linked,
    and the toast says *Added <asset> to Assets, watched by <monitor>* with *Open*.
  The backend needed nothing: `create_link` and `create_asset` were there.
- **The roster's *Watches* header** names its column only while some monitor watches something, so a roster with no
  asset linked shows no empty column heading.
- **The header line** said *Each one watches an asset.* whatever was linked; it now says that only when it is true,
  *N of them watch an asset.* when some do, and nothing when none do.
- ***Assets nothing watches*** with no asset at all said *Every container, service, route, database and VM has a
  monitor*. It now reads the estate's top when the list is empty and says *No assets yet, so nothing is left
  unwatched. Add what you run under Assets, or open a monitor and press New asset for it.*

## What the asset-tree screenshots found, fixed with it

The orchestrator's screenshot pass of the tree (`?fake-ipc`, 1440x868 and 5120x1440) found four things.

1. **The tree's find found nothing under `?fake-ipc`.** The fake's launcher search answered every plain word before it
   looked at the filters, and it holds no asset, so the Tree's asset-only search (`kinds: ["asset"]`) got an empty
   answer. The fake now answers an asset-only search from the estate first. The real backend was right all along:
   on the scale copy with the test estate imported, *kno*, *dock*, *forg*, *uptime* and *db* each answered assets only.
2. **knobas-jira said two things about its monitor.** The fake's roster had jira down watching knobas-jira, while its
   `get_asset` answered `monitoring: []` for every asset and rated an asset by its own status alone, so the pane said
   *Health None* and *Named by the import, not in Kuma yet: jira*, and no row carried an alert. The fake's asset rows
   and pane now read the roster: *Monitoring* lists the monitors watching the asset, and health, *inside* and the
   problem count take the attached monitors into account as `AssetRow::health` says (a paused one gives nothing). The
   real reads agreed already: on the scale copy all five watched monitors, jira down among them, matched their
   asset's *Monitoring* and health, with nothing left waiting.
3. **The route wire ran over the tree.** A wire is drawn only where nothing stands between its far end and the pane:
   a far end in a column with a column or a spine drawn after it is its landing dot alone (`across`), and the pane's
   row names the asset with a link, as it did. Since no line crosses a label any more, the wire layer paints over the
   columns and under the pane, and the dot sits just past the row's end, in the column's padding, off the chevron. At
   full screen the tree takes room for **eight columns beside the pane and no more**
   (`max-width: calc(8 * 216px + var(--side-assets))`), the real estate's six levels and two to open into, so at
   5,120 px the pane stands beside the columns rather than 3,000 px away at the window's edge; past the cap the window
   is page, and the pane closes its right edge with a rule.
4. **The open monitor's panel covered the end of *Watches*.** The roster keeps its columns when a monitor opens (the
   monitors-page pass: narrowing it drew every row again), so with a monitor open each row's *Watches* stops at the
   panel's edge, cut with an ellipsis, its names in the cell's title.

## Two costs found on the way, and taken out

- **The tree's find waited 90 ms for keys that were not coming.** It debounced every key, the first too, so its first
  letter drew in 145 ms at 1x against 100. It now asks the first key after the box settled at once, as the launcher
  does (`Session`); keys typed within the debounce of the last one or of a search sent, or while one is out, wait as
  before.
- **A long column scrolled on the main thread.** `.col` and `.cols` had no background, so Chromium scrolled them on
  the main thread and repainted them each frame (the design system's *A box that scrolls paints the colour it sits
  on*). They paint `--paper` now. And a windowed column of 300 rows swapped rows every 300 px of a scroll: at CPU 6x
  it scrolled at 52 fps. Drawn whole (`from: 1000`) it scrolled at 59.8, but took 340 to 506 ms to reach the screen at
  6x; with `overscan: 1800` it swaps rows every 900 px and holds both lines (below).

## Numbers

Headless Chrome on Björn's PC (the WSL box, Ryzen 5 5600X) under the PC queue, driving the app served from the Mac over
the windowless backend (release build) on a copy of the saved scale database, its secrets moved out, with
`testenv/estate.json` and a perf estate imported: a site, 8 hypervisors, 60 VMs on each, 300 containers on the first VM
and 4 on every other, 2,705 assets, made by this script:

```python
import json, sys
assets = [{"id": "asset:perf-site", "type": "site", "name": "perf estate", "environment": "prod", "owner": "Perf"}]
for h in range(1, 9):
    hv = f"asset:perf-hv{h:02}"
    assets.append({"id": hv, "type": "hypervisor", "name": f"hv-{h:02}", "parent": "asset:perf-site"})
    for v in range(1, 61):
        vm = f"asset:perf-hv{h:02}-vm{v:03}"
        assets.append({"id": vm, "type": "vm", "name": f"vm-{h:02}-{v:03}", "parent": hv})
        for c in range(1, (301 if (h, v) == (1, 1) else 5)):
            assets.append({"id": f"{vm}-c{c:03}", "type": "container", "name": f"svc-{h:02}{v:03}-{c:03}", "parent": vm})
json.dump({"version": 1, "name": "perf estate", "description": "Timing the asset tree.", "assets": assets, "routes": []}, open(sys.argv[1], "w"))
```

applied with `apply_estate_import` (`producer: "estate_file"`, the file's text as `file`). Every read crossed the ssh
tunnel to the Mac, whose load was 9 to 12 from other agents' builds. Lines: 100 / 200 / 300 ms a page and 100 / 100 /
200 ms a key at 1x / 4x / 6x; 55 frames a second scrolling at 4x and 6x.

| Case (`open.mjs`, usable ms at 1x / 4x / 6x) | 1440x868@1 | 2560x1440@1.25 | 5120x1440@1 |
|---|---|---|---|
| Assets by its entry, first open | 54 / 70 / 100 | 56 / 85 / 102 | 56 / 99 / 116 |
| Assets, return | 28 / 22 / 33 | 23 / 23 / 21 | 24 / 22 / 18 |
| An asset pressed (the perf root), first | 56 / 68 / 145 | 37 / 67 / 114 | 40 / 85 / 115 |
| An asset pressed, return | 73 / 72 / 120 | 56 / 104 / 87 | 74 / 89 / 105 |
| The VM of 300 containers, first (`asset-long`) | not run / 102 / 145 | 70 / 116 / 197 | not run / 133 / 174 |
| The VM of 300, return | not run / 137 / 187 | 90 / 170 / 203 | not run / 138 / 172 |

`asset-long` is at `overscan: 1800`, at 1x only at 2560. Nothing moved in any of them. Two cells read over at 1x and were run again alone: an asset's late and synced rounds at
5120 (404, then 104 to 156 ms; the same cells read 85 to 120 at 4x and 6x). Their time is in the reads: `get_asset` on
the perf root alone took 22 to 210 ms on the loaded Mac, and the column waits on it. Owed on a quiet machine.

| Key (`keys.mjs`, median of 5 to 7, rows ms at 1x / 4x / 6x) | 1440x868@1 | 2560x1440@1.25 | 5120x1440@1 |
|---|---|---|---|
| A letter into the tree's find (was 145 / 165 / 146 at 1440) | 44 / 63 / 54 | 55 / 42 / 55 | 76 / 50 / 65 |
| A second letter | 90 / 85 / 112 (rerun) | 48 / 82 / 127 | 48 / 81 / 77 |

| Scroll (`run.mjs`, the column of 300, fps at 1x / 4x / 6x) | 1440x868@1 | 2560x1440@1.25 | 5120x1440@1 |
|---|---|---|---|
| windowed every 300 px, no background (before) | 60 / 60 / 52.3 | 60 / 59.5 / 51.5 | 60 / 60 / 51.8 |
| `overscan: 1800`, `--paper` (after) | 60 / 56 / 55 | 60 / 57.8 / 54.5 | 60 / 56.5 / 54.8 |

The tree's find typed at speed drew each key in 2 to 17 ms at every throttle, and the nav switch to `#/assets` drew its
first frame in 0.3 to 2 ms. `monitors.mjs` at the three sizes: within every line but one dropped frame scrolling the
roster at 2560 4x, which this change does not touch; opening a monitor (where *Watches* now gives way) drew in 29 / 74
/ 117 ms at 1440, 79 / 100 / 119 at 2560 and 51 / 84 / 118 at 5120 (the redesign pass read 42 / 66 / 95, 32 / 86 / 96
and 49 / 93 / 123). `moves.mjs` (`just moves`) on Monitors, a monitor, Assets by address and by its entry, at
1440x868@1 and 2560x1440@1.25, 1x and 4x, on the fixture and the scale copy: nothing moved. Its old `assets` and `asset`
cases had found nothing since the tree's rows stopped being links and a one-root estate started landing on its root;
both are fixed.

## Owed on Windows

The numbers are headless Chrome on Björn's PC, not WebView2 in knobas's window: the column's scroll in WebView2, and the
tree capped at 5,120 px, from the Windows pass.
