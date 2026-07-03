use std::{fmt::Display, iter, ops::Deref, str::FromStr};

use anyhow::{Result, bail};
use itertools::Itertools;

use crate::{Array, parse};

const FLOAT_PRECISION: usize = 2;

// This is the textual table representation, see Spreadsheet for the semantic
// representation.

/// A textual table of whitespace-separated cells.
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
        for line in input.lines() {
            // Tables must be made of contiguous non-empty lines. Empty lines
            // separate tables and should have been filtered out earlier.
            if line.trim().is_empty() {
                bail!("Empty line in table input");
            }

            // Count whitespace-separated words on table lines.
            let line_columns = line.split_whitespace().count();
            assert!(line_columns > 0);
            columns = columns.min(line_columns);
        }

        if columns == usize::MAX {
            // No content seen, return the empty table.
            return Ok(table);
        }

        table.indent_prefix = parse::indent_prefix(input)?;

        for line in input.lines() {
            let line = line.trim();

            // Offsets where words start in the line.
            //
            // Not using split_whitespace here because we want to preserve
            // whatever spacing the final trailing sections of the table lines
            // use between their words.
            let word_starts: Vec<usize> = line
                .char_indices()
                .zip(iter::once(' ').chain(line.chars()))
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

    fn column(&self, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells.iter().filter_map(move |row| row.get(col))
    }

    fn to_the_left(&self, row: usize, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells[row].iter().take(col)
    }

    fn above(&self, row: usize, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells.iter().take(row).filter_map(move |r| r.get(col))
    }

    /// Assign an output value to a cell, doing fancy smart formatting.
    fn assign(&mut self, row: usize, col: usize, looked_at_column: bool, value: &Array) {
        // Infect this cell with scientific notation if we see potential input
        // cells using it.
        let mut use_scientific = self
            .to_the_left(row, col)
            .any(|c| c.uses_scientific_notation());
        if looked_at_column {
            use_scientific |= self.above(row, col).any(|c| c.uses_scientific_notation());
        }

        let cell = &mut self.cells[row][col];

        let Some(num) = value.as_scalar() else {
            // Non-printable value, set the marker and continue.
            cell.set_output("▯");
            return;
        };

        if use_scientific {
            cell.set_output(format!("{num:.p$e}", p = FLOAT_PRECISION));
            return;
        }

        let abs = num.abs();
        // Figure out the precision, with precision 2 we want 1.234 -> "1.23"
        // but 0.000234 -> "0.00023".
        if abs < 1.0 && num != 0.0 {
            let leading_zeros = (-abs.log10().floor() as isize - 1).max(0) as usize;
            cell.set_output(format!("{num:.p$}", p = leading_zeros + FLOAT_PRECISION));
            return;
        }

        let scale = 10f64.powi(FLOAT_PRECISION as i32);
        let rounded = (abs * scale).round();
        let all_decimal_digits_are_zero = rounded.rem_euclid(scale) == 0.0;

        if all_decimal_digits_are_zero {
            cell.set_output(format!("{}", num.trunc()));
        } else {
            cell.set_output(format!("{num:.p$}", p = FLOAT_PRECISION));
        }
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

#[derive(Clone, Debug)]
pub struct Cell {
    /// Always contains complete text of the cell. Must not be empty. Must not
    /// contain whitespace.
    text: String,
    /// Input data from a numeric non-formula cell.
    ///
    /// Formula cells produce output, so input values are not used for them.
    input: Option<f64>,
    /// Spreadsheet formula string, if applicable.
    formula: Option<String>,
}

impl Cell {
    /// Force a text cell, even if the string looks like a number or formula.
    pub fn text(s: impl Into<String>) -> Self {
        let text = s.into();
        assert!(!text.is_empty(), "Cell: Text is empty");
        assert!(
            !text.contains(char::is_whitespace),
            "Cell: Text contains whitespace"
        );

        Cell {
            text,
            input: None,
            formula: None,
        }
    }

    fn is_numeric(&self) -> bool {
        self.input.is_some() || self.formula.is_some()
    }

    /// Return part of the cell string that represents an input or output
    /// number. Empty string if the cell does not contain an acknowledged
    /// numeric value.
    fn number_part(&self) -> &str {
        if let Some(formula) = &self.formula {
            let len = formula.len() + 1; // +1 for the separator comma
            &self.text[..self.text.len() - len]
        } else if self.input.is_some() {
            &self.text
        } else {
            ""
        }
    }

    fn non_formula_part(&self) -> &str {
        if let Some(formula) = &self.formula {
            let len = formula.len() + 1; // +1 for the separator comma
            &self.text[..self.text.len() - len]
        } else {
            &self.text
        }
    }

    /// How much should this cell be indented when printed to a column with
    /// the given maximum left extent.
    fn column_indent(&self, max_left_extent: usize) -> usize {
        if !self.is_numeric() {
            return 0;
        }
        let left_extent = self.left_extension();
        assert!(left_extent <= max_left_extent);
        max_left_extent - left_extent
    }

    pub fn uses_scientific_notation(&self) -> bool {
        let num = self.number_part();
        num.contains('e') || num.contains('E')
    }

    pub fn is_formula_cell(&self) -> bool {
        self.formula.is_some()
    }

    /// Find how much the cell must be shifted to the left so it'll align with
    /// other numbers at the exponent marker or the decimal point. Returns 0
    /// for text cells.
    fn left_extension(&self) -> usize {
        let num = self.number_part();

        if let Some(pos) = num.find(|c| c == 'e' || c == 'E') {
            pos // First try to align by the part before an exponent marker,
        } else if let Some(pos) = num.find('.') {
            pos // then by the part before a decimal point,
        } else {
            // otherwise use the whole number string length.
            num.len()
        }
    }

    pub fn set_output(&mut self, value: impl ToString) {
        let s = value.to_string();
        if s.contains(char::is_whitespace) {
            panic!("Trying to assign a value with whitespace to a cell.");
        }
        let Some(f) = &self.formula else {
            panic!("Trying to set output to a non-formula cell.");
        };
        self.text = format!("{s},{f}");
    }
}

impl FromStr for Cell {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let text = s.trim().to_string();
        if text.is_empty() {
            bail!("Cell text can't be empty.");
        }

        // Only a number.
        if let Ok(num) = text.parse::<f64>() {
            let input = Some(num);
            return Ok(Cell {
                text,
                input,
                formula: None,
            });
        }

        // Formula with optional number value before it.
        if let Some((val, form)) = text.split_once(',') {
            if !val.is_empty() {
                // Valid prefixes for a formula are a parseable float or the
                // marker for a non-printable array value.
                if val != "▯" && val.parse::<f64>().is_err() {
                    // A prefix exists but we can't parse it. Assume this was
                    // actually a misidentified text cell instead of a formula
                    // cell and return a text cell result.
                    return Ok(Cell {
                        text,
                        input: None,
                        formula: None,
                    });
                };
            }

            let formula = Some(form.to_string());

            return Ok(Cell {
                text,
                input: None,
                formula,
            });
        }

        // Just a text cell.
        Ok(Cell {
            text,
            input: None,
            formula: None,
        })
    }
}

impl Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl Default for Cell {
    fn default() -> Self {
        // Cell text can't be empty so Default needs to use a placeholder
        // value.
        Cell::text("-")
    }
}

impl Deref for Cell {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}
