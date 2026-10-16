# Ticket-first shell: conventions so far

How code for the new shell (the TF milestones) is written, as the first tickets already did it. This file records conventions only. It decides nothing new, and the [design system](2026-10-15-ticket-first-design-system.md), the ADRs and spec #808 win wherever they differ. Each line names where it was ruled: the entry in `docs/decisions/2026-10-unattended-rulings.md` and the PR. A PR that departs from a line says why in its Rulings.

Not yet recorded: the conventions of #826 (PR #899 was open when this was written).

## Styles and fonts

- Every class in `tf.css` starts with `tf-`, and `tf-css.test.ts` holds it. `app.css` styles bare names globally (`.btn`, `.toast`, `.timer`) and those would leak. (#809, PR #885)
- Tokens live on the new shell's root, not on `:root`, and the shell has its own stylesheet. (Design system, "Getting it into the app")
- Durations and clocks are Archivo with tabular figures, not mono; mono is for code values. Where the design system and prototype-6 differ, the design system decides. (#809, PR #885; reused by #816, PR #898)
- A selected card is `aria-current="true"` with the `tf-card` class, so the focus ring follows the design system's `--card-ink` rule. Not `aria-selected` on a button. (#810, PR #895)

## Mounting and the shell switch

- The switch between the rooms UI and the new shell is `localStorage` `knobas.shell`, read at startup; switching reloads. A dev build also takes `?shell=tickets`. No command, so a share export does not carry it. (#809, PR #885)
- Rooms-styled dialogs (write queue, launcher, worklog draft, timer picker) mount outside `.tf` and keep their own tokens. A ticket that redraws one ports it. (#809, PR #885)
- Under the new shell `Esc` on a full view with no job goes to Tickets. (#809, PR #885)
- A setting the shell and a page both change (the colour theme) is one store under `tf/`, passed to pages on the shell's context (`TfPageContext`), so a test can hand both the same instance. (#831)

## Routes and addresses

- A route not built yet draws an empty page that says so (the Job page before #816; the Map and Conversation tabs "not built yet"). (#810, PR #895; #816, PR #898)
- The job's address is `#/job/<key>`; an ad-hoc context is `#/job/<ctx id>`. An address holding an entity id (`#/job/jira:PAY-231`) is read as one. (#810, PR #895; #816, PR #898)
- The Time view's addresses are `#/time` and `#/time/<date>` for the Day, and `#/time/week` and `#/time/week/<date>` for the Week containing that date. (#827; #828)
- A Code row opens `#/pr/<entity id>`, `#/repo/<entity id>`, `#/branch/<entity id>` or `#/commit/<entity id>`. The address carries the entity id, because a pull request's number is unique only inside its repository. `branch` and `commit` light the Code nav. (#816, PR #898)
- A Builds row opens `#/build/<entity id>`, a Pages row `#/page/<entity id>`, an Assets row `#/asset/<asset id>`, a Monitors row `#/monitors`. (#817)
- Every job section is `tf/job/TfSection.svelte` reading `list_entities` with `reach: <ticket id>`; a row from the parent (or, from TF5, a link set) carries a `.tf-via` label naming it ("Parent PAY-231"). (#817)

## Data, fixtures and screenshots

- The shell reads declared paths only, never a payload (ADR-0007). A field a card needs becomes a declared path, not a payload read. (#810, PR #895; #816, PR #898)
- `?fake-ipc` fixtures grow with the ticket that needs them: enough rows to compare with prototype-6 (eleven more PAY tickets in #810; a small link graph and the workflow in #816). Where the fixture diverges from the mock adapter, the PR says so. (PR #895; PR #898)
- The `?fake-ipc` fixture is one module per feature under `app/src/lib/shell/dev/fake/`, found by a glob; a ticket adds or edits its feature's module only, except that a row a command returns goes in the module that answers the command (the Week's refused worklog in `fake/write-queue.ts`, #828). A new screen registers itself in its folder's `page.ts` (`definePage`, `tf/page-entry.ts`) and does not edit `TfPage.svelte`. (#908, PR #916)
- A `?fake-ipc` row's state chip, path and source ride in its payload as `chip`, `path` and `source`, which `mirrorRow` reads; the mock adapter declares none of them. (#817)
- Screenshots under `?fake-ipc` are compared with prototype-6 at the same route, theme and 1440 x 900. (Design system, intro)

## Copy and display

- Copy follows prototype-6 word for word unless the PR says why. A source's words are not in the design: nothing is invented for a field no source declares (the card's comment count waited for #894). (Design system, "Words"; #810, PR #895)
- A count badge is drawn only after its read has answered and only above zero. (#809, PR #885)
- An unreachable source is plain text in the status bar; only a credential failure gets the err tone and *Re-enter password*. (#809, PR #885)
- An estimate reads in hours and minutes (`est. 6h`), never days. (#816, PR #898)
- Initials come from the account's spelling (`mara.lindqvist` is ML). (#810, PR #895)

## Tickets

- The *Board / List*, *Lanes* and list sort hold for the session as the page's module `$state`, and are never written to storage or a command; a restart forgets them. (#813)
- A list header that sorts is a `th` carrying `aria-sort` with a button inside; CSS draws ▴ or ▾ on the sorted one. A column that does not sort is a plain `th`. (#813)
- A signal is drawn through `tickets/Signals.svelte`: a `tf-dot` in its tone, then its words. Cards and list rows draw the read's signals and nothing else. (#815)

## Filter keys

- Filter keys on the new shell's search use the launcher's grammar, evaluated on the server; a negation is `-key:value` on the keys only, and a row whose field is absent fails the key and passes its negation. (#811, PR #890; #810, PR #895)
