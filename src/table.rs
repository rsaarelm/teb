use std::{fmt::Display, iter};

use anyhow::{Result, bail};
use itertools::Itertools;

use crate::{Array, Cell, Spreadsheet, Vm, parse};

// This is the textual table representation, see Spreadsheet for the semantic
// representation.

/// A textual table of whitespace-separated cells.
#[derive(Clone, Default, Debug)]
pub struct Table {
    cells: Vec<Vec<Cell>>,
    indent_prefix: String,
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
                        line[a..b].trim_end().parse()?
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

        // Propagate formulas and the accompanying output formats down the
        // column to empty output cells.

        // Propagate output formats down the columns to empty output cells.
        let rows = table.cells.len();
        for col in 0..columns {
            let mut last_formula_cell = None;
            for row in 0..rows {
                if table.cells[row][col].has_formula() {
                    last_formula_cell = Some(table.cells[row][col].clone());
                } else if let Some(last_formula_cell) = last_formula_cell.as_ref() {
                    table.cells[row][col].inherit_from(last_formula_cell)
                }
            }
        }

        Ok(table)
    }

    pub fn column(&self, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells.iter().filter_map(move |row| row.get(col))
    }

    pub fn to_the_left(&self, row: usize, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells[row].iter().take(col)
    }

    pub fn above(&self, row: usize, col: usize) -> impl Iterator<Item = &Cell> + '_ {
        self.cells.iter().take(row).filter_map(move |r| r.get(col))
    }

    pub fn eval(&mut self, vm: &mut Vm) -> Result<()> {
        let mut sheet = Spreadsheet::from(self as &Table);
        sheet.eval(vm)?;
        sheet.apply(self);
        Ok(())
    }

    /// Width of the data-bearing columns. Ignores the potential last right
    /// column which can only contain text data.
    pub fn data_width(&self) -> usize {
        self.cells.iter().map(|row| row.len()).min().unwrap_or(0)
    }

    pub fn rows(&self) -> impl Iterator<Item = &[Cell]> + '_ {
        self.cells.iter().map(|row| row.as_slice())
    }

    pub fn clear_output(&mut self, row: usize, col: usize) {
        self.cells[row][col].set_output_text("");
    }

    /// Assign an output value to a cell.
    pub fn assign(&mut self, row: usize, col: usize, value: &Array) {
        if let Some(num) = value.as_scalar() {
            self.cells[row][col].assign_output(num);
        } else {
            self.cells[row][col].set_output_text("▯");
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
                    .filter_map(|c| c.left_extension().map(|e| e.chars().count()))
                    .max()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();

        // Total width for each column
        let column_widths = (0..columns)
            .map(|i| {
                self.column(i)
                    .map(|c| c.column_indent(left_extents[i]) + c.chars().count())
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

                let right_pad = column_widths[i] - indent - c.chars().count();

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
