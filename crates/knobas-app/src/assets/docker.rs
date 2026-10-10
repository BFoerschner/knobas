//! The **Docker host importer**: the containers of every engine the tree knows
//! a docker context for, as an estate file (issue #510, spec #491 stories
//! 66--68).
//!
//! `CONTEXT.md`, **Importer**: *"a producer of an estate file from a live
//! system ... **Not a source** (ADR-0015)"*. Every clause of that is structural
//! here for [`super::hcloud`]'s reasons, and one more besides: this importer
//! has **no credential at all**. It spawns the docker CLI, which reads this
//! machine's own `docker context ls` and its own `~/.ssh/config`, so there is
//! nothing for the `importer:` keychain namespace to hold and
//! [`super::Produced::TokenNeeded`] is a state it is never in.
//!
//! # What it spawns, and the rule about the argument
//!
//! `docker --context <context> ps --format json`, once per engine, through
//! [`Cli`]. ADR-0016 -- *a spawned command takes no argument from the mirror* --
//! names this importer in as many words: *"the context name it passes to the
//! docker CLI is an asset property a person typed, not a value the mirror
//! holds."* [`ENGINES`] reads `knobas.asset`, which is knobas' own table and
//! not a mirror, and the only value that reaches the argument list is that
//! property's text. There is no shell: [`std::process::Command`] takes the
//! argument vector itself, so a context named `; rm -rf /` is one argument
//! called `; rm -rf /` and docker answers *no context by that name* --
//! `the_context_is_the_only_thing_substituted_and_it_is_one_argument` below,
//! and the stub-driven half in `tests/it/assets_ipc.rs`.
//!
//! The **program** is [`PROGRAM`] and is not a setting: ADR-0016's command
//! templates are for *open in editor* and *open a terminal*, where the binary
//! is a matter of taste. Here it is the docker CLI or nothing, and a setting
//! naming another binary would be a second place for *which program runs* to
//! come from -- which is the half of ADR-0016 that is about the program rather
//! than about its arguments.
//!
//! # Why the origin key is the context and the name, and not the id
//!
//! `CONTEXT.md`, **Origin key**: *"the docker context plus the container name
//! for a container, since a container's id changes on every recreate and its
//! name does not"*. So this module never reads `docker ps`' `ID` at all -- not
//! as a key and not as a property. A property holding it would be a *change* on
//! the first import after every `docker compose up --force-recreate`, which is
//! the fact the two-part key exists to survive.
//!
//! **Both parts are properties of the container entry**, which is the
//! mechanism #508 built: [`super::Producer::origin_key`] is a list of property
//! keys and [`super::origin_key_of`] reads them from a property bag on both
//! sides. So a container carries [`NAME_KEY`] beside its name, and
//! `testenv/hetzner/estate.json` carries it too -- the route the deputy's
//! ruling of 2026-09-08 on #508 (part 2) pointed at, and the one
//! `CONTEXT.md`'s *"the property an importer sets"* already described. The
//! alternative was to teach `Producer` to read a key out of an entry's *field*
//! or its *parent's* property, which is a change to the matching rule itself
//! and a fork this ticket was told to raise rather than take.
//!
//! # What a produced container carries, and what it deliberately does not
//!
//! [`CONTEXT_KEY`] and [`NAME_KEY`], and nothing else. The third acceptance
//! criterion measures this producer against `testenv/hetzner/estate.json` --
//! the produced file must preview **all known with no changes** -- and
//! [`super::preview_import`] walks the *file's* properties, so every key
//! written here is a key that file has to carry with the value this producer
//! reads. One measurement and one reason decided it:
//!
//! * **`image` is not one CLI answer but two** -- measured on 2026-09-08
//!   against all four real contexts. `docker ps`' `Image` column is
//!   the image *reference* where the local engine has one (`gitea/gitea` on
//!   `orbstack`) and the image **id** where it does not -- `e34446c9dbf8`,
//!   `30267c7f633a`, `fe0737ba566a` on the three Hetzner engines, whose images
//!   compose pulled by digest. The estate has carried `image` by hand as
//!   `jetbrains/teamcity-server` since M4.0, so writing this column would put
//!   six containers into *would change* with a hash, on every run.
//! * **`compose_service` says nothing docker owns.** It is a label of one
//!   orchestrator, and a container that is not compose's has none; it is the
//!   estate's own note about how a container is started, which is why the file
//!   carries it and this producer does not write it. (It happens to agree with
//!   the file on the contexts spot-checked that day; that is not the reason,
//!   and it is not measured by anything, since nothing writes it.)
//!
//! An import is silent about what its file does not mention (`super`), so
//! leaving both out costs the estate nothing and keeps this producer's file to
//! the two facts the docker CLI is the authority on.
//!
//! # Running containers, and why a failed context is a refusal
//!
//! `docker ps` and not `docker ps -a`: an exited container is not a thing in
//! the estate, and reading them would put entries into the produced file that
//! match nothing. The notebook's engine holds two of them today -- `docker
//! --context orbstack ps -a` on 2026-09-08 listed `knobas-teamcity` (*Exited
//! (0) 2 days ago*) and `knobas-teamcity-agent` (*Exited (143)*), left behind
//! by the local `real-teamcity` profile -- and `testenv/hetzner/estate.json`
//! records those two names only under `docker_context: knobas-teamcity`, on
//! the Hetzner engine. Under `-a` they would be produced with the key
//! (`orbstack`, `knobas-teamcity`), match nothing, and preview as **new**, so
//! `just estate-live` is red on `-a` on its next run. In `just check` the
//! literal argument list is asserted by
//! `the_context_is_the_only_thing_substituted_and_it_is_one_argument`, which
//! is red on `-a` too. (`testenv/docker-compose.yml`'s `kuma-seed` is *not*
//! the example: `testenv/seed-kuma.sh` runs it `--rm`, so it leaves no exited
//! container behind and `-a` would never show it.)
//!
//! A context that **cannot be read** is a refusal for the whole run and not a
//! gap in the file, and that is the same sentence from the other side: a file
//! that quietly omitted one engine's containers would preview as *nothing to
//! say about them*, indistinguishable from a run where that engine holds
//! nothing. So an ssh host that is down, a context this machine does not have,
//! or a docker that will not start is [`IpcErrorCode::Unreachable`] naming the
//! context and carrying docker's own stderr.
//!
//! # What is not witnessed here
//!
//! That the docker CLI answers in the shape [`containers`] parses is witnessed
//! by a **stub executable** in `tests/it/assets_ipc.rs`, which certifies the
//! parse, the argument rule, the landing and the refusals and can never go red
//! when docker changes its output. That the four real contexts answer in it,
//! and that the `docker_context` and `container_name` values in
//! `testenv/hetzner/estate.json` are the ones the real engines hold, is
//! witnessed by `just estate-live` and by nothing in `just check` (ADR-0013).
//!
//! [`IpcErrorCode::Unreachable`]: crate::IpcErrorCode::Unreachable

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;

use sqlx::{PgPool, Row};

use crate::{IpcError, IpcErrorCode};

use super::{DOCKER_PRODUCER, NAMESPACE, Produced, PropertyValue};

/// The property naming the docker context an engine is reached through, and
/// the first part of a container's [origin key](super::Producer::origin_key).
///
/// One key on two kinds of entry, which is what makes the importer work at
/// all: it is read off the **engine** to decide what to spawn, and written
/// onto every **container** found under it so that the container carries the
/// context it was read through.
pub const CONTEXT_KEY: &str = "docker_context";

/// The property holding a container's docker name -- the second part of the
/// key.
///
/// Beside the asset's own `name` and not instead of it: the mechanism matches
/// on **properties**, and an asset's name is a column. The two are held equal
/// on the checked-in estate by `knobas-core`'s `estate_file.rs`, which is where
/// a hand edit that renamed one and not the other goes red.
pub const NAME_KEY: &str = "container_name";

/// A container's origin key: its context, then its name.
///
/// Declared here and named by [`super::PRODUCERS`], the arrangement
/// [`super::hcloud::ORIGIN_KEY`] has -- the module that *writes* a key is the
/// module that spells it, and
/// `the_origin_key_this_producer_writes_is_the_one_the_planner_matches_on`
/// below holds the two together.
pub const ORIGIN_KEY: &[&str] = &[CONTEXT_KEY, NAME_KEY];

/// The program this importer spawns. Not a setting; see the module docs.
pub const PROGRAM: &str = "docker";

/// The asset type whose [`CONTEXT_KEY`] is read.
///
/// **The filter is load-bearing and not a tidiness.** In
/// `testenv/hetzner/estate.json` the three Hetzner servers and the OrbStack VM
/// carry `docker_context` as well -- a `vm` is where the engine runs, and the
/// property is on both -- so a producer that read every asset carrying the key
/// would spawn each context twice and emit two entries with one origin key.
/// The Import refuses that file (*"describes one asset twice"*), which is a
/// refusal about the file rather than about the read that made it.
const ENGINE_TYPE_ID: &str = "container_engine";

/// The type every produced container carries.
const CONTAINER_TYPE_ID: &str = "container";

/// What the file calls the estate it describes -- the preview's heading.
const FILE_NAME: &str = "Docker hosts";

/// Every container engine, with whatever it holds under [`CONTEXT_KEY`].
///
/// **All of them, not the ones carrying the key**, because an engine with no
/// context is something this run has to *say* rather than silently pass over:
/// it is an engine in the estate whose containers are not in the answer, and
/// nothing else in the file distinguishes that from an engine holding none.
const ENGINES: &str = "select id, name, properties -> $2 as context
                         from knobas.asset
                        where type_id = $1
                        order by id";

/// One engine this run will read, with the context to read it through.
#[derive(Debug)]
struct Engine {
    /// The asset every container found under it lands in.
    asset_id: String,
    context: String,
}

/// What one engine answered: the engine, and the containers running on it.
///
/// A type rather than a `(Engine, Vec<String>)` threaded through two functions:
/// the pair travels together everywhere it appears, and `found.names` reads as
/// what it is where `found.1` reads as a position.
#[derive(Debug)]
struct Found {
    engine: Engine,
    names: Vec<String>,
}

/// The docker CLI, at the program it is spawned as.
///
/// **One constructor, three callers**, [`super::hcloud::client`]'s arrangement
/// and for its reason: `produce_estate_file` runs [`PROGRAM`], `just
/// estate-live` runs [`PROGRAM`], and the stub-driven suite runs a script of
/// its own -- so the live run exercises the spawn the command uses rather than
/// a second one spelled the same way.
#[derive(Debug)]
pub struct Cli {
    program: OsString,
}

/// The CLI as `program`.
///
/// `PROGRAM` for anything that ships; a path to a stub for a test.
pub fn cli(program: impl Into<OsString>) -> Cli {
    Cli {
        program: program.into(),
    }
}

/// The argument vector for one context, and the whole of what is substituted.
///
/// A free function so that a test can read it without a process: the claim
/// *"the context name comes from the engine asset's property and nothing else
/// is substituted"* is a claim about this list, and it is checked here as a
/// list and again through a stub that records what it was really given.
fn argv(context: &str) -> [&str; 5] {
    ["--context", context, "ps", "--format", "json"]
}

impl Cli {
    /// What `docker ps` wrote to stdout for one context.
    ///
    /// # Errors
    ///
    /// [`IpcErrorCode::Unreachable`](crate::IpcErrorCode::Unreachable) if the
    /// program could not be run or answered non-zero -- one code for both,
    /// because from this importer's side they are one fact (*this engine was
    /// not read*) and the sentence carries which it was.
    async fn ps(&self, context: &str) -> Result<String, IpcError> {
        let program = self.program.clone();
        let spawned = context.to_owned();
        // `spawn_blocking` for `knobas_secrets::spawn`'s reason: waiting on a
        // child -- an ssh context reaching a server that is asleep -- parks a
        // tokio worker for as long as it takes.
        let ran = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&program)
                .args(argv(&spawned))
                .output()
        })
        .await
        .map_err(|join| IpcError::internal(format!("the docker task failed: {join}")))?;

        let unreachable = |what: String| IpcError::new(IpcErrorCode::Unreachable, what);
        let output = ran.map_err(|error| {
            unreachable(format!(
                "the Docker importer could not run `{}`: {error}. It spawns the \
                 docker CLI rather than speaking the Engine API, so the CLI has \
                 to be on this app's PATH.",
                self.program.to_string_lossy()
            ))
        })?;
        if !output.status.success() {
            return Err(unreachable(format!(
                "`docker --context {context} ps` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        String::from_utf8(output.stdout).map_err(|error| {
            IpcError::internal(format!(
                "docker answered something that is not text: {error}"
            ))
        })
    }
}

/// One line of `docker ps --format json`.
///
/// Deliberately **not** `deny_unknown_fields`: this is another program's output
/// and it grows columns on its own schedule. `ID` is among the fields not read
/// here, and that is the module docs' point rather than an omission.
#[derive(Debug, serde::Deserialize)]
struct PsLine {
    /// Docker joins a container's names with a comma; the first is the one
    /// `docker ps` shows and the one a person types.
    #[serde(rename = "Names")]
    names: String,
}

/// The container names in one `docker ps --format json` answer, in its order.
///
/// **One JSON object per line, not an array.** That is what the docker CLI on
/// this machine writes, and there is one CLI whatever the engine --
/// `--context` moves the *engine*, not the program -- so this parse is of one
/// program's output and not of four servers'. What witnessed it against all
/// four is `just estate-live` (2026-09-08, eight containers over four engines);
/// nothing in `just check` can, because a stub writes whatever the test wrote
/// into it, and an array shape from a future CLI is the failure this paragraph
/// is the only warning about. A blank line is skipped; anything else that is
/// not an object is a refusal naming the line, because a producer that shrugged
/// at a line it could not read would answer with a file missing a container and
/// no way to tell.
///
/// # Errors
///
/// [`IpcError::internal`] for a line that is not a `docker ps` object, or a
/// container whose name is blank.
fn containers(output: &str) -> Result<Vec<String>, IpcError> {
    let mut names = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: PsLine = serde_json::from_str(line).map_err(|error| {
            IpcError::internal(format!(
                "docker answered a line that is not a container: {error}. The \
                 line was {line:?}."
            ))
        })?;
        let name = parsed
            .names
            .split(',')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if name.is_empty() {
            return Err(IpcError::internal(format!(
                "docker answered a container with no name: {line:?}. A \
                 container's name is half its origin key, so there is nothing \
                 to match this one on."
            )));
        }
        names.push(name);
    }
    Ok(names)
}

/// The engines to read, and the ones that were not read, by name.
///
/// # Errors
///
/// [`IpcError::invalid`] if an engine's [`CONTEXT_KEY`] is not text (a docker
/// context is a name, and a number is not one), or if two engines name the same
/// context -- a context is read once and its containers land under one engine,
/// and there is no answer to which of the two that is.
async fn engines(pool: &PgPool) -> Result<(Vec<Engine>, Vec<String>), IpcError> {
    let mut reading: Vec<Engine> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut by_context: HashMap<String, String> = HashMap::new();

    for row in sqlx::query(ENGINES)
        .bind(ENGINE_TYPE_ID)
        .bind(CONTEXT_KEY)
        .fetch_all(pool)
        .await?
    {
        let asset_id: String = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let stored: Option<serde_json::Value> = row.try_get("context")?;
        let Some(stored) = stored else {
            skipped.push(name);
            continue;
        };
        let value: PropertyValue = serde_json::from_value(stored).map_err(|error| {
            IpcError::internal(format!(
                "`{asset_id}` holds a `{CONTEXT_KEY}` this build cannot read: {error}"
            ))
        })?;
        let PropertyValue::Text { value: context } = value else {
            return Err(IpcError::invalid(format!(
                "`{asset_id}` carries a `{CONTEXT_KEY}` that is a {}; a docker \
                 context is a name, and `docker --context` takes nothing else.",
                value.kind().as_str()
            )));
        };
        if let Some(first) = by_context.insert(context.clone(), asset_id.clone()) {
            return Err(IpcError::invalid(format!(
                "`{first}` and `{asset_id}` both name the docker context \
                 `{context}`. A context is read once and its containers land \
                 under one engine, so there is no answer to which of the two \
                 they belong to."
            )));
        }
        reading.push(Engine { asset_id, context });
    }
    Ok((reading, skipped))
}

/// The id a produced container carries.
///
/// The two parts of the origin key, in their order, with a `/` between them:
/// `/` is legal in neither a docker context name nor a container name, so no
/// two pairs can spell one id -- which a `-` would allow (`a-b` plus `c` and
/// `a` plus `b-c`). Invented rather than looked up, and it survives nothing:
/// what matches a produced entry to the estate's own asset is the origin key,
/// and this id is what a container **not** in the estate keeps once it is
/// imported.
fn asset_id(context: &str, name: &str) -> String {
    format!("{NAMESPACE}:docker-{context}/{name}")
}

/// The estate file for `found`, with a `parent` on the entries in `new`.
fn estate_file(found: &[Found], new: &HashSet<String>) -> Result<String, IpcError> {
    let mut assets = Vec::new();
    for Found { engine, names } in found {
        for name in names {
            let id = asset_id(&engine.context, name);
            let mut entry = serde_json::Map::new();
            entry.insert("id".to_owned(), id.clone().into());
            entry.insert("type".to_owned(), CONTAINER_TYPE_ID.into());
            entry.insert("name".to_owned(), name.clone().into());
            // Only the containers the Import would create. An import never
            // re-parents what is already in the tree (`super`'s second
            // decision), so a `parent` on a matched entry would be the file
            // saying something the import is going to ignore -- and here it
            // would be saying it about a container somebody may have moved by
            // hand.
            if new.contains(&id) {
                entry.insert("parent".to_owned(), engine.asset_id.clone().into());
            }
            entry.insert(
                "properties".to_owned(),
                serde_json::json!({
                    CONTEXT_KEY: engine.context,
                    NAME_KEY: name,
                }),
            );
            assets.push(serde_json::Value::Object(entry));
        }
    }

    super::render_estate_file(FILE_NAME, assets)
}

/// One run of the Docker importer.
///
/// **No landing question and no token.** A container lands under the engine
/// whose context found it (spec #491, story 68), and that engine is in the tree
/// by construction -- it is where the context was read from -- so
/// [`Produced::LandingNeeded`] is unreachable here. So is
/// [`Produced::TokenNeeded`]: there is no credential, and `produce_estate_file`
/// refuses a token sent to this producer rather than storing one nothing would
/// ever read.
///
/// Which containers are **new** is decided by asking the Import, exactly as
/// [`super::hcloud::produce`] decides it: the file is built once with no
/// parents, previewed, and the *new* group is the answer. A second copy of the
/// id-then-origin-key rule living here is the copy that goes stale against
/// #508's.
///
/// # Errors
///
/// [`engines`]', [`Cli::ps`]', [`containers`]' and the preview's.
pub async fn produce(pool: &PgPool, cli: &Cli) -> Result<Produced, IpcError> {
    let (engines, skipped) = engines(pool).await?;

    let mut found: Vec<Found> = Vec::new();
    for engine in engines {
        let names = containers(&cli.ps(&engine.context).await?)?;
        found.push(Found { engine, names });
    }

    // Once with no parents, to ask the Import which of them it would create --
    // `hcloud::produce`'s arrangement, through the call both share.
    let draft = estate_file(&found, &HashSet::new())?;
    let new = super::new_asset_ids(pool, &draft, DOCKER_PRODUCER).await?;

    let new_servers: Vec<String> = found
        .iter()
        .flat_map(|Found { engine, names }| {
            names
                .iter()
                .filter(|name| new.contains(&asset_id(&engine.context, name)))
                .cloned()
        })
        .collect();

    Ok(Produced::Ready {
        file: estate_file(&found, &new)?,
        new_servers,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(asset_id: &str, context: &str) -> Engine {
        Engine {
            asset_id: asset_id.to_owned(),
            context: context.to_owned(),
        }
    }

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("the produced file is JSON")
    }

    /// The origin key this module writes is the one the planner matches on.
    ///
    /// Two constants in two modules, and a file whose keys are spelled
    /// differently from the producer's declaration matches **nothing** -- every
    /// container would preview as new and the import would make a second copy
    /// of every one of them. Nothing else in the tree reads both.
    #[test]
    fn the_origin_key_this_producer_writes_is_the_one_the_planner_matches_on() {
        let declared = crate::assets::PRODUCERS
            .iter()
            .find(|producer| producer.id == DOCKER_PRODUCER)
            .expect("the docker producer is declared");
        assert_eq!(declared.origin_key, ORIGIN_KEY);
        assert_eq!(ORIGIN_KEY, [CONTEXT_KEY, NAME_KEY]);
    }

    /// **The context is the only thing substituted, and it is one argument.**
    ///
    /// ADR-0016's rule at the seam it applies to: the argument list is a
    /// constant with one hole in it, and what goes in the hole is the engine
    /// asset's property. A context carrying shell metacharacters stays one
    /// element -- there is no shell between this list and the program -- so the
    /// worst a mistyped property can do is name a context docker does not have.
    #[test]
    fn the_context_is_the_only_thing_substituted_and_it_is_one_argument() {
        assert_eq!(
            argv("knobas-jira"),
            ["--context", "knobas-jira", "ps", "--format", "json"]
        );
        let hostile = "; rm -rf / #";
        let built = argv(hostile);
        assert_eq!(built[1], hostile, "the property reaches docker verbatim");
        assert_eq!(
            built.iter().filter(|part| part.contains("rm -rf")).count(),
            1,
            "the context is one argument and is not spliced into another"
        );
        // And nothing else in the list moved: `ps` is a read, `--format json`
        // is the shape this module parses, and neither is composed from
        // anything.
        assert_eq!(&built[2..], &["ps", "--format", "json"]);
    }

    /// One object per line, the CLI's own shape, with the id ignored.
    #[test]
    fn every_line_is_one_container_and_the_id_is_read_by_nothing() {
        let output = "{\"ID\":\"3cb4f18ace38\",\"Names\":\"knobas-gitea\",\"Image\":\"gitea/gitea\"}\n\
                      \n\
                      {\"ID\":\"215ec0488ae7\",\"Names\":\"knobas-uptime-kuma\",\"State\":\"running\"}\n";
        assert_eq!(
            containers(output).expect("two containers"),
            ["knobas-gitea", "knobas-uptime-kuma"]
        );
        // The same two containers after a recreate: new ids, same names, and
        // the parse cannot tell -- which is the whole of why the key is the
        // name (`CONTEXT.md`, Origin key).
        let recreated = "{\"ID\":\"ffffffffffff\",\"Names\":\"knobas-gitea\"}\n\
                         {\"ID\":\"eeeeeeeeeeee\",\"Names\":\"knobas-uptime-kuma\"}\n";
        assert_eq!(
            containers(recreated).expect("two containers"),
            containers(output).expect("two containers")
        );
    }

    /// A container with several names is known by the first, and a line that is
    /// not a container is a refusal rather than a silently shorter file.
    #[test]
    fn the_first_name_is_the_containers_and_an_unreadable_line_is_refused() {
        assert_eq!(
            containers("{\"Names\":\"knobas-jira,jira\"}").expect("one container"),
            ["knobas-jira"]
        );
        for bad in [
            "not json at all",
            "{\"Names\":\"\"}",
            "{\"Names\":\"  \"}",
            "7",
        ] {
            let refused = containers(bad).expect_err("a line that is not a container");
            assert_eq!(refused.code, IpcErrorCode::Internal);
        }
    }

    /// **Only a new container is given a parent**, and the parent is the engine
    /// whose context found it -- which is what makes Docker's *land under*
    /// question not exist (spec #491, story 68).
    #[test]
    fn a_container_lands_under_the_engine_whose_context_found_it() {
        let found = vec![
            Found {
                engine: engine("asset:orbstack-docker", "orbstack"),
                names: vec!["knobas-gitea".to_owned()],
            },
            Found {
                engine: engine("asset:hetzner-jira-docker", "knobas-jira"),
                names: vec!["knobas-jira".to_owned(), "knobas-jira-db".to_owned()],
            },
        ];
        let new: HashSet<String> = [asset_id("knobas-jira", "knobas-jira-db")]
            .into_iter()
            .collect();
        let file = parsed(&estate_file(&found, &new).expect("a file"));
        let entries = file["assets"].as_array().expect("a list");
        assert_eq!(entries.len(), 3);

        assert_eq!(
            entries[0]["id"],
            serde_json::json!("asset:docker-orbstack/knobas-gitea")
        );
        assert_eq!(entries[0]["type"], serde_json::json!("container"));
        assert_eq!(entries[0].get("parent"), None, "already in the tree");
        assert_eq!(
            entries[0]["properties"],
            serde_json::json!({ CONTEXT_KEY: "orbstack", NAME_KEY: "knobas-gitea" }),
            "both halves of the origin key, and nothing the estate would have to agree with"
        );

        assert_eq!(
            entries[2]["parent"],
            serde_json::json!("asset:hetzner-jira-docker"),
            "the new one lands under the engine it was read through"
        );
    }

    /// The id cannot be spelled by two different pairs.
    ///
    /// `-` between the two halves would let `("a-b", "c")` and `("a", "b-c")`
    /// invent one id for two containers, which the Import refuses as a file
    /// describing one asset twice -- a refusal about the file rather than about
    /// this function.
    #[test]
    fn no_two_context_and_name_pairs_spell_one_id() {
        assert_ne!(asset_id("a-b", "c"), asset_id("a", "b-c"));
        assert_eq!(
            asset_id("orbstack", "knobas-gitea"),
            "asset:docker-orbstack/knobas-gitea"
        );
    }
}
