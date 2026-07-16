use std::{fmt::Display, str::FromStr};

use anyhow::{Result, anyhow, bail};
use lazy_regex::regex_captures;

pub const FLOAT_PRECISION: usize = 2;

#[derive(Copy, Clone, Default, Eq, PartialEq, Debug)]
pub enum Format {
    /// The default formatter, just regular numbers. Eg. 3.14
    #[default]
    Real,
    /// Always round to full integers, 3.14 = ~3
    Integer,
    /// Use scientific notation, 3268 = 3.27e3
    Scientific,
    /// Hexadecimal, fraction part is truncated. 16.25 = 0x10
    Hexadecimal,
    /// Binary, fraction part is truncated. 3.14 = 0b11
    Binary,
    /// Time of day, converted to seconds from midnight, rounded. 3720 = 01:02
    Time,
    /// Time of day in seconds precision, 3730 = 01:02:10
    TimeSeconds,
    /// Date, converted to seconds from epoch, rounded. 1036800 = 1970-01-13
    Date,
    /// Duration in days, converted to seconds, rounded. 259200 = 3d
    Days,
    /// Percentage, rounded. 0.3254 = 33%
    Percent,
    /// ISO 8601 date and time with timezone offset in seconds. Timezone zero
    /// is treated as UTC.
    Timestamp(i32),
}

impl Format {
    /// Default string that can be used to infer the format from.
    pub fn zero_string(self) -> String {
        match self {
            Format::Real => "0".to_string(),
            Format::Integer => "~0".to_string(),
            Format::Scientific => "0e0".to_string(),
            Format::Hexadecimal => "0x0".to_string(),
            Format::Binary => "0b0".to_string(),
            Format::Time => "00:00".to_string(),
            Format::TimeSeconds => "00:00:00".to_string(),
            Format::Date => "1970-01-01".to_string(),
            Format::Days => "0d".to_string(),
            Format::Percent => "0%".to_string(),
            // XXX: Time zone +00:00 will always turn to Z
            Format::Timestamp(0) => "1970-01-01T00:00:00Z".to_string(),
            Format::Timestamp(n) => {
                let h = n.abs() / 3600;
                let m = (n.abs() % 3600) / 60;
                let sign = if n < 0 { '-' } else { '+' };
                format!("1970-01-01T00:00:00{}{:02}:{:02}", sign, h, m)
            }
        }
    }
}

/// Parse an optionally formatted number.
///
/// ```
/// use teb::{parse, Format, FormattedScalar};
/// for (i, f, o) in [
///    ("3.14", Format::Real, 3.14),
///    ("~3", Format::Integer, 3.0),
///    ("3.14e2", Format::Scientific, 314.0),
///    ("0x10", Format::Hexadecimal, 16.0),
///    ("0b11", Format::Binary, 3.0),
///    ("01:02", Format::Time, 3720.0),
///    ("01:02:10", Format::TimeSeconds, 3730.0),
///    ("1970-01-13", Format::Date, 1036800.0),
///    ("3d", Format::Days, 259200.0),
///    ("33%", Format::Percent, 0.33),
///    ("2006-01-02T22:04:05Z", Format::Timestamp(0), 1136239445.0),
///    ("2006-01-02T15:04:05-07:00", Format::Timestamp(-25200), 1136239445.0),
/// ] {
///     let s = i.parse::<FormattedScalar>().unwrap();
///     assert_eq!(s.0, f);
///     assert_eq!(*s, o);
///     assert_eq!(s.to_string(), i);
/// }
/// assert!("2006-01-02T15:04:05".parse::<FormattedScalar>().is_err()); // Must have a time zone.
/// ```
pub struct FormattedScalar(pub Format, pub f64);

impl std::ops::Deref for FormattedScalar {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl FormattedScalar {
    /// Return the number of chars from this value that should be left of the
    /// alignment point for a right-aligned column.
    pub fn left_extension(&self) -> usize {
        let str = self.to_string();
        if let Some(pos) = str.find(['.', 'e', 'E', 'd', '%']) {
            // Try to clip before decimal point, then before exponent marker,
            // then before unit suffixes.
            str[..pos].chars().count()
        } else {
            // We *could* align timestamps so the time zones are to the right
            // of the alignment point, but eh, you probably won't have a lot
            // of cases where you mix Z and timestamp on the same column.

            // Otherwise just return the whole length.
            str.chars().count()
        }
    }
}

impl FromStr for FormattedScalar {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use chrono::{DateTime, NaiveDate};

        use Format::*;

        // Shorthands you can write in output cells to set the type without having
        // to write the full number.
        match s {
            "e" | "E" => return Ok(Self(Scientific, 0.0)),
            "~" => return Ok(Self(Integer, 0.0)),
            "d" => return Ok(Self(Days, 0.0)),
            "%" => return Ok(Self(Percent, 0.0)),
            "Z" => return Ok(Self(Timestamp(0), 0.0)),
            _ => {}
        }

        // Try to parse a timestamp, use the chrono crate for this.
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            let timestamp = dt.timestamp() as f64;
            let offset = dt.offset().local_minus_utc();
            return Ok(Self(Timestamp(offset), timestamp));
        }

        // The order is important, some formats will match prefixes of other
        // formats, so we must try to match the longer formats first. Eg.
        // TimeSeconds before Time, and Scientific befor Real.
        if let Some((_, num)) = regex_captures!(r"^~(-?[0-9]+)$", s) {
            let num = num.parse::<f64>()?;
            Ok(Self(Integer, num))
        } else if let Some((_, num, _)) = regex_captures!(r"^(-?\d+(\.\d*)?[eE][+-]?\d+)$", s) {
            let num = num.parse::<f64>()?;
            Ok(Self(Scientific, num))
        } else if let Some((_, num)) = regex_captures!(r"^0x([0-9a-fA-F]+)$", s) {
            let num = u64::from_str_radix(num, 16)? as f64;
            Ok(Self(Hexadecimal, num))
        } else if let Some((_, num)) = regex_captures!(r"^0b([01]+)$", s) {
            let num = u64::from_str_radix(num, 2)? as f64;
            Ok(Self(Binary, num))
        } else if let Some((_, h, min, sec)) = regex_captures!(r"^(\d+):([0-5]\d):([0-5]\d)$", s) {
            let hours = h.parse::<f64>()?;
            let minutes = min.parse::<f64>()?;
            let seconds = sec.parse::<f64>()?;
            let num = hours * 3600.0 + minutes * 60.0 + seconds;
            Ok(Self(TimeSeconds, num))
        } else if let Some((_, h, m)) = regex_captures!(r"^(\d+):([0-5]\d)$", s) {
            let hours = h.parse::<f64>()?;
            let minutes = m.parse::<f64>()?;
            let num = hours * 3600.0 + minutes * 60.0;
            Ok(Self(Time, num))
        } else if let Some((_, y, m, d)) = regex_captures!(r"^(\d{4})-(\d{2})-(\d{2})$", s) {
            let year = y.parse::<i32>()?;
            let month = m.parse::<u32>()?;
            let day = d.parse::<u32>()?;
            let dt =
                NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| anyhow!("Invalid date"))?;
            let num = dt
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow!("Invalid date"))?
                .and_utc()
                .timestamp() as f64;
            Ok(Self(Date, num))
        } else if let Some((_, num)) = regex_captures!(r"^(-?\d+)d$", s) {
            let num = num.parse::<f64>()?;
            let num = num * 86400.0;
            Ok(Self(Days, num))
        } else if let Some((_, num)) = regex_captures!(r"^(-?\d+)%$", s) {
            let num = num.parse::<f64>()?;
            let num = num / 100.0;
            Ok(Self(Percent, num))
        } else if let Some((_, num, _)) = regex_captures!(r"^(-?\d+(\.\d*)?)$", s) {
            let num = num.parse::<f64>()?;
            Ok(Self(Real, num))
        } else {
            bail!("No valid formatted number found")
        }
    }
}

impl Display for FormattedScalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};

        use Format::*;

        let num = self.1;
        match self.0 {
            Real => {
                // Smart precision printing logic.

                let abs = num.abs();
                // Figure out the precision, with precision 2 we want 1.234 -> "1.23"
                // but 0.000234 -> "0.00023".
                if abs < 1.0 && num != 0.0 {
                    let leading_zeros = (-abs.log10().floor() as isize - 1).max(0) as usize;
                    return write!(f, "{num:.p$}", p = leading_zeros + FLOAT_PRECISION);
                }

                let default = format!("{num}");
                let rounded = format!("{num:.p$}", p = FLOAT_PRECISION);
                if default.len() < rounded.len() {
                    write!(f, "{default}")
                } else {
                    write!(f, "{rounded}")
                }
            }
            Integer => write!(f, "~{}", num.round() as i64),
            Scientific => write!(f, "{num:.p$e}", p = FLOAT_PRECISION),
            Hexadecimal => write!(f, "0x{:x}", num.trunc() as i32),
            Binary => write!(f, "0b{:b}", num.trunc() as i32),
            Time => {
                let t = (num / 60.0).round() as u32;
                let hours = t / 60;
                let minutes = t % 60;
                write!(f, "{:02}:{:02}", hours, minutes)
            }
            TimeSeconds => {
                let t = num.round() as u32;
                let hours = t / 3600;
                let minutes = (t % 3600) / 60;
                let seconds = t % 60;
                write!(f, "{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
            Date => {
                let dt = DateTime::from_timestamp_secs(num.round() as i64).unwrap_or_default();
                write!(f, "{}", dt.format("%Y-%m-%d"))
            }
            Days => write!(f, "{}d", (num / 86400.0).round() as i64),
            Percent => write!(f, "{}%", (num * 100.0).round() as i64),
            Timestamp(offset) => {
                let dt = DateTime::from_timestamp_secs(num.round() as i64).unwrap_or_default();
                if offset != 0 {
                    let Some(offset) = FixedOffset::east_opt(offset) else {
                        write!(f, "ERR")?;
                        return Ok(());
                    };
                    let dt = dt.with_timezone(&offset);
                    write!(f, "{}", dt.to_rfc3339())
                } else {
                    // Print Z suffix for UTC timestamps.
                    let dt = dt.with_timezone(&Utc);
                    write!(f, "{}", dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                }
            }
        }
    }
}
