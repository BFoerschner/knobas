-- The reviewers a pull request names.
create table if not exists reviewers (
  id bigserial primary key,
  pr text not null,
  login text not null default ''
);

CREATE OR REPLACE FUNCTION reviewers_of(p text) RETURNS jsonb AS $fn$
  SELECT coalesce(jsonb_agg(login order by login), '[]'::jsonb)
  FROM reviewers WHERE pr = p AND login <> '';
$fn$ LANGUAGE sql IMMUTABLE;

insert into reviewers (pr, login) values ('mock#142', 'bjorn');
