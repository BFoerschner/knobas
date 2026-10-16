//! Build chains (#857, R-9.13): the configurations a configuration is joined
//! to by snapshot dependencies, in the order they run.
//!
//! TeamCity answers the configurations upstream and downstream of one in two
//! `buildTypes` listings, each carrying its own `snapshot-dependencies`.
//! [`order`] puts them in running order: a configuration comes after every
//! configuration of the set it depends on, and configurations no dependency
//! orders keep the order the server listed them in. A chain is a graph, not
//! always a line (two configurations can wait on one), and this flattening is
//! what "Step 2 of the chain" counts in.

/// One configuration of a chain, as the chain listings answer it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct ChainType {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "snapshot-dependencies", default)]
    pub snapshot_dependencies: Option<SnapshotDependencies>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
pub(crate) struct SnapshotDependencies {
    #[serde(rename = "snapshot-dependency", default)]
    pub snapshot_dependency: Vec<SnapshotDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct SnapshotDependency {
    #[serde(rename = "source-buildType")]
    pub source_build_type: Option<Ref>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct Ref {
    pub id: String,
}

impl ChainType {
    /// The ids of the configurations this one depends on.
    fn depends_on(&self) -> impl Iterator<Item = &str> {
        self.snapshot_dependencies
            .iter()
            .flat_map(|d| d.snapshot_dependency.iter())
            .filter_map(|d| d.source_build_type.as_ref())
            .map(|r| r.id.as_str())
    }
}

/// What a chain listing of `buildTypes` is asked for.
pub(crate) const CHAIN_TYPE_FIELDS: &str = concat!(
    "count,buildType(id,name,",
    "snapshot-dependencies(snapshot-dependency(source-buildType(id))))"
);

/// The configurations in running order, each once (the first listing of an
/// id wins). Fewer than two is no chain, and answers nothing.
pub(crate) fn order(types: Vec<ChainType>) -> Vec<ChainType> {
    let mut seen = std::collections::HashSet::new();
    let mut left: Vec<ChainType> = types
        .into_iter()
        .filter(|t| seen.insert(t.id.clone()))
        .collect();
    if left.len() < 2 {
        return Vec::new();
    }
    let ids: std::collections::HashSet<String> = left.iter().map(|t| t.id.clone()).collect();
    let mut placed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(left.len());
    while !left.is_empty() {
        let ready = left.iter().position(|t| {
            t.depends_on()
                .all(|d| !ids.contains(d) || placed.contains(d))
        });
        // A cycle TeamCity refuses to save; were one answered, its first
        // member goes next rather than the chain being lost.
        let next = left.remove(ready.unwrap_or(0));
        placed.insert(next.id.clone());
        out.push(next);
    }
    out
}

/// The configurations `id` depends on, itself included, however far back.
/// Measured on the seeded 2026.1.3 server (2026-10-16): `to:` names a
/// configuration's dependencies, `from:` what depends on it.
pub(crate) fn upstream_types(id: &str) -> String {
    format!("snapshotDependency:(to:(id:{id}),includeInitial:true,recursive:true)")
}

/// The configurations that depend on `id`, however far on.
pub(crate) fn downstream_types(id: &str) -> String {
    format!("snapshotDependency:(from:(id:{id}),recursive:true)")
}

/// The facets a chain's build listing widens, the sync's own (canceled,
/// failed to start, every branch, see `rest::Locator`) and every state, so a
/// step queued or running behind this run shows as one.
const EVERY_BUILD: &str = "state:any,canceled:any,failedToStart:any";

/// The builds the build `id` built on and the build itself, however far back.
pub(crate) fn upstream_builds(id: i64) -> String {
    format!(
        "snapshotDependency:(to:(id:{id}),includeInitial:true,recursive:true),\
         branch:default:any,{EVERY_BUILD},count:100"
    )
}

/// The builds that built on the build `id`, however far on.
pub(crate) fn downstream_builds(id: i64) -> String {
    format!(
        "snapshotDependency:(from:(id:{id}),recursive:true),branch:default:any,{EVERY_BUILD},count:100"
    )
}

/// The newest build of configuration `id` on the branch: by name, base64 so
/// a branch name's `(`, `)`, `,` and `:` cannot break the locator; the default
/// branch as `default:true`, because TeamCity names a logical branch `main`
/// apart from `<default>` (see the seed's `queue_build`); a build of no branch
/// under the server's default, which is the default branch.
pub(crate) fn newest_on_branch(id: &str, branch: Option<&str>, default: bool) -> String {
    use base64::Engine as _;
    let branch = match (branch, default) {
        (_, true) => "branch:(default:true),".to_owned(),
        (Some(name), false) => format!(
            "branch:(name:$base64:{}),",
            base64::engine::general_purpose::STANDARD.encode(name)
        ),
        (None, false) => String::new(),
    };
    format!("buildType:(id:{id}),{branch}{EVERY_BUILD},count:1")
}

/// Each step's build out of the run's own chain: what it built on, itself,
/// what built on it -- the newest of a configuration where it has more than
/// one. `None` for a step the chain holds no build of, which the caller looks
/// up on the branch.
pub(crate) fn pick<'b, B>(
    steps: &[ChainType],
    builds: &'b [B],
    type_of: impl Fn(&B) -> Option<&str>,
    id_of: impl Fn(&B) -> i64,
) -> Vec<Option<&'b B>> {
    steps
        .iter()
        .map(|step| {
            builds
                .iter()
                .filter(|b| type_of(b) == Some(step.id.as_str()))
                .max_by_key(|b| id_of(b))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, deps: &[&str]) -> ChainType {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": format!("{id} name"),
            "snapshot-dependencies": { "snapshot-dependency":
                deps.iter().map(|d| serde_json::json!({ "source-buildType": { "id": d } }))
                    .collect::<Vec<_>>() }
        }))
        .expect("a chain listing's shape")
    }

    fn ids(types: &[ChainType]) -> Vec<&str> {
        types.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn a_line_listed_out_of_order_comes_back_in_the_order_it_runs() {
        let got = order(vec![
            t("Deploy_Prod", &["Deploy_Staging"]),
            t("Build", &[]),
            t("Deploy_Staging", &["Docker"]),
            t("Docker", &["Build"]),
        ]);
        assert_eq!(
            ids(&got),
            ["Build", "Docker", "Deploy_Staging", "Deploy_Prod"]
        );
    }

    #[test]
    fn a_configuration_listed_by_both_listings_is_one_step() {
        let got = order(vec![
            t("Build", &[]),
            t("Tests", &["Build"]),
            t("Tests", &["Build"]),
        ]);
        assert_eq!(ids(&got), ["Build", "Tests"]);
    }

    #[test]
    fn a_configuration_alone_is_no_chain() {
        assert!(order(vec![t("Build", &[])]).is_empty());
        assert!(order(Vec::new()).is_empty());
    }

    #[test]
    fn two_that_wait_on_one_both_come_after_it_in_the_servers_order() {
        let got = order(vec![
            t("Lint", &["Build"]),
            t("Tests", &["Build"]),
            t("Build", &[]),
        ]);
        assert_eq!(ids(&got), ["Build", "Lint", "Tests"]);
    }

    #[test]
    fn a_dependency_outside_the_listings_does_not_hold_a_step_back() {
        let got = order(vec![t("Tests", &["Build", "Elsewhere"]), t("Build", &[])]);
        assert_eq!(ids(&got), ["Build", "Tests"]);
    }

    #[test]
    fn a_listing_without_dependencies_parses_as_none() {
        let it: ChainType = serde_json::from_value(serde_json::json!({ "id": "Build" })).unwrap();
        assert_eq!(it.depends_on().count(), 0);
        assert_eq!(it.name, None);
    }

    #[test]
    fn each_step_takes_the_newest_build_of_its_configuration_in_the_chain() {
        let steps = order(vec![
            t("Build", &[]),
            t("Tests", &["Build"]),
            t("Deploy", &["Tests"]),
        ]);
        let builds = [
            (10, "Build"),
            (12, "Tests"),
            (11, "Tests"),
            (3, "Elsewhere"),
        ];
        let got = pick(&steps, &builds, |b| Some(b.1), |b| b.0);
        assert_eq!(got, [Some(&(10, "Build")), Some(&(12, "Tests")), None]);
    }

    #[test]
    fn a_branch_goes_by_name_in_base64_and_the_default_branch_as_default() {
        assert_eq!(
            newest_on_branch("Tests", Some("feature/PAY-231 (x)"), false),
            "buildType:(id:Tests),branch:(name:$base64:ZmVhdHVyZS9QQVktMjMxICh4KQ==),\
             state:any,canceled:any,failedToStart:any,count:1"
        );
        assert_eq!(
            newest_on_branch("Tests", Some("main"), true),
            "buildType:(id:Tests),branch:(default:true),state:any,canceled:any,failedToStart:any,count:1"
        );
        assert_eq!(
            newest_on_branch("Tests", None, false),
            "buildType:(id:Tests),state:any,canceled:any,failedToStart:any,count:1"
        );
    }
}
