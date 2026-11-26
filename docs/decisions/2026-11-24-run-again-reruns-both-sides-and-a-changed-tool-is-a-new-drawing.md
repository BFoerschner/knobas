# 2026-11-24: *Run again* runs both sides, and a changed tool is a new drawing

**Reported (Björn, v3.2.9-ux40):** "in comparisons it doesn't rerun the tool for the folders when I click run again. I once made a folderview with an now outdated tool, and it didn't rerender so the diff is useless because the generation didn't happen with the same version of the tool on both commits".

The judgement calls went to Fable (xhigh) under Björn's standing instruction; Opus built them.

## The causes

1. **Run again ran nothing.** A folder's bar in a comparison said *Run again* once anything was kept, but its press ran only the sides that were not kept. With both sides kept it asked nothing.
2. **A changed tool was not seen.** What a folder renderer makes is kept by the renderer's id, command and output kind, the copy's name and the folder's content (ADR-0044, ADR-0046), on disk across restarts, and shared by the Files tab and comparisons. Nothing named the tool itself. A side made earlier by an older version (on the Files tab, say) was current for the same command, the other side was run with the new version, and the two were compared with no word said.

## The rulings

1. **The tool's stamp is in the key** (`folder_render::run::tool_stamp`): the canonical path, size and modification time of the program (found as the run finds it: a word with a path separator as written, else the first file of that name on `PATH`, with `.exe` on Windows where it has no extension), and the same for every other word that holds no placeholder and names an existing absolute file (`java -jar /opt/tool.jar`, `node /x/cli.js`). A program found nowhere is stamped by its word, so a look never fails over it. Reused for 2 s per command, so a diff of many folders stats once per round of looks. No content hash and no `--version`: a look never starts a program. A changed tool makes every drawing it made before non-current, on the Files tab too, where an open runs it again.
2. **A press:** *Run* (primary) while a side wants a run (not rendered, made from an earlier state, or failed) runs just those sides. Once both are made, the button is *Run again* and runs both sides anew. The band's *Run N folder renders* keeps its rule (folders not rendered or stale) through the same press.
3. **A stale side is held, never compared.** With nothing compared yet the 96 px note box stays. A compare already drawn stays where it is when a side turns stale; only the bar's words and tone change. Other ends, another repository or another command drop what was compared (Local changes keeps its compare when a commit moves its start).
4. **The words.** Held, in the info tone: "Not rendered since ⟨renderer⟩ changed." (a commit's side), "Not rendered for your working tree as it is now." (the working tree's side alone). Under a compare still drawn, in the warn tone: "Made before ⟨renderer⟩ changed." and "Made from an earlier state of your working tree.". On the Files tab a drawing at the folder's own commit reads "Made at abc1234 2 h ago, before ⟨renderer⟩ changed." rather than "The folder is at abc1234 now.".

## Accepted

- Every kept drawing is non-current once after this lands: the key's recipe changed. Each is drawn as made from an earlier state until it is run again.
- A wrapper script's imports, `npx`, a plugin the tool loads from elsewhere or a config file it reads are not seen. *Run again* is the remedy, and it now runs both sides.

## Checked by

- Rust: `folder_render::runs::tests::a_tool_changed_on_disk_makes_what_it_made_before_stale` (fails without the stamp: the older drawing answered `current: true`), `the_tool_s_stamp_is_in_the_key`, `run::tests::a_tool_changed_on_disk_is_another_stamp`.
- Vitest: `folder-compare.test.svelte.ts` (both kept: a press asks both with `again`; a stale side is never compared and *Run* runs it alone; a compare drawn stays when a side turns stale, and goes with other ends; three of them fail without the fix), `TfDiffFolder.test.svelte.ts` (the labels, primary and words), `TfMadeBar` and `TfFolderRender` (the Files tab's words).

## Found in the real app, and fixed after

The main path ran on the WSL box under its own Xvfb with a scratch profile and the live Gitea (`tidewater/payout-service` cloned through *Clone*, a folder renderer `sh gen.sh {path} {outdir}` on `docs` whose script says its version in every file it makes):

- **A kept Compare never looked again.** Its folder sides asked the backend once, when built; with the tool bumped from v1 to v2 while Compare stood, it still said what v1 made on both sides. The window back in focus, or the page shown again, now looks at each grouped folder's two sides again (a look starts nothing), as Compare already reads the working tree again then (ruling 6 of 2026-11-20). Held by a test in `TfDiffView.folders.test.svelte.ts` that fails without it.
- **A phantom last line.** The view diffs a generated text file itself, and `lineDiff` split `x\n` into `x` and an empty line: every generated card drew an empty line 2 on each side, and an added one-line file counted `+2`. The last line break now ends the last line, as the JAR view already does. Held by a test that fails without it.

After both: the Files tab drew `docs` with v1; Compare with the working tree took that drawing for `main` and ran the working tree's side alone with v1. The tool bumped to v2, Compare said "Not rendered since Gen changed." with nothing compared and *Run* primary. *Run* ran both sides with v2 at once (the tool's log: two v2 runs in the same second), and the cards showed `gen v2` on both sides. *Run again* ran both again (two more). The one-line generated file picked in the list was marked, its line clear of the heads; another landed at the band's foot (88 px against the band's 87).

## Reversing it

Take `tool_stamp` out of `clone_key` and `forge_key`, `wants` out of `FolderCompare`, and the stale arm back into `#tokenOf`.
