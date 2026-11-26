//! Firefox Native Messaging host manifest installer for macOS & Windows (ADR-0061).
//!
//! - **macOS**: `~/Library/Application Support/Mozilla/NativeMessagingHosts/dev.knobas.desktop.json`
//! - **Windows**: manifest at `%APPDATA%\dev.knobas.desktop\native-messaging\dev.knobas.desktop.json`
//!   plus HKCU registry key `Software\Mozilla\NativeMessagingHosts\dev.knobas.desktop` whose default
//!   `REG_SZ` value points to that manifest file.
//! - **Linux**: `~/.mozilla/native-messaging-hosts/dev.knobas.desktop.json`.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::link_handoff::protocol::{EXTENSION_ID, HOST_NAME};

/// Manifest installation state shown in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "app.ts")]
pub struct NativeHostStatus {
    /// Whether the manifest (and Windows registry key) is currently installed.
    pub installed: bool,
    /// Whether this OS supports automatic Firefox Native Messaging registration.
    pub supported: bool,
    /// Registered Native Messaging host identifier (`dev.knobas.desktop`).
    pub host_name: String,
    /// Allowed Firefox WebExtension add-on ID (`link-handoff@knobas.dev`).
    pub extension_id: String,
    /// Absolute path of the Native Messaging manifest JSON file.
    pub manifest_path: Option<String>,
    /// Executable path currently recorded in the installed manifest, if any.
    pub executable_path: Option<String>,
    /// True when the installed manifest points to the currently running executable
    /// and lists [`EXTENSION_ID`].
    pub matches_current: bool,
}

/// On-disk Firefox Native Messaging host manifest JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostManifest {
    pub name: String,
    pub description: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub allowed_extensions: Vec<String>,
}

impl HostManifest {
    #[must_use]
    pub fn for_executable(executable: &Path) -> Self {
        Self {
            name: HOST_NAME.to_owned(),
            description: "knobas link handoff native messaging host".to_owned(),
            path: executable.to_string_lossy().into_owned(),
            kind: "stdio".to_owned(),
            allowed_extensions: vec![EXTENSION_ID.to_owned()],
        }
    }
}

/// Target paths for installing the manifest (injectable for tests).
#[derive(Debug, Clone)]
pub struct ManifestTarget {
    pub manifest_path: PathBuf,
    pub write_registry: bool,
}

impl ManifestTarget {
    /// Platform default target for the current user.
    #[must_use]
    pub fn platform() -> Option<Self> {
        #[cfg(target_os = "macos")]
        {
            let home = dirs::home_dir()?;
            Some(Self {
                manifest_path: home
                    .join("Library")
                    .join("Application Support")
                    .join("Mozilla")
                    .join("NativeMessagingHosts")
                    .join(format!("{HOST_NAME}.json")),
                write_registry: false,
            })
        }
        #[cfg(target_os = "windows")]
        {
            let data = dirs::data_dir()?;
            Some(Self {
                manifest_path: data
                    .join(crate::APP_IDENTIFIER)
                    .join("native-messaging")
                    .join(format!("{HOST_NAME}.json")),
                write_registry: true,
            })
        }
        #[cfg(target_os = "linux")]
        {
            let home = dirs::home_dir()?;
            Some(Self {
                manifest_path: home
                    .join(".mozilla")
                    .join("native-messaging-hosts")
                    .join(format!("{HOST_NAME}.json")),
                write_registry: false,
            })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            None
        }
    }
}

/// Read installation status for `target` compared against `current_executable`.
#[must_use]
pub fn status_in(target: Option<&ManifestTarget>, current_executable: &Path) -> NativeHostStatus {
    let Some(target) = target else {
        return NativeHostStatus {
            installed: false,
            supported: false,
            host_name: HOST_NAME.to_owned(),
            extension_id: EXTENSION_ID.to_owned(),
            manifest_path: None,
            executable_path: None,
            matches_current: false,
        };
    };
    let manifest_path = target.manifest_path.to_string_lossy().into_owned();
    let parsed = fs::read_to_string(&target.manifest_path)
        .ok()
        .and_then(|text| serde_json::from_str::<HostManifest>(&text).ok());
    let registry_ok = if target.write_registry {
        windows_registry::points_to(&target.manifest_path)
    } else {
        true
    };
    let current = current_executable.to_string_lossy();
    let (installed, executable_path, matches_current) = match parsed {
        Some(manifest) if registry_ok => {
            let matches = manifest.name == HOST_NAME
                && manifest.kind == "stdio"
                && manifest.path == current.as_ref()
                && manifest
                    .allowed_extensions
                    .iter()
                    .any(|id| id == EXTENSION_ID);
            (true, Some(manifest.path), matches)
        }
        _ => (false, None, false),
    };
    NativeHostStatus {
        installed,
        supported: true,
        host_name: HOST_NAME.to_owned(),
        extension_id: EXTENSION_ID.to_owned(),
        manifest_path: Some(manifest_path),
        executable_path,
        matches_current,
    }
}

/// Atomically install or update the Firefox Native Messaging host manifest.
///
/// # Errors
///
/// OS I/O or Windows registry failure.
pub fn install_in(
    target: &ManifestTarget,
    current_executable: &Path,
) -> Result<NativeHostStatus, String> {
    if let Some(parent) = target.manifest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let manifest = HostManifest::for_executable(current_executable);
    let mut json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("serialize manifest: {error}"))?;
    json.push('\n');

    let staging = target
        .manifest_path
        .with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file = fs::File::create(&staging)
        .map_err(|error| format!("create {}: {error}", staging.display()))?;
    file.write_all(json.as_bytes())
        .map_err(|error| format!("write {}: {error}", staging.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", staging.display()))?;
    drop(file);
    fs::rename(&staging, &target.manifest_path).map_err(|error| {
        let _ = fs::remove_file(&staging);
        format!("replace {}: {error}", target.manifest_path.display())
    })?;

    if target.write_registry {
        windows_registry::register(&target.manifest_path)?;
    }

    Ok(status_in(Some(target), current_executable))
}

/// Remove the Firefox Native Messaging host manifest (and Windows registry key).
///
/// # Errors
///
/// OS I/O or Windows registry failure.
pub fn uninstall_in(
    target: &ManifestTarget,
    current_executable: &Path,
) -> Result<NativeHostStatus, String> {
    if target.manifest_path.exists() {
        fs::remove_file(&target.manifest_path)
            .map_err(|error| format!("remove {}: {error}", target.manifest_path.display()))?;
    }
    if target.write_registry {
        windows_registry::unregister()?;
    }
    Ok(status_in(Some(target), current_executable))
}

/// Current executable path for registering in the Firefox manifest.
///
/// # Errors
///
/// If the OS cannot report the current executable path.
pub fn current_executable() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("current executable: {error}"))
}

#[cfg(target_os = "windows")]
mod windows_registry {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows::core::PCWSTR;

    use crate::link_handoff::protocol::HOST_NAME;

    fn subkey() -> Vec<u16> {
        let path = format!("Software\\Mozilla\\NativeMessagingHosts\\{HOST_NAME}");
        OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn register(manifest_path: &Path) -> Result<(), String> {
        let key_path = subkey();
        let value: Vec<u16> = manifest_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
        let mut hkey = HKEY::default();
        let created = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key_path.as_ptr()),
                Some(0),
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut hkey,
                None,
            )
        };
        if created != ERROR_SUCCESS {
            return Err(format!("RegCreateKeyExW failed: {created:?}"));
        }
        let set = unsafe { RegSetValueExW(hkey, PCWSTR::null(), Some(0), REG_SZ, Some(bytes)) };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if set != ERROR_SUCCESS {
            return Err(format!("RegSetValueExW failed: {set:?}"));
        }
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        let key_path = subkey();
        let deleted = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(key_path.as_ptr())) };
        // Deleting a non-existent key is already uninstalled.
        if deleted != ERROR_SUCCESS && deleted.0 != 2 {
            return Err(format!("RegDeleteTreeW failed: {deleted:?}"));
        }
        Ok(())
    }

    pub fn points_to(manifest_path: &Path) -> bool {
        let key_path = subkey();
        let mut hkey = HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key_path.as_ptr()),
                Some(0),
                KEY_READ,
                &mut hkey,
            )
        };
        if opened != ERROR_SUCCESS {
            return false;
        }
        let mut buf = vec![0_u8; 4096];
        let mut len = buf.len() as u32;
        let queried = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::null(),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut len),
            )
        };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if queried != ERROR_SUCCESS || len < 2 {
            return false;
        }
        let words: Vec<u16> = buf[..(len as usize)]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&w| w != 0)
            .collect();
        String::from_utf16_lossy(&words) == manifest_path.to_string_lossy()
    }
}

#[cfg(not(target_os = "windows"))]
mod windows_registry {
    use std::path::Path;

    pub fn register(_manifest_path: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        Ok(())
    }

    pub fn points_to(_manifest_path: &Path) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_status_and_uninstall_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let target = ManifestTarget {
            manifest_path: dir
                .path()
                .join("Mozilla/NativeMessagingHosts/dev.knobas.desktop.json"),
            write_registry: false,
        };
        let exe = PathBuf::from("/Applications/knobas.app/Contents/MacOS/knobas");
        let other = PathBuf::from("/Applications/old-knobas.app/Contents/MacOS/knobas");

        let before = status_in(Some(&target), &exe);
        assert!(!before.installed);
        assert!(before.supported);
        assert!(!before.matches_current);

        let after = install_in(&target, &exe).unwrap();
        assert!(after.installed);
        assert!(after.matches_current);
        assert_eq!(
            after.executable_path.as_deref(),
            Some("/Applications/knobas.app/Contents/MacOS/knobas")
        );

        let drifted = status_in(Some(&target), &other);
        assert!(drifted.installed);
        assert!(!drifted.matches_current);

        let removed = uninstall_in(&target, &exe).unwrap();
        assert!(!removed.installed);
        assert!(!target.manifest_path.exists());
    }
}
