-- 0055_when_a_sign_in_started_failing.sql -- `knobas.source_config.auth_failing_since`,
-- the moment a source's current run of failed sign-ins began, and the trigger
-- that keeps it (issue #1571).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1571-migration-0055-auth-failing-since.md`).
--
-- ## Why
--
-- A failed sign-in is an inbox item (`knobas_core::inbox`, category
-- `failed_sign_in`), raised once per **episode**: the run of consecutive
-- credential verdicts that say the sign-in fails until somebody acts, which
-- are `unauthorized` and `missing_secret` -- the scheduler's skip list and the
-- shell's `signInFails`. The item is dated at the episode's start, because
-- *done* hides an item while `done_at >= occurred_at`: dated at
-- `auth_checked_at`, which every verdict moves, a retry that is rejected again
-- would bring back an item the reader already answered. Nothing stored said
-- when the episode began, so this column does.
--
-- ## Who keeps it
--
-- A trigger rather than the Rust writer, so that every write of `auth_state`
-- keeps it whoever makes it (`knobas_sync::config::set_health`, the restore's
-- `missing_secret`, a test's raw `update`):
--
-- * a verdict that fails, after one that did not (or on a new row): now;
-- * a verdict that fails, after one that failed: unchanged -- the same
--   episode, including `unauthorized` turning into `missing_secret`;
-- * any other verdict: null, which ends the episode.
--
-- It fires on `auth_state` only, so a statement that writes the column itself
-- and not the state (a test backdating an episode) is not overridden. A
-- data-only restore loads with `--disable-triggers` and keeps the archive's
-- value; an archive taken before this migration carries none, and the
-- restore's own `missing_secret` verdict then starts the episode.
--
-- ## Existing rows
--
-- A source already failing when this runs started its episode at its last
-- check at the latest, so that is its start; `now()` where it was never
-- checked.

alter table knobas.source_config
  add column auth_failing_since timestamptz;

comment on column knobas.source_config.auth_failing_since is
  'When the current run of failed sign-ins (auth_state unauthorized or missing_secret) began; null while the sign-in does not fail. Kept by source_config_auth_failing_since. #1571';

update knobas.source_config
   set auth_failing_since = coalesce(auth_checked_at, now())
 where auth_state in ('unauthorized', 'missing_secret');

create function knobas.keep_auth_failing_since() returns trigger
language plpgsql as $fn$
begin
  if new.auth_state not in ('unauthorized', 'missing_secret') then
    new.auth_failing_since := null;
  elsif tg_op = 'UPDATE' and old.auth_state in ('unauthorized', 'missing_secret') then
    new.auth_failing_since := coalesce(old.auth_failing_since, now());
  else
    new.auth_failing_since := now();
  end if;
  return new;
end
$fn$;

create trigger source_config_auth_failing_since
  before insert or update of auth_state on knobas.source_config
  for each row execute function knobas.keep_auth_failing_since();
