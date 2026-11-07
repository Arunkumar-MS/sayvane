//! Text delivery into another application, shared by every platform.
//!
//! The procedure never presses Return or any submit control, pastes at most once, and
//! restores the previous clipboard only when nothing else has written to it since.

use crate::ipc_types::{InsertionState, PermissionKind, PermissionStatus};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub app_id: String,
    pub app_name: String,
    pub pid: i64,
    /// Opaque window identity (window number / HWND).
    pub window_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Exposes editable text; contents may be readable for confirmation.
    Editable,
    Password,
    NotEditable,
    /// Focused element is not exposed through accessibility (common in some apps).
    Unknown,
    /// Secure desktop, elevated target, or blocked automation.
    Protected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub kind: FieldKind,
    /// Opaque identity of the focused element, when exposed.
    pub field_id: Option<u64>,
    /// Character count of the field's value, when readable.
    pub value_len: Option<usize>,
    /// Hash of the field's value, when readable.
    pub value_hash: Option<u64>,
    pub detail: Option<String>,
}

impl FieldInfo {
    pub fn unknown() -> Self {
        Self { kind: FieldKind::Unknown, field_id: None, value_len: None, value_hash: None, detail: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationToken {
    pub app: AppIdentity,
    pub field_id: Option<u64>,
}

/// Platform services needed for capture and delivery. Implemented natively per OS and by
/// a test double.
pub trait Desktop: Send + Sync {
    fn frontmost(&self) -> Option<AppIdentity>;
    fn focused_field(&self) -> FieldInfo;
    fn is_own_app(&self, app: &AppIdentity) -> bool;
    fn modifiers_down(&self) -> bool;
    fn clipboard_token(&self) -> i64;
    fn clipboard_snapshot(&self) -> Option<ClipboardSnapshot>;
    /// Writes plain text and returns the clipboard token after the write.
    fn clipboard_set_text(&self, text: &str) -> Result<i64, String>;
    fn clipboard_restore(&self, snap: &ClipboardSnapshot) -> Result<(), String>;
    fn send_paste(&self) -> Result<(), String>;
    fn permissions(&self) -> PermissionStatus;
    fn request_permission(&self, kind: PermissionKind);
    fn open_permission_settings(&self, kind: PermissionKind);
}

/// Native clipboard contents: one entry per item, each a list of (format, bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardSnapshot {
    pub items: Vec<Vec<(String, Vec<u8>)>>,
    /// False when some formats could not be captured (e.g. GDI handles on Windows).
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertionReport {
    pub state: InsertionState,
    pub message: String,
    /// Whether the transcript is now on the clipboard (for honest messaging).
    pub left_on_clipboard: bool,
    /// Present when delivery succeeded and the caller should restore later.
    pub restore: Option<PendingRestore>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRestore {
    pub snapshot: ClipboardSnapshot,
    pub our_token: i64,
}

pub const CONFIRM_WINDOW: Duration = Duration::from_millis(600);
pub const RESTORE_DELAY: Duration = Duration::from_millis(700);
pub const MODIFIER_WAIT: Duration = Duration::from_secs(1);

pub fn capture_destination(d: &dyn Desktop) -> Option<DestinationToken> {
    let app = d.frontmost()?;
    if d.is_own_app(&app) {
        return None;
    }
    Some(DestinationToken { app, field_id: d.focused_field().field_id })
}

pub enum Target<'a> {
    /// Automatic delivery: must still be the app and window captured at start.
    Captured(&'a DestinationToken),
    /// Explicit Paste last: whatever field the user has focused now.
    Current,
}

fn saved(state: InsertionState, msg: &str) -> InsertionReport {
    InsertionReport { state, message: msg.into(), left_on_clipboard: false, restore: None }
}

/// Runs the delivery procedure once. `still_current` lets the coordinator veto at the
/// last moment if the session was canceled or superseded.
pub fn deliver(d: &dyn Desktop, text: &str, target: Target<'_>, still_current: &dyn Fn() -> bool) -> InsertionReport {
    if text.trim().is_empty() {
        return saved(InsertionState::SavedOnly, "Nothing to insert");
    }
    let Some(front) = d.frontmost() else {
        return saved(InsertionState::SavedOnly, "Saved to History; select a text field");
    };
    if let Target::Captured(tok) = &target {
        if front.pid != tok.app.pid || front.app_id != tok.app.app_id || front.window_id != tok.app.window_id {
            return saved(InsertionState::DestinationChanged, "You switched apps, so nothing was pasted. Copy it from History");
        }
    }
    if d.is_own_app(&front) {
        return saved(InsertionState::Ineligible, "Saved to History; select a text field in another app");
    }
    let field = d.focused_field();
    match field.kind {
        FieldKind::Password => {
            // Never place a transcript on the clipboard for a password field.
            return saved(InsertionState::Ineligible, "Not pasted into a password field; saved in History");
        }
        FieldKind::Protected => {
            return saved(
                InsertionState::Ineligible,
                field.detail.as_deref().unwrap_or("This window does not accept automated input; saved in History"),
            );
        }
        FieldKind::NotEditable => {
            return saved(InsertionState::SavedOnly, "Saved to History; select a text field");
        }
        FieldKind::Editable | FieldKind::Unknown => {}
    }
    if let (Target::Captured(tok), Some(now)) = (&target, field.field_id) {
        if tok.field_id.is_some_and(|f| f != now) {
            return saved(InsertionState::DestinationChanged, "The text field changed, so nothing was pasted. Copy it from History");
        }
    }

    // Wait for shortcut modifiers so the paste is not read as another combination.
    let t0 = Instant::now();
    while d.modifiers_down() && t0.elapsed() < MODIFIER_WAIT {
        std::thread::sleep(Duration::from_millis(15));
    }

    let snapshot = d.clipboard_snapshot();
    if !still_current() {
        return saved(InsertionState::Canceled, "Canceled");
    }
    let our_token = match d.clipboard_set_text(text) {
        Ok(t) => t,
        Err(e) => return saved(InsertionState::SavedOnly, &format!("Saved to History; clipboard unavailable ({e})")),
    };
    if !still_current() {
        if let Some(s) = &snapshot {
            if d.clipboard_token() == our_token {
                let _ = d.clipboard_restore(s);
            }
        }
        return saved(InsertionState::Canceled, "Canceled");
    }
    if let Err(e) = d.send_paste() {
        return InsertionReport {
            state: InsertionState::SavedOnly,
            message: format!("Could not paste ({e}); the text is on the clipboard and in History"),
            left_on_clipboard: true,
            restore: None,
        };
    }

    // Bounded confirmation where the field exposes its value.
    let confirmed = match (field.value_len, field.value_hash) {
        (Some(before_len), Some(before_hash)) => {
            let t = Instant::now();
            let mut ok = false;
            while t.elapsed() < CONFIRM_WINDOW {
                std::thread::sleep(Duration::from_millis(40));
                let after = d.focused_field();
                if after.field_id != field.field_id {
                    break;
                }
                if after.value_hash.is_some_and(|h| h != before_hash) || after.value_len.is_some_and(|l| l != before_len) {
                    ok = true;
                    break;
                }
            }
            ok
        }
        _ => false,
    };

    if confirmed {
        InsertionReport {
            state: InsertionState::Confirmed,
            message: format!("Inserted in {}", front.app_name),
            left_on_clipboard: true,
            restore: snapshot.map(|snapshot| PendingRestore { snapshot, our_token }),
        }
    } else {
        // Unconfirmed: leave the transcript on the clipboard for recovery, never re-paste.
        InsertionReport {
            state: InsertionState::SentUnconfirmed,
            message: "Paste sent; also saved in History".into(),
            left_on_clipboard: true,
            restore: None,
        }
    }
}

/// Restores the earlier clipboard only if nothing has written to it since our insert.
pub fn restore_if_untouched(d: &dyn Desktop, pending: &PendingRestore) -> bool {
    if d.clipboard_token() != pending.our_token {
        return false;
    }
    d.clipboard_restore(&pending.snapshot).is_ok()
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use parking_lot::Mutex;

    pub struct FakeDesktop {
        pub front: Mutex<Option<AppIdentity>>,
        pub field: Mutex<FieldInfo>,
        pub clip: Mutex<(i64, ClipboardSnapshot)>,
        pub pastes: Mutex<u32>,
        /// Simulates an editor that appends on paste.
        pub editor_value: Mutex<Option<String>>,
    }

    pub fn app(name: &str, pid: i64, win: u64) -> AppIdentity {
        AppIdentity { app_id: format!("com.test.{name}"), app_name: name.into(), pid, window_id: win }
    }

    impl FakeDesktop {
        pub fn new(front: AppIdentity, kind: FieldKind, readable: bool) -> Self {
            let value = readable.then(|| "hello ".to_string());
            let f = Self {
                front: Mutex::new(Some(front)),
                field: Mutex::new(FieldInfo { kind, field_id: Some(1), value_len: None, value_hash: None, detail: None }),
                clip: Mutex::new((
                    10,
                    ClipboardSnapshot { items: vec![vec![("public.utf8-plain-text".into(), b"previous".to_vec())]], complete: true },
                )),
                pastes: Mutex::new(0),
                editor_value: Mutex::new(value),
            };
            f.sync_field();
            f
        }

        fn sync_field(&self) {
            let v = self.editor_value.lock().clone();
            let mut fld = self.field.lock();
            fld.value_len = v.as_ref().map(|s| s.chars().count());
            fld.value_hash = v.as_ref().map(|s| s.len() as u64 * 31 + s.bytes().map(u64::from).sum::<u64>());
        }

        pub fn clip_text(&self) -> String {
            String::from_utf8(self.clip.lock().1.items[0][0].1.clone()).unwrap()
        }

        pub fn user_copies(&self, text: &str) {
            let mut c = self.clip.lock();
            c.0 += 1;
            c.1 = ClipboardSnapshot { items: vec![vec![("public.utf8-plain-text".into(), text.as_bytes().to_vec())]], complete: true };
        }
    }

    impl Desktop for FakeDesktop {
        fn frontmost(&self) -> Option<AppIdentity> {
            self.front.lock().clone()
        }
        fn focused_field(&self) -> FieldInfo {
            self.field.lock().clone()
        }
        fn is_own_app(&self, app: &AppIdentity) -> bool {
            app.app_id == "com.test.self"
        }
        fn modifiers_down(&self) -> bool {
            false
        }
        fn clipboard_token(&self) -> i64 {
            self.clip.lock().0
        }
        fn clipboard_snapshot(&self) -> Option<ClipboardSnapshot> {
            Some(self.clip.lock().1.clone())
        }
        fn clipboard_set_text(&self, text: &str) -> Result<i64, String> {
            self.user_copies(text);
            Ok(self.clip.lock().0)
        }
        fn clipboard_restore(&self, snap: &ClipboardSnapshot) -> Result<(), String> {
            let mut c = self.clip.lock();
            c.0 += 1;
            c.1 = snap.clone();
            Ok(())
        }
        fn send_paste(&self) -> Result<(), String> {
            *self.pastes.lock() += 1;
            let clip = self.clip_text();
            if let Some(v) = self.editor_value.lock().as_mut() {
                v.push_str(&clip);
            }
            self.sync_field();
            Ok(())
        }
        fn permissions(&self) -> PermissionStatus {
            unimplemented!()
        }
        fn request_permission(&self, _: PermissionKind) {}
        fn open_permission_settings(&self, _: PermissionKind) {}
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;

    fn yes() -> bool {
        true
    }

    #[test]
    fn confirmed_insert_then_restore_previous_clipboard() {
        let d = FakeDesktop::new(app("Notes", 5, 1), FieldKind::Editable, true);
        let tok = capture_destination(&d).unwrap();
        let r = deliver(&d, "world", Target::Captured(&tok), &yes);
        assert_eq!(r.state, InsertionState::Confirmed);
        assert_eq!(r.message, "Inserted in Notes");
        assert_eq!(*d.pastes.lock(), 1);
        assert!(restore_if_untouched(&d, r.restore.as_ref().unwrap()));
        assert_eq!(d.clip_text(), "previous");
    }

    #[test]
    fn user_copy_during_restore_delay_is_never_overwritten() {
        let d = FakeDesktop::new(app("Notes", 5, 1), FieldKind::Editable, true);
        let tok = capture_destination(&d).unwrap();
        let r = deliver(&d, "world", Target::Captured(&tok), &yes);
        d.user_copies("something new");
        assert!(!restore_if_untouched(&d, r.restore.as_ref().unwrap()));
        assert_eq!(d.clip_text(), "something new");
    }

    #[test]
    fn unreadable_field_is_unconfirmed_and_pasted_once() {
        let d = FakeDesktop::new(app("Chat", 7, 2), FieldKind::Unknown, false);
        let tok = capture_destination(&d).unwrap();
        let r = deliver(&d, "hi", Target::Captured(&tok), &yes);
        assert_eq!(r.state, InsertionState::SentUnconfirmed);
        assert_eq!(*d.pastes.lock(), 1, "no blind repeat");
        assert!(r.restore.is_none(), "leave text on clipboard for recovery");
        assert_eq!(d.clip_text(), "hi");
    }

    #[test]
    fn password_field_never_touches_clipboard() {
        let d = FakeDesktop::new(app("Login", 9, 3), FieldKind::Password, false);
        let tok = capture_destination(&d).unwrap();
        let r = deliver(&d, "secret words", Target::Captured(&tok), &yes);
        assert_eq!(r.state, InsertionState::Ineligible);
        assert_eq!(*d.pastes.lock(), 0);
        assert_eq!(d.clip_text(), "previous");
    }

    #[test]
    fn app_switch_suspends_automatic_delivery_but_paste_last_works() {
        let d = FakeDesktop::new(app("Notes", 5, 1), FieldKind::Editable, true);
        let tok = capture_destination(&d).unwrap();
        *d.front.lock() = Some(app("Mail", 6, 9));
        let r = deliver(&d, "x", Target::Captured(&tok), &yes);
        assert_eq!(r.state, InsertionState::DestinationChanged);
        assert_eq!(*d.pastes.lock(), 0);
        let r = deliver(&d, "x", Target::Current, &yes);
        assert_eq!(r.state, InsertionState::Confirmed);
        assert_eq!(r.message, "Inserted in Mail");
    }

    #[test]
    fn cancel_right_before_insertion_never_pastes() {
        let d = FakeDesktop::new(app("Notes", 5, 1), FieldKind::Editable, true);
        let tok = capture_destination(&d).unwrap();
        let r = deliver(&d, "x", Target::Captured(&tok), &|| false);
        assert_eq!(r.state, InsertionState::Canceled);
        assert_eq!(*d.pastes.lock(), 0);
        assert_eq!(d.clip_text(), "previous");
    }

    #[test]
    fn own_window_is_not_a_destination() {
        let d = FakeDesktop::new(app("self", 1, 1), FieldKind::Editable, true);
        assert!(capture_destination(&d).is_none());
    }
}
