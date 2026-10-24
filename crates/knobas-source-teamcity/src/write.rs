//! The two writes M2 ratified for TeamCity -- trigger a build, and re-run
//! one -- and setting a build configuration's parameter and running one on a
//! branch with overrides, which the source views added (issues #668, #669).
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
//! `branchName` is deliberately not sent by either. TeamCity builds the
//! configuration's default branch without one, and a trigger or a re-run has
//! no branch a reader picked -- so a branch there would be a guess dressed as
//! a choice. The custom run ([`run`], issue #669) is where a reader picks
//! one: the same endpoint, with `branchName` and `properties` added.

use knobas_source::{BuildParameter, SourceError};

use crate::client::HttpRest;

/// Put `build_type_id`'s configuration on the queue, and answer the queued
/// build's id where TeamCity said it (#1949).
///
/// # Errors
///
/// The [`SourceError`] the request maps to, with TeamCity's own sentence in
/// the message where it sent one. A 404 for a configuration that is gone is a
/// **refusal** the queue does not retry; a 503 while the server restarts is
/// not.
pub(crate) async fn trigger(
    rest: &HttpRest,
    build_type_id: &str,
) -> Result<Option<i64>, SourceError> {
    rest.queue_build(build_type_id).await
}

/// Run again whatever configuration `build_id` belonged to, and answer the
/// queued build's id as [`trigger`] does.
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
pub(crate) async fn rerun(rest: &HttpRest, build_id: i64) -> Result<Option<i64>, SourceError> {
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

/// A custom run's options (#852): what TeamCity's *Run custom build* dialog
/// calls *clean all files in the checkout directory* and *put the build at
/// the queue top*, sent as `triggeringOptions` `cleanSources` and
/// `queueAtTop`, each only when set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RunOptions {
    pub(crate) clean_checkout: bool,
    pub(crate) first_in_queue: bool,
}

/// Put one build of `build_type_id` on the queue on `branch`, with
/// `overrides` for this build only (issue #669), and answer the queued
/// build's id where TeamCity said it.
///
/// The same `POST /app/rest/buildQueue` as [`trigger`], with `branchName`
/// and `properties` -- the body TeamCity's own *Run custom build* sends --
/// and `triggeringOptions` for the [`RunOptions`] set. No key is sent empty: no branch is the default branch, and no
/// override is the configuration's own values, which is exactly the trigger.
/// An override does not change the configuration (measured on the seeded
/// 2026.1.3 server, 2026-10-13: the build's resulting parameter carries the
/// override, the configuration keeps its own value).
///
/// The branch is TeamCity's to resolve against the configuration's branch
/// specification. A branch it does not know is still queued -- TeamCity
/// builds what its VCS root can fetch, and fails the build when it cannot --
/// so that answer comes back as the build, not as this write's refusal.
///
/// # Errors
///
/// [`SourceError::Protocol`], before a request, for a branch that is empty
/// or blank and for an override with no name: the first would be read as
/// neither a branch nor the default, the second is no parameter. Otherwise
/// as [`trigger`].
pub(crate) async fn run(
    rest: &HttpRest,
    build_type_id: &str,
    branch: Option<&str>,
    overrides: &[BuildParameter],
    options: RunOptions,
) -> Result<Option<i64>, SourceError> {
    if let Some(branch) = branch.filter(|b| b.trim().is_empty()) {
        return Err(SourceError::protocol(format!(
            "teamcity: {branch:?} is not a branch -- leave the branch out to run the default one"
        )));
    }
    if overrides.iter().any(|p| p.name.trim().is_empty()) {
        return Err(SourceError::protocol(
            "teamcity: a parameter override has no name".to_owned(),
        ));
    }
    rest.queue_run(build_type_id, branch, overrides, options)
        .await
}
