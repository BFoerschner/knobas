# 2026-11-19: An opening says so where you pressed (ux21)

**Asked.** Björn: "also, the toast in the bottom when opening something is really big and a little irritating honestly.
maybe something different to show it accepted the click? the long path also makes it huge"

**Found.** Every *Open in…* (the menus, a right-click's *Open in… ▸*, the launcher's *Open in*, a toast's *Open in VS
Code*, an application's *Try it*) ended in `Started VS Code: <the whole command line>` at the bottom of the window. With
`?fake-long-paths` that was three lines and 720 px wide at 1440x868, 1,413 px wide at 5120x1440. The Diff tool said the
same. Opening the browser, the log folder or the backup folder said nothing when it worked.

**Taken.** What hands something to another program says so where the press was, small and for about a second, and only a
failure is a toast.

- `shell/ack.svelte.ts` holds one chip, *Opening retry.rs in VS Code* (or *Opening in VS Code*, *Opening c90d11e in Tig*,
  *Opening in the browser*, *Opening the log folder*). The program's name is cut in the middle past 32 characters and the
  item's last name past 28; no path, no command line.
- Where: the shell's capture listener notes each `pointerdown` (a point) and `Enter` or `Space` (the focused control's
  box). The chip stands 12 px above and to the right of the pointer, or 6 px above the control, below either near the top,
  hung to the left near the right edge, and where a toast stands when the last press is over 4 s old. A menu closes on its
  pick, so the chip stands where the row was; that is "in place" without holding a menu open for a second.
- It is drawn at once, before the program answers, fixed in place, and placed before it draws, so nothing moves for it
  and it never moves itself. It goes 1.2 s after the press, or 0.4 s after a later answer (a pull request's worktree can
  take seconds), at once on a refusal, and after 15 s without an answer. Fade 90 ms in, 160 ms out; none under reduced
  motion. `role="status"`, so a reader hears it.
- A failure stays a toast: *Could not start VS Code: 'code {path}' could not be run: No such file or directory (os error
  2)*. The launcher's *Open in* says it in those words too and keeps its sheet.
- Every toast is one line at most 680 px wide that gives up its middle (`toastLine`, `.tf-toast-head` / `.tf-toast-tail`):
  the start says what happened, the end why (after the last `: `), else the last sentence, else a path's last name, and the
  whole line is the title. *Cloned payout-service into C:\Users\…\payout-service, on main* keeps its *Open in VS Code*,
  which now answers with the chip above that button.

Not asked of a Fable agent: menus close on their pick, so the choice between the brief's options (a swap in the row, a chip
by the pointer, a status line) was decided by that. A swap in the row would hold a menu open for a second, and a status
line is far from where the eye is.

**Measured.** See the PR comment for ux21 (shots before and after, long-paths, trips, moves).

**Reversing it** is `said` in `tf/openin/apps.svelte.ts` pushing its toast again, and `.tf-toast-line` taken off: the
toast then wraps as before.
