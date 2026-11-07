//! Home statistics, computed locally from entries with non-empty final text.
//! Whitespace word counting is a product heuristic and is kept replaceable.

use crate::db::DictationRow;
use crate::ipc_types::Stats;
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use std::collections::BTreeSet;

pub const TYPING_WPM: f64 = 40.0;
pub const SPEAKING_WPM_ESTIMATE: f64 = 150.0;
pub const MIN_AUDIO_FOR_SPEED_MS: i64 = 5_000;

pub fn count_words(text: &str) -> u64 {
    text.split_whitespace().count() as u64
}

pub fn compute<Tz: TimeZone>(rows: &[DictationRow], now: DateTime<Tz>) -> Stats {
    let tz = now.timezone();
    let now_utc = now.with_timezone(&Utc);
    let week_ago = now_utc - Duration::days(7);
    let mut words_week = 0u64;
    let mut words_timed = 0u64;
    let mut ms_timed = 0i64;
    let mut typing_min = 0f64;
    let mut speaking_min = 0f64;
    let mut days: BTreeSet<NaiveDate> = BTreeSet::new();
    let mut total = 0u64;

    for r in rows {
        if r.final_text.trim().is_empty() {
            continue;
        }
        let Ok(created) = DateTime::parse_from_rfc3339(&r.created_at_utc) else { continue };
        let created = created.with_timezone(&Utc);
        let words = count_words(&r.final_text);
        total += 1;
        if created > week_ago && created <= now_utc {
            words_week += words;
        }
        if r.duration_ms > 0 {
            words_timed += words;
            ms_timed += r.duration_ms;
            speaking_min += r.duration_ms as f64 / 60_000.0;
        } else {
            speaking_min += words as f64 / SPEAKING_WPM_ESTIMATE;
        }
        typing_min += words as f64 / TYPING_WPM;
        days.insert(created.with_timezone(&tz).date_naive());
    }

    Stats {
        words_this_week: words_week,
        speaking_wpm: (ms_timed >= MIN_AUDIO_FOR_SPEED_MS)
            .then(|| (words_timed as f64 / (ms_timed as f64 / 60_000.0) * 10.0).round() / 10.0),
        minutes_saved: ((typing_min - speaking_min).max(0.0) * 10.0).round() / 10.0,
        streak_days: streak(&days, now.date_naive()),
        total_entries: total,
    }
}

/// Consecutive local days with a transcript, counting back from today, or from
/// yesterday when today has none yet.
pub fn streak(days: &BTreeSet<NaiveDate>, today: NaiveDate) -> u32 {
    let mut day = if days.contains(&today) { today } else { today.pred_opt().unwrap_or(today) };
    let mut n = 0;
    while days.contains(&day) {
        n += 1;
        match day.pred_opt() {
            Some(d) => day = d,
            None => break,
        }
    }
    n
}

pub fn compute_now(rows: &[DictationRow]) -> Stats {
    compute(rows, Local::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc_types::DictationSource;
    use chrono::FixedOffset;

    fn row_at(utc: &str, text: &str, ms: i64) -> DictationRow {
        let mut r = DictationRow::new(DictationSource::Dictation, "base", "transcribe", "{}".into());
        r.created_at_utc = utc.into();
        r.final_text = text.into();
        r.duration_ms = ms;
        r
    }

    #[test]
    fn empty_text_is_ignored_and_speed_needs_five_seconds() {
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let rows = vec![row_at("2026-11-07T10:00:00Z", "one two three", 3000), row_at("2026-11-07T10:00:00Z", "  ", 60000)];
        let s = compute(&rows, now);
        assert_eq!(s.words_this_week, 3);
        assert_eq!(s.speaking_wpm, None);
        assert_eq!(s.total_entries, 1);
    }

    #[test]
    fn speed_and_time_saved() {
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let words = vec!["w"; 120].join(" ");
        let s = compute(&[row_at("2026-11-07T10:00:00Z", &words, 60_000)], now);
        assert_eq!(s.speaking_wpm, Some(120.0));
        // 120 words typed at 40 wpm = 3 min, spoken in 1 min → 2 min saved.
        assert_eq!(s.minutes_saved, 2.0);
    }

    #[test]
    fn streak_counts_local_days_across_midnight() {
        // UTC-8: 2026-11-07T07:30Z is 23:30 on Nov 6 locally.
        let tz = FixedOffset::west_opt(8 * 3600).unwrap();
        let now = tz.with_ymd_and_hms(2026, 11, 7, 9, 0, 0).unwrap();
        let rows = vec![
            row_at("2026-11-07T07:30:00Z", "late night", 0),  // Nov 6 local
            row_at("2026-11-06T02:00:00Z", "evening", 0),     // Nov 5 local
            row_at("2026-11-04T20:00:00Z", "gap before", 0), // Nov 4 local
        ];
        let s = compute(&rows, now);
        // Today (Nov 7) has nothing, so count from yesterday: Nov 6, 5, 4.
        assert_eq!(s.streak_days, 3);
        let rows = vec![row_at("2026-11-05T20:00:00Z", "two days ago", 0)];
        assert_eq!(compute(&rows, now).streak_days, 0);
    }
}
