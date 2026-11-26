# 2026-11-19: What the ux20 Windows pass saw move

**Found (the ux20 Windows pass, Björn's PC, v3.2.9-ux20 in knobas's own WebView2 window).** Three places changed
what they had drawn after their first frame, each on the saved scale profile with a Gitea sync every 45 s:

| Place | What moved | By | Why |
|---|---|---|---|
| Compare, shown again from a held answer (5120 px, every throttle) | the diff under the form | 175 px up | the form drew as *New pull request* (Title, Description: 385 px) until the branch's standing answered, 9 ms at 1x, and then as *Compare* (172 px) for a branch with a pull request open; the diff sits under the taller of the form and the Commits box (210 px) |
| A repository's landing (5120 px, the tree column) | the tree | 18 px down, then up | the held view's line (*As knobas read it 1 min ago. Checking Gitea (live)…*) wrapped to two lines in the tree's column, and the room it keeps once drawn is one |
| *Clone* on a repository, cloning over http | the button's words | *Cloning… 0 %* → *Clone* → *Open in…* | the clone's mark went when the clone ended, and the page learns the folder from its next read of the checkout, about 250 ms later |

**Taken.** As `2026-11-17-nothing-moves-once-drawn.md` rules:

- Compare waits unseen in its room for the branch's standing and shows once, whole (`heldHead`, at most 500 ms;
  `tf-heldbelow` on `.tf-compare`). *Title* takes the focus once the page shows, since a field held unseen takes none.
  The default branch reads no standing and shows at once.
- A held view's line (`TfHeldNote`) is one line, the room it keeps, cut with an ellipsis and whole in its title
  (`.tf-heldnote.tf-held1`), as the Decompiler's line is.
- A clone that ends leaves a `cloned` mark with its folder (`tf/openin/clones.svelte.ts`): *Open in…* draws from it at
  once, and the mark goes when the page's own checkout names the folder (`forget`), or at once when the store's read of
  the checkout finds no folder. A *Clone all* run's clones read nothing more.

**Reversing it** is the hold's class, the line's `tf-held1` and the `cloned` mark taken off; each place then draws as
the table says.
