//! A commit resolved, a tree listed and a blob's bytes: the reads *Files*
//! makes.

use gix::hash::ObjectId;
use gix::objs::Kind;

use crate::{Answer, AskGit, Read, Repo, is_hex};

/// A blob larger than this is left to git, which streams it and is stopped
/// at the caller's cap; here it would be decoded whole.
const WHOLE_BLOB: u64 = 64 * 1024 * 1024;

impl Repo {
    /// `git rev-parse --verify --quiet --end-of-options <name>^{commit}`:
    /// the whole sha of the commit `name` names, and a newline. `name` is a
    /// sha of 7 to 64 hex digits, or one with a trailing `^` for its first
    /// parent.
    ///
    /// # Errors
    ///
    /// [`AskGit`] for a name that is not such a sha, a short sha a ref could
    /// be spelled as (git reads the ref first), and a parent asked of a
    /// shallow clone, whose cut only git places.
    pub fn rev_parse_commit(&self, name: &str) -> Read {
        Ok(match self.commit_id(name)? {
            Some(id) => Answer::Out(format!("{id}\n").into_bytes()),
            None => Answer::Refused,
        })
    }

    /// The commit `name` names, as [`Self::rev_parse_commit`] resolves it.
    pub(crate) fn commit_id(&self, name: &str) -> Result<Option<ObjectId>, AskGit> {
        let (stem, parent) = match name.strip_suffix('^') {
            Some(stem) => (stem, true),
            None => (name, false),
        };
        if !is_hex(stem) {
            return Err(AskGit::because("a name that is not a sha"));
        }
        let Some(commit) = self.peeled_commit(stem)? else {
            return Ok(None);
        };
        if !parent {
            return Ok(Some(commit));
        }
        if self.shallow {
            return Err(AskGit::because("a parent in a shallow clone"));
        }
        let object = self.object(commit)?;
        let Some(first) = parents(&object.data).next() else {
            return Ok(None);
        };
        // `<sha>^^{commit}` needs the parent itself, not only its name.
        Ok(match self.header(first)? {
            Some((Kind::Commit, _)) => Some(first),
            _ => None,
        })
    }

    /// The commit a whole or short sha names: itself, or what a tag there
    /// points at in the end. `None` where the clone holds no such commit.
    fn peeled_commit(&self, hex: &str) -> Result<Option<ObjectId>, AskGit> {
        let mut id = if hex.len() == self.repo.object_hash().len_in_hex() {
            ObjectId::from_hex(hex.as_bytes()).map_err(|_| AskGit::because("not a sha"))?
        } else {
            // git resolves a short name as a ref before it tries it as a
            // sha; a ref spelled in hex digits is git's to read.
            if self.repo.try_find_reference(hex).ok().flatten().is_some() {
                return Err(AskGit::because("a short sha that is also a ref name"));
            }
            match self
                .repo
                .rev_parse_single(format!("{hex}^{{commit}}").as_str())
            {
                Ok(id) => return Ok(Some(id.detach())),
                Err(_) => return Ok(None),
            }
        };
        // A tag is followed to what it names, as `^{commit}` does.
        for _ in 0..64 {
            match self.header(id)? {
                Some((Kind::Commit, _)) => return Ok(Some(id)),
                Some((Kind::Tag, _)) => {
                    let object = self.object(id)?;
                    let Some(target) = tag_target(&object.data) else {
                        return Ok(None);
                    };
                    id = target;
                }
                _ => return Ok(None),
            }
        }
        Ok(None)
    }

    /// An object's kind and size, `None` where the clone does not hold it.
    pub(crate) fn header(&self, id: ObjectId) -> Result<Option<(Kind, u64)>, AskGit> {
        match self.repo.try_find_header(id) {
            Ok(Some(header)) => Ok(Some((header.kind(), header.size()))),
            Ok(None) => Ok(None),
            Err(error) => Err(AskGit::because(format!("an object's header: {error}"))),
        }
    }

    /// An object the clone is known to hold.
    pub(crate) fn object(&self, id: ObjectId) -> Result<gix::Object<'_>, AskGit> {
        self.repo
            .find_object(id)
            .map_err(|error| AskGit::because(format!("an object: {error}")))
    }

    /// `git rev-parse --is-shallow-repository --show-object-format`.
    ///
    /// # Errors
    ///
    /// Never today; a `Read` as every other.
    pub fn shallow_and_format(&self) -> Read {
        Ok(Answer::Out(
            format!("{}\nsha1\n", self.shallow).into_bytes(),
        ))
    }

    /// `git ls-tree -r -t -z --long --full-tree <commit>`: every directory,
    /// file, link and submodule of the commit's tree, in git's order, each
    /// `<mode> <type> <object> <size>\t<path>\0`.
    ///
    /// # Errors
    ///
    /// [`AskGit`] where a blob's size cannot be read (a partial clone), or
    /// a tree does not parse.
    pub fn ls_tree(&self, commit: &str) -> Read {
        let Some(id) = self.commit_id(commit)? else {
            return Ok(Answer::Refused);
        };
        let Some(tree) = tree_of(&self.object(id)?.data) else {
            return Err(AskGit::because("a commit without a tree"));
        };
        let mut out = Vec::new();
        let mut path = Vec::new();
        self.list(tree, &mut path, &mut out)?;
        Ok(Answer::Out(out))
    }

    fn list(&self, tree: ObjectId, path: &mut Vec<u8>, out: &mut Vec<u8>) -> Result<(), AskGit> {
        use gix::objs::tree::EntryKind;
        use std::io::Write as _;
        let object = self.object(tree)?;
        if object.kind != Kind::Tree {
            return Err(AskGit::because("a tree entry that is no tree"));
        }
        let data = object.data.clone();
        drop(object);
        let stem = path.len();
        for entry in gix::objs::TreeRefIter::from_bytes(&data, gix::hash::Kind::Sha1) {
            let entry = entry.map_err(|_| AskGit::because("a tree that does not parse"))?;
            let kind = entry.mode.kind();
            let (mode, name): (u32, &str) = match kind {
                EntryKind::Tree => (0o040_000, "tree"),
                EntryKind::Blob => (0o100_644, "blob"),
                EntryKind::BlobExecutable => (0o100_755, "blob"),
                EntryKind::Link => (0o120_000, "blob"),
                EntryKind::Commit => (0o160_000, "commit"),
            };
            let size = match kind {
                EntryKind::Tree | EntryKind::Commit => "-".to_owned(),
                _ => match self.header(entry.oid.to_owned())? {
                    Some((_, size)) => size.to_string(),
                    None => return Err(AskGit::because("a blob this clone does not hold")),
                },
            };
            if stem > 0 {
                path.push(b'/');
            }
            path.extend_from_slice(entry.filename);
            let _ = write!(out, "{mode:06o} {name} {} {size:>7}\t", entry.oid);
            out.extend_from_slice(path);
            out.push(0);
            if kind == EntryKind::Tree {
                self.list(entry.oid.to_owned(), path, out)?;
            }
            path.truncate(stem);
        }
        Ok(())
    }

    /// `git cat-file blob <blob>`, read as the app's capped reader reads it:
    /// the first `max` bytes, and whether the read stopped there.
    ///
    /// # Errors
    ///
    /// [`AskGit`] for a name that is not a whole sha, an object that is not
    /// a blob (git peels a tag; this does not), and a blob of more than 64
    /// MiB when `max` is less, which git streams.
    pub fn cat_blob(&self, blob: &str, max: usize) -> Result<Option<(Vec<u8>, bool)>, AskGit> {
        if blob.len() != self.repo.object_hash().len_in_hex() || !is_hex(blob) {
            return Err(AskGit::because("a blob name that is not a whole sha"));
        }
        let id = ObjectId::from_hex(blob.as_bytes()).map_err(|_| AskGit::because("not a sha"))?;
        match self.header(id)? {
            None => return Ok(None),
            Some((Kind::Blob, size)) => {
                if size > WHOLE_BLOB && (max as u64) < size {
                    return Err(AskGit::because("a blob too large to decode whole"));
                }
            }
            Some(_) => return Err(AskGit::because("an object that is not a blob")),
        }
        let object = self.object(id)?;
        Ok(Some(crate::capped(&object.data, usize::MAX, max)))
    }
}

/// The header lines of a commit or a tag: each up to the blank line.
fn header_lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|b| *b == b'\n')
        .take_while(|line| !line.is_empty())
}

/// A commit's tree.
pub(crate) fn tree_of(commit: &[u8]) -> Option<ObjectId> {
    header_lines(commit)
        .find_map(|line| line.strip_prefix(b"tree "))
        .and_then(|hex| ObjectId::from_hex(hex).ok())
}

/// A commit's parents, in its order.
pub(crate) fn parents(commit: &[u8]) -> impl Iterator<Item = ObjectId> + '_ {
    header_lines(commit)
        .filter_map(|line| line.strip_prefix(b"parent "))
        .filter_map(|hex| ObjectId::from_hex(hex).ok())
}

/// What a tag points at.
fn tag_target(tag: &[u8]) -> Option<ObjectId> {
    header_lines(tag)
        .find_map(|line| line.strip_prefix(b"object "))
        .and_then(|hex| ObjectId::from_hex(hex).ok())
}
