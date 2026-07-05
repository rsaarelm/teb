use anyhow::{Result, bail};

use crate::Table;

pub fn subscript_digit(s: &str) -> Result<(u32, &str)> {
    if s.is_empty() {
        bail!("No input");
    }

    let c = s.chars().next().unwrap();
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
        _ => bail!("Not a subscript digit: {c}"),
    };

    Ok((digit, &s[c.len_utf8()..]))
}

/// If the nonempty lines in input all share the exact same prefix made of
/// spaces and tabs, return that prefix. If nonempty lines have inconsistent
/// indentation, return an error.
pub fn indent_prefix(text: &str) -> Result<String> {
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
pub fn consecutive_content(input: &str) -> Result<(&str, &str)> {
    let mut lines = input.lines().peekable();
    // Skip leading empty lines.
    while let Some(line) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }

    if lines.peek().is_none() {
        bail!("No non-empty lines in input");
    }

    let start = lines.peek().unwrap().as_ptr() as usize;
    let mut end = input.len() + input.as_ptr() as usize;
    for line in lines {
        if line.trim().is_empty() {
            end = line.as_ptr() as usize;
            break;
        }
    }

    let content = &input[start - input.as_ptr() as usize..end - input.as_ptr() as usize];
    let rest = &input[end - input.as_ptr() as usize..];
    Ok((content, rest))
}

pub fn tables(mut input: &str, parse_numbers: bool) -> Result<Vec<Table>> {
    // While input remains, scan for groups of consecutive non-empty lines and
    // try to parse them into tables.
    let mut ret = Vec::new();

    while let Ok((chunk, rest)) = consecutive_content(input) {
        let table = Table::new(chunk, parse_numbers)?;
        ret.push(table);
        input = rest;
    }

    Ok(ret)
}

/// Read a positive floating point number from start of input, input can have
/// any junk immediately after the number. The number mustn't have a leading +
/// or - sign. Return a parsed number and the remaining input after it if
/// successful.
pub fn positive_float(s: &str) -> Result<(f64, &str)> {
    if s.starts_with('+') || s.starts_with('-') {
        bail!("Number must not have a leading + or - sign");
    }

    let (ret, bytes) = lexical_core::parse_partial::<f64>(s.as_bytes())?;
    Ok((ret, &s[bytes..]))
}

/// Return the first non-whitespace character from input and the remaining
/// input after it.
pub fn char(s: &str) -> Result<(char, &str)> {
    // Remember that we need to skip over any leading whitespace.
    let s = s.trim_start();
    if s.is_empty() {
        bail!("No input");
    }
    let c = s.chars().next().unwrap();
    Ok((c, &s[c.len_utf8()..]))
}
