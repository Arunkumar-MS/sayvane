//! The authoritative dictation state machine.
//!
//! Idle → Starting → Recording → SavingAudio → Transcribing → Polishing? →
//! SavingTranscript → Delivering? → Result → Idle
//!
//! Every asynchronous step re-checks the session generation and cancel flag before it
//! mutates text, state, the clipboard, or another application.

use crate::audio::{self, CaptureEvent, CaptureResult, MAX_DICTATION_MS};
use crate::core::{Core, CoreRef};
use crate::db::{DictationRow, JournalRow};
use crate::gesture::GestureAction;
use crate::insertion::{self, Target};
use crate::ipc_types::*;
use crate::speech::worker::{SpeechJob, LIVE_DEADLINE};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const RESULT_VISIBLE: Duration = Duration::from_millis(2600);
/// Canceled captures shorter than this are discarded rather than kept in History.
pub const MEANINGFUL_AUDIO_MS: i64 = 1000;

impl Core {
    fn set_phase(&self, gen: u64, f: impl FnOnce(&mut SessionView)) -> bool {
        let view = {
            let mut s = self.session.lock();
            if s.view.generation != gen {
                return false;
            }
            f(&mut s.view);
            s.view.clone()
        };
        self.emit(AppEvent::SessionStateChanged { session: view.clone() });
        crate::shell::on_session_changed(self, &view);
        true
    }

    fn is_current(&self, gen: u64) -> bool {
        let s = self.session.lock();
        s.view.generation == gen && !s.cancel.load(Ordering::SeqCst)
    }

    fn busy(&self, reason: &str) {
        let view = {
            let mut s = self.session.lock();
            s.view.busy_reason = Some(reason.to_string());
            s.view.clone()
        };
        self.emit(AppEvent::SessionStateChanged { session: view });
    }

    fn sound(&self, kind: crate::shell::Sound) {
        if self.settings.lock().sounds_enabled {
            crate::shell::play_sound(kind);
        }
    }

    // ---------- commands ----------

    pub fn start_dictation(self: &Arc<Self>, mode: RecordingMode) {
        if let Some(job) = self.background.lock().as_ref() {
            self.busy(&format!("{} is running. Cancel it to dictate.", job.label));
            return;
        }
        let (gen, dictation_id, session_id, snapshot) = {
            let mut s = self.session.lock();
            if !s.is_idle() {
                drop(s);
                self.busy("Finishing the current dictation…");
                return;
            }
            let db = match self.db() {
                Ok(db) => db,
                Err(e) => {
                    drop(s);
                    self.storage_failed(e.clone());
                    self.show_result_now(ResultTone::Error, "History is unavailable, so recording is disabled", None, false);
                    return;
                }
            };
            // Capture the destination before any control is shown.
            let destination = if mode == RecordingMode::Note { None } else { insertion::capture_destination(&*self.desktop) };
            let snapshot = self.settings();
            let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let session_id = uuid::Uuid::new_v4().to_string();
            let source = if mode == RecordingMode::Note { DictationSource::Note } else { DictationSource::Dictation };
            let task = crate::ipc_types::enum_to_str(&snapshot.task);
            let mut row =
                DictationRow::new(source, &snapshot.model_id, &task, serde_json::to_string(&snapshot).unwrap_or_default());
            let capture_rel = format!(".capture/{session_id}.wav");
            row.audio_state = AudioState::Capturing;
            row.destination_app_id = destination.as_ref().map(|d| d.app.app_id.clone());
            row.destination_app_name = destination.as_ref().map(|d| d.app.app_name.clone());
            // The row and journal exist before audio can become orphaned.
            if let Err(e) = db.insert(&row).and_then(|_| {
                db.insert_journal(&JournalRow {
                    session_id: session_id.clone(),
                    dictation_id: row.id.clone(),
                    capture_relative_path: capture_rel.clone(),
                    format_json: "{}".into(),
                    frames_written: 0,
                    state: "starting".into(),
                })
            }) {
                drop(s);
                self.storage_failed(e);
                self.show_result_now(ResultTone::Error, "Could not save to History, so recording did not start", None, false);
                return;
            }
            s.view = SessionView {
                phase: SessionPhase::Starting,
                session_id: Some(session_id.clone()),
                generation: gen,
                mode: Some(mode),
                elapsed_ms: None,
                destination_label: Some(match (&destination, mode) {
                    (_, RecordingMode::Note) => "History".into(),
                    (Some(d), _) => d.app.app_name.clone(),
                    (None, _) => "History".into(),
                }),
                cancel_pending: false,
                result: None,
                busy_reason: None,
            };
            s.dictation_id = Some(row.id.clone());
            s.destination = destination;
            s.snapshot = snapshot.clone();
            s.cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            s.hold_released_early = false;
            s.capture = None;
            s.capture_started = None;
            s.speech_job_id = None;
            (gen, row.id, session_id, snapshot)
        };
        self.emit(AppEvent::SessionStateChanged { session: self.session_view() });
        crate::shell::on_session_changed(self, &self.session_view());
        if let Ok(Some(row)) = self.db().and_then(|d| d.get(&dictation_id)) {
            self.emit_entry(&row);
        }

        let me = self.clone();
        std::thread::Builder::new()
            .name("session-start".into())
            .spawn(move || me.open_microphone(gen, dictation_id, session_id, snapshot))
            .ok();
    }

    fn open_microphone(self: Arc<Self>, gen: u64, dictation_id: String, session_id: String, snapshot: Settings) {
        let capture_path = self.paths.capture.join(format!("{session_id}.wav"));
        let me = self.clone();
        let sid = session_id.clone();
        let on_event: Arc<dyn Fn(CaptureEvent) + Send + Sync> = Arc::new(move |ev| match ev {
            CaptureEvent::Level(level) => me.emit(AppEvent::AudioLevel { session_id: sid.clone(), level }),
            CaptureEvent::Flushed { frames } => {
                if let Ok(db) = me.db() {
                    if let Err(e) = db.update_journal(&sid, frames as i64, "capturing") {
                        me.storage_failed(e);
                    }
                }
            }
            CaptureEvent::NearLimit => me.busy("One minute of recording left."),
            CaptureEvent::LimitReached => {
                let me2 = me.clone();
                std::thread::spawn(move || me2.finish_dictation_gen(gen));
            }
            CaptureEvent::DeviceLost(_) | CaptureEvent::WriteFailed(_) => {
                if let CaptureEvent::WriteFailed(e) = &ev {
                    me.storage_failed(format!("Recording could not be written: {e}"));
                }
                let me2 = me.clone();
                std::thread::spawn(move || me2.finish_dictation_gen(gen));
            }
        });

        let perms = self.desktop.permissions();
        if perms.microphone == PermissionState::Denied {
            self.abort_start(gen, &dictation_id, &session_id, "Microphone access is off. Turn it on in privacy settings.");
            self.emit(AppEvent::Navigate { page: Page::Home });
            return;
        }

        match audio::start(snapshot.microphone_id.clone(), capture_path, MAX_DICTATION_MS, on_event) {
            Ok(handle) => {
                let format_json = serde_json::to_string(&handle.format).unwrap_or_default();
                if let Ok(db) = self.db() {
                    let _ = db.update_journal(&session_id, 0, "capturing");
                    // Record the format so an interrupted file can be repaired.
                    let _ = db.update_journal_format(&session_id, &format_json);
                }
                let discard = {
                    let mut s = self.session.lock();
                    if s.view.generation != gen || s.view.cancel_pending || s.hold_released_early {
                        Some(handle)
                    } else {
                        s.capture = Some(handle);
                        s.capture_started = Some(Instant::now());
                        s.view.phase = SessionPhase::Recording;
                        s.view.elapsed_ms = Some(0);
                        None
                    }
                };
                match discard {
                    // Escape or key release arrived while the device was opening: stop
                    // immediately and dispose of the unusable capture.
                    Some(handle) => {
                        let _ = handle.stop();
                        self.discard_session(gen, &dictation_id, &session_id);
                    }
                    None => {
                        // The start sound plays only once capture is active.
                        self.sound(crate::shell::Sound::Start);
                        self.emit(AppEvent::SessionStateChanged { session: self.session_view() });
                        crate::shell::on_session_changed(&self, &self.session_view());
                    }
                }
            }
            Err(e) => {
                let msg = match (&e, perms.microphone) {
                    (_, PermissionState::Denied) => "Microphone access is off. Turn it on in privacy settings.".to_string(),
                    (audio::AudioError::NoDevice, _) => "No microphone found. Connect one or choose another input.".into(),
                    (audio::AudioError::DeviceMissing(_), _) => {
                        "The selected microphone is unavailable. Choose another in Settings.".into()
                    }
                    _ => e.to_string(),
                };
                self.abort_start(gen, &dictation_id, &session_id, &msg);
            }
        }
    }

    fn abort_start(&self, gen: u64, dictation_id: &str, session_id: &str, message: &str) {
        if let Ok(db) = self.db() {
            let _ = db.delete(dictation_id);
            let _ = db.delete_journal(session_id);
            self.emit(AppEvent::HistoryEntryDeleted { id: dictation_id.into() });
        }
        let _ = std::fs::remove_file(self.paths.capture.join(format!("{session_id}.wav")));
        {
            let mut s = self.session.lock();
            if s.view.generation == gen {
                s.dictation_id = None;
            }
        }
        self.finish_with_result(gen, ResultTone::Error, message, None, false);
    }

    fn discard_session(&self, gen: u64, dictation_id: &str, session_id: &str) {
        if let Ok(db) = self.db() {
            let _ = db.delete(dictation_id);
            let _ = db.delete_journal(session_id);
            self.emit(AppEvent::HistoryEntryDeleted { id: dictation_id.into() });
        }
        let _ = std::fs::remove_file(self.paths.capture.join(format!("{session_id}.wav")));
        let view = {
            let mut s = self.session.lock();
            if s.view.generation != gen {
                return;
            }
            s.dictation_id = None;
            s.capture = None;
            s.view = SessionView { generation: gen, ..SessionView::default() };
            s.view.clone()
        };
        self.emit(AppEvent::SessionStateChanged { session: view.clone() });
        crate::shell::on_session_changed(self, &view);
    }

    pub fn finish_dictation(self: &Arc<Self>) {
        let gen = self.session.lock().view.generation;
        // A stop from the mouse makes any pending key release inert.
        self.gesture.lock().reset();
        self.clone().finish_dictation_gen(gen);
    }

    fn finish_dictation_gen(self: Arc<Self>, gen: u64) {
        let (handle, dictation_id, session_id) = {
            let mut s = self.session.lock();
            if s.view.generation != gen {
                return;
            }
            match s.view.phase {
                SessionPhase::Starting => {
                    // Released while the microphone was opening: never record later.
                    if s.view.mode == Some(RecordingMode::Hold) {
                        s.hold_released_early = true;
                    } else {
                        s.view.cancel_pending = true;
                    }
                    return;
                }
                SessionPhase::Recording => {}
                _ => return,
            }
            let Some(handle) = s.capture.take() else { return };
            s.view.phase = SessionPhase::SavingAudio;
            s.view.elapsed_ms = None;
            (handle, s.dictation_id.clone().unwrap_or_default(), s.view.session_id.clone().unwrap_or_default())
        };
        self.emit(AppEvent::SessionStateChanged { session: self.session_view() });
        crate::shell::on_session_changed(&self, &self.session_view());
        let me = self.clone();
        std::thread::Builder::new()
            .name("session-pipeline".into())
            .spawn(move || me.pipeline(gen, handle, dictation_id, session_id))
            .ok();
    }

    pub fn cancel_session(self: &Arc<Self>) {
        self.gesture.lock().reset();
        let (gen, phase, handle, job, dictation_id, session_id) = {
            let mut s = self.session.lock();
            let gen = s.view.generation;
            match s.view.phase {
                SessionPhase::Idle | SessionPhase::Result => return,
                SessionPhase::Starting => {
                    s.view.cancel_pending = true;
                    drop(s);
                    self.emit(AppEvent::SessionStateChanged { session: self.session_view() });
                    return;
                }
                _ => {}
            }
            s.cancel.store(true, Ordering::SeqCst);
            (
                gen,
                s.view.phase,
                s.capture.take(),
                s.speech_job_id.clone(),
                s.dictation_id.clone(),
                s.view.session_id.clone().unwrap_or_default(),
            )
        };
        if let Some(job) = job {
            self.speech.cancel(&job);
        }
        if phase == SessionPhase::Recording {
            if let (Some(handle), Some(id)) = (handle, dictation_id) {
                let me = self.clone();
                std::thread::spawn(move || {
                    let result = handle.stop();
                    me.sound(crate::shell::Sound::Finish);
                    match result {
                        Ok(r) if r.duration_ms >= MEANINGFUL_AUDIO_MS => {
                            me.persist_audio(&id, &session_id, &r, true);
                            me.finish_with_result(gen, ResultTone::Info, "Canceled. Recording kept in History", Some(id), false);
                        }
                        _ => me.discard_session(gen, &id, &session_id),
                    }
                });
            }
        }
        // Later phases observe the cancel flag and settle themselves.
    }

    /// Moves the finished capture into managed recordings and commits its path.
    fn persist_audio(&self, dictation_id: &str, session_id: &str, r: &CaptureResult, canceled: bool) -> Option<DictationRow> {
        let db = self.db().ok()?;
        let mut row = db.get(dictation_id).ok().flatten()?;
        let src = self.paths.capture.join(format!("{session_id}.wav"));
        let rel = format!("{dictation_id}.wav");
        let dest = self.paths.recordings.join(&rel);
        if let Err(e) = std::fs::rename(&src, &dest) {
            self.storage_failed(format!("The recording could not be moved into History ({e}). It remains at {}.", src.display()));
            row.audio_state = AudioState::Interrupted;
            row.error_code = Some("audio_move_failed".into());
            row.error_message = Some("The recording could not be saved into History.".into());
            row.processing_state = ProcessingState::Failed;
            self.save_row(&row);
            return None;
        }
        crate::paths::restrict_to_user(&dest);
        row.audio_relative_path = Some(rel);
        row.audio_state = if r.interrupted.is_some() { AudioState::Interrupted } else { AudioState::Saved };
        row.duration_ms = r.duration_ms;
        row.processing_state = if canceled { ProcessingState::Canceled } else { ProcessingState::Queued };
        if canceled {
            row.insertion_state = InsertionState::Canceled;
            row.error_code = Some("canceled".into());
            row.error_message = Some("Canceled recording. Recover to transcribe it.".into());
        }
        if let Some(i) = &r.interrupted {
            row.error_message = Some(i.clone());
        }
        if !self.save_row(&row) {
            return None;
        }
        let _ = db.delete_journal(session_id);
        Some(row)
    }

    fn pipeline(self: Arc<Self>, gen: u64, handle: audio::CaptureHandle, dictation_id: String, session_id: String) {
        let capture = handle.stop();
        self.sound(crate::shell::Sound::Finish);
        let capture = match capture {
            Ok(c) => c,
            Err(e) => {
                self.storage_failed(e.to_string());
                if let Ok(Some(mut row)) = self.db().and_then(|d| d.get(&dictation_id)) {
                    row.processing_state = ProcessingState::Failed;
                    row.audio_state = AudioState::Interrupted;
                    row.error_code = Some("capture_failed".into());
                    row.error_message = Some(e.to_string());
                    self.save_row(&row);
                }
                self.finish_with_result(gen, ResultTone::Error, "The recording could not be saved", None, false);
                return;
            }
        };
        let canceled_now = !self.is_current(gen);
        let Some(mut row) = self.persist_audio(&dictation_id, &session_id, &capture, canceled_now) else {
            self.finish_with_result(gen, ResultTone::Error, "The recording could not be saved", None, false);
            return;
        };
        if canceled_now {
            self.finish_with_result(gen, ResultTone::Info, "Canceled. Recording kept in History", Some(dictation_id), false);
            return;
        }
        if capture.duration_ms < 250 {
            row.processing_state = ProcessingState::NoSpeech;
            self.save_row(&row);
            self.finish_with_result(gen, ResultTone::Info, "Too short to transcribe", Some(dictation_id), false);
            return;
        }

        // ---- recognition ----
        let snapshot = self.session.lock().snapshot.clone();
        self.set_phase(gen, |v| v.phase = SessionPhase::Transcribing);
        let job_id = uuid::Uuid::new_v4().to_string();
        self.session.lock().speech_job_id = Some(job_id.clone());
        let outcome = self.recognize(&mut row, &snapshot, &job_id, gen, false);
        self.session.lock().speech_job_id = None;
        if !self.is_current(gen) {
            let mut r = self.db().ok().and_then(|d| d.get(&dictation_id).ok().flatten()).unwrap_or(row);
            if r.processing_state != ProcessingState::Done {
                r.processing_state = ProcessingState::Canceled;
            }
            r.insertion_state = InsertionState::Canceled;
            self.save_row(&r);
            self.finish_with_result(gen, ResultTone::Info, "Canceled. Saved in History", Some(dictation_id), !r.final_text.is_empty());
            return;
        }
        if let Err(msg) = outcome {
            self.finish_with_result(gen, ResultTone::Error, &msg, Some(dictation_id), false);
            return;
        }
        if row.original_text.trim().is_empty() {
            self.finish_with_result(gen, ResultTone::Info, "No speech detected", Some(dictation_id), false);
            return;
        }

        // ---- optional polishing ----
        let mut fallback_note: Option<String> = None;
        if snapshot.polish_enabled {
            self.set_phase(gen, |v| v.phase = SessionPhase::Polishing);
            match self.polish_text(&snapshot, &row.original_text) {
                Some(Ok(polished)) => {
                    if !self.is_current(gen) {
                        // Late response after cancel: ignored.
                    } else {
                        row.polished_text = Some(polished.clone());
                        row.final_text = polished;
                        row.polish_state = PolishState::Applied;
                    }
                }
                Some(Err(e)) => {
                    row.polish_state = if e == crate::providers::ProviderError::TooLarge {
                        PolishState::Skipped
                    } else {
                        PolishState::Fallback
                    };
                    row.error_code = Some(e.code().into());
                    row.error_message = Some(crate::providers::describe(&e));
                    fallback_note = Some(if row.polish_state == PolishState::Skipped {
                        "Too long to polish; original text used".into()
                    } else {
                        "AI unavailable; original text used".into()
                    });
                }
                None => {
                    row.polish_state = PolishState::Skipped;
                    fallback_note = Some("AI polish is not set up; original text used".into());
                }
            }
        }
        if !self.is_current(gen) {
            row.insertion_state = InsertionState::Canceled;
            self.save_row(&row);
            self.finish_with_result(gen, ResultTone::Info, "Canceled. Saved in History", Some(dictation_id), true);
            return;
        }

        // ---- persist final text before delivery ----
        self.set_phase(gen, |v| v.phase = SessionPhase::SavingTranscript);
        row.processing_state = ProcessingState::Done;
        let saved = self.save_row(&row);
        if snapshot.retention == RetentionPolicy::AfterTranscription {
            self.remove_audio_for_retention(&mut row);
        }

        let mode = self.session.lock().view.mode;
        if mode == Some(RecordingMode::Note) {
            let msg = if saved { fallback_note.unwrap_or_else(|| "Saved to History".into()) } else { "History could not be saved".into() };
            let tone = if !saved { ResultTone::Error } else if row.polish_state == PolishState::Fallback { ResultTone::Warning } else { ResultTone::Success };
            self.finish_with_result(gen, tone, &msg, Some(dictation_id), true);
            return;
        }

        // ---- delivery ----
        self.set_phase(gen, |v| v.phase = SessionPhase::Delivering);
        let destination = self.session.lock().destination.clone();
        let cancel = self.session.lock().cancel.clone();
        let me = self.clone();
        let still_current = move || me.session.lock().view.generation == gen && !cancel.load(Ordering::SeqCst);
        let report = match &destination {
            Some(d) => insertion::deliver(&*self.desktop, &row.final_text, Target::Captured(d), &still_current),
            None => insertion::InsertionReport {
                state: InsertionState::SavedOnly,
                message: "Saved to History; select a text field".into(),
                left_on_clipboard: false,
                restore: None,
            },
        };
        row.insertion_state = report.state;
        let saved2 = self.save_row(&row);
        if let Some(pending) = report.restore.clone() {
            let me = self.clone();
            std::thread::spawn(move || {
                std::thread::sleep(insertion::RESTORE_DELAY);
                insertion::restore_if_untouched(&*me.desktop, &pending);
            });
        }
        let (tone, mut msg) = match report.state {
            InsertionState::Confirmed => (ResultTone::Success, report.message),
            InsertionState::SentUnconfirmed => (ResultTone::Success, report.message),
            InsertionState::Canceled => (ResultTone::Info, "Canceled. Saved in History".to_string()),
            _ => (ResultTone::Info, report.message),
        };
        let mut tone = tone;
        if let Some(note) = fallback_note {
            if row.polish_state == PolishState::Fallback {
                tone = ResultTone::Warning;
            }
            msg = format!("{note}. {msg}");
        }
        if !(saved && saved2) {
            tone = ResultTone::Error;
            msg = format!("{msg}. History could not be saved");
        }
        self.finish_with_result(gen, tone, &msg, Some(dictation_id), true);
    }

    /// Runs recognition for a row and persists the original transcript before anything
    /// else happens. Retries keep the last successful text until a new attempt succeeds.
    pub fn recognize(&self, row: &mut DictationRow, snapshot: &Settings, job_id: &str, gen: u64, file_profile: bool) -> Result<(), String> {
        let db = self.db()?;
        let snapshot_json = serde_json::to_string(snapshot).unwrap_or_default();
        let attempt = db.start_attempt(&row.id, &snapshot_json).ok();
        let finish_attempt = |outcome: &str, code: Option<&str>| {
            if let Some(a) = &attempt {
                let _ = db.finish_attempt(a, outcome, code);
            }
        };
        let fail = |row: &mut DictationRow, code: &str, msg: &str| {
            row.processing_state = if code == "canceled" { ProcessingState::Canceled } else { ProcessingState::Failed };
            row.error_code = Some(code.into());
            row.error_message = Some(msg.into());
            self.save_row(row);
        };
        let Some(model_path) = self.models.path_if_installed(&snapshot.model_id) else {
            let msg = "No speech model is installed. Install one in Settings; the recording is saved for Recover.";
            fail(row, "model_missing", msg);
            finish_attempt("failed", Some("model_missing"));
            self.emit(AppEvent::Navigate { page: Page::Settings });
            return Err("Install a speech model. Your recording is saved".into());
        };
        if snapshot.task == Task::Translate && !crate::speech::models::find(&snapshot.model_id).is_some_and(|m| m.can_translate) {
            let msg = "The selected model cannot translate. Choose a translation-capable model or switch output to original language.";
            fail(row, "incompatible_model", msg);
            finish_attempt("failed", Some("incompatible_model"));
            return Err(msg.into());
        }
        let Some(audio_path) = row.audio_relative_path.as_deref().and_then(|p| self.paths.resolve_recording(p)) else {
            fail(row, "audio_missing", "No recording is available.");
            finish_attempt("failed", Some("audio_missing"));
            return Err("No recording is available".into());
        };
        let work = self.paths.work.join(format!("{job_id}.wav"));
        if let Err(e) = crate::audio::resample::wav_to_recognizer_format(&audio_path, &work) {
            fail(row, "audio_read_failed", &e);
            finish_attempt("failed", Some("audio_read_failed"));
            return Err(e);
        }
        row.processing_state = ProcessingState::Transcribing;
        row.model_id = snapshot.model_id.clone();
        row.requested_task = enum_to_str(&snapshot.task);
        self.save_row(row);
        self.set_engine(EngineState::Busy, None);
        let job = SpeechJob {
            job_id: job_id.into(),
            generation: gen,
            model_path,
            audio_path: work.clone(),
            options: speech_options(snapshot, file_profile),
            deadline: if file_profile { crate::speech::worker::FILE_DEADLINE } else { LIVE_DEADLINE },
        };
        let result = self.speech.transcribe(&job);
        let _ = std::fs::remove_file(&work);
        self.set_engine(EngineState::Ready, None);
        match result {
            Ok(r) => {
                row.original_text = r.text.clone();
                row.final_text = r.text.clone();
                row.polished_text = None;
                row.polish_state = PolishState::Off;
                row.detected_language = r.language.clone();
                row.segments_json = serde_json::to_string(&r.segments).ok();
                row.raw_result_json = Some(r.raw.to_string());
                row.model_revision = Some(crate::speech::models::manifest().engine_version.clone());
                row.processing_state = if r.text.trim().is_empty() { ProcessingState::NoSpeech } else { ProcessingState::Polishing };
                if row.processing_state == ProcessingState::NoSpeech {
                    row.error_code = None;
                    row.error_message = None;
                } else {
                    row.error_code = None;
                    row.error_message = None;
                }
                if !self.save_row(row) {
                    finish_attempt("failed", Some("storage"));
                    return Err("The transcript could not be saved".into());
                }
                finish_attempt("succeeded", None);
                Ok(())
            }
            Err(e) => {
                let code = e.code.clone();
                // Keep any previous successful text; only state and error change.
                let msg = if e.is_canceled() { "Canceled. Recover to transcribe it.".to_string() } else { e.message.clone() };
                fail(row, &code, &msg);
                finish_attempt(if e.is_canceled() { "canceled" } else { "failed" }, Some(&code));
                Err(if e.is_canceled() { "Canceled".into() } else { format!("Transcription failed: {}", e.message) })
            }
        }
    }

    /// Returns None when polishing is enabled but no usable profile is selected.
    pub fn polish_text(&self, snapshot: &Settings, text: &str) -> Option<Result<String, crate::providers::ProviderError>> {
        let id = snapshot.active_profile_id.clone()?;
        let profile = self.profiles.lock().get(&id).cloned()?;
        // Cloned so no lock is held while waiting on the network.
        let store = self.profiles.lock().clone();
        let client = self.http.clone();
        let text = text.to_string();
        let vault = &self.vault;
        Some(tauri::async_runtime::block_on(async move {
            crate::providers::polish(&client, &store, vault, crate::providers::PolishJob { profile: &profile, transcript: &text }).await
        }))
    }

    pub fn show_result_now(&self, tone: ResultTone, message: &str, dictation_id: Option<String>, can_copy: bool) {
        let gen = self.session.lock().view.generation;
        self.finish_with_result(gen, tone, message, dictation_id, can_copy);
    }

    fn finish_with_result(&self, gen: u64, tone: ResultTone, message: &str, dictation_id: Option<String>, can_copy: bool) {
        let ok = self.set_phase(gen, |v| {
            v.phase = SessionPhase::Result;
            v.elapsed_ms = None;
            v.cancel_pending = false;
            v.result = Some(SessionResult { tone, message: message.into(), dictation_id: dictation_id.clone(), can_copy });
        });
        {
            let mut s = self.session.lock();
            if s.view.generation == gen {
                s.dictation_id = None;
                s.capture = None;
            }
        }
        if !ok {
            return;
        }
        let app = self.app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(RESULT_VISIBLE);
            if let Some(core) = crate::shell::core(&app) {
                // Starting another operation bumps the generation, voiding this timer.
                core.set_phase(gen, |v| {
                    if v.phase == SessionPhase::Result {
                        *v = SessionView { generation: gen, ..SessionView::default() };
                    }
                });
            }
        });
    }

    // ---------- keyboard ----------

    pub fn hands_free_active(&self) -> bool {
        let s = self.session.lock();
        matches!(s.view.phase, SessionPhase::Starting | SessionPhase::Recording)
            && s.view.mode == Some(RecordingMode::HandsFree)
    }

    pub fn on_gesture(self: &Arc<Self>, action: GestureAction) {
        match action {
            GestureAction::StartHold => self.start_dictation(RecordingMode::Hold),
            GestureAction::FinishHold | GestureAction::Finish => self.finish_dictation_keyboard(),
            GestureAction::DiscardTap => self.discard_tap(),
            GestureAction::StartHandsFree => self.start_dictation(RecordingMode::HandsFree),
            GestureAction::ToggleHandsFree => self.toggle_hands_free(),
        }
    }

    fn finish_dictation_keyboard(self: &Arc<Self>) {
        let gen = self.session.lock().view.generation;
        self.clone().finish_dictation_gen(gen);
    }

    /// A solitary short tap: cancel delivery and discard the sub-threshold capture.
    fn discard_tap(self: &Arc<Self>) {
        let (gen, handle, id, sid) = {
            let mut s = self.session.lock();
            if s.view.mode != Some(RecordingMode::Hold) {
                return;
            }
            match s.view.phase {
                SessionPhase::Starting => {
                    s.hold_released_early = true;
                    return;
                }
                SessionPhase::Recording => {}
                _ => return,
            }
            s.cancel.store(true, Ordering::SeqCst);
            (s.view.generation, s.capture.take(), s.dictation_id.clone(), s.view.session_id.clone())
        };
        if let (Some(h), Some(id), Some(sid)) = (handle, id, sid) {
            let me = self.clone();
            std::thread::spawn(move || {
                let r = h.stop();
                match r {
                    // Initialization can push a deliberate tap over the threshold; keep real audio.
                    Ok(r) if r.duration_ms >= MEANINGFUL_AUDIO_MS => {
                        me.persist_audio(&id, &sid, &r, true);
                        me.finish_with_result(gen, ResultTone::Info, "Canceled. Recording kept in History", Some(id), false);
                    }
                    _ => me.discard_session(gen, &id, &sid),
                }
            });
        }
    }

    pub fn toggle_hands_free(self: &Arc<Self>) {
        let action = {
            let mut s = self.session.lock();
            match (s.view.phase, s.view.mode) {
                (SessionPhase::Idle | SessionPhase::Result, _) => 0,
                (SessionPhase::Starting | SessionPhase::Recording, Some(RecordingMode::Hold)) => {
                    s.view.mode = Some(RecordingMode::HandsFree);
                    1
                }
                (SessionPhase::Recording, Some(RecordingMode::HandsFree)) => 2,
                _ => 3,
            }
        };
        match action {
            0 => self.start_dictation(RecordingMode::HandsFree),
            1 => self.emit(AppEvent::SessionStateChanged { session: self.session_view() }),
            2 => self.finish_dictation_keyboard(),
            _ => self.busy("Finishing the current dictation…"),
        }
    }

    pub fn tick(&self) {
        let recording = matches!(self.session.lock().view.phase, SessionPhase::Recording);
        if recording {
            self.emit(AppEvent::SessionStateChanged { session: self.session_view() });
        }
    }
}

pub fn speech_options(s: &Settings, file_profile: bool) -> serde_json::Value {
    let a = &s.advanced;
    let beam = if file_profile { a.beam_size.or(Some(5)) } else { a.beam_size };
    let mut o = serde_json::json!({
        "task": enum_to_str(&s.task),
        "language": s.language,
        "initialPrompt": s.vocabulary_hints.trim(),
        "temperature": a.temperature,
        "decoding": if beam.is_some() { "beam" } else { "greedy" },
        "wordTimestamps": file_profile || a.word_timestamps,
        "noContext": a.no_context,
        "noSpeechThreshold": a.no_speech_threshold,
        "threads": a.threads,
    });
    if let Some(b) = beam {
        o["beamSize"] = serde_json::json!(b.min(8));
    }
    o
}

pub fn core_ref(c: &CoreRef) -> &Core {
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_dictation_uses_greedy_without_timestamps() {
        let s = Settings::default();
        let o = speech_options(&s, false);
        assert_eq!(o["decoding"], "greedy");
        assert_eq!(o["wordTimestamps"], false);
        assert_eq!(o["temperature"], 0.0);
        assert!(o["language"].is_null());
        assert!(o.get("beamSize").is_none());
    }

    #[test]
    fn file_jobs_use_accurate_profile_with_timestamps() {
        let o = speech_options(&Settings::default(), true);
        assert_eq!(o["decoding"], "beam");
        assert_eq!(o["beamSize"], 5);
        assert_eq!(o["wordTimestamps"], true);
    }
}
