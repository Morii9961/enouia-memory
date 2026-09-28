//! Normalized UTC timestamps. Stored form is exactly `YYYY-MM-DDTHH:MM:SS.mmmZ`
//! with a real calendar date, so lexicographic order equals time order.
//! Raw upstream time strings and zones stay on SourceRecord untouched.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Timestamp(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampError;

fn digits(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    Some(value)
}

pub const fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

pub const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

impl Timestamp {
    pub fn parse(value: &str) -> Result<Self, TimestampError> {
        let b = value.as_bytes();
        if b.len() != 24
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
            || b[19] != b'.'
            || b[23] != b'Z'
        {
            return Err(TimestampError);
        }
        let year = digits(&b[0..4]).ok_or(TimestampError)?;
        let month = digits(&b[5..7]).ok_or(TimestampError)?;
        let day = digits(&b[8..10]).ok_or(TimestampError)?;
        let hour = digits(&b[11..13]).ok_or(TimestampError)?;
        let minute = digits(&b[14..16]).ok_or(TimestampError)?;
        let second = digits(&b[17..19]).ok_or(TimestampError)?;
        digits(&b[20..23]).ok_or(TimestampError)?;
        if !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(TimestampError);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn unix_ms(&self) -> i64 {
        let b = self.0.as_bytes();
        let n = |range: std::ops::Range<usize>| i64::from(digits(&b[range]).unwrap_or(0));
        let days = days_from_civil(n(0..4), n(5..7), n(8..10));
        ((days * 24 + n(11..13)) * 60 + n(14..16)) * 60_000 + n(17..19) * 1000 + n(20..23)
    }

    /// Format a Clock reading. Returns None outside years 0000..=9999.
    pub fn from_unix_ms(ms: i64) -> Option<Self> {
        let days = ms.div_euclid(86_400_000);
        let rem = ms.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        if !(0..=9999).contains(&year) {
            return None;
        }
        let text = format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            rem / 3_600_000,
            rem / 60_000 % 60,
            rem / 1000 % 60,
            rem % 1000
        );
        Self::parse(&text).ok()
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(|_| serde::de::Error::custom("invalid UTC timestamp"))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A business time that may be explicitly unknown (`"unknown"`), used where the
/// design forbids inventing a value, e.g. supersession `effective_from`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusinessTime {
    Known(Timestamp),
    Unknown,
}

impl Serialize for BusinessTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown => serializer.serialize_str("unknown"),
        }
    }
}

impl<'de> Deserialize<'de> for BusinessTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == "unknown" {
            return Ok(Self::Unknown);
        }
        Timestamp::parse(&value)
            .map(Self::Known)
            .map_err(|_| serde::de::Error::custom("invalid business time"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_utc_millisecond_timestamps_parse() {
        assert!(Timestamp::parse("2028-02-29T23:59:59.999Z").is_ok());
        for bad in [
            "2026-02-29T00:00:00.000Z",
            "2026-13-01T00:00:00.000Z",
            "2026-09-28T24:00:00.000Z",
            "2026-09-28T08:00:00Z",
            "2026-09-28T08:00:00.000+08:00",
            "2026-09-28 08:00:00.000Z",
        ] {
            assert!(Timestamp::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn unix_milliseconds_round_trip() {
        let t = Timestamp::parse("2026-09-28T08:30:15.123Z").unwrap();
        assert_eq!(Timestamp::from_unix_ms(t.unix_ms()).unwrap(), t);
        assert_eq!(
            Timestamp::from_unix_ms(0).unwrap().as_str(),
            "1970-01-01T00:00:00.000Z"
        );
    }
}
