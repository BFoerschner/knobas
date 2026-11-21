//! Windows: the installer, started at the app's exit (decisions D9).
//!
//! NSIS: `/P /R /UPDATE`, passive with a progress window and no question, into the folder the
//! registry names, no shortcut made, knobas started again; `/ARGS --demo` in the demo profile. A
//! per-user install needs no administrator. MSI: `msiexec /i <msi> /passive /promptrestart
//! AUTOLAUNCHAPP=True`, the plugin's own arguments; Windows asks for an administrator.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::standing::InstallKind;

/// A program and its arguments, started detached once knobas has stopped its database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// The NSIS installer's arguments.
#[must_use]
pub fn nsis_args(demo: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["/P", "/R", "/UPDATE"]
        .into_iter()
        .map(OsString::from)
        .collect();
    if demo {
        args.push("/ARGS".into());
        args.push("--demo".into());
    }
    args
}

/// `msiexec`'s arguments for `msi`.
#[must_use]
pub fn msi_args(msi: &Path) -> Vec<OsString> {
    vec![
        "/i".into(),
        msi.as_os_str().to_owned(),
        "/passive".into(),
        "/promptrestart".into(),
        "AUTOLAUNCHAPP=True".into(),
    ]
}

/// `%SYSTEMROOT%\System32\msiexec.exe`.
#[must_use]
pub fn msiexec(system_root: Option<OsString>) -> PathBuf {
    PathBuf::from(system_root.unwrap_or_else(|| OsString::from(r"C:\Windows")))
        .join("System32")
        .join("msiexec.exe")
}

/// What starts the staged installer at `staged`, or `None` for a kind that is no Windows one.
#[must_use]
pub fn launch_for(kind: InstallKind, staged: &Path, demo: bool) -> Option<Launch> {
    match kind {
        InstallKind::Nsis => Some(Launch {
            program: staged.to_path_buf(),
            args: nsis_args(demo),
        }),
        InstallKind::Msi => Some(Launch {
            program: msiexec(std::env::var_os("SYSTEMROOT")),
            args: msi_args(staged),
        }),
        _ => None,
    }
}

/// Start `launch` detached from knobas, which is about to leave.
///
/// # Errors
///
/// The process start's.
pub fn start(launch: &Launch) -> std::io::Result<()> {
    let mut command = std::process::Command::new(&launch.program);
    command.args(&launch.args);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: not a child that dies with knobas.
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    command.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// Mutant: drop `/UPDATE`, and the first assertion fails.
    #[test]
    fn the_installers_arguments() {
        assert_eq!(text(&nsis_args(false)), ["/P", "/R", "/UPDATE"]);
        assert_eq!(
            text(&nsis_args(true)),
            ["/P", "/R", "/UPDATE", "/ARGS", "--demo"]
        );
        let msi = Path::new("staged/knobas_3.2.9-ux28_x64_en-US.msi");
        assert_eq!(
            text(&msi_args(msi)),
            [
                "/i",
                "staged/knobas_3.2.9-ux28_x64_en-US.msi",
                "/passive",
                "/promptrestart",
                "AUTOLAUNCHAPP=True"
            ]
        );
        assert_eq!(
            msiexec(Some(r"D:\Win".into())),
            PathBuf::from(r"D:\Win")
                .join("System32")
                .join("msiexec.exe")
        );
        let launch = launch_for(InstallKind::Nsis, Path::new("setup.exe"), true).unwrap();
        assert_eq!(launch.program, PathBuf::from("setup.exe"));
        assert!(launch_for(InstallKind::App, Path::new("x"), false).is_none());
    }
}
