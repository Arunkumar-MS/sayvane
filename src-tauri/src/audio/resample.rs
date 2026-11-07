//! Streaming conversion to the recognizer's 16 kHz mono format.
//! Works chunk by chunk so hour-long recordings never sit in memory.

use std::f32::consts::PI;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub const TARGET_RATE: u32 = 16_000;

/// Anti-aliased linear-interpolation resampler with a windowed-sinc low-pass on the input.
pub struct Resampler {
    step: f64,
    taps: Vec<f32>,
    delay: Vec<f32>,
    delay_pos: usize,
    filtered: Vec<f32>,
    pos: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let step = in_rate as f64 / out_rate as f64;
        let taps = if in_rate > out_rate {
            // Cutoff a little under the output Nyquist, normalized to the input rate.
            let fc = 0.45 * out_rate as f32 / in_rate as f32;
            let n = 63;
            let m = (n - 1) as f32 / 2.0;
            let mut t: Vec<f32> = (0..n)
                .map(|i| {
                    let x = i as f32 - m;
                    let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * PI * fc * x).sin() / (PI * x) };
                    let w = 0.42 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos()
                        + 0.08 * (4.0 * PI * i as f32 / (n - 1) as f32).cos();
                    sinc * w
                })
                .collect();
            let sum: f32 = t.iter().sum();
            t.iter_mut().for_each(|v| *v /= sum);
            t
        } else {
            vec![1.0]
        };
        let len = taps.len();
        Self { step, taps, delay: vec![0.0; len], delay_pos: 0, filtered: Vec::new(), pos: 0.0 }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = self.taps.len();
        for &x in input {
            self.delay[self.delay_pos] = x;
            let mut acc = 0.0f32;
            let mut idx = self.delay_pos;
            for &t in &self.taps {
                acc += t * self.delay[idx];
                idx = if idx == 0 { n - 1 } else { idx - 1 };
            }
            self.delay_pos = (self.delay_pos + 1) % n;
            self.filtered.push(acc);
        }
        while self.pos + 1.0 < self.filtered.len() as f64 {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            out.push(self.filtered[i] * (1.0 - frac) + self.filtered[i + 1] * frac);
            self.pos += self.step;
        }
        let consumed = (self.pos as usize).min(self.filtered.len());
        self.filtered.drain(..consumed);
        self.pos -= consumed as f64;
    }

    /// Flushes the filter's group delay so the tail of the recording is not lost.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let pad = vec![0.0; self.taps.len() / 2 + 2];
        self.process(&pad, out);
    }
}

pub fn downmix_into(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    for frame in interleaved.chunks_exact(channels) {
        out.push(frame.iter().sum::<f32>() / channels as f32);
    }
}

/// Writes 16 kHz mono 16-bit PCM from successive interleaved chunks.
pub struct MonoWriter {
    resampler: Resampler,
    channels: usize,
    writer: hound::WavWriter<BufWriter<File>>,
    mono: Vec<f32>,
    out: Vec<f32>,
    pub frames_out: u64,
}

impl MonoWriter {
    pub fn create(path: &Path, in_rate: u32, channels: usize) -> Result<Self, String> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(path, spec).map_err(|e| format!("Could not write audio: {e}"))?;
        Ok(Self {
            resampler: Resampler::new(in_rate, TARGET_RATE),
            channels: channels.max(1),
            writer,
            mono: Vec::new(),
            out: Vec::new(),
            frames_out: 0,
        })
    }

    pub fn push(&mut self, interleaved: &[f32]) -> Result<(), String> {
        self.mono.clear();
        downmix_into(interleaved, self.channels, &mut self.mono);
        self.out.clear();
        self.resampler.process(&self.mono, &mut self.out);
        self.write_out()
    }

    fn write_out(&mut self) -> Result<(), String> {
        for &s in &self.out {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            self.writer.write_sample(v).map_err(|e| format!("Could not write audio: {e}"))?;
        }
        self.frames_out += self.out.len() as u64;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64, String> {
        self.out.clear();
        self.resampler.finish(&mut self.out);
        self.write_out()?;
        self.writer.finalize().map_err(|e| format!("Could not finalize audio: {e}"))?;
        Ok(self.frames_out)
    }
}

/// Converts any managed WAV (int or float, any rate/channels) into 16 kHz mono.
pub fn wav_to_recognizer_format(input: &Path, output: &Path) -> Result<u64, String> {
    let mut reader = hound::WavReader::open(input).map_err(|e| format!("The recording could not be read: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let mut w = MonoWriter::create(output, spec.sample_rate, channels)?;
    let mut chunk: Vec<f32> = Vec::with_capacity(16_384);
    let flush = |w: &mut MonoWriter, chunk: &mut Vec<f32>| -> Result<(), String> {
        let whole = chunk.len() - chunk.len() % channels.max(1);
        w.push(&chunk[..whole])?;
        chunk.drain(..whole);
        Ok(())
    };
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                chunk.push(s.map_err(|e| format!("The recording is damaged: {e}"))?);
                if chunk.len() >= 16_384 {
                    flush(&mut w, &mut chunk)?;
                }
            }
        }
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            for s in reader.samples::<i32>() {
                chunk.push(s.map_err(|e| format!("The recording is damaged: {e}"))? as f32 * scale);
                if chunk.len() >= 16_384 {
                    flush(&mut w, &mut chunk)?;
                }
            }
        }
    }
    flush(&mut w, &mut chunk)?;
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| (2.0 * PI * hz * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn output_length_matches_ratio_across_chunk_boundaries() {
        let input = tone(48_000, 440.0, 2.0);
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        for c in input.chunks(997) {
            r.process(c, &mut out);
        }
        r.finish(&mut out);
        let expected = 32_000i64;
        assert!((out.len() as i64 - expected).abs() < 40, "got {}", out.len());
    }

    #[test]
    fn passes_speech_band_and_attenuates_aliases() {
        let mut out = Vec::new();
        let mut r = Resampler::new(48_000, 16_000);
        r.process(&tone(48_000, 1_000.0, 1.0), &mut out);
        let pass = rms(&out[200..]);
        let mut out2 = Vec::new();
        let mut r2 = Resampler::new(48_000, 16_000);
        r2.process(&tone(48_000, 12_000.0, 1.0), &mut out2);
        let stop = rms(&out2[200..]);
        assert!(pass > 0.3, "1 kHz preserved ({pass})");
        assert!(stop < 0.02, "12 kHz alias suppressed ({stop})");
    }

    #[test]
    fn converts_stereo_int_wav_to_16k_mono() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for s in tone(44_100, 300.0, 1.0) {
            let v = (s * 32767.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        let dst = dir.path().join("out.wav");
        let frames = wav_to_recognizer_format(&src, &dst).unwrap();
        let r = hound::WavReader::open(&dst).unwrap();
        assert_eq!(r.spec().channels, 1);
        assert_eq!(r.spec().sample_rate, 16_000);
        assert!((frames as i64 - 16_000).abs() < 40);
    }
}
