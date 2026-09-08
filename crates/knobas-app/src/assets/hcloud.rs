//! The **hcloud importer**: every server one Hetzner Cloud token can see, as an
//! estate file (issue #509, spec #491 stories 63--64).
//!
//! `CONTEXT.md`, **Importer**: *"a producer of an estate file from a live
//! system ... offered by the Import's chooser and previewed and applied by the
//! same Import. **Not a source** (ADR-0015): it mirrors nothing, syncs
//! nothing, and appears nowhere sources do; its credential is its own, under
//! the `importer:` keychain namespace."* Every one of those clauses is
//! structural here: this module implements no [`Source`] trait, registers no
//! adapter, writes no `knobas.source_config` row, queues no
//! [`WriteOp`](knobas_source::WriteOp), and reaches the keychain through
//! [`KeychainAccount::importer`](knobas_secrets::KeychainAccount::importer).
//! What it returns is *text* -- an estate file in the checked-in shape -- and
//! [`super::preview_import`] and [`super::apply_import`] do the rest (#508).
//!
//! # Which properties a server becomes, and why those spellings
//!
//! The ticket names what is read: *"name, server type, image, location, IPv4,
//! labels"*. What each becomes is decided by the estate this repository is
//! developed against, `testenv/hetzner/estate.json`, which has carried those
//! facts by hand since M4.0 and is the record the third acceptance criterion
//! measures this producer against -- so the keys are **its** keys and not new
//! ones beside them:
//!
//! | hcloud | property | why that key |
//! |---|---|---|
//! | `id` | [`ORIGIN_KEY`] (`hcloud_id`) | the origin key #508 declared, and the three values it wrote |
//! | `name` | the asset's **name** | not a property: an asset has a name column |
//! | `server_type.name` | `server_type` | the file's own key, chosen over the `vm` type's generic `size` |
//! | `image.name` | `os` | the `vm` type *declares* `os`, and `estate.json` already carries the image name there (`"ubuntu-24.04"`); a second custom `image` beside it would be two keys holding one string, and the pane would draw the fact twice |
//! | `location.name` | `location` | the ticket's *"location kept as a property"* |
//! | `public_net.ipv4.ip` | `ip` | declared by the `vm` type, and the file's key |
//! | `labels` | one property per label, under the label's own key | see below |
//!
//! **Labels keep their own keys.** The alternative -- a prefix, `label_role` --
//! would put `role: teamcity` (which the estate has carried by hand since M4.0)
//! beside a `label_role: teamcity` saying the same thing, and a reader opening
//! the pane would have to know which of the two hcloud wrote. Bare keys mean a
//! label can collide with one of the five above, and a collision is
//! [refused by name](label_collision) rather than resolved: there is no honest
//! answer to which `location` the reader meant. That is `DESCRIPTION_KEY`'s
//! rule in [`super`], applied one level out.
//!
//! # What decides that a server is *new*
//!
//! **The Import, asked.** [`produce`] builds the file once with no parents at
//! all, previews it, and reads the *new* group -- so what the producer calls a
//! new server is what the import would create, by construction, rather than by
//! a second copy of the id-then-origin-key rule living here and going stale
//! against #508's. The cost is one extra preview per run, which writes nothing.
//!
//! Only those entries get a `parent`. An entry the estate already holds is
//! **not** given one, because an import never re-parents what is already in the
//! tree ([`super`]'s second decision) and a file that named a parent for it
//! would be saying something the import is going to ignore.
//!
//! # What is not witnessed here
//!
//! The shape of hcloud's answer is witnessed by a recorded response
//! (`tests/assets_ipc.rs`); that the real API answers in that shape, under a
//! real token, at the real endpoint, is witnessed by `just estate-live` and by
//! nothing in `just check` (ADR-0013). The recorded half cannot fail if
//! Hetzner changes its JSON, and says so rather than implying otherwise.
//!
//! [`Source`]: knobas_source::Source

use std::collections::{BTreeMap, HashSet};

use knobas_http::{HttpClient, Method};
use sqlx::PgPool;

use crate::IpcError;

use super::{FILE_VERSION, HCLOUD_PRODUCER, Landing, NAMESPACE, Produced};

/// Hetzner Cloud's API, versioned as its own documentation versions it.
///
/// A constant and not a setting: hcloud is one instance in the world, unlike
/// every [source](knobas_source), which is why an importer has a token and no
/// base URL. Tests reach the seam one level down ([`servers`] takes a built
/// client), so nothing overrides this at run time and there is no environment
/// variable a mistyped export could point at a machine of somebody's own.
pub const API: &str = "https://api.hetzner.cloud/v1";

/// The property whose value is a server's [origin key](super::HCLOUD_PRODUCER).
///
/// Declared in [`super::PRODUCERS`] by #508 and spelled here as well because
/// this is the module that *writes* it; `the_origin_key_this_producer_writes_is_the_one_the_planner_matches_on`
/// holds the two to each other, so the pair cannot drift into a producer whose
/// files nothing can match.
pub const ORIGIN_KEY: &str = "hcloud_id";

/// The type every produced server carries.
///
/// `vm` and not `hypervisor`: an hcloud server is a virtual machine, which is
/// what the three in `testenv/hetzner/estate.json` are typed as, and a produced
/// entry that disagreed with them would be a *change* on every one of the three
/// rather than a match.
const SERVER_TYPE_ID: &str = "vm";

/// The keys this producer writes itself, each beside what it writes into it.
///
/// **Pairs and not a list**, so that [`label_collision`] reads the description
/// out of this table instead of matching on the key with a wildcard arm. A
/// wildcard would describe a key it has never seen -- a sixth key added here
/// and forgotten there would be refused as *"the IPv4 address"* -- which is
/// ADR-0006's reason for refusing a wildcard `WriteOp` arm, one level down.
/// Adding a key here now carries its own sentence with it.
const OWN_KEYS: &[(&str, &str)] = &[
    (ORIGIN_KEY, "hcloud's id into"),
    ("server_type", "the server type into"),
    ("os", "the image name into"),
    ("location", "the location into"),
    ("ip", "the IPv4 address into"),
];

/// How many servers one page of the API asks for. Hetzner's own maximum is 50.
const PAGE: u32 = 50;

/// One server, at the width #509 asks for and no wider.
///
/// Deliberately **not** `deny_unknown_fields`: this is a foreign API's answer
/// and it grows fields on its own schedule, so a new one must not turn a
/// working importer into a refusal. The estate file this builds *is*
/// `deny_unknown_fields` on the way back in, which is where a closed vocabulary
/// belongs -- that file is knobas' own.
#[derive(Debug, serde::Deserialize)]
struct Server {
    /// hcloud's own id. A number on the wire; the [origin key](ORIGIN_KEY) it
    /// becomes is its decimal spelling as **text**, which is the shape #508
    /// wrote into `testenv/hetzner/estate.json` and the shape
    /// [`super::property_of`] has to see for the stored values to compare
    /// equal (a number and a string that looks like one are two different
    /// origin keys).
    id: i64,
    name: String,
    /// Absent for a server built from a snapshot Hetzner has since removed.
    #[serde(default)]
    image: Option<Named>,
    server_type: Named,
    location: Named,
    public_net: PublicNet,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

/// The one field this module wants from four of hcloud's nested objects.
#[derive(Debug, serde::Deserialize)]
struct Named {
    name: String,
}

#[derive(Debug, serde::Deserialize)]
struct PublicNet {
    /// `null` for a server with no public IPv4 -- an IPv6-only server is a
    /// thing hcloud sells, and it has no `ip` property rather than an empty
    /// one.
    #[serde(default)]
    ipv4: Option<Ipv4>,
}

#[derive(Debug, serde::Deserialize)]
struct Ipv4 {
    ip: String,
}

#[derive(Debug, serde::Deserialize)]
struct Page {
    servers: Vec<Server>,
    #[serde(default)]
    meta: Option<Meta>,
}

#[derive(Debug, serde::Deserialize)]
struct Meta {
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Debug, serde::Deserialize)]
struct Pagination {
    /// `null` on the last page, which is what ends the walk. A page count is
    /// deliberately not consulted: the cursor the server hands back is the
    /// server's own answer about whether there is more.
    #[serde(default)]
    next_page: Option<u32>,
}

/// The refusal a label that shadows one of [`OWN_KEYS`] gets.
///
/// `writes` is that key's own sentence out of the table, so this function
/// describes only keys it has been told about.
fn label_collision(server: &str, key: &str, writes: &str) -> IpcError {
    IpcError::invalid(format!(
        "the server {server:?} carries a label called `{key}`, which is also \
         what this importer writes {writes}. An asset property has one value, \
         and there is no answer to which of the two a reader meant -- rename \
         the label in Hetzner Cloud, or drop it."
    ))
}

/// The client this importer talks to hcloud through.
///
/// **One constructor, three callers, and that is the point.** The command
/// applies [`API`], `just estate-live` applies [`API`], and the recorded-shape
/// suite applies a wiremock URL -- so the live run exercises the client the
/// command uses rather than a second one spelled the same way. It was two
/// spellings before the deputy's ruling of 2026-09-08 on #509 (part 4c), and
/// the live suite's own comment said *"a second spelling here would be a suite
/// certifying a client the command does not use"* -- which was a second
/// spelling saying it was not one, the prose-outruns-the-code class in a test
/// header.
///
/// `base_url` is an argument and **not** a setting: hcloud is one instance in
/// the world, which is why an importer has a token and no base URL, and the
/// only caller that passes anything else is a test pointing at a server in its
/// own process. There is no environment variable a mistyped export could point
/// at a machine of somebody's own.
///
/// The `adapter_kind` reaches the `User-Agent` and nothing else
/// (`knobas/<v> (hcloud/<v>)`), so an admin reading an access log can tell what
/// called them. It is not an adapter kind in the [registry](crate::sources)'s
/// sense: no adapter is registered for this and nothing builds a
/// [`SourceInstance`](knobas_source::instance::SourceInstance) from it.
///
/// # Errors
///
/// [`IpcError`] if the base URL is not http(s) or the TLS backend cannot be
/// built -- carrying **no source id**, for [`servers`]' reason.
pub fn client(base_url: &str, token: &str) -> Result<HttpClient, IpcError> {
    HttpClient::new(knobas_http::HttpConfig {
        base_url: base_url.to_owned(),
        adapter_kind: HCLOUD_PRODUCER.to_owned(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        auth: knobas_http::Auth::Bearer(token.to_owned()),
        ..Default::default()
    })
    .map_err(|error| IpcError::from_source_error(&error, None))
}

/// Every server one token can see, oldest page first.
///
/// # Errors
///
/// Whatever [`HttpClient::send`] classified -- an expired token is
/// [`Unauthorized`](crate::IpcErrorCode::Unauthorized) and reads as *the
/// credential was refused*, which is the one fault a person can act on
/// (ADR-0004).
async fn servers(client: &HttpClient) -> Result<Vec<Server>, IpcError> {
    let mut all = Vec::new();
    let mut page = 1_u32;
    loop {
        let request = client
            .request(Method::GET, "/servers")
            .query(&[("page", page.to_string()), ("per_page", PAGE.to_string())][..]);
        let answer: Page = client
            .send(request)
            .await
            // **No source id on the error.** `IpcError::source_id` is what the
            // shell hangs a *re-enter this source's credential* affordance off,
            // and an importer is not a source (ADR-0015): there is no row to
            // open and no sources view to send anybody to. The refusal reads as
            // itself.
            .map_err(|error| IpcError::from_source_error(&error, None))?
            .json()
            .await
            .map_err(|error| {
                IpcError::internal(format!(
                    "hcloud answered something that is not a server list: {error}"
                ))
            })?;
        all.extend(answer.servers);
        match answer
            .meta
            .and_then(|meta| meta.pagination)
            .and_then(|p| p.next_page)
        {
            Some(next) => page = next,
            None => return Ok(all),
        }
    }
}

/// The estate file for `servers`, with a `parent` on the ones in `new`.
///
/// # Errors
///
/// [`IpcError::invalid`] for a label whose key is one this producer writes
/// itself ([`label_collision`]).
fn estate_file(
    servers: &[Server],
    new: &HashSet<String>,
    land_under: Option<&str>,
) -> Result<String, IpcError> {
    let mut assets = Vec::new();
    for server in servers {
        let id = asset_id(server.id);
        let mut properties = serde_json::Map::new();
        properties.insert(ORIGIN_KEY.to_owned(), server.id.to_string().into());
        properties.insert(
            "server_type".to_owned(),
            server.server_type.name.clone().into(),
        );
        if let Some(image) = &server.image {
            properties.insert("os".to_owned(), image.name.clone().into());
        }
        properties.insert("location".to_owned(), server.location.name.clone().into());
        if let Some(ipv4) = &server.public_net.ipv4 {
            properties.insert("ip".to_owned(), ipv4.ip.clone().into());
        }
        for (key, value) in &server.labels {
            if let Some((_, writes)) = OWN_KEYS.iter().find(|(own, _)| own == key) {
                return Err(label_collision(&server.name, key, writes));
            }
            properties.insert(key.clone(), value.clone().into());
        }

        let mut entry = serde_json::Map::new();
        entry.insert("id".to_owned(), id.clone().into());
        entry.insert("type".to_owned(), SERVER_TYPE_ID.into());
        entry.insert("name".to_owned(), server.name.clone().into());
        // Only the entries the import would create; see the module docs.
        if new.contains(&id)
            && let Some(parent) = land_under
        {
            entry.insert("parent".to_owned(), parent.into());
        }
        entry.insert(
            "properties".to_owned(),
            serde_json::Value::Object(properties),
        );
        assets.push(serde_json::Value::Object(entry));
    }

    let file = serde_json::json!({
        "version": FILE_VERSION,
        "name": "Hetzner Cloud",
        "assets": assets,
        "routes": [],
    });
    serde_json::to_string_pretty(&file)
        .map_err(|error| IpcError::internal(format!("rendering the estate file: {error}")))
}

/// The id a produced entry carries.
///
/// hcloud's **id** and not its name: a server renamed in Hetzner keeps its id,
/// and an id derived from the name would make the first produce after a rename
/// describe an asset that is not there. The origin key would still match it, so
/// nothing would break -- but the id is what a *new* server keeps forever, and
/// the one that is stable is the one to keep.
fn asset_id(server: i64) -> String {
    format!("{NAMESPACE}:hcloud-{server}")
}

/// One run of the hcloud importer.
///
/// **A `land_under` of `Some(Landing { parent: None })` is an answer**, not a
/// missing one: it says *the top of the estate*, and the draft built above --
/// which gives no entry a `parent` -- is already the file that says so. The
/// spelling that means *nothing has been said yet* is the argument being
/// `None`, and only that (#509, second ruling of 2026-09-08).
///
/// # Errors
///
/// The read's ([`servers`]), the preview's, and [`IpcError::invalid`] for a
/// label collision.
pub async fn produce(
    pool: &PgPool,
    client: &HttpClient,
    land_under: Option<&Landing>,
) -> Result<Produced, IpcError> {
    let servers = servers(client).await?;

    // Once with no parents, to ask the Import which of them it would create.
    let draft = estate_file(&servers, &HashSet::new(), None)?;
    let preview = super::preview_import(pool, &draft, HCLOUD_PRODUCER).await?;
    let new: HashSet<String> = preview
        .new
        .iter()
        .filter(|entry| entry.kind == NAMESPACE)
        .map(|entry| entry.id.clone())
        .collect();

    let named: Vec<String> = servers
        .iter()
        .filter(|server| new.contains(&asset_id(server.id)))
        .map(|server| server.name.clone())
        .collect();

    if named.is_empty() {
        // Every server is already in the tree, so there is nothing to ask and
        // the draft *is* the answer -- the state `just estate-live` asserts.
        return Ok(Produced::Ready {
            file: draft,
            new_assets: Vec::new(),
            // hcloud never skips: one token either sees a server or does not
            // know it exists, so there is nothing this run knew of and could
            // not read.
            skipped: Vec::new(),
        });
    }
    let Some(landing) = land_under else {
        return Ok(Produced::LandingNeeded { assets: named });
    };
    // `landing.parent` is `None` for the top of the estate, and `estate_file`
    // then writes no `parent` key at all -- which is what the Import reads as a
    // top-level asset (`FileAsset::parent`). Nothing branches on the top here:
    // the answer *is* the draft's shape, which is why this is one call and not
    // two paths.
    Ok(Produced::Ready {
        file: estate_file(&servers, &new, landing.parent.as_deref())?,
        new_assets: named,
        skipped: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: i64, name: &str, labels: &[(&str, &str)]) -> Server {
        Server {
            id,
            name: name.to_owned(),
            image: Some(Named {
                name: "ubuntu-24.04".to_owned(),
            }),
            server_type: Named {
                name: "cx23".to_owned(),
            },
            location: Named {
                name: "nbg1".to_owned(),
            },
            public_net: PublicNet {
                ipv4: Some(Ipv4 {
                    ip: "203.0.113.7".to_owned(),
                }),
            },
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("the produced file is JSON")
    }

    /// The origin key this module writes is the one the planner matches on.
    ///
    /// Two constants in two modules, and a file whose key is spelled
    /// differently from the producer's declaration matches **nothing** --
    /// every server would preview as new and the import would make a second
    /// copy of the estate. Nothing else in the tree reads both.
    #[test]
    fn the_origin_key_this_producer_writes_is_the_one_the_planner_matches_on() {
        let declared = crate::assets::PRODUCERS
            .iter()
            .find(|producer| producer.id == HCLOUD_PRODUCER)
            .expect("the hcloud producer is declared");
        assert_eq!(declared.origin_key, [ORIGIN_KEY]);
    }

    /// The id is text, because the estate stores it as text.
    #[test]
    fn the_id_is_written_as_text_and_the_entry_is_named_after_it() {
        let file = estate_file(
            &[server(164_750_187, "knobas-teamcity", &[])],
            &HashSet::new(),
            None,
        )
        .expect("a file");
        let entry = &parsed(&file)["assets"][0];
        assert_eq!(entry["id"], serde_json::json!("asset:hcloud-164750187"));
        assert_eq!(entry["type"], serde_json::json!("vm"));
        assert_eq!(entry["name"], serde_json::json!("knobas-teamcity"));
        assert_eq!(
            entry["properties"][ORIGIN_KEY],
            serde_json::json!("164750187"),
            "a number here is a different origin key from the text the estate stores"
        );
    }

    /// Labels arrive under their own keys, and one that shadows a key this
    /// producer writes is refused by name rather than resolved.
    #[test]
    fn a_label_keeps_its_own_key_and_one_that_shadows_a_written_key_is_refused() {
        let file = estate_file(
            &[server(
                1,
                "box",
                &[("knobas", "testenv"), ("role", "teamcity")],
            )],
            &HashSet::new(),
            None,
        )
        .expect("a file");
        let properties = &parsed(&file)["assets"][0]["properties"];
        assert_eq!(properties["knobas"], serde_json::json!("testenv"));
        assert_eq!(properties["role"], serde_json::json!("teamcity"));

        for (key, writes) in OWN_KEYS {
            let refused = estate_file(
                &[server(1, "box", &[(key, "whatever")])],
                &HashSet::new(),
                None,
            )
            .expect_err("a label that shadows a written key");
            assert_eq!(refused.code, crate::IpcErrorCode::Invalid);
            assert!(
                refused.message.contains(key) && refused.message.contains("box"),
                "the refusal names the label and the server: {}",
                refused.message
            );
            // And what it says the key is **for**, read out of the table
            // rather than matched on: a sixth key described as the fifth one
            // is the wildcard arm this table replaced.
            assert!(
                refused.message.contains(writes),
                "the refusal describes `{key}` as something other than {writes:?}: {}",
                refused.message
            );
        }
    }

    /// **Only a new entry is given a parent.** An import never re-parents what
    /// is already in the tree, so a `parent` on a matched entry would be the
    /// file saying something the import is going to ignore.
    #[test]
    fn a_parent_is_written_only_for_the_entries_the_import_would_create() {
        let servers = [server(1, "new-box", &[]), server(2, "known-box", &[])];
        let new: HashSet<String> = [asset_id(1)].into_iter().collect();
        let file = parsed(&estate_file(&servers, &new, Some("asset:site")).expect("a file"));
        assert_eq!(file["assets"][0]["parent"], serde_json::json!("asset:site"));
        assert_eq!(
            file["assets"][1].get("parent"),
            None,
            "the entry the estate already holds names no parent"
        );
    }

    /// A server with no public IPv4 and no image has neither property, rather
    /// than an empty one -- an empty string would be a *change* on every
    /// import against an estate that has the real value.
    #[test]
    fn a_server_without_an_address_or_an_image_carries_neither_property() {
        let mut bare = server(3, "v6-only", &[]);
        bare.image = None;
        bare.public_net.ipv4 = None;
        let file = parsed(&estate_file(&[bare], &HashSet::new(), None).expect("a file"));
        let properties = &file["assets"][0]["properties"];
        assert_eq!(properties.get("ip"), None);
        assert_eq!(properties.get("os"), None);
        assert_eq!(properties["location"], serde_json::json!("nbg1"));
    }

    /// The walk follows hcloud's own cursor, and a page that names no next
    /// page ends it.
    #[test]
    fn the_page_shape_carries_the_cursor_the_walk_follows() {
        let last: Page = serde_json::from_str(
            r#"{"servers":[],"meta":{"pagination":{"page":1,"next_page":null}}}"#,
        )
        .expect("a page");
        assert!(last.meta.unwrap().pagination.unwrap().next_page.is_none());
        let more: Page = serde_json::from_str(
            r#"{"servers":[],"meta":{"pagination":{"page":1,"next_page":2}}}"#,
        )
        .expect("a page");
        assert_eq!(more.meta.unwrap().pagination.unwrap().next_page, Some(2));
    }
}
