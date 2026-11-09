// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 8: reading time and statistics.
//!
//! The reader reports *active* reading in small chunks (see
//! `ui/src/lib/stats/tracker.ts`): seconds read and how far forward the
//! reader moved, per book and local calendar day. Everything stays in the
//! local database; nothing is sent anywhere.

use rusqlite::params;
use serde::Serialize;

use crate::{dates, BookStatus, Library, LibraryError, Result};

/// Largest single report accepted (the UI reports about once a minute).
const MAX_SECONDS_PER_REPORT: i64 = 600;
/// Forward movement above this in one report is a jump (TOC, slider), not reading.
const MAX_PROGRESS_PER_REPORT: f64 = 0.05;

/// Minutes read on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    pub day: String,
    pub seconds: i64,
}

/// Library-wide statistics.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingStats {
    pub today_seconds: i64,
    /// The last 7 days, today included.
    pub week_seconds: i64,
    pub total_seconds: i64,
    /// Consecutive days with reading, ending today (or yesterday, if nothing yet today).
    pub streak_days: i64,
    pub finished_this_year: i64,
    pub books_finished: i64,
    pub books_reading: i64,
    /// One entry per day, oldest first, ending today (zeros included).
    pub daily: Vec<DayTotal>,
}

/// Statistics for one book.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookStats {
    pub seconds: i64,
    pub days_read: i64,
    pub first_day: Option<String>,
    pub last_day: Option<String>,
    /// Share of the book read forward while timed (0..1).
    pub progress_read: f64,
    /// Estimated time to finish at this reader's pace, once there is enough data.
    pub seconds_left: Option<i64>,
}

fn check_day(day: &str) -> Result<i64> {
    dates::parse_day(day).ok_or_else(|| LibraryError::Invalid(format!("bad day {day:?}")))
}

impl Library {
    /// Add active reading time (and forward progress) for a book on a day.
    pub fn record_reading(
        &self,
        book_id: &str,
        day: &str,
        seconds: i64,
        progress: f64,
    ) -> Result<()> {
        // STEP 1: validate; the webview is untrusted.
        check_day(day)?;
        if !(1..=MAX_SECONDS_PER_REPORT).contains(&seconds) {
            return Err(LibraryError::Invalid("seconds out of range".into()));
        }
        let progress =
            if progress.is_finite() && progress > 0.0 && progress <= MAX_PROGRESS_PER_REPORT {
                progress
            } else {
                0.0
            };
        // STEP 2: add to the day's row (the book must exist: foreign key).
        self.conn()
            .execute(
                "INSERT INTO reading_days (book_id, day, seconds, progress) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(book_id, day) DO UPDATE SET
                   seconds = seconds + excluded.seconds, progress = progress + excluded.progress",
                params![book_id, day, seconds, progress],
            )
            .map_err(|_| LibraryError::NotFound(book_id.to_string()))?;
        Ok(())
    }

    /// Library-wide statistics. `today` is the reader's local date; `days` the chart length.
    pub fn reading_stats(&self, today: &str, days: u32) -> Result<ReadingStats> {
        let t = check_day(today)?;
        let days = i64::from(days.clamp(1, 366));

        // STEP 1: totals per day (all books).
        let mut stmt = self
            .conn()
            .prepare("SELECT day, SUM(seconds) FROM reading_days GROUP BY day")?;
        let per_day: std::collections::HashMap<i64, i64> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .filter_map(|r| r.ok())
            .filter_map(|(d, s)| dates::parse_day(&d).map(|n| (n, s)))
            .collect();
        let on = |n: i64| per_day.get(&n).copied().unwrap_or(0);

        // STEP 2: streak — today counts if read, otherwise it may still start from yesterday.
        let mut streak = 0;
        let mut d = if on(t) > 0 { t } else { t - 1 };
        while on(d) > 0 {
            streak += 1;
            d -= 1;
        }

        // STEP 3: shelves and the finished-this-year count (finish dates are UTC days).
        let year = &today[0..4];
        let (mut finished, mut reading, mut this_year) = (0, 0, 0);
        let mut stmt = self
            .conn()
            .prepare("SELECT status, finished_at FROM books")?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })? {
            let (status, at) = row?;
            match BookStatus::parse(&status) {
                BookStatus::Finished => {
                    finished += 1;
                    if at
                        .is_some_and(|ms| dates::day_string(dates::day_of_ms(ms)).starts_with(year))
                    {
                        this_year += 1;
                    }
                }
                BookStatus::Reading => reading += 1,
                BookStatus::Unread => {}
            }
        }

        Ok(ReadingStats {
            today_seconds: on(t),
            week_seconds: (0..7).map(|i| on(t - i)).sum(),
            total_seconds: per_day.values().sum(),
            streak_days: streak,
            finished_this_year: this_year,
            books_finished: finished,
            books_reading: reading,
            daily: (0..days)
                .rev()
                .map(|i| DayTotal {
                    day: dates::day_string(t - i),
                    seconds: on(t - i),
                })
                .collect(),
        })
    }

    /// Statistics for one book, with a time-left estimate from the reader's own pace.
    pub fn book_stats(&self, book_id: &str) -> Result<BookStats> {
        let book = self.get_book(book_id)?;
        let (seconds, days_read, first, last, progress): (i64, i64, Option<String>, Option<String>, f64) =
            self.conn().query_row(
                "SELECT COALESCE(SUM(seconds), 0), COUNT(*), MIN(day), MAX(day), COALESCE(SUM(progress), 0)
                 FROM reading_days WHERE book_id = ?1",
                params![book_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
        // Pace needs a little data: 10 minutes and 2 % of the book read while timed.
        let seconds_left =
            (seconds >= 600 && progress >= 0.02 && book.status != BookStatus::Finished).then(
                || ((1.0 - book.progression).max(0.0) * seconds as f64 / progress).round() as i64,
            );
        Ok(BookStats {
            seconds,
            days_read,
            first_day: first,
            last_day: last,
            progress_read: progress.min(1.0),
            seconds_left,
        })
    }
}
