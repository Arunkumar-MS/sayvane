//! Native adapters behind the shared `Desktop` trait.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

use crate::insertion::{AppIdentity, ClipboardSnapshot, Desktop, FieldInfo};
use crate::ipc_types::{PermissionKind, PermissionState, PermissionStatus, Platform};

pub fn desktop() -> Box<dyn Desktop> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacDesktop);
    #[cfg(windows)]
    return Box::new(windows::WinDesktop);
    #[cfg(not(any(target_os = "macos", windows)))]
    return Box::new(Unsupported);
}

pub fn platform_info() -> Platform {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "other"
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        _ => "other",
    };
    Platform { os: os.into(), arch: arch.into(), os_version: os_version() }
}

fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        String::new()
    }
}

/// Fallback for unsupported targets so the core still compiles for tests.
#[allow(dead_code)]
pub struct Unsupported;

impl Desktop for Unsupported {
    fn frontmost(&self) -> Option<AppIdentity> {
        None
    }
    fn focused_field(&self) -> FieldInfo {
        FieldInfo::unknown()
    }
    fn is_own_app(&self, _: &AppIdentity) -> bool {
        false
    }
    fn modifiers_down(&self) -> bool {
        false
    }
    fn clipboard_token(&self) -> i64 {
        0
    }
    fn clipboard_snapshot(&self) -> Option<ClipboardSnapshot> {
        None
    }
    fn clipboard_set_text(&self, _: &str) -> Result<i64, String> {
        Err("unsupported platform".into())
    }
    fn clipboard_restore(&self, _: &ClipboardSnapshot) -> Result<(), String> {
        Ok(())
    }
    fn send_paste(&self) -> Result<(), String> {
        Err("unsupported platform".into())
    }
    fn permissions(&self) -> PermissionStatus {
        PermissionStatus {
            microphone: PermissionState::Unknown,
            accessibility: PermissionState::NotApplicable,
            input_monitoring: PermissionState::NotApplicable,
        }
    }
    fn request_permission(&self, _: PermissionKind) {}
    fn open_permission_settings(&self, _: PermissionKind) {}
}
