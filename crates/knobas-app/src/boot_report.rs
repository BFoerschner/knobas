//! What the boot screen's *Copy details* carries when the database does not
//! come up (ux28).
//!
//! The ux27 report from a Windows machine Björn has no admin rights on was
//! one line: "error communicating with database: ... (os error 10013)". No
//! log could be fetched from that machine, so the next failure has to explain
//! itself in what a person can paste into a chat: the step that failed, what
//! the data folder held (a fresh start or an adoption), every connection tried
//! with its address or socket path and the operating system's own error, the
//! Windows build, the programs an administrator would allow, and the security
//! products Windows reports.
//!
//! The facts about the machine are read only on a failure: on Windows that
//! is one `powershell` child, which no start that succeeds pays for.

use std::path::PathBuf;
use std::time::Duration;

use knobas_db::report::StartReport;

use crate::commands::app::Blocked;

/// What the failure screen says about the machine, read after the failure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineFacts {
    /// The operating system, with the Windows build where there is one:
    /// `Windows 11 Pro 25H2 build 26200.9974`.
    pub os: String,
    /// This program, the one a firewall rule or security software names.
    pub exe: Option<PathBuf>,
    /// The antivirus and firewall products Windows Security Center reports,
    /// each with whether it is on. `None` where none could be read (not
    /// Windows, Windows Server, or the query failed).
    pub security: Option<Vec<String>>,
    /// Windows Firewall's profiles (on or off, and what they do with an
    /// outbound connection), then its block rules that name knobas or
    /// PostgreSQL. Both read without admin rights.
    pub firewall: Vec<String>,
}

/// How long the failure screen waits for Windows' answer about the machine.
/// The script's lines are read as they come, so a timeout keeps the fast ones.
/// On Björn's PC: PowerShell's start about 150 ms, Security Center 35 to 46,
/// the firewall's profiles 11 to 20, and its rules naming knobas or PostgreSQL
/// 410 to 455 found through the programs' filters (1,542 to 1,613 through the
/// block rules, the other way round).
#[cfg_attr(not(windows), allow(dead_code))]
const FACTS_TIMEOUT: Duration = Duration::from_secs(8);

impl MachineFacts {
    /// What this process knows without asking the system: the program and
    /// the platform. What the failure screen shows until [`read`](Self::read)
    /// answers.
    #[must_use]
    pub fn local() -> MachineFacts {
        MachineFacts {
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            exe: std::env::current_exe().ok(),
            security: None,
            firewall: Vec::new(),
        }
    }

    /// Read the machine's facts. Never fails: what cannot be read is left
    /// out.
    pub async fn read() -> MachineFacts {
        let exe = std::env::current_exe().ok();
        let fallback = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        #[cfg(windows)]
        let facts = parse_facts(&windows_facts().await);
        #[cfg(not(windows))]
        let facts = Facts::default();
        MachineFacts {
            os: facts.os.unwrap_or(fallback),
            exe,
            security: facts.security,
            firewall: facts.firewall,
        }
    }
}

/// The PowerShell that reads the Windows build, Security Center's products
/// and the firewall, one `key=value` line each, fastest first.
/// `root/SecurityCenter2` answers a user without admin rights; Windows Server
/// has no such namespace, and the query then prints nothing.
#[cfg_attr(not(windows), allow(dead_code))]
const FACTS_SCRIPT: &str = r"$ErrorActionPreference = 'SilentlyContinue'
$v = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
'os=' + $v.ProductName + '|' + $v.DisplayVersion + '|' + $v.CurrentBuild + '|' + $v.UBR
Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object { 'av=' + $_.displayName + '|' + $_.productState }
Get-CimInstance -Namespace root/SecurityCenter2 -ClassName FirewallProduct | ForEach-Object { 'fw=' + $_.displayName + '|' + $_.productState }
Get-NetFirewallProfile | ForEach-Object { 'fwp=' + $_.Name + '|' + $_.Enabled + '|' + $_.DefaultOutboundAction }
Get-NetFirewallApplicationFilter | Where-Object { $_.Program -match 'knobas|postgres' } | ForEach-Object { $f = $_; $f | Get-NetFirewallRule | Where-Object { $_.Action -eq 'Block' -and $_.Enabled -eq 'True' } | ForEach-Object { 'fwr=' + $_.Direction + '|' + $f.Program } }
";

/// `FACTS_SCRIPT`'s output, as far as it got within [`FACTS_TIMEOUT`].
#[cfg(windows)]
async fn windows_facts() -> String {
    use base64::Engine as _;
    use tokio::io::AsyncBufReadExt as _;
    // `-EncodedCommand` takes the script as UTF-16LE in base64, so nothing in
    // it meets Windows' command-line quoting.
    let utf16: Vec<u8> = FACTS_SCRIPT
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut command = tokio::process::Command::from(knobas_db::console::command("powershell"));
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(encoded)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            tracing::warn!(%error, "could not ask Windows about this machine");
            return String::new();
        }
    };
    let Some(stdout) = child.stdout.take() else {
        return String::new();
    };
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut out = String::new();
    let deadline = tokio::time::Instant::now() + FACTS_TIMEOUT;
    loop {
        match tokio::time::timeout_at(deadline, lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                out.push_str(&line);
                out.push('\n');
            }
            Ok(Ok(None) | Err(_)) => break,
            Err(_) => {
                tracing::warn!("Windows took too long to say all about this machine");
                break;
            }
        }
    }
    // `kill_on_drop` ends one still running.
    out
}

/// What [`FACTS_SCRIPT`] said.
#[derive(Debug, Default, PartialEq, Eq)]
struct Facts {
    os: Option<String>,
    security: Option<Vec<String>>,
    firewall: Vec<String>,
}

/// `FACTS_SCRIPT`'s lines read: the operating system named from its build
/// (the registry's `ProductName` still says "Windows 10" on Windows 11), each
/// security product as `name (on)` or `name (off)`, the firewall's profiles
/// and its block rules naming knobas or PostgreSQL.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_facts(out: &str) -> Facts {
    let mut facts = Facts::default();
    let mut security = Vec::new();
    let mut profiles = Vec::new();
    for line in out.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("os=") {
            facts.os = windows_name(value);
        } else if let Some((kind, product)) = line
            .strip_prefix("av=")
            .map(|p| ("", p))
            .or_else(|| line.strip_prefix("fw=").map(|p| ("firewall ", p)))
        {
            let (name, state) = product.rsplit_once('|').unwrap_or((product, ""));
            let state = match state.trim().parse::<u32>() {
                Ok(state) if product_enabled(state) => "on",
                Ok(_) => "off",
                Err(_) => "state unknown",
            };
            security.push(format!("{kind}{} ({state})", name.trim()));
        } else if let Some(profile) = line.strip_prefix("fwp=") {
            let mut parts = profile.split('|').map(str::trim);
            let (name, enabled, outbound) = (
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
            );
            let on = if enabled.eq_ignore_ascii_case("true") {
                "on"
            } else {
                "off"
            };
            let outbound = match outbound {
                "" | "NotConfigured" => "outbound allowed".to_owned(),
                other => format!("outbound {}", other.to_lowercase()),
            };
            profiles.push(format!("{name} {on}, {outbound}"));
        } else if let Some(rule) = line.strip_prefix("fwr=") {
            let (direction, program) = rule.split_once('|').unwrap_or(("", rule));
            // A prompt's answer writes one rule for TCP and one for UDP.
            let rule = format!(
                "block rule ({}) for {}",
                direction.trim().to_lowercase(),
                program.trim()
            );
            if !facts.firewall.contains(&rule) {
                facts.firewall.push(rule);
            }
        }
    }
    if !profiles.is_empty() {
        facts
            .firewall
            .insert(0, format!("Windows Firewall: {}", profiles.join("; ")));
    }
    facts.security = (!security.is_empty()).then_some(security);
    facts
}

/// `ProductName|DisplayVersion|CurrentBuild|UBR` as a person names it:
/// `Windows 11 Pro 25H2 build 26200.9974`. Build 22000 and later is Windows
/// 11, whatever `ProductName` says.
fn windows_name(value: &str) -> Option<String> {
    let mut parts = value.split('|').map(str::trim);
    let (product, display, build, ubr) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let number = build.parse::<u32>().ok()?;
    let product = if number >= 22_000 {
        product.replacen("Windows 10", "Windows 11", 1)
    } else {
        product.to_owned()
    };
    let words: Vec<&str> = [product.as_str(), display]
        .into_iter()
        .filter(|word| !word.is_empty())
        .collect();
    let ubr = if ubr.is_empty() {
        String::new()
    } else {
        format!(".{ubr}")
    };
    Some(format!("{} build {build}{ubr}", words.join(" ")))
}

/// Whether Security Center's `productState` says the product is on: its
/// second byte is `0x10` or `0x11` when enabled, `0x00` or `0x01` when not.
fn product_enabled(state: u32) -> bool {
    (state >> 12) & 0xF == 1
}

/// The cause, as the boot screen puts it above what was tried.
const BLOCKED_CAUSE: &str = if cfg!(windows) {
    "Windows or security software on this computer blocked knobas from reaching its own database."
} else {
    "Security software on this computer blocked knobas from reaching its own database."
};

/// The help the boot screen shows when the operating system refused knobas
/// its own server for permissions: what was tried, and the programs an
/// administrator would allow. `None` for any other failure.
#[must_use]
pub fn blocked(report: &StartReport, machine: &MachineFacts) -> Option<Blocked> {
    if !report.blocked() {
        return None;
    }
    let allow = machine
        .exe
        .iter()
        .chain(report.server_exe.iter())
        .map(|path| path.display().to_string())
        .collect();
    Some(Blocked {
        cause: BLOCKED_CAUSE.to_owned(),
        tried: report.failed_routes(),
        allow,
    })
}

/// The text *Copy details* copies: short enough to paste into a chat, and
/// enough to tell where a start stopped without the machine.
#[must_use]
pub fn details(
    version: &str,
    profile: &str,
    error: &str,
    report: &StartReport,
    machine: &MachineFacts,
) -> String {
    let mut out = format!("knobas {version} · profile {profile} · {}\n", machine.os);
    out.push_str(&format!("failed at: {}\n", report.step));
    if let Some(found) = &report.found {
        let adopted = if report.adopted { ", adopted" } else { "" };
        out.push_str(&format!("found: {found}{adopted}\n"));
    }
    if let Some(dir) = &report.data_dir {
        out.push_str(&format!("data: {}\n", dir.display()));
    }
    if report.attempts.is_empty() {
        out.push_str("tried: no connection\n");
    } else {
        out.push_str("tried:\n");
        for attempt in &report.attempts {
            out.push_str(&format!("  {attempt}\n"));
        }
    }
    if let Some(exe) = &machine.exe {
        out.push_str(&format!("knobas: {}\n", exe.display()));
    }
    if let Some(server) = &report.server_exe {
        out.push_str(&format!("postgres: {}\n", server.display()));
    }
    match &machine.security {
        Some(products) => out.push_str(&format!("security: {}\n", products.join("; "))),
        None if cfg!(windows) => out.push_str("security: none reported\n"),
        None => {}
    }
    for line in &machine.firewall {
        out.push_str(&format!("{line}\n"));
    }
    out.push_str(&format!("error: {error}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_db::report::{Found, Step, WSAEACCES};

    fn blocked_report() -> StartReport {
        let mut report = StartReport {
            found: Some(Found::Stopped),
            data_dir: Some(PathBuf::from(
                r"C:\Users\b\AppData\Roaming\dev.knobas.desktop\db\data",
            )),
            server_exe: Some(PathBuf::from(
                r"C:\Users\b\.theseus\postgresql\18.6.0\bin\postgres.exe",
            )),
            ..StartReport::default()
        };
        report.reach(Step::ClientConnect);
        report.record(
            "tcp 127.0.0.1:54321",
            &Err::<(), _>(sqlx::Error::Io(std::io::Error::from_raw_os_error(
                WSAEACCES,
            ))),
        );
        report
    }

    fn machine() -> MachineFacts {
        MachineFacts {
            os: "Windows 11 Pro 23H2 build 22631.4460".into(),
            exe: Some(PathBuf::from(r"C:\Program Files\knobas\knobas-app.exe")),
            security: Some(vec![
                "Sophos Anti-Virus (on)".into(),
                "Windows Defender (off)".into(),
            ]),
            firewall: Vec::new(),
        }
    }

    /// The ux28 ask: every line a reader without the machine needs, in the
    /// order a reader asks for it, and short enough for a chat.
    #[test]
    fn the_details_name_step_folder_routes_programs_and_security() {
        let text = details(
            "3.2.9-ux28",
            "default",
            "knobas could not connect to its own database server (tcp 127.0.0.1:54321): boom",
            &blocked_report(),
            &machine(),
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "knobas 3.2.9-ux28 · profile default · Windows 11 Pro 23H2 build 22631.4460"
        );
        assert_eq!(lines[1], "failed at: knobas connecting to its server");
        assert_eq!(lines[2], "found: no postmaster.pid (fresh start)");
        assert!(lines[3].starts_with(r"data: C:\Users\b\AppData"), "{text}");
        assert_eq!(lines[4], "tried:");
        assert!(lines[5].starts_with("  tcp 127.0.0.1:54321: "), "{text}");
        assert!(lines[5].contains("10013"), "the exact OS error: {text}");
        assert_eq!(lines[6], r"knobas: C:\Program Files\knobas\knobas-app.exe");
        assert_eq!(
            lines[7],
            r"postgres: C:\Users\b\.theseus\postgresql\18.6.0\bin\postgres.exe"
        );
        assert_eq!(
            lines[8],
            "security: Sophos Anti-Virus (on); Windows Defender (off)"
        );
        assert!(
            lines[9].starts_with("error: knobas could not connect"),
            "{text}"
        );
        assert_eq!(lines.len(), 10, "{text}");
        assert!(text.len() < 1200, "pasteable: {} bytes", text.len());
    }

    /// An adoption says so, with the port the lock recorded.
    #[test]
    fn an_adoption_names_the_recorded_port() {
        let mut report = blocked_report();
        report.found = Some(Found::Lock {
            pid: Some(4242),
            port: Some(50861),
            socket_dir: Some(".".to_owned()),
            listen: Some("127.0.0.1".to_owned()),
        });
        report.adopted = true;
        let text = details("v", "demo", "e", &report, &machine());
        assert!(
            text.contains(
                "found: postmaster.pid pid 4242, port 50861, listening on 127.0.0.1, socket in ., adopted\n"
            ),
            "{text}"
        );
    }

    /// The plain-words help is for a refusal for permissions alone, and it
    /// names both programs an administrator would allow.
    #[test]
    fn the_blocked_help_names_what_was_tried_and_what_to_allow() {
        let help = blocked(&blocked_report(), &machine()).expect("blocked");
        assert_eq!(help.tried, vec!["tcp 127.0.0.1:54321".to_owned()]);
        assert_eq!(
            help.allow,
            vec![
                r"C:\Program Files\knobas\knobas-app.exe".to_owned(),
                r"C:\Users\b\.theseus\postgresql\18.6.0\bin\postgres.exe".to_owned()
            ]
        );

        let mut refused = StartReport::default();
        refused.record(
            "tcp 127.0.0.1:1",
            &Err::<(), _>(sqlx::Error::Io(
                std::io::ErrorKind::ConnectionRefused.into(),
            )),
        );
        assert_eq!(blocked(&refused, &machine()), None);
    }

    /// What Björn's PC answered (build 26200 says "Windows 10 Pro" in the
    /// registry), with a third-party product and its firewall beside it.
    #[test]
    fn windows_answers_are_read_into_lines() {
        let facts = parse_facts(
            "os=Windows 10 Pro|25H2|26200|9974\r\n\
             av=Windows Defender|393472\r\n\
             av=Sophos Anti-Virus|266240\r\n\
             fw=Sophos Firewall|266256\r\n\
             fwp=Domain|True|NotConfigured\r\n\
             fwp=Private|True|Block\r\n\
             fwp=Public|False|NotConfigured\r\n\
             fwr=Outbound|C:\\Program Files\\knobas\\knobas-app.exe\r\n\
             fwr=Outbound|C:\\Program Files\\knobas\\knobas-app.exe\r\n",
        );
        assert_eq!(
            facts.os.as_deref(),
            Some("Windows 11 Pro 25H2 build 26200.9974")
        );
        assert_eq!(
            facts.security,
            Some(vec![
                "Windows Defender (off)".to_owned(),
                "Sophos Anti-Virus (on)".to_owned(),
                "firewall Sophos Firewall (on)".to_owned(),
            ])
        );
        assert_eq!(
            facts.firewall,
            vec![
                "Windows Firewall: Domain on, outbound allowed; Private on, outbound block; \
                 Public off, outbound allowed"
                    .to_owned(),
                "block rule (outbound) for C:\\Program Files\\knobas\\knobas-app.exe".to_owned(),
            ]
        );
        assert_eq!(parse_facts("os=|||\n"), Facts::default());
        // Before Windows 11 the registry's name stands.
        assert_eq!(
            windows_name("Windows 10 Enterprise|22H2|19045|5011").as_deref(),
            Some("Windows 10 Enterprise 22H2 build 19045.5011")
        );
    }

    /// The firewall's lines go under the products, one each.
    #[test]
    fn the_details_carry_the_firewall_lines() {
        let mut facts = machine();
        facts.firewall = vec!["Windows Firewall: Domain on, outbound allowed".into()];
        let text = details("v", "default", "e", &blocked_report(), &facts);
        assert!(
            text.contains("security: Sophos Anti-Virus (on); Windows Defender (off)\nWindows Firewall: Domain on, outbound allowed\nerror: e"),
            "{text}"
        );
    }
}
