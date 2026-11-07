//! Transcript exports. Timing formats always use the original time-aligned recognition
//! segments: polishing changes word boundaries, so polished text never gets timestamps.

use crate::db::DictationRow;
use crate::ipc_types::{enum_to_str, ExportFormat, TextVariant};
use serde_json::json;

pub fn extension(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Txt => "txt",
        ExportFormat::Json => "json",
        ExportFormat::Srt => "srt",
        ExportFormat::Vtt => "vtt",
        ExportFormat::Tsv => "tsv",
    }
}

pub fn render(row: &DictationRow, format: ExportFormat, variant: TextVariant) -> Result<String, String> {
    let segments = row.segments();
    let needs_timing = matches!(format, ExportFormat::Srt | ExportFormat::Vtt | ExportFormat::Tsv);
    if needs_timing && segments.is_empty() {
        return Err("This entry has no time-aligned segments, so subtitle and TSV exports are unavailable.".into());
    }
    Ok(match format {
        ExportFormat::Txt => {
            let text = row
                .text_variant(variant)
                .ok_or("This entry has no polished text. Export the final or original text instead.")?;
            let mut s = text.to_string();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
        ExportFormat::Json => {
            let v = json!({
                "format": "sayvane.transcript",
                "version": 1,
                "id": row.id,
                "createdAtUtc": row.created_at_utc,
                "source": enum_to_str(&row.source),
                "durationMs": row.duration_ms,
                "language": row.detected_language,
                "task": row.requested_task,
                "modelId": row.model_id,
                "text": {
                    "final": row.final_text,
                    "original": row.original_text,
                    "polished": row.polished_text,
                },
                "polishState": enum_to_str(&row.polish_state),
                "segments": segments,
            });
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())? + "\n"
        }
        ExportFormat::Srt => {
            let mut out = String::new();
            for (i, s) in segments.iter().enumerate() {
                out.push_str(&format!(
                    "{}\n{} --> {}\n{}\n\n",
                    i + 1,
                    timestamp(s.start, ','),
                    timestamp(s.end, ','),
                    cue_text(&s.text)
                ));
            }
            out
        }
        ExportFormat::Vtt => {
            let mut out = String::from("WEBVTT\n\n");
            for s in &segments {
                out.push_str(&format!(
                    "{} --> {}\n{}\n\n",
                    timestamp(s.start, '.'),
                    timestamp(s.end, '.'),
                    cue_text(&s.text).replace("-->", "->")
                ));
            }
            out
        }
        ExportFormat::Tsv => {
            let mut out = String::from("start\tend\ttext\n");
            for s in &segments {
                out.push_str(&format!(
                    "{}\t{}\t{}\n",
                    (s.start * 1000.0).round() as i64,
                    (s.end * 1000.0).round() as i64,
                    tsv_escape(s.text.trim())
                ));
            }
            out
        }
    })
}

/// `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` (VTT).
pub fn timestamp(seconds: f64, sep: char) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, rem) = (total_ms / 3_600_000, total_ms % 3_600_000);
    let (m, rem) = (rem / 60_000, rem % 60_000);
    let (s, ms) = (rem / 1000, rem % 1000);
    format!("{h:02}:{m:02}:{s:02}{sep}{ms:03}")
}

fn cue_text(t: &str) -> String {
    // A blank line would terminate the cue early.
    t.trim().lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

fn tsv_escape(t: &str) -> String {
    t.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc_types::{DictationSource, Segment};

    fn row() -> DictationRow {
        let mut r = DictationRow::new(DictationSource::Import, "base", "transcribe", "{}".into());
        r.original_text = "Hello there. Ça va?".into();
        r.final_text = "Hello there. Ça va?".into();
        let segs = vec![
            Segment { id: 0, start: 0.0, end: 1.5, text: " Hello there.".into(), words: None },
            Segment { id: 1, start: 3661.25, end: 3662.0049, text: "Ça\tva?\n".into(), words: None },
        ];
        r.segments_json = Some(serde_json::to_string(&segs).unwrap());
        r
    }

    #[test]
    fn timestamps_use_the_right_separator() {
        assert_eq!(timestamp(3661.25, ','), "01:01:01,250");
        assert_eq!(timestamp(0.0005, '.'), "00:00:00.001");
        assert_eq!(timestamp(-1.0, '.'), "00:00:00.000");
    }

    #[test]
    fn srt_vtt_tsv_render_segments() {
        let r = row();
        let srt = render(&r, ExportFormat::Srt, TextVariant::Final).unwrap();
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,500\nHello there.\n\n2\n01:01:01,250 --> 01:01:02,005"));
        let vtt = render(&r, ExportFormat::Vtt, TextVariant::Final).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:01.500\n"));
        let tsv = render(&r, ExportFormat::Tsv, TextVariant::Final).unwrap();
        assert_eq!(tsv, "start\tend\ttext\n0\t1500\tHello there.\n3661250\t3662005\tÇa\\tva?\n");
    }

    #[test]
    fn timing_formats_need_segments_and_polished_needs_text() {
        let mut r = row();
        r.segments_json = None;
        assert!(render(&r, ExportFormat::Srt, TextVariant::Final).is_err());
        assert!(render(&r, ExportFormat::Txt, TextVariant::Polished).is_err());
        assert_eq!(render(&r, ExportFormat::Txt, TextVariant::Original).unwrap(), "Hello there. Ça va?\n");
    }

    #[test]
    fn json_has_variants_and_no_snapshot() {
        let r = row();
        let j: serde_json::Value = serde_json::from_str(&render(&r, ExportFormat::Json, TextVariant::Final).unwrap()).unwrap();
        assert_eq!(j["version"], 1);
        assert_eq!(j["segments"].as_array().unwrap().len(), 2);
        assert!(j.get("settingsSnapshot").is_none());
    }
}
