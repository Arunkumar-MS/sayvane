//! Imported-file decoding. Only formats verified with the bundled decoder are advertised.

use super::resample::MonoWriter;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "flac", "ogg"];

pub struct Decoded {
    pub frames_16k: u64,
    pub duration_ms: i64,
}

/// Decodes `input` into a 16 kHz mono WAV at `output`, streaming packet by packet.
pub fn decode_to_recognizer_format(
    input: &Path,
    output: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<Decoded, String> {
    let file = std::fs::File::open(input).map_err(|e| format!("The file could not be opened: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|_| "This file format is not supported. Use WAV, MP3, M4A, FLAC, or OGG.".to_string())?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
        .ok_or("The file has no audio track.")?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.ok_or("Unknown sample rate.")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| "The audio codec in this file is not supported.".to_string())?;

    let mut writer: Option<MonoWriter> = None;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut channels = 0usize;
    let mut frames_in: u64 = 0;
    loop {
        if cancel() {
            return Err("canceled".into());
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(format!("The file could not be decoded: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // skip a corrupt frame
            Err(e) => return Err(format!("The file could not be decoded: {e}")),
        };
        if sample_buf.is_none() {
            let spec = *decoded.spec();
            channels = spec.channels.count().max(1);
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
            writer = Some(MonoWriter::create(output, rate, channels)?);
        }
        let sb = sample_buf.as_mut().expect("initialized");
        if (decoded.capacity() as u64) * channels as u64 > sb.capacity() as u64 {
            *sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        }
        sb.copy_interleaved_ref(decoded);
        frames_in += (sb.samples().len() / channels) as u64;
        writer.as_mut().expect("initialized").push(sb.samples())?;
    }
    let w = writer.ok_or("The file contains no decodable audio.")?;
    let frames_16k = w.finish()?;
    Ok(Decoded { frames_16k, duration_ms: (frames_in as f64 * 1000.0 / rate as f64) as i64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_wav_through_symphonia() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 22_050, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..22_050 {
            w.write_sample(((i as f32 * 0.05).sin() * 8000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let out = dir.path().join("out.wav");
        let d = decode_to_recognizer_format(&src, &out, &|| false).unwrap();
        assert!((d.duration_ms - 1000).abs() <= 2);
        assert!((d.frames_16k as i64 - 16_000).abs() < 40);
    }

    #[test]
    fn rejects_non_audio() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("x.mp3");
        std::fs::write(&src, b"definitely not audio").unwrap();
        assert!(decode_to_recognizer_format(&src, &dir.path().join("o.wav"), &|| false).is_err());
    }
}
