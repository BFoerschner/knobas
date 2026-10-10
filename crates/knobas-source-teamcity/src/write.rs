//! The two writes M2 ratified for TeamCity: trigger a build, and re-run one.
//!
//! Deliberately free of [`WriteOp`], for the same reason the other two
//! adapters' write modules are: knobas has one outbound write path, and
//! `knobas-sync/tests/it/write_choke_point.rs` refuses any production file that
//! *names* the op enum without implementing the trait. [`crate::source`] --
//! where `impl Source for TeamCitySource` lives -- unpacks; this performs.
//!
//! [`WriteOp`]: knobas_source::WriteOp
//!
//! # One endpoint, two operations
//!
//! Both are `POST /app/rest/buildQueue` with `{"buildType":{"id":…}}`. They
//! differ only in **how the configuration is found**: a trigger is aimed at a
//! build configuration and already has it, and a re-run is aimed at a *build*
//! and has to ask the server which configuration that build belonged to. There
//! is no separate re-run verb in TeamCity's REST API -- re-running is putting
//! the same configuration on the queue again -- and inventing one would be an
//! endpoint no server has.
//!
//! `branchName` is deliberately not sent. TeamCity builds the configuration's
//! default branch without one, and this adapter does not mirror the branch a
//! build ran on as anything a user can pick -- so a branch here would be a
//! guess dressed as a choice. Re-running on the failed build's own branch is
//! the obvious next want and needs a `WriteOp` field, which is an ADR-0006
//! conversation rather than a drive-by.

use knobas_source::SourceError;

use crate::client::HttpRest;

/// Put `build_type_id`'s configuration on the queue.
///
/// # Errors
///
/// The [`SourceError`] the request maps to, with TeamCity's own sentence in
/// the message where it sent one. A 404 for a configuration that is gone is a
/// **refusal** the queue does not retry; a 503 while the server restarts is
/// not.
pub(crate) async fn trigger(rest: &HttpRest, build_type_id: &str) -> Result<(), SourceError> {
    rest.queue_build(build_type_id).await
}

/// Run again whatever configuration `build_id` belonged to.
///
/// Two requests, and the first is not optional: the entity id of a build
/// carries the build's own id and nothing else (`teamcity:build:1187`,
/// interfaces §4.2), so the configuration has to be read from the server. A
/// build that has been cleaned up answers 404, which is the honest answer --
/// there is nothing to re-run.
///
/// # Errors
///
/// [`SourceError::Protocol`] when the build exists but names no configuration,
/// which would otherwise queue nothing while reporting success; otherwise
/// whatever the two requests map to.
pub(crate) async fn rerun(rest: &HttpRest, build_id: i64) -> Result<(), SourceError> {
    let build_type_id = rest.build_type_of(build_id).await?;
    rest.queue_build(&build_type_id).await
}
