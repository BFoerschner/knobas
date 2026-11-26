//! An object's abbreviated name, as git abbreviates it in a patch's `index`
//! line and a `--raw` record: as many hex digits as the repository's size
//! asks for, and more until no other object shares them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use gix::hash::{ObjectId, Prefix};
use gix::odb::pack;

use crate::{AskGit, Repo};

/// The abbreviator of one answer: the pack indices and the loose-object
/// folders are read once, where gix's own prefix lookup lists a folder for
/// every name asked (10,000 files are 20,000 names).
pub(crate) struct Abbrev {
    /// git's automatic length for this many packed objects.
    len: usize,
    indices: Vec<pack::index::File>,
    /// Each object directory, the repository's own and its alternates.
    dirs: Vec<PathBuf>,
    /// The loose objects under each directory's two-digit folder, read when
    /// first asked.
    loose: RefCell<HashMap<(usize, u8), Vec<ObjectId>>>,
    known: RefCell<HashMap<ObjectId, String>>,
}

impl Abbrev {
    /// # Errors
    ///
    /// [`AskGit`] where `core.abbrev` is set to anything but `auto`, and
    /// where a pack index does not open.
    pub(crate) fn new(clone: &Repo) -> Result<Self, AskGit> {
        let config = clone.repo.config_snapshot();
        if let Some(set) = config.string("core.abbrev")
            && !set.eq_ignore_ascii_case(b"auto")
        {
            return Err(AskGit::because("core.abbrev"));
        }
        let count = clone
            .repo
            .objects
            .packed_object_count()
            .map_err(|error| AskGit::because(format!("the pack indices: {error}")))?;
        // git's `find_unique_abbrev_r`: half the bits it takes to count the
        // packed objects, rounded up, and never under seven.
        let bits = 64 - count.leading_zeros();
        let len = (bits.div_ceil(2) as usize).max(7);
        let store = clone.repo.objects.store_ref();
        let mut dirs = vec![store.path().to_path_buf()];
        dirs.extend(
            store
                .alternate_db_paths()
                .map_err(|error| AskGit::because(format!("the alternates: {error}")))?,
        );
        let mut indices = Vec::new();
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir.join("pack")) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e == "idx"))
                .collect();
            paths.sort();
            for path in paths {
                let index = pack::index::File::at(&path, gix::hash::Kind::Sha1)
                    .map_err(|error| AskGit::because(format!("a pack index: {error}")))?;
                indices.push(index);
            }
        }
        Ok(Self {
            len,
            indices,
            dirs,
            loose: RefCell::default(),
            known: RefCell::default(),
        })
    }

    /// `id`, abbreviated. An id the repository does not hold (the null id of
    /// an added file's old side, a submodule's commit) is abbreviated the
    /// same way: until no object here shares the digits.
    pub(crate) fn of(&self, id: &ObjectId) -> String {
        if let Some(known) = self.known.borrow().get(id) {
            return known.clone();
        }
        let hex = id.to_string();
        let mut len = self.len;
        while len < hex.len() && self.shared(id, len) {
            len += 1;
        }
        let short = hex[..len.min(hex.len())].to_owned();
        self.known.borrow_mut().insert(*id, short.clone());
        short
    }

    /// Whether an object other than `id` starts with its first `len` digits.
    fn shared(&self, id: &ObjectId, len: usize) -> bool {
        let Ok(prefix) = Prefix::new(id, len) else {
            return false;
        };
        for index in &self.indices {
            match index.lookup_prefix(prefix, None) {
                None => {}
                Some(Ok(at)) => {
                    if index.oid_at_index(at) != id.as_ref() as &gix::hash::oid {
                        return true;
                    }
                }
                Some(Err(())) => return true,
            }
        }
        let first = id.as_bytes()[0];
        for at in 0..self.dirs.len() {
            let mut loose = self.loose.borrow_mut();
            let found = loose.entry((at, first)).or_insert_with(|| {
                let dir = self.dirs[at].join(format!("{first:02x}"));
                let Ok(entries) = std::fs::read_dir(dir) else {
                    return Vec::new();
                };
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name();
                        let name = name.to_str()?;
                        ObjectId::from_hex(format!("{first:02x}{name}").as_bytes()).ok()
                    })
                    .collect()
            });
            if found
                .iter()
                .any(|other| other != id && prefix.cmp_oid(other) == std::cmp::Ordering::Equal)
            {
                return true;
            }
        }
        false
    }
}
