//! Which copies replace themselves (decisions D7).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// How this copy was installed: which decides whether it can replace itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum InstallKind {
    Nsis,
    Msi,
    App,
    AppImage,
    Deb,
    Rpm,
}

impl InstallKind {
    /// The installer's word in a manifest's platform key (`windows-x86_64-nsis`).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Nsis => "nsis",
            Self::Msi => "msi",
            Self::App => "app",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
        }
    }
}

/// What the bundler patched into the binary (`tauri::utils::platform::bundle_type()`), without
/// naming Tauri here. On a Mac Tauri answers `App` for any binary, marker or not, so the path
/// decides there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleMarker {
    Deb,
    Rpm,
    AppImage,
    Msi,
    Nsis,
    App,
}

/// Why a copy is updated by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum ByHandWhy {
    /// macOS runs the app from a randomised read-only copy until it is moved.
    Translocated,
    /// A `.deb` or `.rpm`: replacing a system package needs root.
    Package,
    /// A debug build, or a binary with no bundle marker.
    Development,
}

/// Whether this copy replaces itself, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "standing", rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum Standing {
    InPlace {
        kind: InstallKind,
    },
    ByHand {
        why: ByHandWhy,
        kind: Option<InstallKind>,
    },
}

/// The operating system, for [`standing_of`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    /// The one this binary was built for.
    #[must_use]
    pub fn this() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// Section 9's table, pure over what the app reads once at start.
#[must_use]
pub fn standing_of(
    os: Os,
    marker: Option<BundleMarker>,
    exe: &Path,
    appimage: Option<&str>,
    debug: bool,
) -> Standing {
    let development = Standing::ByHand {
        why: ByHandWhy::Development,
        kind: None,
    };
    if debug {
        return development;
    }
    match os {
        Os::Windows => match marker {
            Some(BundleMarker::Nsis) => Standing::InPlace {
                kind: InstallKind::Nsis,
            },
            Some(BundleMarker::Msi) => Standing::InPlace {
                kind: InstallKind::Msi,
            },
            _ => development,
        },
        Os::Mac => {
            let path = exe.to_string_lossy();
            if path.contains("/AppTranslocation/") {
                return Standing::ByHand {
                    why: ByHandWhy::Translocated,
                    kind: Some(InstallKind::App),
                };
            }
            if app_bundle_of(exe).is_some() {
                Standing::InPlace {
                    kind: InstallKind::App,
                }
            } else {
                development
            }
        }
        Os::Linux => match marker {
            Some(BundleMarker::AppImage) if appimage.is_some_and(|path| !path.is_empty()) => {
                Standing::InPlace {
                    kind: InstallKind::AppImage,
                }
            }
            Some(BundleMarker::Deb) => Standing::ByHand {
                why: ByHandWhy::Package,
                kind: Some(InstallKind::Deb),
            },
            Some(BundleMarker::Rpm) => Standing::ByHand {
                why: ByHandWhy::Package,
                kind: Some(InstallKind::Rpm),
            },
            _ => development,
        },
    }
}

/// The `<name>.app` folder an executable at `<name>.app/Contents/MacOS/<exe>` belongs to.
#[must_use]
pub fn app_bundle_of(exe: &Path) -> Option<&Path> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n == name);
    let is_app = bundle
        .extension()
        .is_some_and(|extension| extension == "app");
    (named(macos, "MacOS") && named(contents, "Contents") && is_app).then_some(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_EXE: &str = "/Applications/knobas.app/Contents/MacOS/knobas-app";

    /// Section 9's table. Mutant: treat `Deb` as in place.
    #[test]
    fn standing_of_each_copy() {
        let in_place = |kind| Standing::InPlace { kind };
        let by_hand = |why, kind| Standing::ByHand { why, kind };
        let exe = |s: &str| Path::new(s).to_path_buf();
        for (os, marker, path, appimage, debug, want) in [
            (
                Os::Windows,
                Some(BundleMarker::Nsis),
                r"C:\k\knobas-app.exe",
                None,
                false,
                in_place(InstallKind::Nsis),
            ),
            (
                Os::Windows,
                Some(BundleMarker::Msi),
                r"C:\k\knobas-app.exe",
                None,
                false,
                in_place(InstallKind::Msi),
            ),
            (
                Os::Windows,
                None,
                r"C:\k\knobas-app.exe",
                None,
                false,
                by_hand(ByHandWhy::Development, None),
            ),
            (
                Os::Mac,
                Some(BundleMarker::App),
                APP_EXE,
                None,
                false,
                in_place(InstallKind::App),
            ),
            (
                Os::Mac,
                Some(BundleMarker::App),
                "/private/var/folders/x/AppTranslocation/1234/d/knobas.app/Contents/MacOS/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Translocated, Some(InstallKind::App)),
            ),
            (
                Os::Mac,
                Some(BundleMarker::App),
                "/Users/d/k/target/release/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Development, None),
            ),
            (
                Os::Linux,
                Some(BundleMarker::AppImage),
                "/tmp/.mount_x/usr/bin/knobas-app",
                Some("/home/d/knobas.AppImage"),
                false,
                in_place(InstallKind::AppImage),
            ),
            (
                Os::Linux,
                Some(BundleMarker::AppImage),
                "/tmp/.mount_x/usr/bin/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Development, None),
            ),
            (
                Os::Linux,
                Some(BundleMarker::Deb),
                "/usr/bin/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Package, Some(InstallKind::Deb)),
            ),
            (
                Os::Linux,
                Some(BundleMarker::Rpm),
                "/usr/bin/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Package, Some(InstallKind::Rpm)),
            ),
            (
                Os::Linux,
                None,
                "/home/d/k/target/release/knobas-app",
                None,
                false,
                by_hand(ByHandWhy::Development, None),
            ),
            (
                Os::Mac,
                Some(BundleMarker::App),
                APP_EXE,
                None,
                true,
                by_hand(ByHandWhy::Development, None),
            ),
            (
                Os::Windows,
                Some(BundleMarker::Nsis),
                r"C:\k\knobas-app.exe",
                None,
                true,
                by_hand(ByHandWhy::Development, None),
            ),
        ] {
            assert_eq!(
                standing_of(os, marker, &exe(path), appimage, debug),
                want,
                "{os:?} {marker:?} {path}"
            );
        }
    }

    #[test]
    fn the_bundle_of_an_executable_inside_one() {
        assert_eq!(
            app_bundle_of(Path::new(APP_EXE)),
            Some(Path::new("/Applications/knobas.app"))
        );
        assert_eq!(app_bundle_of(Path::new("/usr/bin/knobas-app")), None);
    }
}
