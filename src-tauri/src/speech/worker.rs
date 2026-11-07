//! Client for the bundled `sayvane-speech` worker (newline-delimited JSON over stdio).
//! See speech-worker/README.md for the protocol.

use crate::ipc_types::Segment;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u64 = 1;
pub const LIVE_DEADLINE: Duration = Duration::from_secs(300);
pub const FILE_DEADLINE: Duration = Duration::from_secs(30 * 60);
pub const IDLE_UNLOAD_AFTER: Duration = Duration::from_secs(10 * 60);
const CANCEL_GRACE: Duration = Duration::from_secs(3);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const WARM_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechError {
    pub code: String,
    pub message: String,
}

impl SpeechError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
    pub fn is_canceled(&self) -> bool {
        self.code == "canceled"
    }
}

impl std::fmt::Display for SpeechError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<Segment>,
    pub duration_seconds: f64,
    pub backend: String,
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct SpeechJob {
    pub job_id: String,
    pub generation: u64,
    pub model_path: PathBuf,
    pub audio_path: PathBuf,
    pub options: Value,
    pub deadline: Duration,
}

type ProgressFn = Arc<dyn Fn(&str, u64, f64) + Send + Sync>;

struct Process {
    child: Child,
    stdin: ChildStdin,
    pending: Arc<Mutex<HashMap<String, Sender<Value>>>>,
    alive: Arc<AtomicBool>,
    #[cfg(windows)]
    _job: Option<win_job::JobHandle>,
}

impl Process {
    fn spawn(exe: &Path, on_progress: ProgressFn) -> Result<Self, SpeechError> {
        let mut cmd = Command::new(exe);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            SpeechError::new("worker_missing", format!("The local speech engine could not start ({e}). Reinstall the app."))
        })?;
        #[cfg(windows)]
        let job = win_job::assign(&child);
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let pending: Arc<Mutex<HashMap<String, Sender<Value>>>> = Default::default();
        let alive = Arc::new(AtomicBool::new(true));

        let (p2, a2) = (pending.clone(), alive.clone());
        std::thread::Builder::new()
            .name("speech-stdout".into())
            .spawn(move || {
                let reader = BufReader::with_capacity(64 * 1024, stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.len() > 8 * 1024 * 1024 {
                        log::warn!("speech worker: oversized message dropped");
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(&line) else {
                        log::warn!("speech worker: malformed message dropped");
                        continue;
                    };
                    if v["type"] == "progress" {
                        if let (Some(job), Some(gen), Some(pct)) =
                            (v["jobId"].as_str(), v["generation"].as_u64(), v["percent"].as_f64())
                        {
                            on_progress(job, gen, pct);
                        }
                        continue;
                    }
                    if let Some(id) = v["requestId"].as_str() {
                        if let Some(tx) = p2.lock().remove(id) {
                            let _ = tx.send(v);
                        }
                    }
                }
                a2.store(false, Ordering::SeqCst);
                p2.lock().clear(); // wakes every waiter with Disconnected
            })
            .map_err(|e| SpeechError::new("internal", e.to_string()))?;
        std::thread::Builder::new()
            .name("speech-stderr".into())
            .spawn(move || {
                // Drained continuously; contents may include paths, so only at debug level.
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::debug!("speech worker: {}", line.chars().take(300).collect::<String>());
                }
            })
            .map_err(|e| SpeechError::new("internal", e.to_string()))?;

        Ok(Self {
            child,
            stdin,
            pending,
            alive,
            #[cfg(windows)]
            _job: job,
        })
    }

    fn send(&mut self, mut msg: Value) -> Result<mpsc::Receiver<Value>, SpeechError> {
        let id = uuid::Uuid::new_v4().to_string();
        msg["protocolVersion"] = json!(PROTOCOL_VERSION);
        msg["requestId"] = json!(id);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().insert(id.clone(), tx);
        let mut line = serde_json::to_vec(&msg).map_err(|e| SpeechError::new("internal", e.to_string()))?;
        line.push(b'\n');
        if let Err(e) = self.stdin.write_all(&line).and_then(|_| self.stdin.flush()) {
            self.pending.lock().remove(&id);
            return Err(SpeechError::new("worker_crashed", format!("The speech engine stopped unexpectedly ({e}).")));
        }
        Ok(rx)
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive.store(false, Ordering::SeqCst);
    }
}

fn response_error(v: &Value) -> SpeechError {
    SpeechError::new(
        v["error"]["code"].as_str().unwrap_or("internal"),
        v["error"]["message"].as_str().unwrap_or("The speech engine reported an error."),
    )
}

pub struct SpeechService {
    exe: PathBuf,
    proc: Mutex<Option<Process>>,
    /// Serializes warm/transcribe/unload; the worker rejects overlap with `busy`.
    job_lock: Mutex<()>,
    loaded_model: Mutex<Option<PathBuf>>,
    backend: Mutex<Option<String>>,
    last_used: Mutex<Instant>,
    cancel_requested: Mutex<Option<String>>,
    on_progress: ProgressFn,
}

impl SpeechService {
    pub fn new(exe: PathBuf, on_progress: ProgressFn) -> Arc<Self> {
        let s = Arc::new(Self {
            exe,
            proc: Mutex::new(None),
            job_lock: Mutex::new(()),
            loaded_model: Mutex::new(None),
            backend: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
            cancel_requested: Mutex::new(None),
            on_progress,
        });
        let weak = Arc::downgrade(&s);
        std::thread::Builder::new()
            .name("speech-idle".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let Some(s) = weak.upgrade() else { break };
                if s.loaded_model.lock().is_some() && s.last_used.lock().elapsed() > IDLE_UNLOAD_AFTER {
                    if let Some(_g) = s.job_lock.try_lock() {
                        let _ = s.control(json!({"command": "unload"}), CONTROL_TIMEOUT);
                        *s.loaded_model.lock() = None;
                        log::info!("speech model unloaded after idle timeout");
                    }
                }
            })
            .ok();
        s
    }

    /// Resolves the sidecar next to the main executable (Tauri places it there).
    pub fn default_exe() -> PathBuf {
        let name = if cfg!(windows) { "sayvane-speech.exe" } else { "sayvane-speech" };
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(name)))
            .unwrap_or_else(|| PathBuf::from(name))
    }

    pub fn loaded_model(&self) -> Option<PathBuf> {
        self.loaded_model.lock().clone()
    }

    pub fn backend(&self) -> Option<String> {
        self.backend.lock().clone()
    }

    fn ensure_process(&self) -> Result<(), SpeechError> {
        let mut guard = self.proc.lock();
        let dead = guard.as_ref().is_none_or(|p| !p.alive.load(Ordering::SeqCst));
        if dead {
            if let Some(mut old) = guard.take() {
                old.kill();
            }
            *self.loaded_model.lock() = None;
            let mut p = Process::spawn(&self.exe, self.on_progress.clone())?;
            let rx = p.send(json!({"command": "hello"}))?;
            let hello = rx
                .recv_timeout(CONTROL_TIMEOUT)
                .map_err(|_| SpeechError::new("worker_crashed", "The speech engine did not start correctly."))?;
            if hello["cpuSupported"] == false {
                p.kill();
                return Err(SpeechError::new(
                    "unsupported_cpu",
                    "This computer's processor is not supported by the bundled speech engine build.",
                ));
            }
            *guard = Some(p);
        }
        Ok(())
    }

    fn restart(&self) {
        if let Some(mut p) = self.proc.lock().take() {
            p.kill();
        }
        *self.loaded_model.lock() = None;
    }

    fn control(&self, msg: Value, timeout: Duration) -> Result<Value, SpeechError> {
        self.ensure_process()?;
        let rx = self.proc.lock().as_mut().expect("process").send(msg)?;
        match rx.recv_timeout(timeout) {
            Ok(v) if v["ok"] == true => Ok(v),
            Ok(v) => Err(response_error(&v)),
            Err(RecvTimeoutError::Timeout) => {
                self.restart();
                Err(SpeechError::new("timeout", "The speech engine stopped responding and was restarted."))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(SpeechError::new("worker_crashed", "The speech engine stopped unexpectedly."))
            }
        }
    }

    pub fn warm(&self, model_path: &Path) -> Result<String, SpeechError> {
        let _g = self.job_lock.lock();
        if self.loaded_model.lock().as_deref() == Some(model_path) {
            return Ok(self.backend().unwrap_or_else(|| "cpu".into()));
        }
        let v = self.control(
            json!({"command": "warm", "modelPath": model_path.to_string_lossy(), "backend": "auto"}),
            WARM_TIMEOUT,
        )?;
        let backend = v["backend"].as_str().unwrap_or("cpu").to_string();
        *self.loaded_model.lock() = Some(model_path.to_path_buf());
        *self.backend.lock() = Some(backend.clone());
        *self.last_used.lock() = Instant::now();
        Ok(backend)
    }

    /// Runs one recognition job with an absolute deadline. Progress never extends it.
    pub fn transcribe(&self, job: &SpeechJob) -> Result<RecognitionResult, SpeechError> {
        let _g = self.job_lock.lock();
        *self.cancel_requested.lock() = None;
        self.ensure_process()?;
        let rx = self.proc.lock().as_mut().expect("process").send(json!({
            "command": "transcribe",
            "jobId": job.job_id,
            "generation": job.generation,
            "modelPath": job.model_path.to_string_lossy(),
            "audioPath": job.audio_path.to_string_lossy(),
            "backend": "auto",
            "options": job.options,
        }))?;
        let started = Instant::now();
        let mut cancel_seen_at: Option<Instant> = None;
        let outcome = loop {
            if cancel_seen_at.is_none() && self.cancel_requested.lock().as_deref() == Some(job.job_id.as_str()) {
                cancel_seen_at = Some(Instant::now());
            }
            if let Some(t) = cancel_seen_at {
                if t.elapsed() > CANCEL_GRACE {
                    self.restart();
                    break Err(SpeechError::new("canceled", "Canceled."));
                }
            }
            if started.elapsed() > job.deadline {
                self.send_cancel(Some(&job.job_id));
                match rx.recv_timeout(CANCEL_GRACE) {
                    Ok(_) => {}
                    Err(_) => self.restart(),
                }
                break Err(SpeechError::new(
                    "timeout",
                    "Recognition took longer than allowed and was stopped. The recording is saved; try Retry.",
                ));
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => break Ok(v),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    self.restart();
                    break Err(SpeechError::new(
                        "worker_crashed",
                        "The speech engine stopped unexpectedly. The recording is saved; try Retry.",
                    ));
                }
            }
        };
        *self.last_used.lock() = Instant::now();
        let v = outcome?;
        if v["jobId"].as_str() != Some(job.job_id.as_str()) || v["generation"].as_u64() != Some(job.generation) {
            return Err(SpeechError::new("internal", "The speech engine returned a result for a different job."));
        }
        match v["type"].as_str() {
            Some("result") if v["ok"] == true => {
                *self.loaded_model.lock() = Some(job.model_path.clone());
                parse_result(&v)
            }
            Some("canceled") => Err(SpeechError::new("canceled", "Canceled.")),
            _ => Err(response_error(&v)),
        }
    }

    fn send_cancel(&self, job_id: Option<&str>) {
        let mut msg = json!({"command": "cancel"});
        if let Some(id) = job_id {
            msg["jobId"] = json!(id);
        }
        if let Some(p) = self.proc.lock().as_mut() {
            // The acknowledgement is not awaited; the job's own reply settles it.
            let _ = p.send(msg);
        }
    }

    /// Cancels a running job. If the worker does not settle within a grace period the
    /// transcribe call restarts it.
    pub fn cancel(&self, job_id: &str) {
        *self.cancel_requested.lock() = Some(job_id.to_string());
        self.send_cancel(Some(job_id));
    }

    pub fn shutdown(&self) {
        if let Some(mut p) = self.proc.lock().take() {
            if let Ok(rx) = p.send(json!({"command": "shutdown"})) {
                let _ = rx.recv_timeout(Duration::from_secs(2));
            }
            p.kill();
        }
    }
}

fn parse_result(v: &Value) -> Result<RecognitionResult, SpeechError> {
    let r = &v["result"];
    let text = r["text"].as_str().ok_or_else(|| SpeechError::new("internal", "Malformed recognition result."))?;
    let segments: Vec<Segment> = serde_json::from_value(r["segments"].clone()).unwrap_or_default();
    Ok(RecognitionResult {
        text: text.trim().to_string(),
        language: r["language"].as_str().map(str::to_owned),
        segments,
        duration_seconds: r["durationSeconds"].as_f64().unwrap_or(0.0),
        backend: r["backend"].as_str().unwrap_or("cpu").to_string(),
        raw: r.clone(),
    })
}

#[cfg(windows)]
mod win_job {
    //! Ties the worker's lifetime to ours: closing the job handle (including when the
    //! app crashes) terminates the worker.
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::*;

    pub struct JobHandle(HANDLE);
    unsafe impl Send for JobHandle {}
    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn assign(child: &std::process::Child) -> Option<JobHandle> {
        unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())).ok()?;
            Some(JobHandle(job))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker() -> Option<PathBuf> {
        let triple = if cfg!(target_arch = "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" };
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("sayvane-speech-{triple}"));
        (cfg!(target_os = "macos") && p.metadata().map(|m| m.len() > 1000).unwrap_or(false)).then_some(p)
    }

    #[test]
    fn missing_worker_is_a_clear_error() {
        let s = SpeechService::new(PathBuf::from("/nonexistent/sayvane-speech"), Arc::new(|_, _, _| {}));
        let e = s.warm(Path::new("/tmp/x.bin")).unwrap_err();
        assert_eq!(e.code, "worker_missing");
    }

    #[test]
    fn missing_model_and_audio_errors_are_structured() {
        let Some(exe) = worker() else { return };
        let s = SpeechService::new(exe, Arc::new(|_, _, _| {}));
        let e = s.warm(Path::new("/nonexistent/model.bin")).unwrap_err();
        assert_eq!(e.code, "model_load_failed");
        s.shutdown();
    }

    /// Full recognition with a real model; runs only when SAYVANE_TEST_MODEL points to one.
    #[test]
    fn transcribes_fixture_with_real_model() {
        let (Some(exe), Ok(model)) = (worker(), std::env::var("SAYVANE_TEST_MODEL")) else { return };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/hello.wav");
        let s = SpeechService::new(exe, Arc::new(|_, _, _| {}));
        s.warm(Path::new(&model)).unwrap();
        let job = SpeechJob {
            job_id: "j1".into(),
            generation: 7,
            model_path: model.clone().into(),
            audio_path: fixture.clone(),
            options: json!({"task": "transcribe", "temperature": 0, "decoding": "greedy"}),
            deadline: LIVE_DEADLINE,
        };
        let r = s.transcribe(&job).unwrap();
        assert!(r.text.to_lowercase().contains("hello"), "{}", r.text);
        assert!(!r.segments.is_empty());
        // The model stays loaded between utterances.
        assert_eq!(s.loaded_model().as_deref(), Some(Path::new(&model)));
        let r2 = s.transcribe(&SpeechJob { job_id: "j2".into(), generation: 8, ..job }).unwrap();
        assert!(!r2.text.is_empty());
        s.shutdown();
    }
}
