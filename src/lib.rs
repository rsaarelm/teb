use std::{
    fmt::Display,
    iter::once,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use anyhow::{Result, bail};
use itertools::Itertools;

mod array;

/// A table of whitespace-separated cells.
#[derive(Clone, Default, Debug)]
pub struct Table {
    pub cells: Vec<Vec<Cell>>,
    pub indent_prefix: String,
}

impl Table {
    /// Construct a new table from an outline with parsed cells and column
    /// count. If `parse_numbers` is false, all table cells will be treated as
    /// text and no formula execution is attempted.
    pub fn new(input: &str, parse_numbers: bool) -> Result<Self> {
        let mut table = Table::default();

        // Figure out how many columns we'll be aligning.
        //
        // Tables can have trailing data after the last column which varies
        // per row. This will not be aligned. So we're looking for the minimum
        // number of whitespace-separated words every non-empty line has.
        // Every line that has items past this gets all of the extra content
        // put verbatim in the last column, spaces and all.
        let mut columns = usize::MAX;
        let mut is_empty = true;
        for line in input.lines() {
            // Completely empty lines are allowed.
            if line.trim().is_empty() {
                continue;
            }

            is_empty = false;

            // Count whitespace-separated words on table lines.
            let line_columns = line.split_whitespace().count();
            assert!(line_columns > 0);
            columns = columns.min(line_columns);
        }

        if is_empty {
            // No content seen, return the empty table.
            return Ok(table);
        }

        table.indent_prefix = indent_prefix(input)?;

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                table.cells.push(Vec::new());
                continue;
            }

            // Offsets where words start in the line.
            //
            // Not using split_whitespace here because we want to preserve
            // whatever spacing the final trailing sections of the table lines
            // use between their words.
            let word_starts: Vec<usize> = line
                .char_indices()
                .zip(once(' ').chain(line.chars()))
                .filter_map(|((i, c), prev)| {
                    (prev.is_whitespace() && !c.is_whitespace()).then_some(i)
                })
                .collect();

            let mut row = Vec::new();

            for (i, (&a, &b)) in word_starts
                .iter()
                .chain(Some(&line.len()))
                .tuple_windows()
                .enumerate()
            {
                if i < columns {
                    // Keep pushing individual words while we have columns.
                    let cell = if parse_numbers {
                        line[a..b].parse()?
                    } else {
                        // If number parsing is disabled, all cells are forced
                        // to be text.
                        Cell::text(line[a..b].trim())
                    };
                    row.push(cell);
                } else {
                    // If there's still content left, push all of it to the extra
                    // column that's never numeric.
                    row.push(Cell::text(line[a..].trim()));
                    break;
                }
            }

            // Sanity-check our earlier column calculation.
            assert!(columns <= row.len() && row.len() <= columns + 1);
            table.cells.push(row);
        }

        Ok(table)
    }

    /// Evalaute spreadsheet formulas in all cells and insert their results.
    pub fn eval(&mut self, clear_outputs: bool) -> Result<()> {
        for row in 0..self.cells.len() {
            for col in 0..self.cells[row].len() {
                if clear_outputs {
                    if self.cells[row][col].is_formula() {
                        self.cells[row][col].assign(NumberValue::empty());
                    }
                } else {
                    self.eval_cell(row, col)?;
                }
            }
        }
        Ok(())
    }

    fn eval_cell(&mut self, row: usize, col: usize) -> Result<()> {
        let Some(c) = self
            .cells
            .get_mut(row)
            .and_then(|row| row.get_mut(col))
            .cloned()
        else {
            return Ok(());
        };

        // Construct the stack and extract the formula string. The formula
        // marker indicates whether we build a horizontal (from the table row
        // before current cell) or vertical (from the table column above
        // current cell) stack.
        let (mut s, formula) = match c {
            Cell::HorizontalFormula(_, ref f) => (Stack::horizontal(self, row, col), f),
            Cell::VerticalFormula(_, ref f) => (Stack::vertical(self, row, col), f),
            _ => return Ok(()),
        };

        // Must have some stack values to evaluate the formula.
        if s.is_empty() {
            self.cells[row][col].assign(NumberValue::empty());
            return Ok(());
        }

        // Initial stack length, does not change even though formula
        // evaluation may consume and emit stack values.
        let stack_length = s.len();

        // Number literal accumulator.
        let mut acc = f64::NAN;

        for c in formula.chars() {
            // Number literal parsing, only natural numbers are supported.
            if c.is_ascii_digit() {
                if acc.is_nan() {
                    acc = 0.0;
                }

                // Accumulate digits into a number.
                acc = acc * 10.0 + (c as u8 - b'0') as f64;
                continue;
            } else if !acc.is_nan() {
                s.push(acc);
                acc = f64::NAN;
            }

            // Weird glyphs are inspired by uiua.
            match c {
                '+' => {
                    // Addition
                    let val = s.pop()? + s.pop()?;
                    s.push(val);
                }
                '-' => {
                    // Subtraction
                    let a = s.pop()?;
                    let b = s.pop()?;
                    s.push(b - a);
                }
                '%' | '÷' => {
                    // Division
                    let a = s.pop()?;
                    let b = s.pop()?;
                    s.push(b / a);
                }
                '*' | '×' => {
                    // Multiplication
                    let val = s.pop()? * s.pop()?;
                    s.push(val);
                }
                '·' => {
                    // Drop stack item
                    s.pop()?;
                }
                '⁅' => {
                    // Round to nearest integer.
                    let val = s.pop()?;
                    s.push(val.round());
                }
                '#' => {
                    // Initial stack length.
                    // We generally never want the length of the stack after
                    // we've started operating on it, so this returns a cached
                    // value from when the stack was initialized.
                    s.push(stack_length as f64);
                }
                '√' => {
                    // Square root
                    let val = s.pop()?;
                    s.push(val.sqrt());
                }
                '~' => {
                    // Swap top elements
                    let a = s.pop()?;
                    let b = s.pop()?;
                    s.push(a);
                    s.push(b);
                }
                '.' => {
                    // Duplicate top element
                    let a = s.pop()?;
                    s.push(a);
                    s.push(a);
                }
                '²' => {
                    // Square top element
                    let a = s.pop()?;
                    s.push(a * a);
                }
                // If we had reduce (/), sum and product would be shorthand
                // for /+ and /*
                'Σ' => {
                    // Stack sum
                    let sum: f64 = s.iter().sum();
                    s.clear();
                    s.push(sum);
                }
                'Π' => {
                    // Stack product
                    let prod: f64 = s.iter().product();
                    s.clear();
                    s.push(prod);
                }

                c => {
                    bail!("tf: Unsupported formula character '{c}' at ({row}, {col})")
                }
            }
        }
        if let Some(result) = s.top() {
            self.cells[row][col].assign(result);
        } else {
            bail!("tf: Stack underflow at ({row}, {col})")
        }
        Ok(())
    }

    fn column(&self, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells.iter().filter_map(move |row| row.get(col))
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let columns = self
            .cells
            .iter()
            .filter_map(|row| (!row.is_empty()).then_some(row.len()))
            .min()
            .unwrap_or(0);

        // Each column's maximum left extension value.
        let left_extents = (0..columns)
            .map(|i| {
                self.column(i)
                    .map(|c| c.left_extension())
                    .max()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();

        // Total width for each column
        let column_widths = (0..columns)
            .map(|i| {
                self.column(i)
                    .map(|c| c.column_indent(left_extents[i]) + c.len())
                    .max()
                    .unwrap_or(0)
                    + 2 // The 2-space gap between columns
            })
            .collect::<Vec<_>>();

        // Print the table.
        for row in &self.cells {
            if row.is_empty() {
                writeln!(f)?;
                continue;
            }

            write!(f, "{}", self.indent_prefix)?;

            for (i, c) in row.iter().enumerate() {
                // The final bit, always push it in as is.
                if i >= columns {
                    write!(f, "{c}")?;
                    continue;
                }

                let indent = c.column_indent(left_extents[i]);

                // Pad to meet left pos. To stay IDM-compatible, the leftmost
                // column needs to be padded with NBSPs (\u{00A0}) that don't read as
                // whitespace to IDM.
                if indent > 0 {
                    if i == 0 {
                        write!(f, "{:\u{00A0}^indent$}", "",)?;
                    } else {
                        // Otherwise use spaces.
                        write!(f, "{: ^indent$}", "",)?;
                    }
                }

                write!(f, "{c}")?;

                let right_pad = column_widths[i] - indent - c.len();

                // Right-padding and space between columns.
                if i < row.len() - 1 {
                    write!(f, "{: <right_pad$}", "",)?;
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

struct Stack {
    stack: Vec<f64>,
    is_scientific: bool,
}

impl Stack {
    fn horizontal(table: &Table, row: usize, col: usize) -> Self {
        let mut is_scientific = false;
        let mut stack = Vec::new();

        if let Some(row) = table.cells.get(row) {
            for c in row.iter().take(col) {
                let Some(val) = c.value() else {
                    continue;
                };

                if val.is_scientific() {
                    is_scientific = true;
                }
                stack.push(val.val());
            }
        }

        Stack {
            stack,
            is_scientific,
        }
    }

    fn vertical(table: &Table, row: usize, col: usize) -> Self {
        let mut is_scientific = false;
        let mut stack = Vec::new();

        for r in 0..row {
            if let Some(c) = table.cells.get(r).and_then(|row| row.get(col)) {
                let Some(val) = c.value() else {
                    continue;
                };

                if val.is_scientific() {
                    is_scientific = true;
                }
                stack.push(val.val());
            }
        }

        Stack {
            stack,
            is_scientific,
        }
    }

    fn pop(&mut self) -> Result<f64> {
        self.stack
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Stack underflow"))
    }

    fn top(&self) -> Option<NumberValue> {
        if let Some(&val) = self.stack.last() {
            if self.is_scientific {
                Some(NumberValue::scientific(val))
            } else {
                Some(NumberValue::new(val))
            }
        } else {
            None
        }
    }
}

impl Deref for Stack {
    type Target = Vec<f64>;

    fn deref(&self) -> &Self::Target {
        &self.stack
    }
}

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stack
    }
}

#[derive(Clone, Debug)]
pub enum Cell {
    Text(String),
    Num(NumberValue),
    VerticalFormula(NumberValue, String),
    HorizontalFormula(NumberValue, String),
}
use Cell::*;

impl FromStr for Cell {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        if let Some((val, form)) = s.split_once(',') {
            if let Ok(val) = val.parse::<NumberValue>() {
                return Ok(HorizontalFormula(val, form.to_string()));
            } else if val.is_empty() {
                return Ok(HorizontalFormula(NumberValue::empty(), form.to_string()));
            }
        } else if let Some((val, form)) = s.split_once('^') {
            if let Ok(val) = val.parse::<NumberValue>() {
                return Ok(VerticalFormula(val, form.to_string()));
            } else if val.is_empty() {
                return Ok(VerticalFormula(NumberValue::empty(), form.to_string()));
            }
        } else if let Ok(val) = s.parse::<NumberValue>() {
            return Ok(Num(val));
        }
        Ok(Text(s.to_string()))
    }
}

impl Cell {
    /// Force a text cell, even if the string looks like a number or formula.
    fn text(s: impl Into<String>) -> Self {
        Cell::Text(s.into())
    }

    fn len(&self) -> usize {
        match self {
            Text(s) => s.len(),
            Num(n) => n.as_str().len(),
            VerticalFormula(n, form) => n.as_str().len() + 1 + form.len(),
            HorizontalFormula(n, form) => n.as_str().len() + 1 + form.len(),
        }
    }

    /// How much should this cell be indented when printed to a column wtih
    /// the given maximum left extent.
    fn column_indent(&self, max_left_extent: usize) -> usize {
        if !self.is_numeric() {
            return 0;
        }
        let left_extent = self.left_extension();
        assert!(left_extent <= max_left_extent);
        max_left_extent - left_extent
    }

    fn is_numeric(&self) -> bool {
        !matches!(self, Cell::Text(_))
    }

    fn value(&self) -> Option<&NumberValue> {
        match self {
            Text(_) => None,
            Num(n) | VerticalFormula(n, _) | HorizontalFormula(n, _) => Some(n),
        }
    }

    fn is_formula(&self) -> bool {
        matches!(self, VerticalFormula(_, _) | HorizontalFormula(_, _))
    }

    fn assign(&mut self, val: NumberValue) {
        match self {
            VerticalFormula(n, _) | HorizontalFormula(n, _) => {
                *n = val;
            }
            cell => {
                *cell = Cell::Num(val);
            }
        }
    }

    /// Find how much the cell must be shifted to the left so it'll align with
    /// other numbers at the exponent marker or the decimal point. Returns 0
    /// for text cells.
    fn left_extension(&self) -> usize {
        let num_part = match self {
            Text(_) => return 0,
            Num(n) | VerticalFormula(n, _) | HorizontalFormula(n, _) => n.as_str(),
        };

        if let Some(pos) = num_part.find('e') {
            pos // Try to align by exponent marker first,
        } else if let Some(pos) = num_part.find('.') {
            pos // then by the decimal point,
        } else {
            // Otherwise use the whole number string length.
            num_part.len()
        }
    }
}

impl Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Text(s) => write!(f, "{s}"),
            Num(n) => write!(f, "{n}"),
            VerticalFormula(n, form) => write!(f, "{n}^{form}"),
            HorizontalFormula(n, form) => write!(f, "{n},{form}"),
        }
    }
}

/// Value for numbers that stores the original string representation.
#[derive(Clone, Debug)]
pub struct NumberValue(f64, String);

impl FromStr for NumberValue {
    type Err = std::num::ParseFloatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = s.parse::<f64>()?;
        Ok(NumberValue(val, s.to_string()))
    }
}

impl NumberValue {
    /// Construct a new NumberValue with scientific notation in
    /// representation.
    pub fn scientific(val: f64) -> Self {
        let s = format!("{val:.2e}");
        let (n, e) = decompose_float(&s);
        NumberValue(val, format!("{n}{e}"))
    }

    /// Construct a new NumberValue with pretty-printed string representation.
    pub fn new(val: f64) -> Self {
        let s = if 0.01 > val.abs() && val.abs() > 1e-14 {
            // Always format small nonzero numbers in sci notation.
            format!("{val:.2e}")
        } else {
            // Otherwise do normal number, but only have max two decimal
            // precision, YAGNI more.
            format!("{val:.2}")
        };
        let (n, e) = decompose_float(&s);
        NumberValue(val, format!("{n}{e}"))
    }

    pub fn is_scientific(&self) -> bool {
        self.1.contains('e')
    }

    /// Construct a special NumberValue that evaluates to 0.0 and prints an
    /// empty string.
    pub fn empty() -> Self {
        // This is formally the default value for the type, but it's not
        // declared as Default implementation because printing an empty string
        // is something that should be explicit.
        NumberValue(0.0, String::new())
    }

    pub fn val(&self) -> f64 {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.1
    }
}

impl Display for NumberValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}

/// Split float into truncated decimal part and exponent part.
///
/// "1.234e5" => "1.234", "e5"
/// "1.200e4" => "1.2", "e4" (strip trailing zeroes from float part)
/// "1.000" => "1", "" (strip trailing dot if all decimals are gone)
fn decompose_float(repr: &str) -> (&str, &str) {
    if let Some(pos) = repr.find('e') {
        let (float_part, exp_part) = repr.split_at(pos);
        let float_part = float_part.trim_end_matches('0').trim_end_matches('.');
        (float_part, exp_part)
    } else {
        let float_part = repr.trim_end_matches('0').trim_end_matches('.');
        (float_part, "")
    }
}

/// If the nonempty lines in input all share the exact same prefix made of
/// spaces and tabs, return that prefix. If nonempty lines have inconsistent
/// indentation, return an error.
fn indent_prefix(text: &str) -> Result<String> {
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
                anyhow::bail!("Inconsistent indentation on line {}", i + 1);
            }
        } else {
            prefix = Some(line_prefix);
        }
    }

    Ok(prefix.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn tf(input: &str, expected: &str) {
        let mut table = Table::new(&input, true).unwrap();
        table.eval(false).unwrap();
        let output = table.to_string();
        assert_eq!(output.trim(), expected.trim());
    }

    #[test]
    fn test_number_value() {
        let n = NumberValue::new(123.456789);
        assert_eq!(n.val(), 123.456789);
        assert_eq!(n.as_str(), "123.46");

        let n2 = NumberValue::new(1.0);
        assert_eq!(n2.val(), 1.0);
        assert_eq!(n2.as_str(), "1");

        let n2 = NumberValue::new(6.674e-11 * 5.972e24 / 6.371e6 / 6.371e6);
        assert_eq!(n2.as_str(), "9.82");

        let n3 = NumberValue::new(0.000123456);
        assert_eq!(n3.val(), 0.000123456);
        assert_eq!(n3.as_str(), "1.23e-4");

        let n3 = NumberValue::new(0.999999);
        assert_eq!(n3.val(), 0.999999);
        assert_eq!(n3.as_str(), "1");
    }

    #[test]
    fn basic_tables() {
        tf(
            "\
1 2 3
4 5 6",
            "\
1  2  3
4  5  6",
        );

        tf(
            "\
a b c d
- 123 - -
e f g h",
            "\
a  b    c  d
-  123  -  -
e  f    g  h",
        );

        // Scientific contagion
        tf("1000000 2000000 ,*", "1000000  2000000  2000000000000,*");

        tf("1e10 2e10 ,*", "1e10  2e10  2e20,*");
    }
}
