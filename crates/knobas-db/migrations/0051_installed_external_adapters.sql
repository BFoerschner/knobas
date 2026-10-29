-- 0051_installed_external_adapters.sql -- the external adapters a reader
-- installed (#1202, ADR-0028).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1202-migration-0051-installed-external-adapters.md`).
--
-- ## `external_adapter`
--
-- One row per installed adapter, written once its manifest and artifact pass
-- the static checks. The artifact itself is a file in the profile's
-- `adapters/` directory, named by its hash; this row is what pins it. At app
-- start each row's file is read and hashed again, and a file whose hash is
-- not the row's is refused (`sources::install::load_installed`).
--
-- Keyed by the adapter kind: the registry holds one adapter per kind, and a
-- built-in kind is refused at install. No foreign key from
-- `source_config.kind`: a source of an adapter that is no longer installed
-- still lists, so the reader can remove it, as a source of a compiled-out
-- adapter does.
--
-- `hosts` and `limits` are the manifest's, stored as written; the allowlist
-- (#1203) and the call limits (#1204) read them.

create table knobas.external_adapter (
  adapter_kind      text primary key,
  adapter_version   text not null,
  interface_version text not null,
  -- Where it was installed from: a local manifest's absolute path, or (from
  -- #1209) the manifest URL.
  source_url        text not null,
  -- The release tag. A local manifest is no release, so it has none.
  tag               text,
  artifact_sha256   text not null check (artifact_sha256 ~ '^[0-9a-f]{64}$'),
  hosts             text[] not null default '{}',
  limits            jsonb not null default '{}'::jsonb,
  installed_at      timestamptz not null default now()
);

comment on table knobas.external_adapter is
  'An installed external adapter (#1202, ADR-0028): where it came from, its pinned artifact hash, and the allowlist and limits its manifest asked for.';
