//! The diff between two commits' trees, printed as `git diff` prints it:
//! the `--raw` records, the `--numstat` counts, or the patch.
//!
//! The pipeline is git's (`gitdiffcore(7)`): the two trees are walked side
//! by side into added, deleted and changed files; deleted and added files
//! are paired into renames ([`crate::renames`]); each pair is then written
//! in the form asked for. What decides the text, beyond the two trees, is
//! read from the repository's configuration and attributes, and where a
//! setting would change git's output in a way this writer does not
//! reproduce, the answer is [`AskGit`].

use std::collections::HashMap;
use std::io::Write as _;
use std::rc::Rc;

use gix::hash::ObjectId;
use gix::objs::Kind;
use gix::objs::tree::EntryKind;

use crate::abbrev::Abbrev;
use crate::read::tree_of;
use crate::renames::{self, Contents, MAX_SCORE};
use crate::{Answer, AskGit, Read, Repo, lines, quote};

/// The empty tree's name, which every git knows without holding it.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// A blob larger than this is left to git: rename detection and the line
/// differ read a file whole.
const WHOLE_FILE: u64 = 64 * 1024 * 1024;

/// Which of `git diff`'s outputs is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffForm {
    /// `--raw -z`: one record a file, no content read but for renames.
    Raw,
    /// `--raw --numstat -z`: the records, then each file's counts.
    RawNumstat,
    /// The patch.
    Patch,
}

/// One `git diff --no-ext-diff --no-color --no-textconv -M
/// --src-prefix=a/ --dst-prefix=b/` between two commits.
#[derive(Debug, Clone)]
pub struct DiffAsk<'a> {
    /// The old side as the command names it: a sha, `<sha>^{commit}`, the
    /// empty tree's sha, or with no `new` the whole range `<a>...<b>`
    /// (from the merge base of the two).
    pub old: &'a str,
    /// The new side: a sha or `<sha>^{commit}`.
    pub new: Option<&'a str>,
    pub form: DiffForm,
    /// `-w`.
    pub ignore_space: bool,
    /// Literal paths after `--` (`GIT_LITERAL_PATHSPECS=1`): only these
    /// files, or what lies under them.
    pub paths: &'a [&'a str],
    /// The reader's caps: writing stops at the first file that ends past
    /// either, since the reader keeps nothing beyond them.
    pub max_lines: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Side {
    /// `0`: no file on this side.
    mode: u32,
    oid: ObjectId,
}

impl Side {
    fn none() -> Self {
        Self {
            mode: 0,
            oid: ObjectId::null(gix::hash::Kind::Sha1),
        }
    }
    fn valid(&self) -> bool {
        self.mode != 0
    }
}

struct Pair {
    /// Where the file is after the change (before it, for a deletion).
    path: Vec<u8>,
    /// Where it was, for a rename.
    from: Option<Vec<u8>>,
    old: Side,
    new: Side,
    /// `0`, or a rename's score out of [`MAX_SCORE`].
    score: u32,
}

impl Pair {
    fn status(&self) -> u8 {
        if self.from.is_some() {
            b'R'
        } else if !self.old.valid() {
            b'A'
        } else if !self.new.valid() {
            b'D'
        } else if (self.old.mode ^ self.new.mode) & 0o170_000 != 0 {
            b'T'
        } else {
            b'M'
        }
    }
    fn similarity(&self) -> u32 {
        self.score * 100 / MAX_SCORE
    }
}

fn mode_of(kind: EntryKind) -> u32 {
    match kind {
        EntryKind::Tree => 0o040_000,
        EntryKind::Blob => 0o100_644,
        EntryKind::BlobExecutable => 0o100_755,
        EntryKind::Link => 0o120_000,
        EntryKind::Commit => 0o160_000,
    }
}

fn regular(mode: u32) -> bool {
    mode & 0o170_000 == 0o100_000
}
fn gitlink(mode: u32) -> bool {
    mode & 0o170_000 == 0o160_000
}

/// What the `diff` attribute says of a path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Driver {
    /// Unspecified: binary if it looks binary.
    Default,
    /// `-diff` (and `binary`): binary whatever it holds.
    Binary,
    /// `diff`: text whatever it holds.
    Text,
}

/// What git reads besides the trees.
struct Settings {
    context: u32,
    inter: u32,
    quote_high: bool,
    rename_limit: u64,
}

impl Repo {
    /// The diff `ask` names.
    ///
    /// # Errors
    ///
    /// [`AskGit`] where git's output depends on something this writer does
    /// not reproduce: a `diff.algorithm` other than Myers, `diff.context`
    /// of nought, an order file, a named diff driver on a changed path,
    /// `GIT_DIFF_OPTS`, a blob a partial clone lacks, a file past 64 MiB.
    pub fn diff(&self, ask: &DiffAsk<'_>) -> Read {
        let settings = self.settings()?;
        let (old, new) = match ask.new {
            Some(new) => (self.tree_named(ask.old)?, self.tree_named(new)?),
            None => {
                let Some((a, b)) = ask.old.split_once("...") else {
                    return Err(AskGit::because("one name that is no range"));
                };
                let (a, b) = (commit_name(a), commit_name(b));
                let Some(base) = self.merge_base_id(a, b)? else {
                    return Ok(Answer::Refused);
                };
                (self.tree_named(&base.to_string())?, self.tree_named(b)?)
            }
        };
        let (Some(old), Some(new)) = (old, new) else {
            return Ok(Answer::Refused);
        };
        let mut pairs = Vec::new();
        self.walk(old, new, &mut Vec::new(), ask.paths, &mut pairs)?;
        let mut work = Work {
            clone: self,
            settings,
            ignore_space: ask.ignore_space,
            blobs: HashMap::new(),
            sizes: HashMap::new(),
            drivers: HashMap::new(),
            attributes: None,
            abbrev: None,
            failed: None,
        };
        let pairs = work.renamed(pairs)?;
        let mut out = Vec::new();
        match ask.form {
            DiffForm::Raw | DiffForm::RawNumstat => {
                let numstat = ask.form == DiffForm::RawNumstat;
                // The counts, where they are printed or decide what is.
                let counts = if numstat || ask.ignore_space {
                    let mut counts = Vec::with_capacity(pairs.len());
                    for pair in &pairs {
                        counts.push(work.counted(pair)?);
                    }
                    Some(counts)
                } else {
                    None
                };
                work.raw(&mut out, &pairs, counts.as_deref())?;
                if numstat {
                    work.numstat(&mut out, &pairs, counts.as_deref().unwrap_or_default());
                }
            }
            DiffForm::Patch => work.patch(&mut out, &pairs, ask.max_lines, ask.max_bytes)?,
        }
        Ok(Answer::Out(out))
    }

    fn settings(&self) -> Result<Settings, AskGit> {
        // git reads these from its environment; this reader reads none.
        for name in [
            "GIT_DIFF_OPTS",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
            "GIT_CONFIG_NOSYSTEM",
            "GIT_ATTR_NOSYSTEM",
            "GIT_ATTR_SOURCE",
        ] {
            if std::env::var_os(name).is_some() {
                return Err(AskGit::because(format!("{name} is set")));
            }
        }
        let config = self.repo.config_snapshot();
        let text = |key: &str| config.string(key).map(|v| v.to_string());
        if let Some(algorithm) = text("diff.algorithm")
            && !matches!(algorithm.to_ascii_lowercase().as_str(), "myers" | "default")
        {
            return Err(AskGit::because("diff.algorithm"));
        }
        for (key, default) in [
            ("diff.indentHeuristic", true),
            ("diff.relative", false),
            ("diff.suppressBlankEmpty", false),
        ] {
            match config.try_boolean(key) {
                Ok(None) => {}
                Ok(Some(set)) if set == default => {}
                _ => return Err(AskGit::because(key)),
            }
        }
        for key in [
            "diff.orderFile",
            "diff.ignoreSubmodules",
            "core.bigFileThreshold",
            "diff.wordRegex",
        ] {
            if text(key).is_some() {
                return Err(AskGit::because(key));
            }
        }
        if let Some(format) = text("diff.submodule")
            && format != "short"
        {
            return Err(AskGit::because("diff.submodule"));
        }
        let number = |key: &'static str, default: i64| match config.try_integer(key) {
            Ok(None) => Ok(default),
            Ok(Some(n)) if (0..=1_000_000).contains(&n) => Ok(n),
            _ => Err(AskGit::because(key)),
        };
        let context = number("diff.context", 3)?;
        if context == 0 {
            // With no context git trims the texts' common tail before it
            // compares them, which moves where a change is shown.
            return Err(AskGit::because("diff.context of nought"));
        }
        let quote_high = match config.try_boolean("core.quotePath") {
            Ok(set) => set.unwrap_or(true),
            Err(_) => return Err(AskGit::because("core.quotePath")),
        };
        Ok(Settings {
            context: context as u32,
            inter: number("diff.interHunkContext", 0)? as u32,
            quote_high,
            rename_limit: number("diff.renameLimit", renames::RENAME_LIMIT as i64)? as u64,
        })
    }

    /// The tree `name` names: a commit's, or the empty tree. `None` where
    /// the clone holds no such commit.
    fn tree_named(&self, name: &str) -> Result<Option<Option<ObjectId>>, AskGit> {
        let name = commit_name(name);
        if name == EMPTY_TREE {
            return Ok(Some(None));
        }
        let Some(commit) = self.commit_id(name)? else {
            return Ok(None);
        };
        match tree_of(&self.object(commit)?.data) {
            Some(tree) => Ok(Some(Some(tree))),
            None => Err(AskGit::because("a commit without a tree")),
        }
    }

    fn entries(&self, tree: Option<ObjectId>) -> Result<Vec<(Vec<u8>, u32, ObjectId)>, AskGit> {
        let Some(tree) = tree else {
            return Ok(Vec::new());
        };
        // The empty tree is known without being held.
        if tree == ObjectId::empty_tree(gix::hash::Kind::Sha1) {
            return Ok(Vec::new());
        }
        let object = self.object(tree)?;
        if object.kind != Kind::Tree {
            return Err(AskGit::because("a tree entry that is no tree"));
        }
        gix::objs::TreeRefIter::from_bytes(&object.data, gix::hash::Kind::Sha1)
            .map(|entry| {
                entry
                    .map(|e| {
                        (
                            e.filename.to_vec(),
                            mode_of(e.mode.kind()),
                            e.oid.to_owned(),
                        )
                    })
                    .map_err(|_| AskGit::because("a tree that does not parse"))
            })
            .collect()
    }

    /// git's `diff_tree_paths`: the two trees side by side in tree order (a
    /// directory sorts as its name and a slash), every file added, deleted
    /// or changed pushed in path order.
    fn walk(
        &self,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        base: &mut Vec<u8>,
        only: &[&str],
        out: &mut Vec<Pair>,
    ) -> Result<(), AskGit> {
        if old == new {
            return Ok(());
        }
        let (a, b) = (self.entries(old)?, self.entries(new)?);
        let (mut i, mut j) = (0, 0);
        let is_tree = |mode: u32| mode == 0o040_000;
        while i < a.len() || j < b.len() {
            let order = match (a.get(i), b.get(j)) {
                (Some(x), Some(y)) => {
                    gix::objs::tree::name_order(&x.0, is_tree(x.1), &y.0, is_tree(y.1))
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Greater,
            };
            let (x, y) = match order {
                std::cmp::Ordering::Less => {
                    i += 1;
                    (Some(&a[i - 1]), None)
                }
                std::cmp::Ordering::Greater => {
                    j += 1;
                    (None, Some(&b[j - 1]))
                }
                std::cmp::Ordering::Equal => {
                    i += 1;
                    j += 1;
                    (Some(&a[i - 1]), Some(&b[j - 1]))
                }
            };
            if let (Some(x), Some(y)) = (x, y)
                && x.1 == y.1
                && x.2 == y.2
            {
                continue;
            }
            let name = &x.or(y).expect("one side").0;
            let stem = base.len();
            if stem > 0 {
                base.push(b'/');
            }
            base.extend_from_slice(name);
            let dir = x.is_some_and(|x| is_tree(x.1)) || y.is_some_and(|y| is_tree(y.1));
            if wanted(base, dir, only) {
                if dir {
                    self.walk(x.map(|x| x.2), y.map(|y| y.2), base, only, out)?;
                } else {
                    let side = |e: Option<&(Vec<u8>, u32, ObjectId)>| {
                        e.map_or_else(Side::none, |e| Side {
                            mode: e.1,
                            oid: e.2,
                        })
                    };
                    out.push(Pair {
                        path: base.clone(),
                        from: None,
                        old: side(x),
                        new: side(y),
                        score: 0,
                    });
                }
            }
            base.truncate(stem);
        }
        Ok(())
    }
}

/// `<sha>^{commit}` without the peel, which every name here gets anyway.
fn commit_name(name: &str) -> &str {
    name.strip_suffix("^{commit}").unwrap_or(name)
}

/// Whether `path` is one of `only`, lies under one, or (a directory) leads
/// to one. Every path is wanted where `only` is empty.
fn wanted(path: &[u8], dir: bool, only: &[&str]) -> bool {
    only.is_empty()
        || only.iter().any(|spec| {
            let spec = spec.as_bytes();
            let spec = spec.strip_suffix(b"/").unwrap_or(spec);
            path == spec
                || (path.len() > spec.len() && path.starts_with(spec) && path[spec.len()] == b'/')
                || (dir
                    && spec.len() > path.len()
                    && spec.starts_with(path)
                    && spec[path.len()] == b'/')
        })
}

/// One answer's working state: what was read, kept while it is written.
struct Work<'a> {
    clone: &'a Repo,
    settings: Settings,
    ignore_space: bool,
    blobs: HashMap<ObjectId, Rc<Vec<u8>>>,
    sizes: HashMap<ObjectId, Option<u64>>,
    drivers: HashMap<Vec<u8>, Driver>,
    attributes: Option<(gix::AttributeStack<'a>, gix::attrs::search::Outcome)>,
    abbrev: Option<Abbrev>,
    /// The first read that failed inside rename detection, whose trait has
    /// no error to give.
    failed: Option<AskGit>,
}

impl Contents for Work<'_> {
    fn size(&mut self, file: &renames::File<'_>) -> Option<u64> {
        match self.size_of(file.oid) {
            Ok(size) => Some(size),
            Err(why) => {
                self.failed.get_or_insert(why);
                None
            }
        }
    }
    fn data(&mut self, file: &renames::File<'_>) -> Option<(Rc<Vec<u8>>, bool)> {
        let side = Side {
            mode: file.mode,
            oid: file.oid,
        };
        let read = self
            .content(&side, true)
            .and_then(|data| Ok((data.clone(), self.binary(&side, file.path, &data)?)));
        match read {
            Ok(found) => Some(found),
            Err(why) => {
                self.failed.get_or_insert(why);
                None
            }
        }
    }
}

impl<'a> Work<'a> {
    fn size_of(&mut self, oid: ObjectId) -> Result<u64, AskGit> {
        if let Some(known) = self.sizes.get(&oid) {
            return known.ok_or_else(|| AskGit::because("a blob this clone does not hold"));
        }
        let size = self.clone.header(oid)?.map(|(_, size)| size);
        self.sizes.insert(oid, size);
        size.ok_or_else(|| AskGit::because("a blob this clone does not hold"))
    }

    /// One side's text as git diffs it: a blob's bytes, a link's target, a
    /// submodule's `Subproject commit` line, nothing for no file. `keep`:
    /// held for the answer (rename detection reads a file many times).
    fn content(&mut self, side: &Side, keep: bool) -> Result<Rc<Vec<u8>>, AskGit> {
        if !side.valid() {
            return Ok(Rc::new(Vec::new()));
        }
        if gitlink(side.mode) {
            return Ok(Rc::new(
                format!("Subproject commit {}\n", side.oid).into_bytes(),
            ));
        }
        if let Some(held) = self.blobs.get(&side.oid) {
            return Ok(held.clone());
        }
        if self.size_of(side.oid)? > WHOLE_FILE {
            return Err(AskGit::because("a file too large to read whole"));
        }
        let object = self.clone.object(side.oid)?;
        if object.kind != Kind::Blob {
            return Err(AskGit::because("a file entry that is no blob"));
        }
        let data = Rc::new(object.data.clone());
        if keep {
            self.blobs.insert(side.oid, data.clone());
        }
        Ok(data)
    }

    /// The `diff` attribute of `path`.
    fn driver(&mut self, path: &[u8]) -> Result<Driver, AskGit> {
        if let Some(known) = self.drivers.get(path) {
            return Ok(*known);
        }
        if self.attributes.is_none() {
            let repo = &self.clone.repo;
            let index = repo
                .index_or_empty()
                .map_err(|error| AskGit::because(format!("the index: {error}")))?;
            let stack = repo
                .attributes_only(
                    &index,
                    gix::worktree::stack::state::attributes::Source::WorktreeThenIdMapping,
                )
                .map_err(|error| AskGit::because(format!("the attributes: {error}")))?;
            let outcome = stack.selected_attribute_matches(["diff"]);
            self.attributes = Some((stack, outcome));
        }
        let (stack, outcome) = self.attributes.as_mut().expect("just made");
        let platform = stack
            .at_entry(
                gix::bstr::BStr::new(path),
                Some(gix::index::entry::Mode::FILE),
            )
            .map_err(|error| AskGit::because(format!("a path's attributes: {error}")))?;
        platform.matching_attributes(outcome);
        let mut driver = Driver::Default;
        for found in outcome.iter_selected() {
            use gix::attrs::StateRef;
            driver = match found.assignment.state {
                StateRef::Unspecified => Driver::Default,
                StateRef::Unset => Driver::Binary,
                StateRef::Set => Driver::Text,
                // A named driver may bring its own function lines or say
                // "binary": git knows its drivers.
                StateRef::Value(_) => return Err(AskGit::because("a named diff driver")),
            };
        }
        self.drivers.insert(path.to_vec(), driver);
        Ok(driver)
    }

    /// git's `diff_filespec_is_binary`: the attribute for a regular file,
    /// else a NUL in the first 8,000 bytes.
    fn binary(&mut self, side: &Side, path: &[u8], data: &[u8]) -> Result<bool, AskGit> {
        if !side.valid() {
            return Ok(false);
        }
        if regular(side.mode) {
            match self.driver(path)? {
                Driver::Binary => return Ok(true),
                Driver::Text => return Ok(false),
                Driver::Default => {}
            }
        }
        Ok(lines::is_binary(data))
    }

    fn short(&mut self, oid: &ObjectId) -> Result<String, AskGit> {
        if self.abbrev.is_none() {
            self.abbrev = Some(Abbrev::new(self.clone)?);
        }
        Ok(self.abbrev.as_ref().expect("just made").of(oid))
    }

    /// The pairs with each added file that is a rename put in the place of
    /// its addition, its deleted side gone.
    fn renamed(&mut self, pairs: Vec<Pair>) -> Result<Vec<Pair>, AskGit> {
        let deleted: Vec<usize> = (0..pairs.len())
            .filter(|&i| pairs[i].status() == b'D')
            .collect();
        let added: Vec<usize> = (0..pairs.len())
            .filter(|&i| pairs[i].status() == b'A')
            .collect();
        if deleted.is_empty() || added.is_empty() {
            return Ok(pairs);
        }
        let found = {
            let file = |i: usize, side: fn(&Pair) -> &Side| renames::File {
                path: &pairs[i].path,
                mode: side(&pairs[i]).mode,
                oid: side(&pairs[i]).oid,
            };
            let sources: Vec<renames::File<'_>> =
                deleted.iter().map(|&i| file(i, |p| &p.old)).collect();
            let targets: Vec<renames::File<'_>> =
                added.iter().map(|&i| file(i, |p| &p.new)).collect();
            let limit = self.settings.rename_limit;
            renames::pair(&sources, &targets, limit, self)
        };
        if let Some(why) = self.failed.take() {
            return Err(why);
        }
        // Rename detection is done with the files it read.
        self.blobs.clear();
        let mut source_of: HashMap<usize, (usize, u32)> = HashMap::new();
        let mut gone = vec![false; pairs.len()];
        for (d, s, score) in found {
            source_of.insert(added[d], (deleted[s], score));
            gone[deleted[s]] = true;
        }
        let froms: HashMap<usize, (Vec<u8>, Side)> = source_of
            .values()
            .map(|&(s, _)| (s, (pairs[s].path.clone(), pairs[s].old)))
            .collect();
        Ok(pairs
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !gone[*i])
            .map(|(i, mut pair)| {
                if let Some(&(s, score)) = source_of.get(&i) {
                    let (from, old) = froms[&s].clone();
                    pair.from = Some(from);
                    pair.old = old;
                    pair.score = score;
                }
                pair
            })
            .collect())
    }

    /// `--raw -z`. With blanks ignored (`counts` given), a changed file in
    /// which nothing is left to show is not listed, as git lists since 2.52:
    /// one whose lines differ in blanks alone. An added, deleted or renamed
    /// file, and a changed mode or kind, are listed whatever is ignored.
    /// These are the files `--numstat` gives no count, so the two halves
    /// hold the same files. (An older git lists every changed path here and
    /// leaves the same files out of `--numstat` alone; the app pairs the two
    /// by path.)
    fn raw(
        &mut self,
        out: &mut Vec<u8>,
        pairs: &[Pair],
        counts: Option<&[Option<(u32, u32)>]>,
    ) -> Result<(), AskGit> {
        for (at, pair) in pairs.iter().enumerate() {
            if self.ignore_space
                && let Some(counts) = counts
                && pair.status() == b'M'
                && pair.old.mode == pair.new.mode
                && counts[at] == Some((0, 0))
            {
                continue;
            }
            let (a, b) = (self.short(&pair.old.oid)?, self.short(&pair.new.oid)?);
            let _ = write!(
                out,
                ":{:06o} {:06o} {a} {b} {}",
                pair.old.mode,
                pair.new.mode,
                pair.status() as char
            );
            if pair.score > 0 {
                let _ = write!(out, "{:03}", pair.similarity());
            }
            out.push(0);
            if let Some(from) = &pair.from {
                out.extend_from_slice(from);
                out.push(0);
            }
            out.extend_from_slice(&pair.path);
            out.push(0);
        }
        Ok(())
    }

    /// The lines added and removed between the two sides, `None` for a
    /// binary file (`builtin_diffstat`).
    fn counted(&mut self, pair: &Pair) -> Result<Option<(u32, u32)>, AskGit> {
        let from = pair.from.as_deref().unwrap_or(&pair.path);
        let same = pair.old.valid() && pair.new.valid() && pair.old.oid == pair.new.oid;
        let (old, new) = (
            self.content(&pair.old, false)?,
            self.content(&pair.new, false)?,
        );
        if self.binary(&pair.old, from, &old)? || self.binary(&pair.new, &pair.path, &new)? {
            return Ok(None);
        }
        if same {
            return Ok(Some((0, 0)));
        }
        let (a, b) = (lines::split(&old), lines::split(&new));
        Ok(Some(lines::counts(&lines::changes(
            &a,
            &b,
            self.ignore_space,
            false,
        ))))
    }

    /// `--numstat -z`, after the raw records: each pair's `counts`.
    fn numstat(&mut self, out: &mut Vec<u8>, pairs: &[Pair], counts: &[Option<(u32, u32)>]) {
        for (pair, counted) in pairs.iter().zip(counts) {
            match counted {
                None => out.extend_from_slice(b"-\t-\t"),
                Some((added, removed)) => {
                    // A changed file in which nothing counts (blanks alone,
                    // ignored) has no line, unless its mode changed.
                    if pair.status() == b'M'
                        && *added == 0
                        && *removed == 0
                        && pair.old.mode == pair.new.mode
                        && pair.old.oid != pair.new.oid
                    {
                        continue;
                    }
                    let _ = write!(out, "{added}\t{removed}\t");
                }
            }
            if let Some(from) = &pair.from {
                out.push(0);
                out.extend_from_slice(from);
                out.push(0);
            }
            out.extend_from_slice(&pair.path);
            out.push(0);
        }
    }

    /// The patch.
    fn patch(
        &mut self,
        out: &mut Vec<u8>,
        pairs: &[Pair],
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<(), AskGit> {
        let mut lines_out = 0usize;
        for pair in pairs {
            // The reader stops at its caps: nothing past them is kept.
            if out.len() >= max_bytes || lines_out >= max_lines {
                break;
            }
            let start = out.len();
            if pair.status() == b'T' {
                // A file that became a link (or the other way) is written
                // as a deletion and then a creation.
                self.file(out, pair, &pair.old, &Side::none())?;
                self.file(out, pair, &Side::none(), &pair.new)?;
            } else {
                self.file(out, pair, &pair.old, &pair.new)?;
            }
            lines_out += out[start..].iter().filter(|b| **b == b'\n').count();
        }
        Ok(())
    }

    /// One `diff --git` block (`builtin_diff`).
    fn file(
        &mut self,
        out: &mut Vec<u8>,
        pair: &Pair,
        one: &Side,
        two: &Side,
    ) -> Result<(), AskGit> {
        let high = self.settings.quote_high;
        let from = pair.from.as_deref().unwrap_or(&pair.path);
        let name_a = if one.valid() { from } else { &pair.path };
        let name_b = if two.valid() {
            pair.path.as_slice()
        } else {
            name_a
        };
        let a_one = quote::two("a/", name_a, high);
        let b_two = quote::two("b/", name_b, high);
        let null = b"/dev/null".to_vec();
        let label_a = if one.valid() {
            a_one.clone()
        } else {
            null.clone()
        };
        let label_b = if two.valid() { b_two.clone() } else { null };

        // What follows the mode lines: a rename's lines, and the index line
        // where the content changed (`fill_metainfo`).
        let mut meta = Vec::new();
        let mut show = false;
        if pair.from.is_some() && one.valid() && two.valid() {
            let _ = writeln!(meta, "similarity index {}%", pair.similarity());
            meta.extend_from_slice(b"rename from ");
            meta.extend_from_slice(&quote::one(from, high));
            meta.extend_from_slice(b"\nrename to ");
            meta.extend_from_slice(&quote::one(&pair.path, high));
            meta.push(b'\n');
            show = true;
        }
        if one.oid != two.oid {
            let (a, b) = (self.short(&one.oid)?, self.short(&two.oid)?);
            let _ = write!(meta, "index {a}..{b}");
            if one.mode == two.mode {
                let _ = write!(meta, " {:06o}", one.mode);
            }
            meta.push(b'\n');
        }

        let mut header = Vec::new();
        header.extend_from_slice(b"diff --git ");
        header.extend_from_slice(&a_one);
        header.push(b' ');
        header.extend_from_slice(&b_two);
        header.push(b'\n');
        if !one.valid() {
            let _ = writeln!(header, "new file mode {:06o}", two.mode);
            show = true;
        } else if !two.valid() {
            let _ = writeln!(header, "deleted file mode {:06o}", one.mode);
            show = true;
        } else if one.mode != two.mode {
            let _ = writeln!(header, "old mode {:06o}", one.mode);
            let _ = writeln!(header, "new mode {:06o}", two.mode);
            show = true;
        }
        header.extend_from_slice(&meta);

        let (old, new) = (self.content(one, false)?, self.content(two, false)?);
        if self.binary(one, name_a, &old)? || self.binary(two, name_b, &new)? {
            if one.oid == two.oid {
                if show {
                    out.extend_from_slice(&header);
                }
                return Ok(());
            }
            out.extend_from_slice(&header);
            out.extend_from_slice(b"Binary files ");
            out.extend_from_slice(&label_a);
            out.extend_from_slice(b" and ");
            out.extend_from_slice(&label_b);
            out.extend_from_slice(b" differ\n");
            return Ok(());
        }
        if show {
            out.extend_from_slice(&header);
        }
        if one.oid == two.oid {
            return Ok(());
        }
        let (a, b) = (lines::split(&old), lines::split(&new));
        let changes = lines::changes(&a, &b, self.ignore_space, true);
        if changes.is_empty() {
            return Ok(());
        }
        // A file whose header waited for a change shows it now.
        if !show {
            out.extend_from_slice(&header);
        }
        for (sign, label) in [(&b"--- "[..], &label_a), (&b"+++ "[..], &label_b)] {
            out.extend_from_slice(sign);
            out.extend_from_slice(label);
            // A name with a blank in it ends in a tab, for `patch`.
            if label.contains(&b' ') {
                out.push(b'\t');
            }
            out.push(b'\n');
        }
        lines::write_hunks(
            out,
            &a,
            &b,
            &changes,
            self.settings.context,
            self.settings.inter,
        );
        Ok(())
    }
}
