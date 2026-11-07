//! Application core: owns storage, settings, services, and the event bus.
//! The dictation state machine lives in `session.rs`; background jobs in `jobs.rs`.

use crate::credentials::OsVault;
use crate::db::{Db, DictationRow};
use crate::gesture::Gesture;
use crate::insertion::{Desktop, DestinationToken};
use crate::ipc_types::*;
use crate::paths::Paths;
use crate::providers::ProfileStore;
use crate::speech::models::ModelManager;
use crate::speech::worker::SpeechService;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

pub struct SessionInner {
    pub view: SessionView,
    pub dictation_id: Option<String>,
    pub capture: Option<crate::audio::CaptureHandle>,
    pub capture_started: Option<Instant>,
    pub destination: Option<DestinationToken>,
    pub snapshot: Settings,
    pub cancel: Arc<AtomicBool>,
    pub hold_released_early: bool,
    pub speech_job_id: Option<String>,
}

impl SessionInner {
    pub fn new() -> Self {
        Self {
            view: SessionView::default(),
            dictation_id: None,
            capture: None,
            capture_started: None,
            destination: None,
            snapshot: Settings::default(),
            cancel: Arc::new(AtomicBool::new(false)),
            hold_released_early: false,
            speech_job_id: None,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.view.phase, SessionPhase::Idle | SessionPhase::Result)
    }
}

/// A background job that uses the speech engine (recovery or file import).
#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub label: String,
    pub cancel: Arc<AtomicBool>,
    pub speech_job_id: Option<String>,
}

pub struct Core {
    pub app: AppHandle,
    pub paths: Paths,
    pub db: Option<Db>,
    pub storage_error: Mutex<Option<String>>,
    pub settings: Mutex<Settings>,
    pub settings_warning: Mutex<Option<String>>,
    pub profiles: Mutex<ProfileStore>,
    pub vault: OsVault,
    pub models: ModelManager,
    pub speech: Arc<SpeechService>,
    pub desktop: Box<dyn Desktop>,
    pub http: reqwest::Client,
    pub session: Mutex<SessionInner>,
    pub gesture: Mutex<Gesture>,
    pub generation: AtomicU64,
    seq: AtomicU64,
    pub engine: Mutex<EngineStatus>,
    pub shortcuts: Mutex<Vec<ShortcutStatus>>,
    pub shortcuts_suspended: AtomicBool,
    pub imports: Mutex<Vec<ImportJob>>,
    pub background: Mutex<Option<BackgroundJob>>,
    pub permissions: Mutex<PermissionStatus>,
    pub epoch: Instant,
}

pub type CoreRef = Arc<Core>;

impl Core {
    pub fn new(app: AppHandle, paths: Paths) -> Arc<Self> {
        let loaded = crate::settings::load(&paths.settings_file());
        let (db, storage_error) = match Db::open(&paths.database_file()) {
            Ok(db) => (Some(db), None),
            Err(e) => {
                log::error!("history database unavailable: {e}");
                (None, Some(format!("{e} Location: {}", paths.database_file().display())))
            }
        };
        let profiles = ProfileStore::load(&paths.profiles_file());
        let models = ModelManager::new(paths.models.clone());
        let desktop = crate::platform::desktop();
        let permissions = desktop.permissions();
        let app_for_progress = app.clone();
        let speech = SpeechService::new(
            SpeechService::default_exe(),
            Arc::new(move |job, _gen, pct| {
                log::debug!("speech progress {job} {pct:.0}%");
                let _ = &app_for_progress;
            }),
        );
        let core = Arc::new(Self {
            app,
            paths,
            db,
            storage_error: Mutex::new(storage_error),
            settings: Mutex::new(loaded.settings),
            settings_warning: Mutex::new(loaded.warning),
            profiles: Mutex::new(profiles),
            vault: OsVault::new(),
            models,
            speech,
            desktop,
            http: crate::providers::http_client(),
            session: Mutex::new(SessionInner::new()),
            gesture: Mutex::new(Gesture::default()),
            generation: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            engine: Mutex::new(EngineStatus { state: EngineState::Cold, model_id: None, backend: None, message: None }),
            shortcuts: Mutex::new(Vec::new()),
            shortcuts_suspended: AtomicBool::new(false),
            imports: Mutex::new(Vec::new()),
            background: Mutex::new(None),
            permissions: Mutex::new(permissions),
            epoch: Instant::now(),
        });
        core.refresh_engine_status();
        core
    }

    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    // ---------- events ----------

    pub fn emit(&self, event: AppEvent) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let env = EventEnvelope { schema_version: 1, seq, event };
        let _ = self.app.emit(EVENT_CHANNEL, env);
    }

    pub fn event_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    pub fn storage_failed(&self, message: impl Into<String>) {
        let m = message.into();
        log::error!("storage: {m}");
        *self.storage_error.lock() = Some(m.clone());
        self.emit(AppEvent::StorageError { message: m });
    }

    pub fn db(&self) -> Result<&Db, String> {
        self.db.as_ref().ok_or_else(|| {
            self.storage_error.lock().clone().unwrap_or_else(|| "History is unavailable.".into())
        })
    }

    pub fn entry_for(&self, row: &DictationRow) -> HistoryEntry {
        let exists = row
            .audio_relative_path
            .as_deref()
            .and_then(|p| self.paths.resolve_recording(p))
            .is_some_and(|p| p.exists());
        row.to_entry(exists)
    }

    pub fn emit_entry(&self, row: &DictationRow) {
        self.emit(AppEvent::HistoryEntryUpdated { entry: self.entry_for(row) });
    }

    /// Saves a row and announces it; a failure is surfaced, never swallowed.
    pub fn save_row(&self, row: &DictationRow) -> bool {
        match self.db().and_then(|db| db.save(row)) {
            Ok(()) => {
                self.emit_entry(row);
                true
            }
            Err(e) => {
                self.storage_failed(e);
                false
            }
        }
    }

    // ---------- settings ----------

    pub fn settings(&self) -> Settings {
        self.settings.lock().clone()
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> Result<Settings, String> {
        let next = {
            let cur = self.settings.lock();
            crate::settings::apply_patch(&cur, patch)?
        };
        crate::settings::save(&self.paths.settings_file(), &next).map_err(|e| format!("Settings could not be saved: {e}"))?;
        *self.settings.lock() = next.clone();
        self.emit(AppEvent::SettingsChanged { settings: next.clone() });
        self.refresh_engine_status();
        Ok(next)
    }

    // ---------- engine ----------

    pub fn refresh_engine_status(&self) {
        let s = self.settings();
        let installed = self.models.is_installed(&s.model_id);
        let loaded = self.speech.loaded_model();
        let next = if !installed {
            EngineStatus {
                state: EngineState::NoModel,
                model_id: Some(s.model_id.clone()),
                backend: None,
                message: Some("Install a speech model to start dictating.".into()),
            }
        } else {
            let mut cur = self.engine.lock().clone();
            let loaded_this = loaded.is_some() && loaded == self.models.path_if_installed(&s.model_id);
            if cur.model_id.as_deref() != Some(&s.model_id) || !matches!(cur.state, EngineState::Loading | EngineState::Busy) {
                cur.state = if loaded_this { EngineState::Ready } else { EngineState::Cold };
            }
            cur.model_id = Some(s.model_id.clone());
            cur.backend = self.speech.backend();
            cur.message = None;
            cur
        };
        let changed = {
            let mut e = self.engine.lock();
            let changed = e.state != next.state || e.model_id != next.model_id || e.backend != next.backend;
            *e = next.clone();
            changed
        };
        if changed {
            self.emit(AppEvent::EngineChanged { engine: next });
        }
    }

    pub fn set_engine(&self, state: EngineState, message: Option<String>) {
        let next = {
            let mut e = self.engine.lock();
            e.state = state;
            e.message = message;
            e.backend = self.speech.backend();
            e.clone()
        };
        self.emit(AppEvent::EngineChanged { engine: next });
    }

    /// Loads the selected model in the background if it is installed. Never downloads.
    pub fn warm_in_background(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || {
            let s = me.settings();
            let Some(path) = me.models.path_if_installed(&s.model_id) else { return };
            if me.speech.loaded_model().as_deref() == Some(path.as_path()) {
                return;
            }
            me.set_engine(EngineState::Loading, Some("Loading speech model…".into()));
            match me.speech.warm(&path) {
                Ok(_) => me.set_engine(EngineState::Ready, None),
                Err(e) => me.set_engine(EngineState::Error, Some(e.message)),
            }
        });
    }

    // ---------- permissions ----------

    pub fn refresh_permissions(&self) {
        let p = self.desktop.permissions();
        let changed = {
            let mut cur = self.permissions.lock();
            let changed = *cur != p;
            *cur = p.clone();
            changed
        };
        if changed {
            self.emit(AppEvent::PermissionsChanged { permissions: p });
        }
    }

    // ---------- snapshot ----------

    pub fn snapshot(&self) -> AppSnapshot {
        let storage_error = self.storage_error.lock().clone();
        AppSnapshot {
            brand: crate::brand::brand().clone(),
            platform: crate::platform::platform_info(),
            session: self.session_view(),
            settings: self.settings(),
            permissions: self.permissions.lock().clone(),
            engine: self.engine.lock().clone(),
            shortcuts: self.shortcuts.lock().clone(),
            storage: StorageInfo {
                data_dir: self.paths.root.display().to_string(),
                ok: storage_error.is_none(),
                error: storage_error,
            },
            history_count: self.db.as_ref().and_then(|d| d.count().ok()).unwrap_or(0),
            imports: self.imports.lock().clone(),
            event_seq: self.event_seq(),
            settings_warning: self.settings_warning.lock().clone(),
        }
    }

    pub fn session_view(&self) -> SessionView {
        let s = self.session.lock();
        let mut v = s.view.clone();
        if matches!(v.phase, SessionPhase::Recording) {
            v.elapsed_ms = s.capture_started.map(|t| t.elapsed().as_millis() as u64);
        }
        v
    }

    // ---------- startup reconciliation ----------

    /// Finalizes interrupted captures and pending file operations from a previous run.
    /// Nothing is transcribed, polished, played, or pasted automatically.
    pub fn reconcile(&self) {
        let Ok(db) = self.db() else { return };
        match db.open_journals() {
            Ok(journals) => {
                for j in journals {
                    let Some(capture) = self.paths.resolve_recording(&j.capture_relative_path) else { continue };
                    let Ok(Some(mut row)) = db.get(&j.dictation_id) else {
                        let _ = db.delete_journal(&j.session_id);
                        continue;
                    };
                    if capture.exists() {
                        let format: Option<crate::audio::CaptureFormat> = serde_json::from_str(&j.format_json).ok();
                        let repaired = format.as_ref().map(|f| crate::audio::repair_wav(&capture, f));
                        let rel = format!("{}.wav", row.id);
                        let dest = self.paths.recordings.join(&rel);
                        match repaired {
                            Some(Ok(frames)) if frames > 0 && std::fs::rename(&capture, &dest).is_ok() => {
                                let f = format.expect("checked");
                                row.audio_relative_path = Some(rel);
                                row.audio_state = AudioState::Interrupted;
                                row.duration_ms = (frames * 1000 / f.sample_rate.max(1) as u64) as i64;
                                row.error_code = Some("interrupted".into());
                                row.error_message =
                                    Some("Recording was interrupted. The audio up to that point is saved; Recover to transcribe it.".into());
                            }
                            Some(Err(e)) => {
                                row.audio_state = AudioState::Interrupted;
                                row.error_code = Some("capture_damaged".into());
                                row.error_message = Some(format!("The interrupted recording could not be repaired: {e} The file was kept."));
                            }
                            _ => {
                                row.audio_state = AudioState::Missing;
                                row.error_code = Some("capture_empty".into());
                                row.error_message = Some("The recording was interrupted before any audio was saved.".into());
                            }
                        }
                    } else {
                        row.audio_state = AudioState::Missing;
                        row.error_code = Some("capture_missing".into());
                        row.error_message = Some("The recording was interrupted before any audio was saved.".into());
                    }
                    row.processing_state = ProcessingState::Interrupted;
                    if db.save(&row).is_ok() {
                        let _ = db.delete_journal(&j.session_id);
                    }
                }
            }
            Err(e) => self.storage_failed(e),
        }
        if let Err(e) = db.mark_interrupted() {
            self.storage_failed(e);
        }
        if let Ok(ops) = db.pending_file_ops() {
            for op in ops {
                if op.op == "delete" {
                    if let Some(p) = self.paths.resolve_recording(&op.relative_path) {
                        match std::fs::remove_file(&p) {
                            Ok(()) => {
                                let _ = db.complete_file_op(&op.id);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                let _ = db.complete_file_op(&op.id);
                            }
                            Err(e) => {
                                let _ = db.fail_file_op(&op.id, &e.to_string());
                            }
                        }
                    }
                }
            }
        }
        self.apply_retention();
    }

    pub fn apply_retention(&self) {
        let days = match self.settings().retention {
            RetentionPolicy::Days7 => 7,
            RetentionPolicy::Days30 => 30,
            _ => return,
        };
        let Ok(db) = self.db() else { return };
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Ok(rows) = db.audio_older_than(&cutoff) {
            for mut r in rows {
                self.remove_audio_for_retention(&mut r);
            }
        }
    }

    pub fn remove_audio_for_retention(&self, r: &mut DictationRow) {
        let Some(rel) = r.audio_relative_path.clone() else { return };
        if let Some(p) = self.paths.resolve_recording(&rel) {
            if std::fs::remove_file(&p).is_ok() || !p.exists() {
                r.audio_state = AudioState::RemovedByRetention;
                self.save_row(r);
            }
        }
    }

    /// Deletes an entry and its managed audio. Imported originals are never touched,
    /// because only the managed copy is referenced.
    pub fn delete_entry(&self, id: &str) -> Result<(), String> {
        let db = self.db()?;
        let row = db.get(id)?.ok_or("That entry no longer exists.")?;
        if self.session.lock().dictation_id.as_deref() == Some(id) {
            return Err("This entry is still being recorded or processed.".into());
        }
        let op = match &row.audio_relative_path {
            Some(rel) => Some(db.add_file_op(Some(id), "delete", rel)?),
            None => None,
        };
        db.delete(id)?;
        self.emit(AppEvent::HistoryEntryDeleted { id: id.into() });
        if let (Some(op), Some(rel)) = (op, &row.audio_relative_path) {
            let path = self.paths.resolve_recording(rel);
            match path.map(std::fs::remove_file) {
                Some(Ok(())) | None => {
                    let _ = db.complete_file_op(&op);
                }
                Some(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    let _ = db.complete_file_op(&op);
                }
                Some(Err(e)) => {
                    let _ = db.fail_file_op(&op, &e.to_string());
                    self.storage_failed(format!(
                        "The entry was removed, but its audio file could not be deleted ({e}). It will be retried at next launch."
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn delete_all(&self, confirm: &str) -> Result<(), String> {
        if confirm != "DELETE" {
            return Err("Type DELETE to confirm.".into());
        }
        if !self.session.lock().is_idle() || self.background.lock().is_some() {
            return Err("Wait for the current recording or job to finish.".into());
        }
        self.db()?.delete_all()?;
        let mut failures = 0;
        if let Ok(rd) = std::fs::read_dir(&self.paths.recordings) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && std::fs::remove_file(&p).is_err() {
                    failures += 1;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.paths.work);
        let _ = std::fs::create_dir_all(&self.paths.work);
        let defaults = Settings::default();
        let _ = crate::settings::save(&self.paths.settings_file(), &defaults);
        *self.settings.lock() = defaults.clone();
        self.emit(AppEvent::HistoryCleared);
        self.emit(AppEvent::SettingsChanged { settings: defaults });
        if failures > 0 {
            self.storage_failed(format!("{failures} recording file(s) could not be deleted from {}.", self.paths.recordings.display()));
        }
        Ok(())
    }
}
