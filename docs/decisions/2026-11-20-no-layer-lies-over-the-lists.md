# No layer lies over the lists (2026-11-20)

The ux21 Windows pass, PR #2479.

## What happened

In knobas's own WebView2 window, ux21 scrolled Tickets' board and a Docs space on the main thread, where ux20
scrolled them on the compositor (`run.mjs --kinds wheel`, window size): the board moved at 71 frames a second at CPU
4x and 29 at 6x with gaps up to 71 ms, a Docs space at 86 at 6x with a 117 ms gap. Inbox, Code, Builds and Monitors
stayed on the compositor.

## Why

Bisected in the real window by serving each commit's built front end into it over CDP `Fetch` (the ux18 pass's
method), the board's and a Docs space's wheel at 4x each time: ux20's front end on the compositor, `df4e3a6e` and
`70b0f548` on the compositor, `5ff3b6b3` (*An opening says so where you pressed*) on the main thread. ux21 with
`<TfAck />` taken out of the shell scrolls on the compositor; with it in and its two window listeners taken out, on
the main thread. So it is the chips' layer: `.tf-acks { position: fixed; inset: 0; z-index: 40; pointer-events:
none; }`, a box over the whole window on every page. With it there, WebView2 hit-tests each wheel on the main thread
over the board's and a Docs space's scrollers (Chromium's scroll-bottleneck overlay labels the board *main thread
scroll hit test*). Letting every press through does not spare it.

A CSS override injected from the probe proves nothing here: the window's CSP (`style-src 'self'`) refuses an inline
`<style>`, so such a test runs the page unchanged.

## What changed

The layer takes no room: `position: fixed; top: 0; left: 0; width: 0; height: 0`. The chip is `position: fixed`
itself, so it still places against the window, and with no chip showing nothing lies over a list. Served into the
window, the board and a Docs space scroll on the compositor again (4x: the board 166 frames a second against 89, its
longest gap 12 ms against 42). `tf-css.test.ts` holds the layer to no room.

A layer that must be over the page while it is not in use (a toast stack, a chip host) is a box of no size or of its
content's size, never `inset: 0`. The app root (`.tf`) is fixed over the window too, but under the lists, as in ux20;
dialogs, the scrim and a drag's cover cover it only while they are up.
