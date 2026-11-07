//! Versioned, non-secret settings document with atomic persistence and recovery.

use crate::ipc_types::*;
use crate::paths::atomic_write;
use serde_json::Value;
use std::path::Path;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MODEL_ID: &str = "base";

pub fn default_shortcuts() -> Vec<ShortcutBinding> {
    let b = |action, accelerator: &str| ShortcutBinding { action, accelerator: accelerator.to_string() };
    if cfg!(target_os = "macos") {
        vec![
            b(ShortcutAction::PushToTalk, "Fn"),
            b(ShortcutAction::ToggleHandsFree, "Control+Alt+Shift+Space"),
            b(ShortcutAction::PasteLast, "Control+Super+V"),
            b(ShortcutAction::CopyLast, "Control+Super+C"),
            b(ShortcutAction::Cancel, "Escape"),
        ]
    } else {
        vec![
            b(ShortcutAction::PushToTalk, "Control+Alt+Space"),
            b(ShortcutAction::ToggleHandsFree, "Control+Alt+Shift+Space"),
            b(ShortcutAction::PasteLast, "Control+Alt+V"),
            b(ShortcutAction::CopyLast, "Control+Alt+C"),
            b(ShortcutAction::Cancel, "Escape"),
        ]
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            model_id: DEFAULT_MODEL_ID.into(),
            language: None,
            task: Task::Transcribe,
            vocabulary_hints: String::new(),
            microphone_id: None,
            advanced: AdvancedRecognition::default(),
            polish_enabled: false,
            active_profile_id: None,
            shortcuts: default_shortcuts(),
            fn_gesture_enabled: cfg!(target_os = "macos"),
            retention: RetentionPolicy::Keep,
            launch_at_login: false,
            reduced_motion: ReducedMotion::System,
            sounds_enabled: true,
        }
    }
}

pub struct LoadedSettings {
    pub settings: Settings,
    pub warning: Option<String>,
}

/// Loads settings. A malformed file is backed up, valid fields are salvaged one by one,
/// and AI polishing is never switched on by recovery.
pub fn load(path: &Path) -> LoadedSettings {
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return LoadedSettings { settings: Settings::default(), warning: None };
        }
        Err(e) => {
            return LoadedSettings {
                settings: Settings::default(),
                warning: Some(format!("Settings could not be read ({e}); defaults are in use.")),
            }
        }
    };
    if let Ok(s) = serde_json::from_slice::<Settings>(&raw) {
        if s.schema_version == SETTINGS_SCHEMA_VERSION {
            return LoadedSettings { settings: validate(s), warning: None };
        }
    }
    let backup = path.with_extension(format!("invalid-{}.json", chrono::Utc::now().format("%Y%m%dT%H%M%S")));
    let _ = std::fs::write(&backup, &raw);
    let (settings, lost) = salvage(&raw);
    let warning = Some(if lost.is_empty() {
        "Settings were migrated to the current format.".to_string()
    } else {
        format!(
            "Some settings were unreadable and were reset: {}. A backup was kept at {}.",
            lost.join(", "),
            backup.display()
        )
    });
    LoadedSettings { settings, warning }
}

fn salvage(raw: &[u8]) -> (Settings, Vec<String>) {
    let mut out = serde_json::to_value(Settings::default()).expect("defaults serialize");
    let mut lost = Vec::new();
    match serde_json::from_slice::<Value>(raw) {
        Ok(Value::Object(map)) => {
            for (k, v) in map {
                if k == "schemaVersion" || k == "polishEnabled" {
                    continue;
                }
                let mut candidate = out.clone();
                candidate[&k] = v;
                if serde_json::from_value::<Settings>(candidate.clone()).is_ok() {
                    out = candidate;
                } else {
                    lost.push(k);
                }
            }
        }
        _ => lost.push("all settings".into()),
    }
    let s: Settings = serde_json::from_value(out).unwrap_or_default();
    (validate(Settings { polish_enabled: false, ..s }), lost)
}

fn validate(mut s: Settings) -> Settings {
    s.schema_version = SETTINGS_SCHEMA_VERSION;
    s.advanced.temperature = s.advanced.temperature.clamp(0.0, 1.0);
    s.advanced.no_speech_threshold = s.advanced.no_speech_threshold.clamp(0.0, 1.0);
    s.advanced.threads = s.advanced.threads.min(32);
    s.advanced.beam_size = s.advanced.beam_size.map(|b| b.clamp(1, 10));
    if s.vocabulary_hints.len() > 2000 {
        s.vocabulary_hints = s.vocabulary_hints.chars().take(2000).collect();
    }
    for d in default_shortcuts() {
        if !s.shortcuts.iter().any(|b| b.action == d.action) {
            s.shortcuts.push(d);
        }
    }
    s
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?;
    atomic_write(path, &bytes)
}

/// Applies a patch, validating before it replaces the working configuration.
pub fn apply_patch(current: &Settings, patch: SettingsPatch) -> Result<Settings, String> {
    let mut s = current.clone();
    if let Some(v) = patch.model_id {
        if v.trim().is_empty() {
            return Err("Choose a speech model.".into());
        }
        s.model_id = v;
    }
    if let Some(v) = patch.language {
        if let Some(code) = &v {
            if code.is_empty() || code.len() > 8 || !code.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
                return Err("Language must be a language code such as en or pt-BR.".into());
            }
        }
        s.language = v;
    }
    if let Some(v) = patch.task {
        s.task = v;
    }
    if let Some(v) = patch.vocabulary_hints {
        s.vocabulary_hints = v;
    }
    if let Some(v) = patch.microphone_id {
        s.microphone_id = v;
    }
    if let Some(v) = patch.advanced {
        s.advanced = v;
    }
    if let Some(v) = patch.polish_enabled {
        s.polish_enabled = v;
    }
    if let Some(v) = patch.active_profile_id {
        s.active_profile_id = v;
    }
    if let Some(v) = patch.shortcuts {
        s.shortcuts = v;
    }
    if let Some(v) = patch.fn_gesture_enabled {
        s.fn_gesture_enabled = v && cfg!(target_os = "macos");
    }
    if let Some(v) = patch.retention {
        s.retention = v;
    }
    if let Some(v) = patch.launch_at_login {
        s.launch_at_login = v;
    }
    if let Some(v) = patch.reduced_motion {
        s.reduced_motion = v;
    }
    if let Some(v) = patch.sounds_enabled {
        s.sounds_enabled = v;
    }
    Ok(validate(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_has_polish_off_and_balanced_model() {
        let s = Settings::default();
        assert!(!s.polish_enabled);
        assert_eq!(s.model_id, "base");
        assert_eq!(s.retention, RetentionPolicy::Keep);
        assert!(!s.launch_at_login);
    }

    #[test]
    fn roundtrip_and_persisted_polish_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("settings.json");
        let s = Settings { polish_enabled: true, ..Settings::default() };
        save(&f, &s).unwrap();
        let l = load(&f);
        assert!(l.warning.is_none());
        assert!(l.settings.polish_enabled);
    }

    #[test]
    fn malformed_settings_are_salvaged_without_enabling_polish() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("settings.json");
        std::fs::write(&f, br#"{"modelId":"small","polishEnabled":true,"retention":"forever","language":"de"}"#).unwrap();
        let l = load(&f);
        assert_eq!(l.settings.model_id, "small");
        assert_eq!(l.settings.language.as_deref(), Some("de"));
        assert_eq!(l.settings.retention, RetentionPolicy::Keep);
        assert!(!l.settings.polish_enabled);
        assert!(l.warning.unwrap().contains("retention"));
        assert!(std::fs::read_dir(dir.path()).unwrap().count() >= 2, "backup kept");
    }

    #[test]
    fn garbage_settings_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("settings.json");
        std::fs::write(&f, b"\x00not json").unwrap();
        let l = load(&f);
        assert_eq!(l.settings, Settings::default());
        assert!(l.warning.is_some());
    }

    #[test]
    fn invalid_patch_is_rejected_without_changing_settings() {
        let s = Settings::default();
        let bad = SettingsPatch { language: Some(Some("en; rm -rf".into())), ..Default::default() };
        assert!(apply_patch(&s, bad).is_err());
        let ok = apply_patch(&s, SettingsPatch { language: Some(Some("pt-BR".into())), ..Default::default() }).unwrap();
        assert_eq!(ok.language.as_deref(), Some("pt-BR"));
    }
}
