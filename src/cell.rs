use std::{
    fmt::{Display, Write},
    str::FromStr,
};

use anyhow::{Result, anyhow, bail};
use lazy_regex::regex_captures;

use crate::vm;

pub const FLOAT_PRECISION: usize = 2;

#[derive(Clone, Debug)]
pub enum Cell {
    Text(String),
    Input(Value),
    Output {
        value: Value,
        formula: String,
        hidden_formula: bool,
    },
}

impl Cell {
    /// Force a text cell, no matter what the input looks like.
    pub fn text(text: impl Into<String>) -> Self {
        Cell::Text(text.into())
    }

    pub fn set_output_text(&mut self, text: impl Into<String>) {
        if let Cell::Output { value, .. } = self {
            value.set_text(text)
        }
    }

    pub fn assign_output(&mut self, val: f64) {
        if let Cell::Output { value, .. } = self {
            value.assign(val)
        }
    }

    pub fn left_extension(&self) -> Option<&str> {
        match self {
            Cell::Text(_) => None,
            Cell::Input(v) => Some(v.left_extension()),
            Cell::Output { value, .. } => Some(value.left_extension()),
        }
    }

    pub fn has_formula(&self) -> bool {
        matches!(self, Cell::Output { formula, hidden_formula, .. } if !formula.is_empty() && !hidden_formula)
    }

    pub fn inherit_from(&mut self, other: &Cell) {
        // Self must be an output cell with an empty formula.
        match (self, other) {
            (
                Cell::Output {
                    value,
                    formula,
                    hidden_formula,
                },
                Cell::Output {
                    value: other_v,
                    formula: other_f,
                    ..
                },
            ) if formula.is_empty() => {
                // If we don't specify formatting, inherit from other.
                if value.is_empty() {
                    *value = Value::empty_formatted(other_v.format());
                }
                // Inherit formula from other.
                *formula = other_f.clone();
                // Set formula as hidden so it won't be echoed.
                *hidden_formula = true;
            }
            _ => {}
        }
    }

    /// How much should this cell be indented when printed to a column with
    /// the given maximum left extent.
    pub fn column_indent(&self, max_extension: usize) -> usize {
        match self.left_extension() {
            // Text is left-aligned.
            None => 0,
            Some(e) => max_extension.saturating_sub(e.chars().count()),
        }
    }

    pub fn chars(&self) -> Box<dyn Iterator<Item = char> + '_> {
        match self {
            Cell::Text(s) => Box::new(s.chars()),
            Cell::Input(v) => Box::new(v.as_ref().chars()),
            Cell::Output {
                value,
                hidden_formula: true,
                ..
            } => Box::new(value.as_ref().chars().chain(std::iter::once('<'))),
            Cell::Output { value, formula, .. } => Box::new(
                value
                    .as_ref()
                    .chars()
                    .chain(std::iter::once('<'))
                    .chain(formula.chars()),
            ),
        }
    }
}

impl FromStr for Cell {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        use Cell::*;

        // An input value?
        if let Ok(mut value) = Value::from_str(s) {
            value.prettify();
            return Ok(Input(value));
        }

        // Formula with optional value before it.
        if let Some((val, formula)) = s.split_once('<') {
            let formula = vm::prettify_formula(&formula);

            // The value must be parseable or empty.
            if let Ok(mut value) = if !val.is_empty() {
                Value::from_str(val)
            } else {
                Ok(Default::default())
            } {
                value.prettify();
                return Ok(Output {
                    value,
                    formula,
                    hidden_formula: false,
                });
            }
        }

        // Anything else is a text cell.
        Ok(Text(s.to_string()))
    }
}

impl Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.chars() {
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

/////

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Format {
    /// The default formatter, just regular numbers. Eg. 3.14. Has optional
    /// precision.
    Real(u8),
    /// Always round to full integers, 3.14 = ~3
    Integer,
    /// Use scientific notation, 3268 = 3.27e3
    Scientific(u8),
    /// [Magnitude notation](https://magworld.pw/articles/notation/), a more
    /// human-friendly variant of scientific notation. 20000 = ↑4.3 = ^4.3
    Mag,
    /// Hexadecimal, fraction part is truncated. 16.25 = 0x10. Has optional
    /// zero padding count.
    Hexadecimal(u8),
    /// Binary, fraction part is truncated. 3.14 = 0b11. Has optional zero
    /// padding count.
    Binary(u8),
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

impl Default for Format {
    fn default() -> Self {
        Format::Real(0)
    }
}

impl Format {
    /// Default string that can be used to infer the format from.
    pub fn zero_string(self) -> String {
        match self {
            Format::Real(p) => {
                if p == 0 {
                    "0".to_string()
                } else {
                    format!("{:.p$}", 0.0, p = p as usize)
                }
            }
            Format::Integer => "~0".to_string(),
            Format::Scientific(p) => {
                if p == 0 {
                    "0e0".to_string()
                } else {
                    format!("{:.p$}e0", 0.0, p = p as usize)
                }
            }
            Format::Mag => "↑0".to_string(),
            Format::Hexadecimal(p) => {
                if p == 0 {
                    "0x0".to_string()
                } else {
                    format!("0x{:0<p$}", "", p = p as usize)
                }
            }
            Format::Binary(p) => {
                if p == 0 {
                    "0b0".to_string()
                } else {
                    format!("0b{:0<p$}", "", p = p as usize)
                }
            }
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

    pub fn write(&self, w: &mut impl Write, num: f64) -> std::fmt::Result {
        use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};

        use Format::*;

        match *self {
            Real(p) => {
                // Smart precision printing logic.

                // If we specify a precision, we always get 2.00, but the
                // default print will drop zeros and give us 2. On the other
                // hand the default will also give use a long string of
                // decimals, so just try both and pick the shorter of the two
                // here.

                if p > 0 {
                    return write!(w, "{num:.p$}", p = p as usize);
                }

                let default = format!("{num}");
                let rounded = format!("{num:.p$}", p = FLOAT_PRECISION);
                if default.len() < rounded.len() {
                    write!(w, "{default}")
                } else {
                    write!(w, "{rounded}")
                }
            }
            Integer => write!(w, "~{}", num.round() as i64),
            Scientific(p) => {
                if p == 0 {
                    write!(w, "{num:.p$e}", p = FLOAT_PRECISION)
                } else {
                    write!(w, "{num:.p$e}", p = p as usize)
                }
            }
            Mag => {
                // One decimal precision only on mags, if you need more, use sci
                // notation.
                let m = num.log10();
                let r = (m * 10.0).round() / 10.0;
                if r.fract().abs() < 0.001 {
                    write!(w, "↑{}", m.round())
                } else {
                    write!(w, "↑{m:.1}")
                }
            }
            Hexadecimal(p) => {
                if p == 0 {
                    write!(w, "0x{:x}", num.trunc() as i32)
                } else {
                    write!(w, "0x{:0p$x}", num.trunc() as i32, p = p as usize)
                }
            }
            Binary(p) => {
                if p == 0 {
                    write!(w, "0b{:b}", num.trunc() as i32)
                } else {
                    write!(w, "0b{:0p$b}", num.trunc() as i32, p = p as usize)
                }
            }
            Time => {
                let t = (num / 60.0).round() as u32;
                let hours = t / 60;
                let minutes = t % 60;
                write!(w, "{:02}:{:02}", hours, minutes)
            }
            TimeSeconds => {
                let t = num.round() as u32;
                let hours = t / 3600;
                let minutes = (t % 3600) / 60;
                let seconds = t % 60;
                write!(w, "{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
            Date => {
                let dt = DateTime::from_timestamp_secs(num.round() as i64).unwrap_or_default();
                write!(w, "{}", dt.format("%Y-%m-%d"))
            }
            Days => write!(w, "{}d", (num / 86400.0).round() as i64),
            Percent => write!(w, "{}%", (num * 100.0).round() as i64),
            Timestamp(offset) => {
                let dt = DateTime::from_timestamp_secs(num.round() as i64).unwrap_or_default();
                if offset != 0 {
                    let Some(offset) = FixedOffset::east_opt(offset) else {
                        write!(w, "ERR")?;
                        return Ok(());
                    };
                    let dt = dt.with_timezone(&offset);
                    write!(w, "{}", dt.to_rfc3339())
                } else {
                    // Print Z suffix for UTC timestamps.
                    let dt = dt.with_timezone(&Utc);
                    write!(w, "{}", dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                }
            }
        }
    }
}

/////

/// Representation of numbers that may have different string representations.
///
/// ```
/// use teb::{parse, Format, Value};
/// for (i, f, o) in [
///    ("3.14", Format::Real(0), 3.14),  // Precision only registers when it goes above the default 2.
///    ("3.1415", Format::Real(4), 3.1415),
///    ("~3", Format::Integer, 3.0),
///    ("3.14e2", Format::Scientific(0), 314.0),
///    ("3.1415e2", Format::Scientific(4), 314.15),
///    ("↑3", Format::Mag, 1000.0),
///    ("^3", Format::Mag, 1000.0),
///    ("↑-3", Format::Mag, 0.001),
///    ("^-3", Format::Mag, 0.001),
///    ("0x10", Format::Hexadecimal(0), 16.0),
///    ("0x0010", Format::Hexadecimal(4), 16.0),
///    ("0b11", Format::Binary(0), 3.0),
///    ("0b0011", Format::Binary(4), 3.0),
///    ("01:02", Format::Time, 3720.0),
///    ("01:02:10", Format::TimeSeconds, 3730.0),
///    ("1970-01-13", Format::Date, 1036800.0),
///    ("3d", Format::Days, 259200.0),
///    ("33%", Format::Percent, 0.33),
///    ("2006-01-02T22:04:05Z", Format::Timestamp(0), 1136239445.0),
///    ("2006-01-02T15:04:05-07:00", Format::Timestamp(-25200), 1136239445.0),
/// ] {
///     let s = i.parse::<Value>().unwrap();
///     assert_eq!(s.format(), f);
///     assert_eq!(s.as_f64(), o);
///     assert_eq!(s.to_string(), i);
/// }
/// assert!("2006-01-02T15:04:05".parse::<Value>().is_err()); // Must have a time zone.
/// ```
#[derive(Clone, Default, Debug)]
pub struct Value {
    /// Display format of the value.
    format: Format,
    /// Verbatim text of the value. May be different from
    /// `self.format.write(self.value)`, eg. "00123" vs the canonical "123".
    text: String,
    /// Numeric value.
    value: f64,
    /// Offset to self.text for the fragment that goes left of the alignment
    /// point of a right-aligned column.
    left_extent: usize,
}

impl Value {
    pub fn new(format: Format, value: f64) -> Self {
        let mut text = String::new();
        format.write(&mut text, value).unwrap();
        Self::build(format, value, text)
    }

    pub fn empty_formatted(format: Format) -> Self {
        Self::build(format, 0.0, "")
    }

    pub fn prettify(&mut self) {
        if self.format == Format::Mag {
            // Mag notation has the ASCII-friendly ^ character, but we want
            // clean tables to use the unicode arrow prefix instead.
            if let Some(t) = self.text.strip_prefix('^') {
                self.set_text(format!("↑{t}"));
            }
        }
    }

    /// Set text of the value.
    ///
    /// Only call this directly when you know what you're doing, you can
    /// rewrite the value to look like anything and a random text value won't
    /// parse back to the original format and numeric value again.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();

        if text.contains(char::is_whitespace) {
            panic!("Trying to assign a value with whitespace to a cell.");
        }

        self.text = text;

        // Figure out the left extent. Decimal parts, exponent parts and unit
        // suffixes should go right of the alignment point.

        self.left_extent = if self.text.starts_with("0x") {
            // Hex strings can have es and ds so catch them first.
            self.text.len()
        } else if let Some(pos) = self.text.find(['e', 'E']) {
            // Snap to exponents before you snap to decimal points, a column
            // of scientific notation values should group by exponent.
            pos
        } else if let Some(pos) = self.text.find(['.', 'd', '%']) {
            pos
        } else {
            self.text.len()
        };
    }

    /// Assign a new numeric value, it will be formatted according to the
    /// value's current format.
    pub fn assign(&mut self, value: f64) {
        self.value = value;
        let mut text = String::new();
        self.format.write(&mut text, value).unwrap();
        self.set_text(text);
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn left_extension(&self) -> &str {
        &self.text[..self.left_extent]
    }

    pub fn as_f64(&self) -> f64 {
        self.value
    }

    fn build(format: Format, value: f64, text: impl Into<String>) -> Self {
        let mut ret = Value {
            format,
            value,
            ..Default::default()
        };
        ret.set_text(text);

        ret
    }
}

impl AsRef<str> for Value {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl FromStr for Value {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use chrono::{DateTime, NaiveDate};

        use Format::*;

        // Shorthands you can write in output cells to set the type without having
        // to write the full number.
        match s {
            "e" | "E" => return Ok(Self::build(Scientific(0), 0.0, s)),
            "↑" | "^" => return Ok(Self::build(Mag, 0.0, s)),
            "~" => return Ok(Self::build(Integer, 0.0, s)),
            "d" => return Ok(Self::build(Days, 0.0, s)),
            "%" => return Ok(Self::build(Percent, 0.0, s)),
            "Z" => return Ok(Self::build(Timestamp(0), 0.0, s)),
            "0x" => return Ok(Self::build(Hexadecimal(0), 0.0, s)),
            "0b" => return Ok(Self::build(Binary(0), 0.0, s)),
            _ => {}
        }

        // Try to parse a timestamp, use the chrono crate for this.
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            let timestamp = dt.timestamp() as f64;
            let offset = dt.offset().local_minus_utc();
            return Ok(Self::build(Timestamp(offset), timestamp, s));
        }

        // The order is important, some formats will match prefixes of other
        // formats, so we must try to match the longer formats first. Eg.
        // TimeSeconds before Time, and Scientific befor Real.
        if let Some((_, num)) = regex_captures!(r"^~(-?[0-9]+)$", s) {
            let num = num.parse::<f64>()?;
            Ok(Self::build(Integer, num, s))
        } else if let Some((_, num, _)) = regex_captures!(r"^(-?\d+(\.\d*)?[eE][+-]?\d+)$", s) {
            let num = num.parse::<f64>()?;

            let mut p = 0;
            // Determine precision, count consecutive digits after decimal
            // dot.
            if let Some(dot_pos) = s.find('.') {
                let precision = s[dot_pos + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                if precision > FLOAT_PRECISION {
                    p = precision as u8;
                }
            }

            Ok(Self::build(Scientific(p), num, s))
        } else if let Some((_, num, _)) = regex_captures!(r"^[↑^](-?\d+(\.\d*)?)$", s) {
            let mut num = num.parse::<f64>()?;
            num = 10f64.powf(num);
            Ok(Self::build(Mag, num, s))
        } else if let Some((_, num)) = regex_captures!(r"^0x([0-9a-fA-F]+)$", s) {
            let num = u64::from_str_radix(num, 16)? as f64;
            let len = s.chars().count() - 2;
            if len > 1 && s.contains("x0") {
                Ok(Self::build(Hexadecimal(len as u8), num, s))
            } else {
                Ok(Self::build(Hexadecimal(0), num, s))
            }
        } else if let Some((_, num)) = regex_captures!(r"^0b([01]+)$", s) {
            let num = u64::from_str_radix(num, 2)? as f64;
            let len = s.chars().count() - 2;
            if len > 1 && s.contains("b0") {
                Ok(Self::build(Binary(len as u8), num, s))
            } else {
                Ok(Self::build(Binary(0), num, s))
            }
        } else if let Some((_, h, m, sec)) = regex_captures!(r"^(\d+):([0-5]\d):([0-5]\d)$", s) {
            let hours = h.parse::<f64>()?;
            let minutes = m.parse::<f64>()?;
            let seconds = sec.parse::<f64>()?;
            let num = hours * 3600.0 + minutes * 60.0 + seconds;
            Ok(Self::build(TimeSeconds, num, s))
        } else if let Some((_, h, m)) = regex_captures!(r"^(\d+):([0-5]\d)$", s) {
            let hours = h.parse::<f64>()?;
            let minutes = m.parse::<f64>()?;
            let num = hours * 3600.0 + minutes * 60.0;
            Ok(Self::build(Time, num, s))
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
            Ok(Self::build(Date, num, s))
        } else if let Some((_, num)) = regex_captures!(r"^(-?\d+)d$", s) {
            let num = num.parse::<f64>()?;
            let num = num * 86400.0;
            Ok(Self::build(Days, num, s))
        } else if let Some((_, num)) = regex_captures!(r"^(-?\d+)%$", s) {
            let num = num.parse::<f64>()?;
            let num = num / 100.0;
            Ok(Self::build(Percent, num, s))
        } else if let Some((_, num, _)) = regex_captures!(r"^(-?\d+(\.\d*)?)$", s) {
            let num = num.parse::<f64>()?;

            let mut p = 0;
            // Determine precision, count consecutive digits after decimal
            // dot.
            if let Some(dot_pos) = s.find('.') {
                let precision = s[dot_pos + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                if precision > FLOAT_PRECISION {
                    p = precision as u8;
                }
            }

            Ok(Self::build(Real(p), num, s))
        } else {
            bail!("No valid formatted number found")
        }
    }
}
