//! The docs contract (#2444, ADR-0040, spec #2432 story 50): what
//! `knobas adapter check` runs on a component that claims the wiki role or
//! exports `docs`, beside the contract battery.
//!
//! The battery holds a [`Source`](knobas_source::Source); the docs exports
//! are no part of one (a projection takes no instance), so this holds them
//! over what the source's own sync emitted:
//!
//! 1. [`Check::Export`]: a wiki claim comes with the `docs` export.
//! 2. [`Check::PageDrawn`]: the sync emits a page, and the projection draws
//!    every page it emitted.
//! 3. [`Check::OtherKindRefused`]: the projection draws no other kind, the
//!    sync's own items of other kinds and a kind the descriptor does not
//!    declare (with a page's payload) alike. The Docs frame draws `page`.
//! 4. [`Check::SectionsNumbered`]: a page's sections are numbered 1, 2, …
//!    in order, each either editable (its text) or refused (its reason).
//! 5. Where the descriptor declares `update_page`:
//!    [`Check::RefusedSectionRefused`], a section the projection refuses is
//!    refused by the edit for the same reason;
//!    [`Check::EditableSectionEdited`], an editable one is edited, against
//!    the version the projection names; and [`Check::MissingSectionMissing`],
//!    section 0, the one past the last and any section of a kind it does
//!    not draw are missing.
//! 6. Where it declares `create_page`, [`Check::NewPageBody`]: a new page's
//!    body from text is markup, not nothing.
//!
//! A call that traps, runs out its deadline or its memory is a breach of the
//! check that made it. The first breach is the answer, naming its check.

use knobas_source::docs::{DocContent, EditRefused, PAGE_KIND};
use knobas_source::{SourceDescriptor, SyncItem};

use crate::WasmAdapter;

/// A kind no descriptor declares, asked of the projection and the edit with
/// a page's payload: both refuse it. Named after the contract, so no
/// adapter's real kind is called this.
pub const PROBE_KIND: &str = "docs_contract_probe";

/// The text a new page's body is made from.
const NEW_PAGE_TEXT: &str = "## Docs contract\nA new page.\n\n- one item";

/// One clause of the docs contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Export,
    PageDrawn,
    OtherKindRefused,
    SectionsNumbered,
    RefusedSectionRefused,
    EditableSectionEdited,
    MissingSectionMissing,
    NewPageBody,
}

impl Check {
    /// The check's name, as `knobas adapter check` prints it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Export => "docs export",
            Self::PageDrawn => "page drawn",
            Self::OtherKindRefused => "other kind refused",
            Self::SectionsNumbered => "sections numbered",
            Self::RefusedSectionRefused => "refused section refused",
            Self::EditableSectionEdited => "editable section edited",
            Self::MissingSectionMissing => "missing section missing",
            Self::NewPageBody => "new page body",
        }
    }
}

/// A component that breaks the docs contract: the check, and what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breach {
    pub check: Check,
    pub detail: String,
}

impl std::fmt::Display for Breach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "docs contract, {}: {}", self.check.name(), self.detail)
    }
}

impl std::error::Error for Breach {}

fn breach(check: Check, detail: impl Into<String>) -> Breach {
    Breach {
        check,
        detail: detail.into(),
    }
}

/// Run the docs contract on `adapter`, whose source of `descriptor` emitted
/// `items` in a full sync. `Ok` for a component that neither claims the wiki
/// role nor exports `docs`: there is nothing to hold.
///
/// # Errors
///
/// The first [`Breach`], naming its [`Check`].
pub fn check(
    adapter: &WasmAdapter,
    descriptor: &SourceDescriptor,
    items: &[SyncItem],
) -> Result<(), Breach> {
    let claims = descriptor
        .roles
        .iter()
        .any(|claim| claim.role == knobas_source::role::WIKI);
    if !adapter.projects() {
        return if claims {
            Err(breach(
                Check::Export,
                "the descriptor claims the wiki role and the component exports no `docs`",
            ))
        } else {
            Ok(())
        };
    }
    let declares = |op: &str| descriptor.write_ops.iter().any(|o| o == op);
    let live = items.iter().filter(|item| !item.deleted);
    let pages: Vec<&SyncItem> = live.clone().filter(|item| item.kind == PAGE_KIND).collect();
    if pages.is_empty() {
        return Err(breach(
            Check::PageDrawn,
            "the full sync emitted no `page`, so nothing of the projection is held",
        ));
    }
    for item in live.filter(|item| item.kind != PAGE_KIND) {
        if adapter.project(&item.kind, &item.payload).is_some() {
            return Err(breach(
                Check::OtherKindRefused,
                format!("{} of kind {:?} is drawn as a page", item.entity, item.kind),
            ));
        }
    }
    for page in pages {
        let Some(content) = adapter.project(PAGE_KIND, &page.payload) else {
            return Err(breach(
                Check::PageDrawn,
                format!(
                    "{} is drawn as no page (the projection answered none, trapped or ran out \
                     of time or memory)",
                    page.entity
                ),
            ));
        };
        if adapter.project(PROBE_KIND, &page.payload).is_some() {
            return Err(breach(
                Check::OtherKindRefused,
                format!(
                    "{}'s payload is drawn as a page of {PROBE_KIND:?}, a kind the descriptor \
                     does not declare",
                    page.entity
                ),
            ));
        }
        numbered(page, &content)?;
        if declares("update_page") {
            edits(adapter, page, &content)?;
        }
    }
    if declares("create_page") {
        match adapter.body_from_text(NEW_PAGE_TEXT) {
            Some(body) if !body.trim().is_empty() => {}
            Some(_) => {
                return Err(breach(
                    Check::NewPageBody,
                    "a new page's body from text is empty",
                ));
            }
            None => {
                return Err(breach(
                    Check::NewPageBody,
                    "body-from-text trapped or ran out of time or memory",
                ));
            }
        }
    }
    Ok(())
}

/// Check 4 on one page.
fn numbered(page: &SyncItem, content: &DocContent) -> Result<(), Breach> {
    for (section, number) in content.sections.iter().zip(1u32..) {
        if section.number != number {
            return Err(breach(
                Check::SectionsNumbered,
                format!(
                    "{}'s section {number} is numbered {}",
                    page.entity, section.number
                ),
            ));
        }
        if section.text.is_some() == section.refusal.is_some() {
            return Err(breach(
                Check::SectionsNumbered,
                format!(
                    "{}'s section {number} must have either its text or its refusal",
                    page.entity
                ),
            ));
        }
    }
    Ok(())
}

/// Check 5 on one page.
fn edits(adapter: &WasmAdapter, page: &SyncItem, content: &DocContent) -> Result<(), Breach> {
    let edit = |kind: &str, number: u32, text: &str| {
        adapter.edit_section(kind, &page.payload, number, text)
    };
    for section in &content.sections {
        let number = section.number;
        let answer = edit(PAGE_KIND, number, section.text.as_deref().unwrap_or("x"));
        match (section.refusal, section.text.as_ref(), answer) {
            (Some(refusal), _, Some(Err(EditRefused::Section(r)))) if r == refusal => {}
            (Some(refusal), _, answer) => {
                return Err(breach(
                    Check::RefusedSectionRefused,
                    format!(
                        "{}'s section {number} is drawn refused ({refusal:?}) and the edit \
                         answered {}",
                        page.entity,
                        shown(answer.as_ref())
                    ),
                ));
            }
            (None, Some(_), Some(Ok(made))) => {
                if let Some(version) = content.version
                    && i64::try_from(version).ok() != Some(made.base_version)
                {
                    return Err(breach(
                        Check::EditableSectionEdited,
                        format!(
                            "{}'s section {number} is edited against version {} and the page \
                             is drawn at version {version}",
                            page.entity, made.base_version
                        ),
                    ));
                }
            }
            // A page drawn without a version has nothing to edit against.
            (None, Some(_), Some(Err(EditRefused::Missing))) if content.version.is_none() => {}
            (None, _, answer) => {
                return Err(breach(
                    Check::EditableSectionEdited,
                    format!(
                        "{}'s section {number} is drawn editable and the edit answered {}",
                        page.entity,
                        shown(answer.as_ref())
                    ),
                ));
            }
        }
    }
    let past = u32::try_from(content.sections.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    for (kind, number) in [(PAGE_KIND, 0), (PAGE_KIND, past), (PROBE_KIND, 1)] {
        let answer = edit(kind, number, "x");
        if answer != Some(Err(EditRefused::Missing)) {
            return Err(breach(
                Check::MissingSectionMissing,
                format!(
                    "the edit of {}'s section {number} as kind {kind:?} answered {}, not missing",
                    page.entity,
                    shown(answer.as_ref())
                ),
            ));
        }
    }
    Ok(())
}

fn shown(answer: Option<&Result<knobas_source::docs::SectionEdit, EditRefused>>) -> String {
    match answer {
        None => "nothing (it trapped or ran out of time or memory)".to_owned(),
        Some(Ok(_)) => "an edit".to_owned(),
        Some(Err(EditRefused::Missing)) => "missing".to_owned(),
        Some(Err(EditRefused::Section(r))) => format!("refused ({r:?})"),
    }
}
