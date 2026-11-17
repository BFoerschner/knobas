-- 0096_a_view_that_opens_with_every_ticket.sql -- *Show every ticket when
-- the view opens* on a ticket view (ux18, PR #2479;
-- `docs/decisions/2026-11-17-a-view-that-opens-with-every-ticket.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## `show_all`
--
-- Björn: "a toggle in the edit view page of the tickets to always show all
-- the tickets on open, so I don't have to show them myself". On, the view
-- opens as if every *Show N more* had been pressed: each board column whole
-- (no eight, no three for Done), every lane, the list whole, and a filter's
-- words narrow every ticket, not fifty a column.
--
-- The reader's, like the layout and the lanes, so it lives here beside them
-- and a view from a ticket system keeps it too: the sync engine's upsert of
-- those rows (`knobas_sync::saved_views`) never writes it. Off by default, so
-- every view opens as it did.

alter table knobas.ticket_view
  add column show_all boolean not null default false;

comment on column knobas.ticket_view.show_all is
  'Show every ticket when the view opens (ux18): no column, lane or list cut. The reader''s; the sync never writes it.';
