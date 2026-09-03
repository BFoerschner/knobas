//! The container entry point: every mock API on the fixed ports of the
//! interfaces doc §5, over one shared [`MockState`].
//!
//! Four flags do not earn a `clap` dependency, so they are parsed by hand.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use knobas_mockd::{MockState, admin, jira, teamcity};

const HELP: &str = "\
mockd -- knobas' faithful mocks of the source APIs it cannot self-host.

USAGE:
    mockd [OPTIONS]

OPTIONS:
    --bind <ADDR>            Interface to bind [default: 0.0.0.0]
    --admin-port <PORT>      Health + /__mock/* admin API [default: 8200]
    --jira-port <PORT>       Jira Data Center REST v2  [default: 8210]
    --teamcity-port <PORT>   TeamCity REST             [default: 8212]
    -h, --help               Print this help

ENVIRONMENT:
    MOCKD_PUBLIC_BASE_URL    Scheme and host the *outside* reaches mockd on,
                             e.g. `http://mockd`. Each API's own port is
                             appended, so `self`/`webUrl` links point at
                             `http://mockd:8210` and `http://mockd:8212`
                             rather than at the container's own address.
    MOCKD_JIRA_BASE_URL      Exact base URL for one API, port included. Wins
    MOCKD_TEAMCITY_BASE_URL  over MOCKD_PUBLIC_BASE_URL when set.

NOTE:
    mockd is DEPRECATED (ADR-0013, 2026-09-03): frozen, nothing new goes in,
    and it is deleted once the live suites assert what its tests assert. The
    real container is the witness.

    Port 8213 (Flowrun, M4) is *reserved* by interfaces doc §5 and deliberately
    not bound: that API does not exist yet. 8211, once reserved for a Confluence
    half, is unreserved: there will be none.
";

struct Args {
    bind: IpAddr,
    admin_port: u16,
    jira_port: u16,
    teamcity_port: u16,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            bind: IpAddr::from([0, 0, 0, 0]),
            admin_port: 8200,
            jira_port: 8210,
            teamcity_port: 8212,
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut out = Args::default();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        // A flag with no value is an error rather than a default: a typo in a
        // compose file must not start a server on a port nobody expects.
        let mut value = || {
            argv.next()
                .ok_or_else(|| format!("{flag} needs a value; see --help"))
        };
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--bind" => out.bind = value()?.parse().map_err(|e| format!("--bind: {e}"))?,
            "--admin-port" => {
                out.admin_port = value()?.parse().map_err(|e| format!("--admin-port: {e}"))?;
            }
            "--jira-port" => {
                out.jira_port = value()?.parse().map_err(|e| format!("--jira-port: {e}"))?;
            }
            "--teamcity-port" => {
                out.teamcity_port = value()?
                    .parse()
                    .map_err(|e| format!("--teamcity-port: {e}"))?;
            }
            other => return Err(format!("unknown flag {other:?}; see --help")),
        }
    }
    Ok(out)
}

/// What the outside world should see as this API's base URL.
fn public_base_url(api: &str, exact: &str, port: u16, bind: IpAddr) -> String {
    if let Ok(v) = std::env::var(exact).map(|v| v.trim().to_owned())
        && !v.is_empty()
    {
        return v;
    }
    if let Ok(v) = std::env::var("MOCKD_PUBLIC_BASE_URL").map(|v| v.trim().to_owned())
        && !v.is_empty()
    {
        return format!("{}:{port}", v.trim_end_matches('/'));
    }
    let _ = api;
    format!("http://{}", SocketAddr::new(bind, port))
}

async fn bind(addr: SocketAddr, app: axum::Router, what: &str) -> tokio::task::JoinHandle<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("mockd: cannot bind {what} on {addr}: {e}"));
    println!("mockd: {what} on http://{addr}");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    })
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mockd: {e}");
            std::process::exit(2);
        }
    };

    let state: Arc<MockState> = MockState::from_fixture();
    state.set_base_url(
        "jira",
        &public_base_url("jira", "MOCKD_JIRA_BASE_URL", args.jira_port, args.bind),
    );
    state.set_base_url(
        "teamcity",
        &public_base_url(
            "teamcity",
            "MOCKD_TEAMCITY_BASE_URL",
            args.teamcity_port,
            args.bind,
        ),
    );

    let mut tasks = Vec::new();
    tasks.push(
        bind(
            SocketAddr::new(args.bind, args.admin_port),
            admin::router(state.clone()),
            "admin + health",
        )
        .await,
    );
    tasks.push(
        bind(
            SocketAddr::new(args.bind, args.jira_port),
            jira::router(state.clone()),
            "jira",
        )
        .await,
    );
    tasks.push(
        bind(
            SocketAddr::new(args.bind, args.teamcity_port),
            teamcity::router(state.clone()),
            "teamcity",
        )
        .await,
    );
    println!(
        "mockd: DEPRECATED (ADR-0013) -- the real container is the witness; \
         port 8213 (flowrun, M4) is reserved, not bound; 8211 is unreserved; \
         fixture today = {}",
        knobas_source_mock::fixture().today
    );

    tokio::signal::ctrl_c().await.ok();
    println!("mockd: shutting down");
    for t in tasks {
        t.abort();
    }
}
