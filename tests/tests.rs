use std::sync::LazyLock;

use pretty_assertions::assert_eq;

use teb;

/// Pairs of input / expected output.
static SUITE: LazyLock<Vec<String>> = LazyLock::new(|| {
    let input = include_str!("suite.txt");
    let mut ret = vec![String::new()];
    for line in input.lines() {
        let line = line.trim_end();
        // Empty lines separate inputs.
        if line.is_empty() {
            ret.push(String::new());
            continue;
        }
        let s = ret.last_mut().unwrap();
        // Represent empty line in input with a solitary '%'.
        if line == "%" {
            s.push('\n');
            continue;
        } else {
            s.push_str(line);
            s.push('\n');
        }
    }
    ret
});

#[test]
fn test_suite() {
    for a in SUITE.chunks(2) {
        let input = &a[0];

        let tables = teb::parse::tables(&input, true).unwrap();
        let mut output = String::new();
        for (i, table) in tables.iter().enumerate() {
            output.push_str(&table.to_string());
            if i < tables.len() - 1 {
                output.push('\n');
            }
        }

        assert_eq!(output, a[1]);
    }
}
