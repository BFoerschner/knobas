//! knobas updates itself (ADR-0052, `docs/decisions/2026-11-21-knobas-updates-itself.md`).
//!
//! The parts of the updater that need no window: the version order ([`version`]), the official
//! updater's `latest.json` ([`manifest`]), the compiled-in keys ([`keys`]) and the signature check
//! ([`verify`]), the two channels ([`channel`]) over a [`channel::Fetch`] whose app half is
//! [`fetch::HttpFetch`], the schedule ([`schedule`]), which copies replace themselves
//! ([`standing`]), the download ([`download`]), the two files beside the profile ([`store`]) and
//! the installers ([`install`]). `knobas-app`'s `update` module wires them to the app.
//!
//! What a release carries is `tauri-plugin-updater`'s own format, and the install steps follow its
//! 2.14.1 source (MIT or Apache-2.0), so the plugin can take this crate's client half over after a
//! Tauri bump with no change to any release.

pub mod channel;
pub mod download;
pub mod fetch;
pub mod install;
pub mod keys;
pub mod manifest;
pub mod schedule;
pub mod standing;
pub mod store;
pub mod verify;
pub mod version;
