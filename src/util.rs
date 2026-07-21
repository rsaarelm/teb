use anyhow::{bail, Result};

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
    use crate::util;

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

        assert!(util::decipher(ALIASES, "gibberish").is_err());
        assert_eq!(
            util::decipher(ALIASES, "addceilingdivide").unwrap(),
            vec!["+", "⌈", "÷"]
        );
        assert_eq!(
            util::decipher(ALIASES, "fstdivceiid").unwrap(),
            vec!["⊢", "÷", "⌈", "∘"]
        );
        // Plain "foo" is ambiguous, so this won't resolve to anything.
        assert!(util::decipher(ALIASES, "foodivceiid").is_err());
        assert_eq!(
            util::decipher(ALIASES, "foobdivceiid").unwrap(),
            vec!["x", "÷", "⌈", "∘"]
        );
        // "foo" isn't unique, so can't switch to matching "barfoo" yet and
        // this resolves fully to "foobar" instead"
        assert_eq!(util::decipher(ALIASES, "foobar").unwrap(), vec!["x"]);
        assert_eq!(
            util::decipher(ALIASES, "fooxdivceiid").unwrap(),
            vec!["y", "÷", "⌈", "∘"]
        );
        // Greedily grab the second valid abbrev instead of continuing along
        // "exponential".
        assert_eq!(util::decipher(ALIASES, "expone").unwrap(), vec!["ₑ", "q"]);
        // And now we can't match the second one, so it's just exponential again.
        assert_eq!(util::decipher(ALIASES, "exponen").unwrap(), vec!["ₑ"]);

        // Explicit abbreviation resolves an ambiguous match.
        assert_eq!(util::decipher(ALIASES, "ran").unwrap(), vec!["^"]);
    }
}
