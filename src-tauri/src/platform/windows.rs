//! Windows adapter: UI Automation inspection, native clipboard, Ctrl+V via SendInput,
//! and microphone privacy state. Runs as a standard user; elevated targets and the
//! secure desktop produce an honest "saved" outcome instead of a privilege request.

use crate::insertion::{AppIdentity, ClipboardSnapshot, Desktop, FieldInfo, FieldKind};
use crate::ipc_types::{PermissionKind, PermissionState, PermissionStatus};
use std::hash::{Hash, Hasher};
use std::time::Duration;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HGLOBAL, HWND};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
    UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_ImageControlTypeId, UIA_MenuItemControlTypeId,
    UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub struct WinDesktop;

// Formats that are GDI handles, not HGLOBAL memory; they cannot be copied byte-wise.
const HANDLE_FORMATS: &[u32] = &[2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elev = TOKEN_ELEVATION::default();
        let mut ret = 0u32;
        let r = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        );
        let _ = CloseHandle(token);
        r.ok()?;
        Some(elev.TokenIsElevated != 0)
    }
}

fn pid_elevated(pid: u32) -> Option<bool> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let r = token_elevated(h);
        let _ = CloseHandle(h);
        r
    }
}

fn on_secure_desktop() -> bool {
    unsafe {
        match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) {
            Ok(d) => {
                let _ = CloseDesktop(d);
                false
            }
            Err(_) => true,
        }
    }
}

fn automation() -> Option<IUIAutomation> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
    }
}

struct ClipboardGuard;
impl ClipboardGuard {
    fn open() -> Result<Self, String> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("another app is using the clipboard".into())
    }
}
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn set_global(format: u32, bytes: &[u8]) -> Result<(), String> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|e| e.to_string())?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            return Err("clipboard memory could not be locked".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        SetClipboardData(format, Some(HANDLE(h.0))).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mic_consent() -> PermissionState {
    let read = |root| -> Option<String> {
        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\0"
            .encode_utf16()
            .collect();
        let value: Vec<u16> = "Value\0".encode_utf16().collect();
        let mut buf = [0u16; 32];
        let mut len = (buf.len() * 2) as u32;
        unsafe {
            RegGetValueW(
                root,
                windows::core::PCWSTR(subkey.as_ptr()),
                windows::core::PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                Some(&mut len),
            )
            .ok()
            .ok()?;
        }
        let s = String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]);
        Some(s)
    };
    match (read(HKEY_LOCAL_MACHINE).as_deref(), read(HKEY_CURRENT_USER).as_deref()) {
        (Some("Deny"), _) | (_, Some("Deny")) => PermissionState::Denied,
        (_, Some("Allow")) => PermissionState::Granted,
        _ => PermissionState::Unknown,
    }
}

impl Desktop for WinDesktop {
    fn frontmost(&self) -> Option<AppIdentity> {
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let path = process_path(pid).unwrap_or_else(|| format!("pid:{pid}"));
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            Some(AppIdentity { app_id: path.to_lowercase(), app_name: name, pid: pid as i64, window_id: hwnd.0 as u64 })
        }
    }

    fn focused_field(&self) -> FieldInfo {
        if on_secure_desktop() {
            return FieldInfo {
                kind: FieldKind::Protected,
                detail: Some("Windows is showing a secure screen; saved in History".into()),
                ..FieldInfo::unknown()
            };
        }
        if let Some(front) = self.frontmost() {
            let target_elevated = pid_elevated(front.pid as u32);
            let self_elevated = token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false);
            // An elevated window we can't inspect also rejects simulated input from us.
            if target_elevated.unwrap_or(true) && !self_elevated && target_elevated.is_some() {
                return FieldInfo {
                    kind: FieldKind::Protected,
                    detail: Some("That app is running as administrator and blocks pasting; saved in History".into()),
                    ..FieldInfo::unknown()
                };
            }
        }
        let Some(uia) = automation() else { return FieldInfo::unknown() };
        unsafe {
            let Ok(el) = uia.GetFocusedElement() else { return FieldInfo::unknown() };
            let control = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
            let hwnd = el.CurrentNativeWindowHandle().map(|h| h.0 as u64).unwrap_or(0);
            let rect = el.CurrentBoundingRectangle().ok();
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (hwnd, control, rect.map(|r| (r.left, r.top))).hash(&mut h);
            let field_id = Some(h.finish());
            if el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false) {
                return FieldInfo { kind: FieldKind::Password, field_id, value_len: None, value_hash: None, detail: None };
            }
            let value_pattern: Option<IUIAutomationValuePattern> =
                el.GetCurrentPattern(UIA_ValuePatternId).ok().and_then(|p| p.cast().ok());
            let read_only = value_pattern.as_ref().and_then(|v| v.CurrentIsReadOnly().ok()).map(|b| b.as_bool());
            let kind = if control == UIA_EditControlTypeId.0 || control == UIA_DocumentControlTypeId.0 {
                if read_only == Some(true) { FieldKind::NotEditable } else { FieldKind::Editable }
            } else if read_only == Some(false) {
                FieldKind::Editable
            } else if [UIA_ButtonControlTypeId.0, UIA_CheckBoxControlTypeId.0, UIA_ImageControlTypeId.0, UIA_MenuItemControlTypeId.0]
                .contains(&control)
            {
                FieldKind::NotEditable
            } else {
                FieldKind::Unknown
            };
            let value = if kind == FieldKind::Editable {
                value_pattern.and_then(|v| v.CurrentValue().ok()).map(|b| b.to_string())
            } else {
                None
            };
            FieldInfo {
                kind,
                field_id,
                value_len: value.as_ref().map(|v| v.chars().count()),
                value_hash: value.as_deref().map(hash_str),
                detail: None,
            }
        }
    }

    fn is_own_app(&self, app: &AppIdentity) -> bool {
        app.pid == unsafe { GetCurrentProcessId() } as i64
    }

    fn modifiers_down(&self) -> bool {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0)
    }

    fn clipboard_token(&self) -> i64 {
        unsafe { GetClipboardSequenceNumber() as i64 }
    }

    fn clipboard_snapshot(&self) -> Option<ClipboardSnapshot> {
        let _g = ClipboardGuard::open().ok()?;
        let mut formats = Vec::new();
        let mut complete = true;
        unsafe {
            let mut f = EnumClipboardFormats(0);
            while f != 0 {
                if HANDLE_FORMATS.contains(&f) {
                    complete = false;
                } else if let Ok(h) = GetClipboardData(f) {
                    let g = HGLOBAL(h.0);
                    let size = GlobalSize(g);
                    let p = GlobalLock(g) as *const u8;
                    if !p.is_null() && size > 0 {
                        formats.push((f.to_string(), std::slice::from_raw_parts(p, size).to_vec()));
                        let _ = GlobalUnlock(g);
                    } else {
                        complete = false;
                    }
                }
                f = EnumClipboardFormats(f);
            }
        }
        Some(ClipboardSnapshot { items: vec![formats], complete })
    }

    fn clipboard_set_text(&self, text: &str) -> Result<i64, String> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
        {
            let _g = ClipboardGuard::open()?;
            unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
            set_global(13 /* CF_UNICODETEXT */, bytes)?;
        }
        Ok(self.clipboard_token())
    }

    fn clipboard_restore(&self, snap: &ClipboardSnapshot) -> Result<(), String> {
        let _g = ClipboardGuard::open()?;
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        for item in &snap.items {
            for (fmt, bytes) in item {
                if let Ok(f) = fmt.parse::<u32>() {
                    let _ = set_global(f, bytes);
                }
            }
        }
        Ok(())
    }

    fn send_paste(&self) -> Result<(), String> {
        let inputs = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            return Err("Windows blocked the paste keystroke".into());
        }
        Ok(())
    }

    fn permissions(&self) -> PermissionStatus {
        PermissionStatus {
            microphone: mic_consent(),
            accessibility: PermissionState::NotApplicable,
            input_monitoring: PermissionState::NotApplicable,
        }
    }

    fn request_permission(&self, kind: PermissionKind) {
        self.open_permission_settings(kind);
    }

    fn open_permission_settings(&self, kind: PermissionKind) {
        if kind == PermissionKind::Microphone {
            let _ = std::process::Command::new("explorer").arg("ms-settings:privacy-microphone").spawn();
        }
    }
}

/// Makes a window non-activating so clicking the bar never steals focus.
pub fn make_non_activating(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    };
    unsafe {
        let h = HWND(hwnd as *mut core::ffi::c_void);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize);
    }
}
