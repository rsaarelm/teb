use anyhow::{Result, bail};

use crate::Table;

pub fn subscript_digit(s: &str) -> Result<(usize, &str)> {
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

pub fn subscript_number(s: &str) -> Result<(u32, &str)> {
    let mut number = 0;
    let mut rest = s;

    while let Ok((digit, r)) = subscript_digit(rest) {
        number = number * 10 + digit as u32;
        rest = r;
    }

    if rest == s {
        bail!("No subscript digits found");
    }

    Ok((number, rest))
}

/// Parse "_123" into "₁₂₃".
pub fn ascii_subscript(s: &str) -> Result<(String, &str)> {
    let rest = literal(s, "_")?;
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
            _ => bail!("Invalid digit for subscript: '{d}'"),
        }
    }

    Ok((ret, rest))
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

pub fn word(s: &str) -> Result<(&str, &str)> {
    // Take alphabetical characters as far as you can.
    let end = s
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or_else(|| s.len());
    if end == 0 {
        bail!("No word found");
    }
    Ok((&s[..end], &s[end..]))
}

pub fn literal<'a>(s: &'a str, literal: &str) -> Result<&'a str> {
    if s.starts_with(literal) {
        Ok(&s[literal.len()..])
    } else {
        bail!("Expected literal '{}'", literal);
    }
}

pub fn digits(s: &str) -> Result<(&str, &str)> {
    // Take digits as far as you can.
    let end = s
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or_else(|| s.len());
    if end == 0 {
        bail!("No digits found");
    }
    Ok((&s[..end], &s[end..]))
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

/// Decipher a string of concatenated and possible abbreviated aliases into a
/// sequence of their canonical counterparts. The lexicon list maps aliases
/// into the canonical strings and must be sorted.
pub fn decipher(lexicon: &[(&str, &str)], input: &str) -> Result<Vec<String>> {
    // Match a sequence of multiple potentially abbreviated aliases smushed
    // into one word into a vector of the corresponding symbols.
    //
    // An abbreviated prefix must be at least three letters to be considered.
    //
    // The results are in the second fields of the alias tuples.
    //
    // It's assumed that the aliases arreay is sorted alphabetically so you
    // can use binary search on them to find aliases.
    //
    // We don't know how long of an abbreviation to expect past the three
    // letters, so we need to run a greedy backtracking algorithm. When a word
    // is unambiguosly matched by a prefix, split to considering the rest of
    // the input as the start of the next word, and as the continuation of the
    // prefix. If a short word is matched completely, that is always treated
    // as decisive and takes precedence over any prefix.
    const MIN_PREFIX_LEN: usize = 3;

    let mut ret = Vec::new();

    if input.is_empty() {
        return Ok(ret);
    }

    // Start of the prefix under consideration.
    let mut start = 0;
    // End of the prefix under consideration.
    let mut pos = input.ceil_char_boundary(1);

    // Scan forward in input until we
    // 1. Run out of input, if there's any pending input, fail, otherwise
    //    return [].
    // 2. Get zero matches, fail.
    // 3. Match a complete word, add it to result and continue scanning. A
    //    complete match always takes precedence over a prefix.
    // 4. Unambiguously match a prefix, try to greedily match the rest of the
    //    input assuming it starts from a new word. If that fails, continue
    //    matching more of this prefix.
    while start < input.len() {
        let prefix = &input[start..pos];

        match lexicon.binary_search_by(|(alias, _)| alias.cmp(&prefix)) {
            Ok(idx) => {
                // Matched the whole word, add it to the result and continue
                // deciphering the rest of the string.
                ret.push(lexicon[idx].1.to_string());
                start = pos;
                pos = input.ceil_char_boundary(pos + 1);
            }
            Err(idx) => {
                // No complete match, but maybe a prefix match.
                if idx == lexicon.len() || !lexicon[idx].0.starts_with(prefix) {
                    // No prefix match either.
                    bail!("decipher: No match");
                }

                // There is at least one prefix match. Check if it's unambiguous.
                let mut next_idx = idx;
                while next_idx < lexicon.len() && lexicon[next_idx].0.starts_with(prefix) {
                    next_idx += 1;
                }

                if next_idx - idx == 1 && prefix.len() >= MIN_PREFIX_LEN {
                    // Unambiguous prefix match (and our prefix is long enough
                    // to count). Try to greedily match the rest of the input.
                    let preliminary_result = &lexicon[idx].1;
                    let Ok(rest) = decipher(lexicon, &input[pos..]) else {
                        // Greedy scan failed. Continue matching this prefix.
                        pos = input.ceil_char_boundary(pos + 1);
                        continue;
                    };

                    // Matched the rest! Build the full solution.
                    ret.push(preliminary_result.to_string());
                    ret.extend(rest);
                    break;
                } else if pos < input.len() {
                    // Ambiguous prefix match or prefix is still too short,
                    // continue scanning forward.
                    pos = input.ceil_char_boundary(pos + 1);
                } else {
                    bail!("decipher: Ambiguous prefix at end of input");
                }
            }
        }
    }

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use crate::parse;

    #[test]
    fn test_decipher() {
        static ALIASES: &[(&str, &str)] = &[
            ("add", "+"),
            ("barfoo", "z"),
            ("ceiling", "⌈"),
            ("divide", "÷"),
            ("exponential", "ₑ"),
            ("first", "⊢"),
            ("floor", "⌊"),
            ("flor", "⌊"),
            ("flr", "⌊"),
            ("foobar", "x"),
            ("fooxor", "y"),
            ("fork", "⊃"),
            ("fst", "⊢"),
            ("id", "∘"),
            ("onesie", "q"),
            ("ran", "^"),
            ("random", "r"),
            ("range", "^"),
            ("rng", "r"),
        ];

        assert!(parse::decipher(ALIASES, "gibberish").is_err());
        assert_eq!(
            parse::decipher(ALIASES, "addceilingdivide").unwrap(),
            vec!["+", "⌈", "÷"]
        );
        assert_eq!(
            parse::decipher(ALIASES, "fstdivceiid").unwrap(),
            vec!["⊢", "÷", "⌈", "∘"]
        );
        // Plain "foo" is ambiguous, so this won't resolve to anything.
        assert!(parse::decipher(ALIASES, "foodivceiid").is_err());
        assert_eq!(
            parse::decipher(ALIASES, "foobdivceiid").unwrap(),
            vec!["x", "÷", "⌈", "∘"]
        );
        // "foo" isn't unique, so can't switch to matching "barfoo" yet and
        // this resolves fully to "foobar" instead"
        assert_eq!(parse::decipher(ALIASES, "foobar").unwrap(), vec!["x"]);
        assert_eq!(
            parse::decipher(ALIASES, "fooxdivceiid").unwrap(),
            vec!["y", "÷", "⌈", "∘"]
        );
        // Greedily grab the second valid abbrev instead of continuing along
        // "exponential".
        assert_eq!(parse::decipher(ALIASES, "expone").unwrap(), vec!["ₑ", "q"]);
        // And now we can't match the second one, so it's just exponential again.
        assert_eq!(parse::decipher(ALIASES, "exponen").unwrap(), vec!["ₑ"]);

        // Explicit abbreviation resolves an ambiguous match.
        assert_eq!(parse::decipher(ALIASES, "ran").unwrap(), vec!["^"]);
    }
}
