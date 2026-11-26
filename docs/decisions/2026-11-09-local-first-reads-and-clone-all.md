# 2026-11-09: Read from the checkout first, check the source behind, and *Clone all*

**Asked (Björn, on PR #2479):** "creating pull requests is very slow, browsing git files is very slow. [...] see that things keep being smooth. Especially when the external instance is very slow. There's too much blocking happening. Maybe make everything use local clone files first and then have a warning if something you did doesn't work or is differently than the app predicted since files changed? [...] I want the app to be smooth at all times. Also, I'd like to have a clone-all button so I don't have to go into all the repos by myself."

**The fork.** ADR-0023 let knobas clone and never fetch into an existing clone; ADR-0036 let it read a diff at a checkout only between two shas, never two branch names, and never write the object store. Reading *Files* and *New pull request* from the checkout needs the branches' tips, and a checkout that is behind needs them fetched.

**Taken** (ADR-0041): the tips are the mirror's (a branch's declared `commit`, compared by name in the mirror, never passed to git), the tree and files are read at the checkout when it holds that commit, and the source is read behind to check; a checkout that lacks the commit has that one sha fetched into its object store, with no ref, no `FETCH_HEAD` and no file written. A prediction the source contradicts is replaced in place and said on the page; a pull request sent before the check answered is told in a toast.

**Why.** It is what was asked: the checkout first, a warning where the source differs. The checkout already holds the tree; the mirror already holds the tips. Measured before and after on PR #2479 through a proxy slowing the testenv Gitea.

**Also taken:** *Clone all…* on Code's header and on Settings › Checkouts' clones root: said first in a dialog (how many, from which sources, what is already here, what cannot be cloned and why; the size is not known before a clone), one run per app in the background, two clones at a time, one summary toast with every failure. Each clone is *Clone*'s own (ADR-0023).

**Reversing it** costs the four commands and the event (they stay, answering `null`), and *Files* and Compare drawing from the source alone again.
