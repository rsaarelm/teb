//! Stateless parsing primitives.
use std::f64;

use anyhow::bail;
use lazy_regex::regex_captures;

use crate::Table;

type Result<'a, T> = std::result::Result<(T, &'a str), &'a str>;

pub fn subscript_digit(s: &str) -> Result<'_, usize> {
    let (c, rest) = char(s)?;
    let digit = match c {
        '₀' => 0,
        '₁' => 1,
        '₂' => 2,
        '₃' => 3,
        '₄' => 4,
        '₅' => 5,
        '₆' => 6,
        '₇' => 7,
        '₈' => 8,
        '₉' => 9,
        _ => return Err(s),
    };

    Ok((digit, rest))
}

pub fn subscript_number(s: &str) -> Result<'_, u32> {
    let mut rest = s;
    let c = &mut rest;

    let mut number = 0;
    while let Ok(digit) = r(c, subscript_digit) {
        number = number * 10 + digit as u32;
    }

    if rest == s {
        return Err(s);
    }

    Ok((number, rest))
}

/// Parse "_123" into "₁₂₃".
pub fn ascii_subscript(s: &str) -> Result<'_, String> {
    let (_, rest) = literal(s, "_")?;
    let (digits, rest) = digits(rest)?;
    let mut ret = String::new();
    for d in digits.chars() {
        match d {
            '0' => ret.push('₀'),
            '1' => ret.push('₁'),
            '2' => ret.push('₂'),
            '3' => ret.push('₃'),
            '4' => ret.push('₄'),
            '5' => ret.push('₅'),
            '6' => ret.push('₆'),
            '7' => ret.push('₇'),
            '8' => ret.push('₈'),
            '9' => ret.push('₉'),
            _ => return Err(s),
        }
    }

    Ok((ret, rest))
}

/// If the nonempty lines in input all share the exact same prefix made of
/// spaces and tabs, return that prefix. If nonempty lines have inconsistent
/// indentation, return an error.
pub fn indent_prefix(text: &str) -> anyhow::Result<String> {
    // Note that we can't use Rust's whitespace trimming functions here
    // because they treat NBSPs as whitespace. We want to treat NBSPs as a
    // non-whitespace character we can use to shape left trims of tables that
    // have numbers in the leftmost column.
    let mut prefix: Option<String> = None;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_prefix = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect::<String>();
        if let Some(ref p) = prefix {
            if p != &line_prefix {
                bail!("Inconsistent indentation on line {}", i + 1);
            }
        } else {
            prefix = Some(line_prefix);
        }
    }

    Ok(prefix.unwrap_or_default())
}

/// Return the next chunk of consecutive non-empty lines from input (with any
/// preceding empty lines skipped), and the remaining input after the chunk.
/// Return an error if there are no non-empty lines in the input.
pub fn consecutive_content(s: &str) -> Result<'_, &str> {
    let mut lines = s.lines().peekable();
    // Skip leading empty lines.
    while let Some(line) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }

    if lines.peek().is_none() {
        return Err(s);
    }

    let start = lines.peek().unwrap().as_ptr() as usize;
    let mut end = s.len() + s.as_ptr() as usize;
    for line in lines {
        if line.trim().is_empty() {
            end = line.as_ptr() as usize;
            break;
        }
    }

    let content = &s[start - s.as_ptr() as usize..end - s.as_ptr() as usize];
    let rest = &s[end - s.as_ptr() as usize..];
    Ok((content, rest))
}

pub fn tables(s: &str, parse_numbers: bool) -> anyhow::Result<Vec<Table>> {
    // While input remains, scan for groups of consecutive non-empty lines and
    // try to parse them into tables.
    let mut ret = Vec::new();

    let mut rest = s;
    let c = &mut rest;
    while let Ok(chunk) = r(c, consecutive_content) {
        ret.push(Table::new(chunk, parse_numbers)?);
    }

    Ok(ret)
}

/// Read a positive floating point number from start of input, input can have
/// any junk immediately after the number. The number mustn't have a leading +
/// or - sign. Return a parsed number and the remaining input after it if
/// successful.
pub fn positive_float(s: &str) -> Result<'_, f64> {
    if let Some((num, _, _)) = regex_captures!(r"^\d+(\.\d+)?([eE][+-]?\d+)?", s) {
        Ok((num.parse().unwrap(), &s[num.len()..]))
    } else {
        Err(s)
    }
}

pub fn literal<'a>(s: &'a str, literal: &str) -> Result<'a, &'a str> {
    if s.starts_with(literal) {
        Ok((&s[..literal.len()], &s[literal.len()..]))
    } else {
        Err(s)
    }
}

pub fn literals<'a>(s: &'a str, literals: &[&str]) -> Result<'a, &'a str> {
    for &lit in literals {
        if let Ok((lit, rest)) = literal(s, lit) {
            return Ok((lit, rest));
        }
    }
    Err(s)
}

fn one_or_more<'a>(s: &str, f: impl Fn(char) -> bool) -> Result<'_, &str> {
    match s.find(|c: char| !f(c)).unwrap_or_else(|| s.len()) {
        0 => Err(s),
        end => Ok((&s[..end], &s[end..])),
    }
}

pub fn word(s: &str) -> Result<'_, &str> {
    one_or_more(s, char::is_alphabetic)
}

pub fn digits(s: &str) -> Result<'_, &str> {
    one_or_more(s, |c| c.is_ascii_digit())
}

/// Return the first non-whitespace character from input and the remaining
/// input after it.
pub fn char(s: &str) -> Result<'_, char> {
    let c = s.chars().next().ok_or(s)?;
    Ok((c, &s[c.len_utf8()..]))
}

/// Wrapper that treats a string reference as an advancing cursor pointer.
fn r<'a, T>(
    s: &mut &'a str,
    f: impl Fn(&'a str) -> Result<'a, T>,
) -> std::result::Result<T, &'a str> {
    let (ret, rest) = f(s)?;
    *s = rest;
    Ok(ret)
}
