//! The M1 IPC surface, mirrored in `app/src/lib/ipc/`.
//!
//! One module per owning stream (interfaces §2), which is what keeps seven
//! branches off each other's toes: `app` and `entity` are stream D's, `sources`
//! is stream F's, `search` is stream E's. This file and the
//! `generate_handler!` list in `crate::run` are the only shared surfaces, both
//! append-only and orchestrator-owned.
//!
//! Every command is a thin shim: take the arguments Tauri deserialised, call
//! the crate that owns the behaviour, convert the failure into an
//! [`IpcError`](crate::IpcError). Nothing here decides anything -- when a
//! command grows a policy, that policy belongs in a crate below it, with its
//! own tests.

pub mod app;
pub mod entity;
pub mod search;
pub mod sources;
