//! **A repository's files, as *Everything* finds them** (the everything-search
//! decisions, 2026-11-20, D6 and D8; ADR-0048).
//!
//! One tantivy index in the profile (`<profile>/files-1/`), one document a
//! file of a mirrored repository's default branch: the repository's number
//! (`sync.code_repo.no`), the path, the path's words, and, where the file's
//! text is read, the text's words and the text. The words are
//! `knobas_core::code_text`'s, which reads what was typed too. No other crate
//! names a tantivy type: the launcher asks [`FileIndex::ask`], the pane
//! [`FileIndex::in_repo`], the files pass writes through a [`FileWriter`].
//!
//! **Every method blocks**: call it on `tokio::task::spawn_blocking`. A read
//! never starts a thread of its own: the searcher's executor is tantivy's
//! single-threaded default.

mod read;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use knobas_core::code_text;
use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
};
use tantivy::tokenizer::{PreTokenizedString, Token};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

pub use code_text::Line;

/// The most word lists a typed word is read across, as a word's start, in
/// one field over every segment, before it is read whole (D8 rule 2): `test`
/// begins 179,636 of them on the measured corpus.
pub const BOUND: u32 = 4_096;

/// The writer's memory: one indexing thread with 50 MB (D6).
const WRITER_BYTES: usize = 50 << 20;

/// What went wrong with the index.
#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    /// tantivy's own error.
    #[error("the files index: {0}")]
    Index(#[from] tantivy::TantivyError),
    /// The folder.
    #[error("the files index's folder: {0}")]
    Io(#[from] std::io::Error),
    /// Another writer holds the index: one at a time.
    #[error("the files index is being written already")]
    Busy,
}

/// Which of a file's two fields a read asks (Settings › Search, D13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Reads {
    /// The file's path.
    pub names: bool,
    /// The file's text.
    pub text: bool,
}

impl Reads {
    /// Both.
    pub const BOTH: Self = Self {
        names: true,
        text: true,
    };

    /// Whether neither is asked.
    #[must_use]
    pub fn none(self) -> bool {
        !self.names && !self.text
    }
}

/// One repository the files found (D8 rule 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InRepo {
    /// `sync.code_repo.no`.
    pub no: u64,
    /// Its files the text found.
    pub by_text: u32,
    /// Its files the path found.
    pub by_name: u32,
    /// The file it is shown by: one the path found before one the text found,
    /// the shortest path first.
    pub best: FileHit,
}

/// One file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHit {
    /// Its path in the repository.
    pub path: String,
    /// Whether its path holds the words.
    pub in_name: bool,
    /// The line shown, where its text holds the words (D8 rule 9).
    pub line: Option<Line>,
}

/// What the index knows of one repository beside its number, read from the
/// database (`sync.code_repo`, `knobas.favourite`, `sync.item`), so a key
/// asks the database nothing to learn which repository a number is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Its entity id.
    pub entity_id: String,
    /// Its source.
    pub source_id: String,
    /// Whether it is a favourite.
    pub favourite: bool,
    /// When it changed last, in seconds: the newer first among equals.
    pub updated: i64,
}

/// The repositories the index knows, by number.
pub type Repos = HashMap<u64, Repo>;

/// The index's fields (D6). Pinned: a change needs a new folder name.
#[derive(Clone, Copy)]
struct Fields {
    repo: Field,
    key: Field,
    path: Field,
    plen: Field,
    ptok: Field,
    body: Field,
    text: Field,
}

/// The schema, version 1 (the folder's `-1`).
fn schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_u64_field("repo", FAST | INDEXED);
    builder.add_text_field("key", STRING);
    builder.add_text_field("path", STORED);
    builder.add_u64_field("plen", FAST);
    let words = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("raw")
            .set_index_option(IndexRecordOption::Basic)
            .set_fieldnorms(false),
    );
    builder.add_text_field("ptok", words.clone());
    builder.add_text_field("body", words);
    builder.add_text_field("text", STORED);
    builder.build()
}

fn fields(schema: &Schema) -> Option<Fields> {
    let field = |name: &str| schema.get_field(name).ok();
    Some(Fields {
        repo: field("repo")?,
        key: field("key")?,
        path: field("path")?,
        plen: field("plen")?,
        ptok: field("ptok")?,
        body: field("body")?,
        text: field("text")?,
    })
}

struct Inner {
    index: Index,
    reader: IndexReader,
    fields: Fields,
}

/// The files index. Cheap to clone: every clone is the one index.
#[derive(Clone)]
pub struct FileIndex {
    inner: Option<Arc<Inner>>,
    repos: Arc<RwLock<Arc<Repos>>>,
    bound: u32,
}

impl std::fmt::Debug for FileIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileIndex")
            .field("open", &self.inner.is_some())
            .field("bound", &self.bound)
            .finish_non_exhaustive()
    }
}

impl FileIndex {
    /// The index in `dir`, made there if there is none. A folder it cannot
    /// read, or one written with another schema, is removed and made again
    /// empty: the files pass reads every repository again (D7).
    ///
    /// # Errors
    ///
    /// [`FilesError`] if the folder cannot be made or the index opened.
    pub fn open(dir: &Path) -> Result<Self, FilesError> {
        std::fs::create_dir_all(dir)?;
        let index = match Index::open_in_dir(dir) {
            Ok(index) if index.schema() == schema() => index,
            Ok(_) | Err(_) if dir.join("meta.json").exists() => {
                tracing::warn!(dir = %dir.display(), "the files index could not be read, so it is made again");
                std::fs::remove_dir_all(dir)?;
                std::fs::create_dir_all(dir)?;
                Index::create_in_dir(dir, schema())?
            }
            _ => {
                // A folder with no index in it, or with what is not one.
                for entry in std::fs::read_dir(dir)? {
                    let path = entry?.path();
                    if path.is_dir() {
                        std::fs::remove_dir_all(&path)?;
                    } else {
                        std::fs::remove_file(&path)?;
                    }
                }
                Index::create_in_dir(dir, schema())?
            }
        };
        let fields = fields(&index.schema()).expect("the schema just checked holds every field");
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            inner: Some(Arc::new(Inner {
                index,
                reader,
                fields,
            })),
            repos: Arc::default(),
            bound: BOUND,
        })
    }

    /// An index that holds nothing and answers nothing: a searcher with it
    /// answers as one with no index at all.
    #[must_use]
    pub fn none() -> Self {
        Self {
            inner: None,
            repos: Arc::default(),
            bound: BOUND,
        }
    }

    /// The same index with D8's bound at `lists` word lists, for a test.
    #[must_use]
    pub fn with_bound(mut self, lists: u32) -> Self {
        self.bound = lists;
        self
    }

    /// Whether it is an index at all, and not [`Self::none`].
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.is_some()
    }

    /// Whether it holds any file.
    #[must_use]
    pub fn holds_files(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.reader.searcher().num_docs() > 0)
    }

    /// The files it holds of each repository: what bring-up compares with
    /// the database's count (D7).
    ///
    /// # Errors
    ///
    /// [`FilesError`] if a segment cannot be read.
    pub fn held(&self) -> Result<HashMap<u64, u32>, FilesError> {
        let Some(inner) = &self.inner else {
            return Ok(HashMap::new());
        };
        read::held(&inner.reader.searcher())
    }

    /// The repositories it knows.
    #[must_use]
    pub fn repos(&self) -> Arc<Repos> {
        Arc::clone(
            &self
                .repos
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Know these repositories from now on.
    pub fn set_repos(&self, repos: Repos) {
        *self
            .repos
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(repos);
    }

    /// The repositories whose files `typed` finds (D8), in D9's order, at
    /// most `most`: those whose files' paths hold the words first (more such
    /// files first, then more files whose text holds them, then a favourite,
    /// then the newer), then those whose text alone holds them (more files
    /// first, then a favourite, then the newer). Each with the file it is
    /// shown by and that file's line. A repository the index knows no
    /// [`Repo`] for is left out.
    #[must_use]
    pub fn ask(&self, typed: &str, reads: Reads, most: usize) -> Vec<InRepo> {
        let Some(inner) = &self.inner else {
            return Vec::new();
        };
        let asked = code_text::query(typed);
        if asked.is_empty() || reads.none() || most == 0 {
            return Vec::new();
        }
        let repos = self.repos();
        match read::ask(
            &inner.reader.searcher(),
            &inner.fields,
            &asked,
            reads,
            most,
            self.bound,
            &repos,
        ) {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%error, "the files index could not be read");
                Vec::new()
            }
        }
    }

    /// The files of repository `no` that `typed` finds, at most `most`, those
    /// whose path holds the words first, then the shortest path; and how
    /// many it finds in all (D11).
    #[must_use]
    pub fn in_repo(&self, no: u64, typed: &str, reads: Reads, most: usize) -> (Vec<FileHit>, u32) {
        let Some(inner) = &self.inner else {
            return (Vec::new(), 0);
        };
        let asked = code_text::query(typed);
        if asked.is_empty() || reads.none() {
            return (Vec::new(), 0);
        }
        match read::in_repo(
            &inner.reader.searcher(),
            &inner.fields,
            no,
            &asked,
            reads,
            most,
            self.bound,
        ) {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%error, "the files index could not be read");
                (Vec::new(), 0)
            }
        }
    }

    /// The one writer.
    ///
    /// # Errors
    ///
    /// [`FilesError::Busy`] while another writer is open; [`FilesError`] if
    /// the index cannot be written. An index of [`Self::none`] has no writer.
    pub fn writer(&self) -> Result<FileWriter, FilesError> {
        let Some(inner) = &self.inner else {
            return Err(FilesError::Busy);
        };
        let writer = match inner.index.writer_with_num_threads(1, WRITER_BYTES) {
            Ok(writer) => writer,
            Err(tantivy::TantivyError::LockFailure(..)) => return Err(FilesError::Busy),
            Err(error) => return Err(error.into()),
        };
        Ok(FileWriter {
            inner: Arc::clone(inner),
            writer,
        })
    }
}

/// The one writer of the index: what the files pass writes through (D7).
pub struct FileWriter {
    inner: Arc<Inner>,
    writer: IndexWriter,
}

/// A text's distinct words as tantivy is handed them: no tokenizer of its own.
fn words(text: &str) -> PreTokenizedString {
    let tokens = code_text::distinct_tokens(text)
        .into_iter()
        .enumerate()
        .map(|(position, text)| Token {
            offset_from: 0,
            offset_to: 0,
            position,
            text,
            position_length: 1,
        })
        .collect();
    PreTokenizedString {
        text: String::new(),
        tokens,
    }
}

fn key(no: u64, path: &str) -> String {
    format!("{no}/{path}")
}

impl FileWriter {
    /// Write the file at `path` of repository `no`, replacing what was
    /// there: found by its path, and by `text` where it is read.
    ///
    /// # Errors
    ///
    /// [`FilesError`] if the writer is gone.
    pub fn put(&mut self, no: u64, path: &str, text: Option<&str>) -> Result<(), FilesError> {
        let fields = self.inner.fields;
        self.writer
            .delete_term(Term::from_field_text(fields.key, &key(no, path)));
        let mut document = TantivyDocument::default();
        document.add_u64(fields.repo, no);
        document.add_text(fields.key, key(no, path));
        document.add_text(fields.path, path);
        document.add_u64(fields.plen, u64::try_from(path.len()).unwrap_or(u64::MAX));
        document.add_pre_tokenized_text(fields.ptok, words(path));
        if let Some(text) = text {
            document.add_pre_tokenized_text(fields.body, words(text));
            document.add_text(fields.text, text);
        }
        self.writer.add_document(document)?;
        Ok(())
    }

    /// Remove the file at `path` of repository `no`.
    pub fn remove(&mut self, no: u64, path: &str) {
        self.writer
            .delete_term(Term::from_field_text(self.inner.fields.key, &key(no, path)));
    }

    /// Remove every file of repository `no`.
    pub fn remove_repo(&mut self, no: u64) {
        self.writer
            .delete_term(Term::from_field_u64(self.inner.fields.repo, no));
    }

    /// Make what was written durable and readable: the next read finds it.
    ///
    /// # Errors
    ///
    /// [`FilesError`] if the commit or the reload fails; nothing since the
    /// last commit is then kept.
    pub fn commit(&mut self) -> Result<(), FilesError> {
        self.writer.commit()?;
        self.inner.reader.reload()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema's names and options are pinned: a change needs a new
    /// folder name (`files-<n>`), or an index written before reads wrong.
    #[test]
    fn the_schema_is_pinned() {
        let schema = schema();
        let names: Vec<&str> = schema.fields().map(|(_, entry)| entry.name()).collect();
        assert_eq!(
            names,
            ["repo", "key", "path", "plen", "ptok", "body", "text"]
        );
        let entry = |name: &str| {
            schema
                .get_field_entry(schema.get_field(name).unwrap())
                .clone()
        };
        assert!(entry("repo").is_fast() && entry("repo").is_indexed());
        assert!(entry("key").is_indexed() && !entry("key").is_stored());
        assert!(entry("path").is_stored() && !entry("path").is_indexed());
        assert!(entry("plen").is_fast() && !entry("plen").is_indexed());
        for words in ["ptok", "body"] {
            let entry = entry(words);
            assert!(
                entry.is_indexed() && !entry.is_stored() && !entry.has_fieldnorms(),
                "{words}"
            );
            let tantivy::schema::FieldType::Str(options) = entry.field_type() else {
                panic!("{words} is text");
            };
            let indexing = options.get_indexing_options().unwrap();
            assert_eq!(indexing.tokenizer(), "raw");
            assert_eq!(indexing.index_option(), IndexRecordOption::Basic);
        }
        assert!(entry("text").is_stored() && !entry("text").is_indexed());
    }
}
