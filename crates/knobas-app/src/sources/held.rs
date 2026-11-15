//! **What knobas last read, kept between sessions** (ADR-0042, the second
//! smoothness pass on PR #2479): `sync.held_read`, where a page's source read
//! waits for the next time the page opens, so the page shows it at once and
//! reads the source behind it ([`super::live::LiveSources::shown`]).
//!
//! The store itself is [`knobas_sync::held_read`], since the held-files pass
//! (2026-11-15) holds rows there too: each open pull request's *Files*, read
//! after a run, which a page draws as it draws its own earlier reads. A row a
//! page reads or holds here is a person's, and stays in the store before any
//! the sync held that no page drew.

pub use knobas_sync::held_read::{BOUND, ROW_CAP, forget, get, put, recipe, view_ask};
