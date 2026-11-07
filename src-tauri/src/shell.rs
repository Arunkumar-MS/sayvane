//! Desktop shell: main window, floating bar, tray, global shortcuts, and sounds.

use crate::core::{Core, CoreRef};
use crate::gesture::GestureAction;
use crate::ipc_types::*;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const MAIN: &str = "main";
pub const BAR: &str = "bar";
const BAR_W: f64 = 440.0;
const BAR_H: f64 = 92.0;
const BAR_MARGIN: f64 = 6.0;

pub fn core(app: &AppHandle) -> Option<CoreRef> {
    app.try_state::<CoreRef>().map(|s| s.inner().clone())
}

// ---------- sounds ----------

#[derive(Debug, Clone, Copy)]
pub enum Sound {
    Start,
    Finish,
}

pub fn play_sound(kind: Sound) {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2::msg_send;
        use objc2_foundation::NSString;
        let name = match kind {
            Sound::Start => "Tink",
            Sound::Finish => "Pop",
        };
        unsafe {
            if let Some(cls) = AnyClass::get(c"NSSound") {
                let s: *mut AnyObject = msg_send![cls, soundNamed: &*NSString::from_str(name)];
                if !s.is_null() {
                    let _: () = msg_send![s, setVolume: 0.25f32];
                    let _: bool = msg_send![s, play];
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = kind; // Windows: silent by default; system sounds are too loud for this cue.
}

// ---------- tray icon drawing ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Recording,
    Processing,
}

/// Draws a 36×36 RGBA glyph. On macOS it is a template (alpha only); elsewhere it uses
/// the brand accent so it reads on light and dark taskbars.
fn tray_image(state: TrayState) -> Image<'static> {
    const N: usize = 36;
    let (r, g, b) = if cfg!(target_os = "macos") { (0, 0, 0) } else { (0xE8, 0x57, 0x3A) };
    let mut px = vec![0u8; N * N * 4];
    let mut cover = |x: usize, y: usize, a: f32| {
        let i = (y * N + x) * 4;
        let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
        if a > px[i + 3] {
            px[i..i + 4].copy_from_slice(&[r, g, b, a]);
        }
    };
    // Signed-distance helpers, sampled at pixel centers for anti-aliasing.
    let capsule = |px: f32, py: f32, cx: f32, top: f32, bot: f32, rad: f32| -> f32 {
        let y = py.clamp(top, bot);
        ((px - cx).powi(2) + (py - y).powi(2)).sqrt() - rad
    };
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = match state {
                TrayState::Idle => [(9.0, 13.0, 23.0), (18.0, 7.0, 29.0), (27.0, 11.0, 25.0)]
                    .iter()
                    .map(|&(cx, t, b)| capsule(fx, fy, cx, t, b, 2.6))
                    .fold(f32::MAX, f32::min),
                TrayState::Recording => {
                    let ring = (((fx - 18.0).powi(2) + (fy - 18.0).powi(2)).sqrt() - 13.0).abs() - 1.6;
                    let dot = ((fx - 18.0).powi(2) + (fy - 18.0).powi(2)).sqrt() - 7.5;
                    ring.min(dot)
                }
                TrayState::Processing => [9.0, 18.0, 27.0]
                    .iter()
                    .map(|&cx| ((fx - cx).powi(2) + (fy - 18.0).powi(2)).sqrt() - 3.4)
                    .fold(f32::MAX, f32::min),
            };
            cover(x, y, 0.5 - d);
        }
    }
    Image::new_owned(px, N as u32, N as u32)
}

// ---------- tray ----------

pub struct TrayHandles {
    pub icon: TrayIcon,
    pub status: MenuItem<tauri::Wry>,
    pub toggle: MenuItem<tauri::Wry>,
    pub paste: MenuItem<tauri::Wry>,
    pub copy: MenuItem<tauri::Wry>,
    pub polish: CheckMenuItem<tauri::Wry>,
    pub state: Mutex<Option<TrayState>>,
}

fn build_tray(app: &AppHandle, initial: &CoreRef) -> tauri::Result<TrayHandles> {
    let name = crate::brand::name();
    let status = MenuItem::with_id(app, "status", format!("{name}: Ready"), false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Start dictation", true, None::<&str>)?;
    let paste = MenuItem::with_id(app, "paste", "Paste last transcript", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "copy", "Copy last transcript", true, None::<&str>)?;
    let polish = CheckMenuItem::with_id(app, "polish", "AI polishing", true, initial.settings().polish_enabled, None::<&str>)?;
    let home = MenuItem::with_id(app, "home", format!("Open {name}"), true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", format!("Quit {name}"), true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[&status, &sep()?, &toggle, &paste, &copy, &sep()?, &polish, &sep()?, &home, &history, &settings, &sep()?, &quit],
    )?;
    let icon = TrayIconBuilder::with_id("main-tray")
        .icon(tray_image(TrayState::Idle))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(format!("{name} — ready"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let Some(core) = core(app) else { return };
            match event.id.as_ref() {
                "toggle" => {
                    let phase = core.session.lock().view.phase;
                    match phase {
                        SessionPhase::Idle | SessionPhase::Result => core.start_dictation(RecordingMode::HandsFree),
                        SessionPhase::Recording | SessionPhase::Starting => core.finish_dictation(),
                        _ => {}
                    }
                }
                "paste" => {
                    let _ = core.paste_last();
                }
                "copy" => {
                    let _ = core.copy_last();
                }
                "polish" => {
                    let next = !core.settings().polish_enabled;
                    let _ = set_polish(&core, next);
                }
                "home" => show_main(app, Page::Home),
                "history" => show_main(app, Page::History),
                "settings" => show_main(app, Page::Settings),
                "quit" => {
                    core.speech.shutdown();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(TrayHandles { icon, status, toggle, paste, copy, polish, state: Mutex::new(Some(TrayState::Idle)) })
}

/// Turning polish on requires a configured provider; otherwise route to Settings.
pub fn set_polish(core: &CoreRef, enabled: bool) -> Result<Settings, String> {
    if enabled {
        let s = core.settings();
        let configured = s
            .active_profile_id
            .as_deref()
            .and_then(|id| core.profiles.lock().get(id).cloned())
            .is_some_and(|p| !p.model_id.is_empty());
        if !configured {
            show_main(&core.app, Page::Settings);
            return Err("Choose an AI provider and model in Settings › AI Polish first.".into());
        }
    }
    let s = core.update_settings(SettingsPatch { polish_enabled: Some(enabled), ..Default::default() })?;
    if let Some(t) = core.app.try_state::<TrayHandles>() {
        let _ = t.polish.set_checked(s.polish_enabled);
    }
    Ok(s)
}

/// Keeps tray, Escape registration, and enabled actions in line with the session.
pub fn on_session_changed(core: &Core, view: &SessionView) {
    let busy = !matches!(view.phase, SessionPhase::Idle | SessionPhase::Result);
    if let Some(t) = core.app.try_state::<TrayHandles>() {
        let state = match view.phase {
            SessionPhase::Starting | SessionPhase::Recording => TrayState::Recording,
            SessionPhase::Idle | SessionPhase::Result => TrayState::Idle,
            _ => TrayState::Processing,
        };
        let mut cur = t.state.lock();
        if *cur != Some(state) {
            *cur = Some(state);
            let _ = t.icon.set_icon(Some(tray_image(state)));
            let _ = t.icon.set_icon_as_template(cfg!(target_os = "macos"));
            let name = crate::brand::name();
            let (label, tip) = match state {
                TrayState::Idle => ("Ready", "ready"),
                TrayState::Recording => ("Recording", "recording"),
                TrayState::Processing => ("Processing", "processing"),
            };
            let _ = t.status.set_text(format!("{name}: {label}"));
            let _ = t.icon.set_tooltip(Some(format!("{name} — {tip}")));
        }
        let recording = matches!(view.phase, SessionPhase::Starting | SessionPhase::Recording);
        let _ = t.toggle.set_text(if recording { "Finish dictation" } else { "Start dictation" });
        // No overlapping capture or delivery; Copy of a saved transcript stays available.
        let _ = t.toggle.set_enabled(!busy || recording);
        let _ = t.paste.set_enabled(!busy);
    }
    #[cfg(not(target_os = "macos"))]
    set_escape_registered(core, busy);
    #[cfg(target_os = "macos")]
    let _ = busy;
}

// ---------- main window ----------

pub fn show_main(app: &AppHandle, page: Page) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    } else {
        let name = crate::brand::name().to_string();
        let built = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
            .title(name)
            .inner_size(980.0, 680.0)
            .min_inner_size(880.0, 600.0)
            .resizable(true)
            .center()
            .visible(true)
            .build();
        if let Ok(w) = built {
            let app2 = app.clone();
            w.on_window_event(move |e| match e {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Closing leaves the tray app running; Quit is explicit.
                    api.prevent_close();
                    if let Some(w) = app2.get_webview_window(MAIN) {
                        let _ = w.hide();
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let a = app2.clone();
                        let _ = a.run_on_main_thread(|| crate::platform::macos::set_dock_visible(false));
                    }
                }
                tauri::WindowEvent::Focused(true) => {
                    if let Some(c) = core(&app2) {
                        c.refresh_permissions();
                    }
                }
                _ => {}
            });
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Become a regular app first, then show: a window ordered front while the
        // activation policy is accessory/prohibited stays off screen.
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            crate::platform::macos::set_dock_visible(true);
            if let Some(w) = a.get_webview_window(MAIN) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        });
    }
    if let Some(c) = core(app) {
        // Delay so a freshly created window has subscribed before navigation arrives.
        let c2 = c.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            c2.emit(AppEvent::Navigate { page });
        });
        c.refresh_permissions();
    }
}

// ---------- floating bar ----------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct OverlayPositions {
    positions: BTreeMap<String, (f64, f64)>,
}

pub struct BarState {
    pub hit: Mutex<(f64, f64, f64, f64)>,
    pub ignoring: AtomicBool,
    save_pending: AtomicBool,
}

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(BarPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

fn monitor_key(m: &tauri::Monitor) -> String {
    let s = m.size();
    format!("{}:{}x{}", m.name().cloned().unwrap_or_default(), s.width, s.height)
}

fn load_positions(core: &Core) -> OverlayPositions {
    std::fs::read(core.paths.overlay_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Bottom-center of the work area, or a remembered spot clamped into the visible area.
fn bar_position(app: &AppHandle, core: &Core, ignore_saved: bool) -> Option<(LogicalPosition<f64>, String)> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let wa = monitor.work_area();
    let (wx, wy) = (wa.position.x as f64 / scale, wa.position.y as f64 / scale);
    let (ww, wh) = (wa.size.width as f64 / scale, wa.size.height as f64 / scale);
    let key = monitor_key(&monitor);
    let saved = if ignore_saved { None } else { load_positions(core).positions.get(&key).copied() };
    let (mut x, mut y) = saved.unwrap_or((wx + (ww - BAR_W) / 2.0, wy + wh - BAR_H - BAR_MARGIN));
    x = x.clamp(wx, wx + ww - BAR_W);
    y = y.clamp(wy, wy + wh - BAR_H);
    Some((LogicalPosition::new(x, y), key))
}

pub fn create_bar(app: &AppHandle, core: &CoreRef) -> tauri::Result<()> {
    let (pos, _) = bar_position(app, core, false).unwrap_or((LogicalPosition::new(200.0, 600.0), String::new()));
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::{CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};
        let panel = PanelBuilder::<_, BarPanel>::new(app, BAR)
            .url(WebviewUrl::App("bar.html".into()))
            .title("Recording bar")
            .size(tauri::Size::Logical(LogicalSize::new(BAR_W, BAR_H)))
            .position(tauri::Position::Logical(pos))
            .level(PanelLevel::Floating)
            .transparent(true)
            .opaque(false)
            .has_shadow(false)
            .no_activate(true)
            .hides_on_deactivate(false)
            .becomes_key_only_if_needed(true)
            .collection_behavior(CollectionBehavior::new().can_join_all_spaces().full_screen_auxiliary().stationary())
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| w.decorations(false).resizable(false).skip_taskbar(true).shadow(false).focused(false))
            .build()?;
        panel.show();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let w = WebviewWindowBuilder::new(app, BAR, WebviewUrl::App("bar.html".into()))
            .title("Recording bar")
            .inner_size(BAR_W, BAR_H)
            .position(pos.x, pos.y)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(true)
            .build()?;
        #[cfg(windows)]
        if let Ok(h) = w.hwnd() {
            crate::platform::windows::make_non_activating(h.0 as isize);
        }
    }
    app.manage(BarState { hit: Mutex::new((0.0, 0.0, BAR_W, BAR_H)), ignoring: AtomicBool::new(false), save_pending: AtomicBool::new(false) });
    if let Some(w) = app.get_webview_window(BAR) {
        let app2 = app.clone();
        w.on_window_event(move |e| match e {
            tauri::WindowEvent::Moved(_) => schedule_save_bar_position(&app2),
            tauri::WindowEvent::ScaleFactorChanged { .. } => {
                let a = app2.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    clamp_bar(&a);
                });
            }
            _ => {}
        });
    }
    start_hit_test_loop(app.clone());
    Ok(())
}

fn schedule_save_bar_position(app: &AppHandle) {
    let Some(state) = app.try_state::<BarState>() else { return };
    if state.save_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        if let Some(st) = app.try_state::<BarState>() {
            st.save_pending.store(false, Ordering::SeqCst);
        }
        let (Some(core), Some(w)) = (core(&app), app.get_webview_window(BAR)) else { return };
        let (Ok(pos), Ok(Some(mon))) = (w.outer_position(), w.current_monitor()) else { return };
        let scale = mon.scale_factor();
        let mut all = load_positions(&core);
        all.positions.insert(monitor_key(&mon), (pos.x as f64 / scale, pos.y as f64 / scale));
        if let Ok(bytes) = serde_json::to_vec(&all) {
            let _ = crate::paths::atomic_write(&core.paths.overlay_file(), &bytes);
        }
    });
}

fn clamp_bar(app: &AppHandle) {
    let (Some(core), Some(w)) = (core(app), app.get_webview_window(BAR)) else { return };
    if let Some((pos, _)) = bar_position(app, &core, false) {
        let _ = w.set_position(tauri::Position::Logical(pos));
    }
}

pub fn reset_bar_position(app: &AppHandle) {
    let (Some(core), Some(w)) = (core(app), app.get_webview_window(BAR)) else { return };
    let _ = std::fs::remove_file(core.paths.overlay_file());
    if let Some((pos, _)) = bar_position(app, &core, true) {
        let _ = w.set_position(tauri::Position::Logical(pos));
    }
}

pub fn set_bar_hit_region(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) {
    if let Some(st) = app.try_state::<BarState>() {
        *st.hit.lock() = (x, y, w, h);
    }
}

/// Lets clicks on the transparent part of the bar window reach whatever is beneath it.
fn start_hit_test_loop(app: AppHandle) {
    std::thread::Builder::new()
        .name("bar-hit-test".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let (Some(w), Some(st)) = (app.get_webview_window(BAR), app.try_state::<BarState>()) else { continue };
            let (Ok(cursor), Ok(origin), Ok(scale)) = (app.cursor_position(), w.inner_position(), w.scale_factor()) else {
                continue;
            };
            let (hx, hy, hw, hh) = *st.hit.lock();
            let lx = (cursor.x - origin.x as f64) / scale;
            let ly = (cursor.y - origin.y as f64) / scale;
            let inside = lx >= hx && lx <= hx + hw && ly >= hy && ly <= hy + hh;
            let ignore = !inside;
            if st.ignoring.swap(ignore, Ordering::SeqCst) != ignore {
                let _ = w.set_ignore_cursor_events(ignore);
            }
        })
        .ok();
}

pub fn show_bar_menu(app: &AppHandle) -> tauri::Result<()> {
    let Some(core) = core(app) else { return Ok(()) };
    let Some(w) = app.get_webview_window(BAR) else { return Ok(()) };
    let polish = CheckMenuItem::with_id(app, "bar-polish", "AI polishing", true, core.settings().polish_enabled, None::<&str>)?;
    let history = MenuItem::with_id(app, "bar-history", "History", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "bar-copy", "Copy last transcript", true, None::<&str>)?;
    let paste = MenuItem::with_id(app, "bar-paste", "Paste last transcript", core.session.lock().is_idle(), None::<&str>)?;
    let reset = MenuItem::with_id(app, "bar-reset", "Reset bar position", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&polish, &history, &PredefinedMenuItem::separator(app)?, &copy, &paste, &PredefinedMenuItem::separator(app)?, &reset])?;
    w.popup_menu(&menu)
}

pub fn handle_menu_event(app: &AppHandle, id: &str) {
    let Some(core) = core(app) else { return };
    match id {
        "bar-polish" => {
            let next = !core.settings().polish_enabled;
            let _ = set_polish(&core, next);
        }
        "bar-history" => show_main(app, Page::History),
        "bar-copy" => {
            let _ = core.copy_last();
        }
        "bar-paste" => {
            let _ = core.paste_last();
        }
        "bar-reset" => reset_bar_position(app),
        _ => {}
    }
}

// ---------- global shortcuts ----------

fn parse(accel: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(accel).map_err(|e| format!("“{accel}” is not a valid shortcut ({e})"))
}

fn is_fn(accel: &str) -> bool {
    accel.eq_ignore_ascii_case("fn")
}

/// Registers every binding except Escape (only while a session runs) and the macOS Fn
/// gesture (handled by the event tap). Conflicts are reported, never forced.
pub fn register_shortcuts(core: &Core) -> Vec<ShortcutStatus> {
    let gs = core.app.global_shortcut();
    let _ = gs.unregister_all();
    let mut out = Vec::new();
    for b in core.settings().shortcuts {
        let (registered, error) = if b.action == ShortcutAction::Cancel {
            (true, None)
        } else if is_fn(&b.accelerator) {
            if cfg!(target_os = "macos") {
                (core.settings().fn_gesture_enabled, None)
            } else {
                (false, Some("The Fn key is not available as a shortcut on Windows.".into()))
            }
        } else if core.shortcuts_suspended.load(Ordering::SeqCst) {
            (false, Some("Paused while you record a new shortcut.".into()))
        } else {
            match parse(&b.accelerator).and_then(|s| gs.register(s).map_err(|e| e.to_string())) {
                Ok(()) => (true, None),
                Err(e) => (false, Some(format!("Could not register — another app may be using it ({e}).")) ),
            }
        };
        out.push(ShortcutStatus { action: b.action, accelerator: b.accelerator, registered, error });
    }
    *core.shortcuts.lock() = out.clone();
    core.emit(AppEvent::ShortcutsChanged { shortcuts: out.clone() });
    out
}

#[cfg(not(target_os = "macos"))]
fn set_escape_registered(core: &Core, on: bool) {
    let Some(esc) = core.settings().shortcuts.into_iter().find(|b| b.action == ShortcutAction::Cancel) else { return };
    let Ok(s) = parse(&esc.accelerator) else { return };
    let gs = core.app.global_shortcut();
    let registered = gs.is_registered(s);
    if on && !registered {
        let _ = gs.register(s);
    } else if !on && registered {
        let _ = gs.unregister(s);
    }
}

/// Validates a replacement by registering it before committing; the old binding is
/// restored on failure.
pub fn set_shortcut(core: &Core, action: ShortcutAction, accel: &str) -> Result<Vec<ShortcutStatus>, String> {
    let accel = accel.trim();
    if !is_fn(accel) {
        let new = parse(accel)?;
        let mods = new.mods;
        let is_bare_key = mods.is_empty() && action != ShortcutAction::Cancel;
        if is_bare_key {
            return Err("Add a modifier such as Control or Alt so ordinary typing is not captured.".into());
        }
    } else if !cfg!(target_os = "macos") || action != ShortcutAction::PushToTalk {
        return Err("Fn can only be used for push to talk on macOS.".into());
    }
    let mut s = core.settings();
    if s.shortcuts.iter().any(|b| b.action != action && b.accelerator.eq_ignore_ascii_case(accel)) {
        return Err("That shortcut is already used for another action.".into());
    }
    if !is_fn(accel) && action != ShortcutAction::Cancel {
        let gs = core.app.global_shortcut();
        let sc = parse(accel)?;
        if !gs.is_registered(sc) {
            gs.register(sc).map_err(|e| format!("Could not register — another app may be using it ({e})."))?;
            let _ = gs.unregister(sc);
        }
    }
    for b in s.shortcuts.iter_mut() {
        if b.action == action {
            b.accelerator = accel.to_string();
        }
    }
    let patch = SettingsPatch {
        shortcuts: Some(s.shortcuts.clone()),
        fn_gesture_enabled: is_fn(accel).then_some(true),
        ..Default::default()
    };
    core.update_settings(patch)?;
    Ok(register_shortcuts(core))
}

pub fn handle_shortcut(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    let Some(core) = core(app) else { return };
    let binding = core.settings().shortcuts.into_iter().find(|b| parse(&b.accelerator).ok().as_ref() == Some(shortcut));
    let Some(b) = binding else { return };
    let pressed = state == ShortcutState::Pressed;
    match b.action {
        ShortcutAction::PushToTalk => {
            let now = core.now_ms();
            let action = if pressed {
                let hf = core.hands_free_active();
                core.gesture.lock().key_down(now, hf)
            } else {
                // Finishes on release of the primary key, whatever order modifiers lift in.
                core.gesture.lock().key_up(now)
            };
            if let Some(a) = action {
                core.on_gesture(a);
            }
        }
        ShortcutAction::ToggleHandsFree if pressed => core.toggle_hands_free(),
        ShortcutAction::PasteLast if pressed => {
            let _ = core.paste_last();
        }
        ShortcutAction::CopyLast if pressed => {
            let _ = core.copy_last();
        }
        ShortcutAction::Cancel if pressed => core.cancel_session(),
        _ => {}
    }
}

#[cfg(target_os = "macos")]
struct FnHandler(AppHandle);

#[cfg(target_os = "macos")]
impl crate::platform::macos::KeyHandler for FnHandler {
    fn fn_down(&self) {
        let Some(core) = core(&self.0) else { return };
        if !core.settings().fn_gesture_enabled || core.shortcuts_suspended.load(Ordering::SeqCst) {
            return;
        }
        let hf = core.hands_free_active();
        let a = core.gesture.lock().key_down(core.now_ms(), hf);
        if let Some(a) = a {
            std::thread::spawn(move || core.on_gesture(a));
        }
    }
    fn fn_up(&self) {
        let Some(core) = core(&self.0) else { return };
        let a = core.gesture.lock().key_up(core.now_ms());
        if let Some(a) = a {
            std::thread::spawn(move || core.on_gesture(a));
        }
    }
    fn space_down(&self) -> bool {
        let Some(core) = core(&self.0) else { return false };
        if !core.settings().fn_gesture_enabled {
            return false;
        }
        let (consume, action) = core.gesture.lock().space_down();
        if let Some(GestureAction::ToggleHandsFree) = action {
            std::thread::spawn(move || core.toggle_hands_free());
        }
        consume
    }
    fn escape_down(&self) -> bool {
        let Some(core) = core(&self.0) else { return false };
        let active = !core.session.lock().is_idle();
        if active {
            std::thread::spawn(move || core.cancel_session());
        }
        active
    }
}

// ---------- setup ----------

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_data_dir()?;
    let paths = crate::paths::Paths::new(root)?;
    let core = Core::new(app.clone(), paths);
    app.manage(core.clone());
    core.reconcile();

    let tray = build_tray(app, &core)?;
    app.manage(tray);
    if let Err(e) = create_bar(app, &core) {
        log::error!("floating bar unavailable: {e}");
    }
    register_shortcuts(&core);

    #[cfg(target_os = "macos")]
    {
        let status = crate::platform::macos::start_key_tap(Arc::new(FnHandler(app.clone())));
        if let crate::platform::macos::TapStatus::Unavailable(msg) = status {
            log::warn!("{msg}");
            let mut list = core.shortcuts.lock();
            for s in list.iter_mut().filter(|s| is_fn(&s.accelerator)) {
                s.registered = false;
                s.error = Some(msg.clone());
            }
        }
        let c = core.clone();
        crate::platform::macos::watch_sleep_and_lock(Arc::new(move |why| {
            let c = c.clone();
            log::info!("system {why}: stopping any capture");
            std::thread::spawn(move || {
                // Finalize what was captured; never restart the microphone on wake.
                if matches!(c.session.lock().view.phase, SessionPhase::Recording | SessionPhase::Starting) {
                    c.finish_dictation();
                }
            });
        }));
    }

    // Elapsed-time ticks while recording (UI also interpolates locally).
    let c = core.clone();
    std::thread::Builder::new()
        .name("session-tick".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            c.tick();
        })?;

    // Retention runs daily while the app is open.
    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(6 * 3600));
        c.apply_retention();
    });

    core.warm_in_background();

    // First launch, or anything needing attention: open Home.
    let s = core.settings();
    let needs_setup = !core.models.is_installed(&s.model_id) || core.permissions.lock().microphone != PermissionState::Granted;
    if needs_setup || core.storage_error.lock().is_some() {
        show_main(app, Page::Home);
    } else {
        #[cfg(target_os = "macos")]
        crate::platform::macos::set_dock_visible(false);
    }
    Ok(())
}
