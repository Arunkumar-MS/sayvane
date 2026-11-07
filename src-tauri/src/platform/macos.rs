//! macOS adapter: Accessibility inspection, native clipboard, Cmd+V delivery,
//! permissions, sleep/lock notifications, and the Fn/Space/Escape event tap.
#![allow(unused_unsafe, non_upper_case_globals)]

use crate::insertion::{AppIdentity, ClipboardSnapshot, Desktop, FieldInfo, FieldKind};
use crate::ipc_types::{PermissionKind, PermissionState, PermissionStatus};
use accessibility_sys::*;
use core_foundation::base::{CFHash, CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ProtocolObject};
use objc2::msg_send;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::{NSArray, NSData, NSString};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::sync::Arc;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
    fn CGRequestListenEventAccess() -> bool;
}

const KEY_V: u16 = 9;
const KEY_SPACE: i64 = 49;
const KEY_ESCAPE: i64 = 53;
const KEY_FN: i64 = 63;
const MODIFIER_MASK: u64 = 0x0010_0000 | 0x0008_0000 | 0x0004_0000 | 0x0002_0000 | 0x0080_0000; // cmd opt ctrl shift fn

pub struct MacDesktop;

struct AxElement(AXUIElementRef);
impl Drop for AxElement {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0 as CFTypeRef) }
        }
    }
}

fn copy_attr(el: AXUIElementRef, name: &str) -> Option<CFType> {
    let attr = CFString::new(name);
    let mut v: CFTypeRef = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(el, attr.as_concrete_TypeRef(), &mut v) };
    if err == kAXErrorSuccess && !v.is_null() {
        Some(unsafe { CFType::wrap_under_create_rule(v) })
    } else {
        None
    }
}

fn attr_string(el: AXUIElementRef, name: &str) -> Option<String> {
    copy_attr(el, name).and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string())
}

fn system_focused_element() -> Option<CFType> {
    let sys = AxElement(unsafe { AXUIElementCreateSystemWide() });
    unsafe { AXUIElementSetMessagingTimeout(sys.0, 0.25) };
    copy_attr(sys.0, kAXFocusedUIElementAttribute)
}

fn is_settable(el: AXUIElementRef, name: &str) -> bool {
    let attr = CFString::new(name);
    let mut settable: u8 = 0;
    let err = unsafe { AXUIElementIsAttributeSettable(el, attr.as_concrete_TypeRef(), &mut settable as *mut u8 as *mut _) };
    err == kAXErrorSuccess && settable != 0
}

fn hash_str(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn ns_to_string(s: Option<Retained<NSString>>) -> Option<String> {
    s.map(|s| s.to_string())
}

impl Desktop for MacDesktop {
    fn frontmost(&self) -> Option<AppIdentity> {
        let app = unsafe { NSWorkspace::sharedWorkspace().frontmostApplication() }?;
        let pid = unsafe { app.processIdentifier() };
        let app_id = ns_to_string(unsafe { app.bundleIdentifier() }).unwrap_or_else(|| format!("pid:{pid}"));
        let app_name = ns_to_string(unsafe { app.localizedName() }).unwrap_or_else(|| app_id.clone());
        let window_id = {
            let ax = AxElement(unsafe { AXUIElementCreateApplication(pid) });
            unsafe { AXUIElementSetMessagingTimeout(ax.0, 0.25) };
            copy_attr(ax.0, kAXFocusedWindowAttribute).map(|w| unsafe { CFHash(w.as_CFTypeRef()) } as u64).unwrap_or(0)
        };
        Some(AppIdentity { app_id, app_name, pid: pid as i64, window_id })
    }

    fn focused_field(&self) -> FieldInfo {
        if !unsafe { AXIsProcessTrusted() } {
            return FieldInfo { detail: Some("Accessibility access is off".into()), ..FieldInfo::unknown() };
        }
        let Some(el) = system_focused_element() else { return FieldInfo::unknown() };
        let r = el.as_CFTypeRef() as AXUIElementRef;
        let role = attr_string(r, kAXRoleAttribute).unwrap_or_default();
        let subrole = attr_string(r, kAXSubroleAttribute).unwrap_or_default();
        let field_id = Some(unsafe { CFHash(el.as_CFTypeRef()) } as u64);
        if subrole == kAXSecureTextFieldSubrole || role == "AXSecureTextField" {
            return FieldInfo { kind: FieldKind::Password, field_id, value_len: None, value_hash: None, detail: None };
        }
        let editable_role = matches!(role.as_str(), "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField");
        let non_text_role = matches!(
            role.as_str(),
            "AXButton" | "AXMenuItem" | "AXMenuBar" | "AXMenuBarItem" | "AXCheckBox" | "AXRadioButton" | "AXSlider"
                | "AXImage" | "AXDockItem" | "AXPopUpButton" | "AXTabGroup"
        );
        let settable = is_settable(r, kAXValueAttribute);
        let kind = if editable_role || settable {
            FieldKind::Editable
        } else if non_text_role {
            FieldKind::NotEditable
        } else {
            FieldKind::Unknown
        };
        // Only a length and hash are kept, for confirmation; the contents are never stored.
        let value = (kind == FieldKind::Editable)
            .then(|| attr_string(r, kAXValueAttribute))
            .flatten();
        FieldInfo {
            kind,
            field_id,
            value_len: value.as_ref().map(|v| v.chars().count()),
            value_hash: value.as_deref().map(hash_str),
            detail: None,
        }
    }

    fn is_own_app(&self, app: &AppIdentity) -> bool {
        app.pid == std::process::id() as i64
    }

    fn modifiers_down(&self) -> bool {
        let flags = unsafe { CGEventSourceFlagsState(1 /* HIDSystemState */) };
        flags & MODIFIER_MASK != 0
    }

    fn clipboard_token(&self) -> i64 {
        unsafe { NSPasteboard::generalPasteboard().changeCount() as i64 }
    }

    fn clipboard_snapshot(&self) -> Option<ClipboardSnapshot> {
        let pb = unsafe { NSPasteboard::generalPasteboard() };
        let items = unsafe { pb.pasteboardItems() }?;
        let mut snap = ClipboardSnapshot { items: Vec::new(), complete: true };
        for item in items.iter() {
            let mut formats = Vec::new();
            for t in unsafe { item.types() }.iter() {
                match unsafe { item.dataForType(&t) } {
                    Some(data) => formats.push((t.to_string(), data.to_vec())),
                    None => snap.complete = false, // promised data that was never provided
                }
            }
            snap.items.push(formats);
        }
        Some(snap)
    }

    fn clipboard_set_text(&self, text: &str) -> Result<i64, String> {
        let pb = unsafe { NSPasteboard::generalPasteboard() };
        unsafe { pb.clearContents() };
        let ok = unsafe { pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
        if !ok {
            return Err("the clipboard rejected the text".into());
        }
        Ok(unsafe { pb.changeCount() as i64 })
    }

    fn clipboard_restore(&self, snap: &ClipboardSnapshot) -> Result<(), String> {
        let pb = unsafe { NSPasteboard::generalPasteboard() };
        unsafe { pb.clearContents() };
        if snap.items.is_empty() {
            return Ok(());
        }
        let mut writers: Vec<Retained<ProtocolObject<dyn objc2_app_kit::NSPasteboardWriting>>> = Vec::new();
        for formats in &snap.items {
            let item = unsafe { NSPasteboardItem::new() };
            for (t, bytes) in formats {
                let data = NSData::with_bytes(bytes);
                unsafe { item.setData_forType(&data, &NSString::from_str(t)) };
            }
            writers.push(ProtocolObject::from_retained(item));
        }
        let array = NSArray::from_retained_slice(&writers);
        if unsafe { pb.writeObjects(&array) } {
            Ok(())
        } else {
            Err("the clipboard could not be restored".into())
        }
    }

    fn send_paste(&self) -> Result<(), String> {
        let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| "no event source".to_string())?;
        let down = CGEvent::new_keyboard_event(src.clone(), KEY_V, true).map_err(|_| "key event failed".to_string())?;
        let up = CGEvent::new_keyboard_event(src, KEY_V, false).map_err(|_| "key event failed".to_string())?;
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(8));
        up.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn permissions(&self) -> PermissionStatus {
        let mic = unsafe {
            let cls = AnyClass::get(c"AVCaptureDevice");
            match cls {
                Some(cls) => {
                    let status: isize = msg_send![cls, authorizationStatusForMediaType: AVMediaTypeAudio];
                    match status {
                        0 => PermissionState::NotDetermined,
                        1 | 2 => PermissionState::Denied,
                        3 => PermissionState::Granted,
                        _ => PermissionState::Unknown,
                    }
                }
                None => PermissionState::Unknown,
            }
        };
        PermissionStatus {
            microphone: mic,
            accessibility: if unsafe { AXIsProcessTrusted() } { PermissionState::Granted } else { PermissionState::Denied },
            // The Fn listener is an active event tap, which runs on Accessibility alone;
            // Input Monitoring is not required, so it is not requested or shown.
            input_monitoring: PermissionState::NotApplicable,
        }
    }

    fn request_permission(&self, kind: PermissionKind) {
        match kind {
            PermissionKind::Microphone => unsafe {
                if let Some(cls) = AnyClass::get(c"AVCaptureDevice") {
                    let block = block2::RcBlock::new(|_granted: Bool| {});
                    let _: () = msg_send![cls, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*block];
                }
            },
            PermissionKind::Accessibility => {
                let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
                let opts = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
                unsafe { AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) };
            }
            PermissionKind::InputMonitoring => unsafe {
                CGRequestListenEventAccess();
            },
        }
    }

    fn open_permission_settings(&self, kind: PermissionKind) {
        let anchor = match kind {
            PermissionKind::Microphone => "Privacy_Microphone",
            PermissionKind::Accessibility => "Privacy_Accessibility",
            PermissionKind::InputMonitoring => "Privacy_ListenEvent",
        };
        let _ = std::process::Command::new("open")
            .arg(format!("x-apple.systempreferences:com.apple.preference.security?{anchor}"))
            .spawn();
    }
}

pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Clears this app's own privacy decisions so macOS asks again. Needed after reinstalling
/// an ad-hoc signed build: the old grant belongs to the previous code identity and no
/// longer applies, even though System Settings still shows it switched on.
/// Returns the services that could not be reset.
pub fn reset_privacy(bundle_id: &str) -> Vec<String> {
    ["Microphone", "Accessibility", "ListenEvent"]
        .iter()
        .filter(|service| {
            !std::process::Command::new("/usr/bin/tccutil")
                .args(["reset", service, bundle_id])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect()
}

/// Calls `on_suspend` when the Mac sleeps, the screens sleep, the session is switched
/// away, or the screen locks. Capture stops; it never restarts automatically.
pub fn watch_sleep_and_lock(on_suspend: Arc<dyn Fn(&'static str) + Send + Sync>) {
    use objc2_app_kit::{
        NSWorkspaceScreensDidSleepNotification, NSWorkspaceSessionDidResignActiveNotification,
        NSWorkspaceWillSleepNotification,
    };
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification};
    unsafe {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let pairs: [(&NSString, &'static str); 3] = [
            (NSWorkspaceWillSleepNotification, "sleep"),
            (NSWorkspaceScreensDidSleepNotification, "screen sleep"),
            (NSWorkspaceSessionDidResignActiveNotification, "session switch"),
        ];
        for (name, why) in pairs {
            let cb = on_suspend.clone();
            let block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| cb(why));
            let token = center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block);
            std::mem::forget(token); // lives for the whole process
        }
        let dist = NSDistributedNotificationCenter::defaultCenter();
        let cb = on_suspend.clone();
        let block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| cb("screen lock"));
        let name = NSString::from_str("com.apple.screenIsLocked");
        let token = dist.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block);
        std::mem::forget(token);
    }
}

// ---------- Fn / Space / Escape event tap ----------

pub trait KeyHandler: Send + Sync {
    fn fn_down(&self);
    fn fn_up(&self);
    /// Return true to consume the Space event (and its key-up).
    fn space_down(&self) -> bool;
    /// Return true to consume Escape (only while a session is active).
    fn escape_down(&self) -> bool;
}

#[derive(Debug, Clone)]
pub enum TapStatus {
    Running,
    Unavailable(String),
}

/// Starts the keyboard tap on its own run-loop thread. Plain typing passes through
/// untouched; only gesture-owned Space/Escape events (and their key-ups) are consumed.
pub fn start_key_tap(handler: Arc<dyn KeyHandler>) -> TapStatus {
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{CGEventTap, CGEventTapOptions, CGEventTapPlacement, CallbackResult};
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<TapStatus>();
    std::thread::Builder::new()
        .name("key-tap".into())
        .spawn(move || {
            let fn_down = Arc::new(AtomicBool::new(false));
            let consumed_space = Arc::new(AtomicBool::new(false));
            let consumed_escape = Arc::new(AtomicBool::new(false));
            let tap_port: Arc<AtomicPtr<std::ffi::c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
            let tp = tap_port.clone();
            let h = handler.clone();
            let callback = move |_proxy, etype: CGEventType, event: &CGEvent| -> CallbackResult {
                match etype {
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        let p = tp.load(Ordering::SeqCst);
                        if !p.is_null() {
                            extern "C" {
                                fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
                            }
                            unsafe { CGEventTapEnable(p, true) };
                        }
                        CallbackResult::Keep
                    }
                    CGEventType::FlagsChanged => {
                        let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        if code == KEY_FN {
                            let down = event.get_flags().contains(CGEventFlags::CGEventFlagSecondaryFn);
                            if down != fn_down.swap(down, Ordering::SeqCst) {
                                if down {
                                    h.fn_down()
                                } else {
                                    h.fn_up()
                                }
                            }
                        }
                        CallbackResult::Keep
                    }
                    CGEventType::KeyDown | CGEventType::KeyUp => {
                        let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        let repeat = event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0;
                        let is_down = matches!(etype, CGEventType::KeyDown);
                        let slot = match code {
                            KEY_SPACE => &consumed_space,
                            KEY_ESCAPE => &consumed_escape,
                            _ => return CallbackResult::Keep,
                        };
                        if !is_down {
                            return if slot.swap(false, Ordering::SeqCst) { CallbackResult::Drop } else { CallbackResult::Keep };
                        }
                        if repeat {
                            return if slot.load(Ordering::SeqCst) { CallbackResult::Drop } else { CallbackResult::Keep };
                        }
                        let consume = if code == KEY_SPACE { h.space_down() } else { h.escape_down() };
                        slot.store(consume, Ordering::SeqCst);
                        if consume {
                            CallbackResult::Drop
                        } else {
                            CallbackResult::Keep
                        }
                    }
                    _ => CallbackResult::Keep,
                }
            };
            let tap = CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![CGEventType::FlagsChanged, CGEventType::KeyDown, CGEventType::KeyUp],
                callback,
            );
            let tap = match tap {
                Ok(t) => t,
                Err(_) => {
                    let _ = tx.send(TapStatus::Unavailable(
                        "Keyboard shortcuts need Accessibility access. Grant it in System Settings, then reopen the app."
                            .into(),
                    ));
                    return;
                }
            };
            tap_port.store(tap.mach_port().as_concrete_TypeRef() as *mut _, Ordering::SeqCst);
            let source = match tap.mach_port().create_runloop_source(0) {
                Ok(s) => s,
                Err(_) => {
                    let _ = tx.send(TapStatus::Unavailable("The keyboard listener could not start.".into()));
                    return;
                }
            };
            let rl = CFRunLoop::get_current();
            unsafe { rl.add_source(&source, kCFRunLoopCommonModes) };
            tap.enable();
            let _ = tx.send(TapStatus::Running);
            CFRunLoop::run_current();
        })
        .ok();
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or(TapStatus::Unavailable("The keyboard listener did not start.".into()))
}

/// Keeps the dock icon when the main window shows, hides it otherwise.
pub fn set_dock_visible(visible: bool) {
    unsafe {
        let Some(cls) = AnyClass::get(c"NSApplication") else { return };
        let app: *mut AnyObject = msg_send![cls, sharedApplication];
        if app.is_null() {
            return;
        }
        // NSApplicationActivationPolicyRegular = 0, Accessory = 1
        let _: Bool = msg_send![app, setActivationPolicy: if visible { 0isize } else { 1isize }];
    }
}
