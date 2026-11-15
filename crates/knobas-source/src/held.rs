//! Sections the sync holds (the held-files pass of PR #2479, 2026-11-15;
//! ADR-0042 as amended that day): a view section the sync reads behind a
//! run, so a page's first open of it draws from `sync.held_read` at once and
//! checks the source behind, as it does any read a person made before.
//!
//! One section a kind, and only records that say they are open: Gitea holds
//! each open pull request's *Files*, read again only when the two commits the
//! diff is between moved. The adapter says where its mirror record keeps
//! those (`payload #>> path`); the pass compares them with what the held
//! answer's diff stands at ([`crate::view::DiffRange`]'s `base` and `head`),
//! and reads the section through [`Source::view`](crate::Source::view), the
//! read a page makes, so what is held is the page's own answer.
//!
//! Plain data, as every SPI type.

/// Where this adapter's mirror records keep what the held-sections pass plans
/// from, as paths into the kind's stored payload (`payload #>> path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldShape {
    /// The kind whose records are held (`pr`), and the section of their view
    /// the pass reads (`files`).
    pub kind: &'static str,
    pub section: &'static str,
    /// Where the record names its state, and the state that is open: only an
    /// open record's section is held.
    pub state: &'static [&'static str],
    pub open: &'static str,
    /// Where the record names the two commits the section stands at, each a
    /// whole sha (a pull request's merge base and head): a section is read
    /// again only when either moved, and never for a record that names none.
    pub base: &'static [&'static str],
    pub head: &'static [&'static str],
}
