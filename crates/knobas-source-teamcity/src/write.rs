//! The two writes M2 ratified for TeamCity -- trigger a build, and re-run
//! one -- and setting a build configuration's parameter, which the source
//! views added (issue #668).
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

/// Set parameter `name` of build configuration `build_type_id` to `value`,
/// unless it changed since the reader's edit.
///
/// `PUT /app/rest/buildTypes/id:{id}/parameters/{name}/value` with the value
/// as `text/plain`. On a parameter the configuration inherits from a project
/// or template, TeamCity creates an own parameter of that name that overrides
/// the inherited one; the inherited value stays where it was. On a password
/// parameter only the value changes and the spec stays `password`, so no
/// view draws it afterwards (both measured on the seeded 2026.1.3 server,
/// 2026-10-12).
///
/// # `base` is the concurrency check
///
/// The configuration's mirrored record does not carry its parameters, so the
/// write queue cannot hold this op when a colleague changes the same
/// parameter, and TeamCity's `PUT` has no version to refuse on. So with a
/// `base` -- the value the view drew when the reader started editing -- the
/// parameter is read again just before the `PUT`, drawn by the view's own
/// rule ([`crate::view::drawn_value`]), and a value that is not `base` any
/// more is refused with the value it is now. Drawn, never raw: a URL's
/// credential and a secret stay out of the refusal as they stay out of the
/// view. A narrower window remains between that read and the `PUT`, as it
/// does for every read-then-write REST has no condition for. Without a
/// `base` (a secret, whose value the reader never saw) nothing is read, and
/// the write is last-write-wins.
///
/// Idempotent, so a retry after a lost answer sets the same value twice
/// (ADR-0012): a parameter that already holds `value` is not a change since
/// the edit, it is this write having landed, and passes the check.
///
/// # Errors
///
/// As [`trigger`]: a 403 (the account may not edit the project) is
/// `Unauthorized`, a 404 for a configuration or parameter that is gone a
/// refusal carrying TeamCity's sentence, and an empty, `.` or `..` name is
/// refused before a request ([`crate::ids::parameter_segment`]).
/// [`SourceError::Protocol`] when the parameter changed since `base`.
pub(crate) async fn set_parameter(
    rest: &HttpRest,
    build_type_id: &str,
    name: &str,
    value: &str,
    base: Option<&str>,
) -> Result<(), SourceError> {
    if let Some(base) = base {
        let now = rest.parameter(build_type_id, name).await?;
        let (drawn, secret) = crate::view::drawn_value(
            &now.name,
            now.value.as_deref(),
            crate::view::is_password(&now),
        );
        if secret {
            return Err(SourceError::protocol(format!(
                "teamcity: {name:?} changed since you edited: it is now a secret, whose \
                 value is not shown"
            )));
        }
        if drawn != base && drawn != value {
            return Err(SourceError::protocol(format!(
                "teamcity: {name:?} changed since you edited: now {drawn:?}"
            )));
        }
    }
    rest.put_parameter(build_type_id, name, value).await
}
