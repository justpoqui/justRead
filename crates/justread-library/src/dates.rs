// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Calendar days as `YYYY-MM-DD` strings and day numbers (days since
//! 1970-01-01), without a date library. The conversions follow Howard
//! Hinnant's public-domain "civil from days" algorithms
//! (<https://howardhinnant.github.io/date_algorithms.html>).

/// Day number → (year, month, day).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// (year, month, day) → day number.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DD` for a day number.
pub fn day_string(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse a strict `YYYY-MM-DD` (a real calendar date) into a day number.
pub fn parse_day(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > 31 {
        return None;
    }
    let n = days_from_civil(y, m, d);
    // Round-trip rejects dates like 2026-02-30.
    (day_string(n) == s).then_some(n)
}

/// UTC calendar day of a Unix time in milliseconds.
pub fn day_of_ms(ms: i64) -> i64 {
    ms.div_euclid(86_400_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for n in [-1000, 0, 1, 10_957, 20_401, 50_000] {
            assert_eq!(parse_day(&day_string(n)), Some(n));
        }
        assert_eq!(day_string(0), "1970-01-01");
        assert_eq!(parse_day("2000-02-29"), Some(11_016));
        assert_eq!(parse_day("2026-02-30"), None);
        assert_eq!(parse_day("2026-1-01"), None);
        assert_eq!(parse_day("abcd-ef-gh"), None);
    }
}
