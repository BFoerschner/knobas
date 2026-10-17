- **The Jira adapter's `config_schema` — issue #1136 (2026-10-17): the `boards` property's
  description says favourite filters.** Spec #808, *Jira views*; follows #1096 (PR #1135), which
  relabelled the choice "Only boards built on my favourite filters". The description's second
  choice read "only the boards marked as favourites in Jira", a board star the adapter never
  reads; it now reads "only the boards built on your favourite filters", what #878 implemented
  (a board whose saved filter is one of `GET rest/api/2/filter/favourite`). Wording only: the
  key `boards`, its enum (`favourites` included), its default and its `x-knobas-section` are
  unchanged, so no stored config changes meaning. Transcribed in
  `app/src/lib/sources/fixtures.ts`. No migration, no command, no DTO, no event, no `WriteOp`.
  Witnessed by `live_jira_seeded`'s
  `only_the_boards_built_on_my_favourite_filters_come_in_when_the_choice_is_set`.
