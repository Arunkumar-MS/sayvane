//! Background speech jobs (Recover/Retry, file imports) and History actions.
//! Jobs run one at a time, only while no dictation is active, and never paste.

use crate::core::{BackgroundJob, Core};
use crate::db::DictationRow;
use crate::insertion::{self, Target};
use crate::ipc_types::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl Core {
    fn claim_background(&self, label: &str) -> Result<Arc<AtomicBool>, String> {
        if !self.session.lock().is_idle() {
            return Err("Finish the current dictation first.".into());
        }
        let mut bg = self.background.lock();
        if let Some(j) = bg.as_ref() {
            return Err(format!("{} is already running.", j.label));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *bg = Some(BackgroundJob { label: label.into(), cancel: cancel.clone(), speech_job_id: None });
        Ok(cancel)
    }

    fn release_background(&self) {
        *self.background.lock() = None;
    }

    /// Recover (no transcript yet) or Retry (re-run). Updates the same entry; the last
    /// successful text stays until a new attempt succeeds. Never pastes.
    pub fn recover_entry(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let row = self.db()?.get(id)?.ok_or("That entry no longer exists.")?;
        let entry = self.entry_for(&row);
        if !entry.can_retry {
            return Err(entry.retry_unavailable_reason.unwrap_or_else(|| "This entry cannot be retried.".into()));
        }
        self.claim_background("A retry")?;
        let me = self.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            let snapshot = me.settings();
            let job_id = uuid::Uuid::new_v4().to_string();
            if let Some(bg) = me.background.lock().as_mut() {
                bg.speech_job_id = Some(job_id.clone());
            }
            let Ok(Some(mut row)) = me.db().and_then(|d| d.get(&id)) else {
                me.release_background();
                return;
            };
            let previous = row.clone();
            let gen = me.generation.load(Ordering::SeqCst);
            let file_profile = row.source == DictationSource::Import;
            match me.recognize(&mut row, &snapshot, &job_id, gen, file_profile) {
                Ok(()) => {
                    if snapshot.polish_enabled && !row.original_text.trim().is_empty() {
                        match me.polish_text(&snapshot, &row.original_text) {
                            Some(Ok(p)) => {
                                row.polished_text = Some(p.clone());
                                row.final_text = p;
                                row.polish_state = PolishState::Applied;
                            }
                            Some(Err(e)) => {
                                row.polish_state = PolishState::Fallback;
                                row.error_code = Some(e.code().into());
                                row.error_message = Some(crate::providers::describe(&e));
                            }
                            None => row.polish_state = PolishState::Skipped,
                        }
                    }
                    if row.processing_state != ProcessingState::NoSpeech {
                        row.processing_state = ProcessingState::Done;
                    }
                    me.save_row(&row);
                }
                Err(_) => {
                    // Keep the last good transcript visible alongside the new error.
                    if !previous.original_text.trim().is_empty() {
                        row.original_text = previous.original_text;
                        row.polished_text = previous.polished_text;
                        row.final_text = previous.final_text;
                        row.segments_json = previous.segments_json;
                        row.processing_state = ProcessingState::Done;
                        me.save_row(&row);
                    }
                }
            }
            me.release_background();
        });
        Ok(())
    }

    pub fn cancel_background(&self) {
        if let Some(j) = self.background.lock().as_ref() {
            j.cancel.store(true, Ordering::SeqCst);
            if let Some(job) = &j.speech_job_id {
                self.speech.cancel(job);
            }
        }
    }

    // ---------- imports ----------

    fn set_import(&self, job_id: &str, f: impl FnOnce(&mut ImportJob)) {
        let list = {
            let mut l = self.imports.lock();
            if let Some(j) = l.iter_mut().find(|j| j.job_id == job_id) {
                f(j);
            }
            l.clone()
        };
        self.emit(AppEvent::ImportsChanged { imports: list });
    }

    pub fn queue_imports(self: &Arc<Self>, files: Vec<PathBuf>) -> Result<usize, String> {
        self.db()?;
        let mut added = Vec::new();
        {
            let mut l = self.imports.lock();
            l.retain(|j| matches!(j.state, ImportState::Queued | ImportState::Copying | ImportState::Transcribing));
            for f in files {
                let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "audio".into());
                let supported = crate::audio::decode::SUPPORTED_EXTENSIONS.contains(&ext.as_str());
                let job = ImportJob {
                    job_id: uuid::Uuid::new_v4().to_string(),
                    file_name: name,
                    state: if supported { ImportState::Queued } else { ImportState::Failed },
                    dictation_id: None,
                    error: (!supported).then(|| "Unsupported format. Use WAV, MP3, M4A, FLAC, or OGG.".into()),
                };
                if supported {
                    added.push((job.job_id.clone(), f));
                }
                l.push(job);
            }
        }
        self.emit(AppEvent::ImportsChanged { imports: self.imports.lock().clone() });
        let n = added.len();
        if n > 0 {
            let me = self.clone();
            std::thread::spawn(move || me.run_imports(added));
        }
        Ok(n)
    }

    fn run_imports(self: Arc<Self>, files: Vec<(String, PathBuf)>) {
        for (job_id, src) in files {
            // Sequential, and only while idle.
            let cancel = loop {
                match self.claim_background("An import") {
                    Ok(c) => break c,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
                }
            };
            if self.imports.lock().iter().any(|j| j.job_id == job_id && j.state == ImportState::Canceled) {
                self.release_background();
                continue;
            }
            let r = self.import_one(&job_id, &src, &cancel);
            match r {
                Ok(id) => self.set_import(&job_id, |j| {
                    j.state = ImportState::Done;
                    j.dictation_id = Some(id);
                }),
                Err(e) if cancel.load(Ordering::SeqCst) => self.set_import(&job_id, |j| {
                    j.state = ImportState::Canceled;
                    j.error = Some(e);
                }),
                Err(e) => self.set_import(&job_id, |j| {
                    j.state = ImportState::Failed;
                    j.error = Some(e);
                }),
            }
            let canceled_all = cancel.load(Ordering::SeqCst);
            self.release_background();
            if canceled_all {
                // Cancel stops the current file; the rest of the queue is canceled too
                // so the app is free for dictation.
                let ids: Vec<String> = self
                    .imports
                    .lock()
                    .iter()
                    .filter(|j| j.state == ImportState::Queued)
                    .map(|j| j.job_id.clone())
                    .collect();
                for id in ids {
                    self.set_import(&id, |j| j.state = ImportState::Canceled);
                }
                break;
            }
        }
    }

    fn import_one(&self, job_id: &str, src: &std::path::Path, cancel: &Arc<AtomicBool>) -> Result<String, String> {
        self.set_import(job_id, |j| j.state = ImportState::Copying);
        let snapshot = self.settings();
        let db = self.db()?;
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("audio").to_lowercase();
        let mut row = DictationRow::new(
            DictationSource::Import,
            &snapshot.model_id,
            &enum_to_str(&snapshot.task),
            serde_json::to_string(&snapshot).unwrap_or_default(),
        );
        row.import_name = src.file_name().map(|n| n.to_string_lossy().into_owned());
        row.processing_state = ProcessingState::Queued;
        // Copy into managed storage first; the user's original is never modified or deleted.
        let rel = format!("{}.{}", row.id, ext);
        let dest = self.paths.recordings.join(&rel);
        std::fs::copy(src, &dest).map_err(|e| format!("The file could not be copied: {e}"))?;
        crate::paths::restrict_to_user(&dest);
        row.audio_relative_path = Some(rel);
        row.audio_state = AudioState::Saved;
        if let Err(e) = db.insert(&row) {
            let _ = std::fs::remove_file(&dest);
            self.storage_failed(e.clone());
            return Err(e);
        }
        self.emit_entry(&row);
        self.set_import(job_id, |j| {
            j.state = ImportState::Transcribing;
            j.dictation_id = Some(row.id.clone());
        });

        // Decode to a 16 kHz WAV in the work directory, then recognize.
        let work = self.paths.work.join(format!("{}-import.wav", row.id));
        let c2 = cancel.clone();
        let decoded = crate::audio::decode::decode_to_recognizer_format(&dest, &work, &move || c2.load(Ordering::SeqCst));
        let decoded = match decoded {
            Ok(d) => d,
            Err(e) => {
                let _ = std::fs::remove_file(&work);
                row.processing_state = if cancel.load(Ordering::SeqCst) { ProcessingState::Canceled } else { ProcessingState::Failed };
                row.error_code = Some("decode_failed".into());
                row.error_message = Some(e.clone());
                self.save_row(&row);
                return Err(e);
            }
        };
        row.duration_ms = decoded.duration_ms;
        // Recognition reads the managed copy through the converter; point it at the decoded WAV.
        let decoded_rel = format!("{}.wav", row.id);
        let decoded_dest = self.paths.recordings.join(&decoded_rel);
        if ext != "wav" {
            std::fs::rename(&work, &decoded_dest).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&dest);
            row.audio_relative_path = Some(decoded_rel);
        } else {
            let _ = std::fs::remove_file(&work);
        }
        self.save_row(&row);
        let job_id_speech = uuid::Uuid::new_v4().to_string();
        if let Some(bg) = self.background.lock().as_mut() {
            bg.speech_job_id = Some(job_id_speech.clone());
        }
        if cancel.load(Ordering::SeqCst) {
            row.processing_state = ProcessingState::Canceled;
            self.save_row(&row);
            return Err("Canceled".into());
        }
        let gen = self.generation.load(Ordering::SeqCst);
        self.recognize(&mut row, &snapshot, &job_id_speech, gen, true)?;
        if row.processing_state != ProcessingState::NoSpeech {
            row.processing_state = ProcessingState::Done;
        }
        self.save_row(&row);
        Ok(row.id)
    }

    // ---------- clipboard actions ----------

    pub fn copy_entry(&self, id: &str, variant: TextVariant) -> Result<(), String> {
        let row = self.db()?.get(id)?.ok_or("That entry no longer exists.")?;
        let text = row.text_variant(variant).ok_or("There is no polished text for this entry.")?;
        if text.trim().is_empty() {
            return Err("This entry has no text to copy.".into());
        }
        // Explicit Copy intentionally replaces the clipboard and is not restored.
        self.desktop.clipboard_set_text(text).map(|_| ())
    }

    pub fn copy_last(&self) -> Result<(), String> {
        let row = self.db()?.latest_with_text()?.ok_or("There is no transcript to copy yet.")?;
        self.copy_entry(&row.id, TextVariant::Final)
    }

    /// Pastes the latest transcript into the field the user has focused now, with the
    /// same eligibility checks as automatic delivery.
    pub fn paste_last(self: &Arc<Self>) -> Result<(), String> {
        if !self.session.lock().is_idle() {
            return Err("Finish the current dictation first.".into());
        }
        let row = self.db()?.latest_with_text()?.ok_or("There is no transcript to paste yet.")?;
        let me = self.clone();
        std::thread::spawn(move || {
            let report = insertion::deliver(&*me.desktop, &row.final_text, Target::Current, &|| true);
            if let Some(pending) = report.restore.clone() {
                std::thread::sleep(insertion::RESTORE_DELAY);
                insertion::restore_if_untouched(&*me.desktop, &pending);
            }
            let tone = match report.state {
                InsertionState::Confirmed | InsertionState::SentUnconfirmed => ResultTone::Success,
                _ => ResultTone::Info,
            };
            me.show_result_now(tone, &report.message, Some(row.id.clone()), true);
        });
        Ok(())
    }

    pub fn export_entry(&self, id: &str, format: ExportFormat, variant: TextVariant, dest: &std::path::Path) -> Result<(), String> {
        let row = self.db()?.get(id)?.ok_or("That entry no longer exists.")?;
        let body = crate::export::render(&row, format, variant)?;
        crate::paths::atomic_write(dest, body.as_bytes()).map_err(|e| format!("The export could not be written: {e}"))
    }

    pub fn suggested_export_name(&self, id: &str, format: ExportFormat) -> String {
        let row = self.db().ok().and_then(|d| d.get(id).ok().flatten());
        let stem = row
            .as_ref()
            .and_then(|r| r.import_name.clone().map(|n| n.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(n)))
            .or_else(|| {
                row.as_ref().map(|r| {
                    let words: Vec<&str> = r.final_text.split_whitespace().take(6).collect();
                    if words.is_empty() { "transcript".into() } else { words.join(" ") }
                })
            })
            .unwrap_or_else(|| "transcript".into());
        format!("{}.{}", crate::paths::sanitize_file_stem(&stem), crate::export::extension(format))
    }
}
