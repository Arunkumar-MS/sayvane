//! Microphone capture over CoreAudio (macOS) and WASAPI (Windows) via cpal 0.15.3.
//!
//! The real-time callback only converts samples and does a non-blocking hand-off. A
//! writer loop on the capture thread streams 16-bit PCM to disk, flushes the WAV header
//! about once a second so an interrupted capture stays recoverable, and derives levels.

pub mod decode;
pub mod resample;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::ipc_types::AudioInput;

pub const MAX_DICTATION_MS: u64 = 60 * 60 * 1000;
pub const WARN_BEFORE_LIMIT_MS: u64 = 60 * 1000;
const FLUSH_EVERY: Duration = Duration::from_millis(1000);
const LEVEL_EVERY: Duration = Duration::from_millis(40); // 25 updates per second

#[derive(Debug, Clone, thiserror::Error)]
pub enum AudioError {
    #[error("No microphone is available. Connect one or choose another input.")]
    NoDevice,
    #[error("The selected microphone “{0}” is not available.")]
    DeviceMissing(String),
    #[error("The microphone could not be opened: {0}")]
    OpenFailed(String),
    #[error("Audio could not be saved: {0}")]
    Write(String),
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

#[derive(Debug, Clone)]
pub enum CaptureEvent {
    Level(f32),
    Flushed { frames: u64 },
    NearLimit,
    LimitReached,
    DeviceLost(String),
    WriteFailed(String),
}

#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub frames: u64,
    pub duration_ms: i64,
    pub format: CaptureFormat,
    pub interrupted: Option<String>,
    pub dropped_buffers: u64,
}

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<CaptureResult, AudioError>>>,
    pub format: CaptureFormat,
    pub path: PathBuf,
}

impl CaptureHandle {
    /// Stops capture and finalizes the file. Blocks briefly while buffers drain.
    pub fn stop(mut self) -> Result<CaptureResult, AudioError> {
        self.stop.store(true, Ordering::SeqCst);
        match self.thread.take().map(|t| t.join()) {
            Some(Ok(r)) => r,
            _ => Err(AudioError::Write("the capture thread stopped unexpectedly".into())),
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn list_inputs() -> Vec<AudioInput> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let mut out: Vec<AudioInput> = host
        .input_devices()
        .map(|it| {
            it.filter_map(|d| d.name().ok())
                .map(|name| AudioInput { is_default: Some(&name) == default_name.as_ref(), id: name.clone(), name })
                .collect()
        })
        .unwrap_or_default();
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn find_device(id: Option<&str>) -> Result<cpal::Device, AudioError> {
    let host = cpal::default_host();
    match id {
        None => host.default_input_device().ok_or(AudioError::NoDevice),
        Some(id) => host
            .input_devices()
            .map_err(|e| AudioError::OpenFailed(e.to_string()))?
            .find(|d| d.name().map(|n| n == id).unwrap_or(false))
            .ok_or_else(|| AudioError::DeviceMissing(id.to_string())),
    }
}

/// Opens the device and starts writing to `path`. Returns once audio is flowing, so
/// the caller can play the start sound only after capture is active.
pub fn start(
    device_id: Option<String>,
    path: PathBuf,
    max_ms: u64,
    on_event: Arc<dyn Fn(CaptureEvent) + Send + Sync>,
) -> Result<CaptureHandle, AudioError> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<CaptureFormat, AudioError>>();
    let stop_t = stop.clone();
    let path_t = path.clone();
    let thread = std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn(move || capture_thread(device_id, path_t, max_ms, stop_t, ready_tx, on_event))
        .map_err(|e| AudioError::OpenFailed(e.to_string()))?;
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(format)) => Ok(CaptureHandle { stop, thread: Some(thread), format, path }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(AudioError::OpenFailed("the microphone did not start in time".into()))
        }
    }
}

fn capture_thread(
    device_id: Option<String>,
    path: PathBuf,
    max_ms: u64,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<CaptureFormat, AudioError>>,
    on_event: Arc<dyn Fn(CaptureEvent) + Send + Sync>,
) -> Result<CaptureResult, AudioError> {
    let fail = |e: AudioError| {
        let _ = ready.send(Err(e.clone()));
        Err(e)
    };
    let device = match find_device(device_id.as_deref()) {
        Ok(d) => d,
        Err(e) => return fail(e),
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => return fail(AudioError::OpenFailed(e.to_string())),
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let format = CaptureFormat { sample_rate: config.sample_rate.0, channels: config.channels, bits_per_sample: 16 };

    let spec = hound::WavSpec {
        channels: format.channels,
        sample_rate: format.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = match hound::WavWriter::create(&path, spec) {
        Ok(w) => w,
        Err(e) => return fail(AudioError::Write(e.to_string())),
    };
    crate::paths::restrict_to_user(&path);

    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(256);
    let dropped = Arc::new(AtomicU64::new(0));
    let device_error: Arc<parking_lot::Mutex<Option<String>>> = Default::default();
    let err_slot = device_error.clone();
    let err_fn = move |e: cpal::StreamError| {
        *err_slot.lock() = Some(e.to_string());
    };

    let stream = {
        let dropped = dropped.clone();
        let build = |tx: SyncSender<Vec<f32>>| -> Result<cpal::Stream, cpal::BuildStreamError> {
            macro_rules! build_for {
                ($t:ty, $conv:expr) => {{
                    let dropped = dropped.clone();
                    device.build_input_stream(
                        &config,
                        move |data: &[$t], _: &cpal::InputCallbackInfo| {
                            let buf: Vec<f32> = data.iter().map($conv).collect();
                            if let Err(TrySendError::Full(_)) = tx.try_send(buf) {
                                dropped.fetch_add(1, Ordering::Relaxed);
                            }
                        },
                        err_fn.clone(),
                        None,
                    )
                }};
            }
            match sample_format {
                cpal::SampleFormat::F32 => build_for!(f32, |s: &f32| *s),
                cpal::SampleFormat::I16 => build_for!(i16, |s: &i16| *s as f32 / 32768.0),
                cpal::SampleFormat::U16 => build_for!(u16, |s: &u16| (*s as f32 - 32768.0) / 32768.0),
                cpal::SampleFormat::I32 => build_for!(i32, |s: &i32| *s as f32 / 2_147_483_648.0),
                cpal::SampleFormat::F64 => build_for!(f64, |s: &f64| *s as f32),
                other => Err(cpal::BuildStreamError::BackendSpecific {
                    err: cpal::BackendSpecificError { description: format!("unsupported sample format {other:?}") },
                }),
            }
        };
        match build(tx) {
            Ok(s) => s,
            Err(e) => {
                drop(writer);
                let _ = std::fs::remove_file(&path);
                return fail(AudioError::OpenFailed(e.to_string()));
            }
        }
    };
    if let Err(e) = stream.play() {
        drop(writer);
        let _ = std::fs::remove_file(&path);
        return fail(AudioError::OpenFailed(e.to_string()));
    }
    let _ = ready.send(Ok(format));

    let channels = format.channels.max(1) as u64;
    let rate = format.sample_rate as u64;
    let mut samples_written: u64 = 0;
    let mut last_flush = Instant::now();
    let mut last_level = Instant::now();
    let mut level = 0f32;
    let mut peak_sq = 0f32;
    let mut peak_n = 0usize;
    let mut warned = false;
    let mut interrupted: Option<String> = None;

    let write_buf = |writer: &mut hound::WavWriter<_>, buf: &[f32], written: &mut u64| -> Result<(), AudioError> {
        for &s in buf {
            writer
                .write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)
                .map_err(|e| AudioError::Write(e.to_string()))?;
        }
        *written += buf.len() as u64;
        Ok(())
    };

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if let Some(e) = device_error.lock().take() {
            interrupted = Some(format!("The microphone stopped: {e}"));
            on_event(CaptureEvent::DeviceLost(e));
            break;
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(buf) => {
                for s in &buf {
                    peak_sq += s * s;
                }
                peak_n += buf.len();
                if let Err(e) = write_buf(&mut writer, &buf, &mut samples_written) {
                    on_event(CaptureEvent::WriteFailed(e.to_string()));
                    interrupted = Some(e.to_string());
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_level.elapsed() >= LEVEL_EVERY && peak_n > 0 {
            let rms = (peak_sq / peak_n as f32).sqrt();
            let db = 20.0 * rms.max(1e-6).log10();
            let target = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
            // Fast attack, slower decay.
            level = if target > level { level * 0.3 + target * 0.7 } else { level * 0.85 + target * 0.15 };
            on_event(CaptureEvent::Level(level));
            peak_sq = 0.0;
            peak_n = 0;
            last_level = Instant::now();
        }
        if last_flush.elapsed() >= FLUSH_EVERY {
            if let Err(e) = writer.flush() {
                on_event(CaptureEvent::WriteFailed(e.to_string()));
                interrupted = Some(format!("Audio could not be saved: {e}"));
                break;
            }
            on_event(CaptureEvent::Flushed { frames: samples_written / channels });
            last_flush = Instant::now();
        }
        let elapsed_ms = samples_written / channels * 1000 / rate.max(1);
        if !warned && elapsed_ms + WARN_BEFORE_LIMIT_MS >= max_ms {
            warned = true;
            on_event(CaptureEvent::NearLimit);
        }
        if elapsed_ms >= max_ms {
            on_event(CaptureEvent::LimitReached);
            break;
        }
    }

    drop(stream);
    // Drain what the callback already queued.
    while let Ok(buf) = rx.try_recv() {
        if write_buf(&mut writer, &buf, &mut samples_written).is_err() {
            break;
        }
    }
    writer.finalize().map_err(|e| AudioError::Write(e.to_string()))?;
    if let Ok(f) = std::fs::File::open(&path) {
        let _ = f.sync_all();
    }
    let frames = samples_written / channels;
    Ok(CaptureResult {
        frames,
        duration_ms: (frames * 1000 / rate.max(1)) as i64,
        format,
        interrupted,
        dropped_buffers: dropped.load(Ordering::Relaxed),
    })
}

/// Repairs the RIFF and data sizes of a WAV whose writer stopped before finalizing.
/// Only applies to the 16-bit PCM layout this module writes; returns the frame count.
pub fn repair_wav(path: &Path, format: &CaptureFormat) -> Result<u64, String> {
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = vec![0u8; 4096.min(len as usize)];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    if head.len() < 44 || &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err("The recording header is missing, so its format cannot be trusted.".into());
    }
    let mut off = 12usize;
    let mut data_off = None;
    while off + 8 <= head.len() {
        let id = &head[off..off + 4];
        let size = u32::from_le_bytes(head[off + 4..off + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            data_off = Some(off);
            break;
        }
        off += 8 + size + (size & 1);
    }
    let data_off = data_off.ok_or("The recording has no audio data section.")?;
    let block = (format.channels.max(1) as u64) * (format.bits_per_sample as u64 / 8);
    let data_len = (len - data_off as u64 - 8) / block * block;
    f.seek(SeekFrom::Start(data_off as u64 + 4)).map_err(|e| e.to_string())?;
    f.write_all(&(data_len as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
    f.write_all(&((data_off as u64 + 8 + data_len - 8) as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.set_len(data_off as u64 + 8 + data_len).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    Ok(data_len / block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_an_unfinalized_wav() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cap.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..4800 {
            w.write_sample((i % 100) as i16).unwrap();
        }
        w.flush().unwrap();
        for _ in 0..9600 {
            w.write_sample(7i16).unwrap(); // written after the last flush
        }
        std::mem::forget(w); // simulate a crash: no finalize
        let mut raw = std::fs::read(&p).unwrap();
        raw.extend_from_slice(&[1]); // torn trailing byte
        std::fs::write(&p, raw).unwrap();
        let fmt = CaptureFormat { sample_rate: 48_000, channels: 2, bits_per_sample: 16 };
        let frames = repair_wav(&p, &fmt).unwrap();
        let r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.duration() as u64, frames);
        assert!(frames >= 2400, "kept flushed audio ({frames})");
    }

    #[test]
    fn refuses_headerless_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.wav");
        std::fs::write(&p, vec![0u8; 100]).unwrap();
        let fmt = CaptureFormat { sample_rate: 16_000, channels: 1, bits_per_sample: 16 };
        assert!(repair_wav(&p, &fmt).is_err());
    }
}
