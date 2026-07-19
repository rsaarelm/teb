use anyhow::Result;

use crate::{Array, Cell, Table, Vm};

#[derive(Clone, Debug, Default)]
pub struct Spreadsheet {
    cells: Vec<Vec<DataCell>>,
}

impl Spreadsheet {
    /// Apply calculated values in output cells back to the table.
    pub fn apply(&self, table: &mut Table) {
        for (i, j) in self.posns() {
            // TODO: Proper support for "which input cells did we look
            // at" to propagate scientific notation. Needs a more
            // complex value struct at spreadsheet level that can
            // track input cells.
            match &self.cells[i][j] {
                DataCell::Output(Some(value), _) => table.assign(i, j, value),
                DataCell::Output(None, _) => table.clear_output(i, j),
                _ => {}
            }
            if let DataCell::Output(Some(value), _) = &self.cells[i][j] {
                table.assign(i, j, value);
            }
        }
    }

    /// Evaluate spreadsheet formulas left-to-right and top-to-bottom.
    pub fn eval(&mut self, vm: &mut Vm) -> Result<()> {
        for (i, j) in self.posns().collect::<Vec<_>>() {
            // XXX: Awkward borrow checker dance, should rethink Cell type...
            let Some(formula) = (match &self.cells[i][j] {
                DataCell::Output(_, formula) => Some(formula.clone()),
                _ => None,
            }) else {
                continue;
            };
            let value = self.eval_at(vm, i, j, &formula)?;
            self.cells[i][j].set_output(value);
        }
        Ok(())
    }

    fn eval_at(&self, vm: &mut Vm, i: usize, j: usize, formula: &str) -> Result<Option<Array>> {
        let cursor = Cursor {
            row: i,
            col: j,
            sheet: self,
        };
        vm.run(&cursor, formula)
    }

    fn posns(&self) -> impl Iterator<Item = (usize, usize)> {
        debug_assert!(
            self.cells
                .iter()
                .all(|row| row.len() == self.cells[0].len()),
            "All rows must be equal length"
        );

        let height = self.cells.len();
        let width = self.cells.first().map(|row| row.len()).unwrap_or(0);

        (0..height).flat_map(move |i| (0..width).map(move |j| (i, j)))
    }
}

impl From<&Table> for Spreadsheet {
    fn from(table: &Table) -> Self {
        let width = table.data_width();
        let mut cells = Vec::new();

        for row in table.rows() {
            let row = row
                .iter()
                .take(width)
                .map(DataCell::from)
                .collect::<Vec<_>>();
            cells.push(row);
        }

        Spreadsheet { cells }
    }
}

#[derive(Clone, Debug, Default)]
enum DataCell {
    #[default]
    Empty,
    Input(Array),
    // Value and formula.
    Output(Option<Array>, String),
}

impl From<&Cell> for DataCell {
    fn from(cell: &Cell) -> Self {
        use Cell::*;
        match cell {
            Text(_) => DataCell::Empty,
            Input(value) => DataCell::Input(value.as_f64().into()),
            Output { formula, .. } => DataCell::Output(None, formula.clone()),
        }
    }
}

impl DataCell {
    fn set_output(&mut self, value: Option<Array>) {
        // XXX: Inefficient cloning of formula, should shift to struct-style
        // enum.
        if let DataCell::Output(_, formula) = self {
            *self = DataCell::Output(value, formula.clone());
        }
    }

    fn value(&self) -> Option<&Array> {
        match self {
            DataCell::Input(value) => Some(value),
            DataCell::Output(Some(value), _) => Some(value),
            _ => None,
        }
    }
}

/// Access object for spreadsheet.
pub struct Cursor<'a> {
    row: usize,
    col: usize,
    sheet: &'a Spreadsheet,
}

impl<'a> Cursor<'a> {
    pub fn column_above(&self, offset: usize) -> Vec<Array> {
        // Return array of values above curret pos, offset to the left by
        // `offset`.

        if self.col < offset {
            return Vec::new();
        }

        let col = self.col - offset;
        self.sheet
            .cells
            .iter()
            .take(self.row)
            .filter_map(|row| row.get(col).and_then(DataCell::value))
            .cloned()
            .collect()
    }

    pub fn row_left(&self) -> Vec<Array> {
        self.sheet.cells[self.row]
            .iter()
            .take(self.col)
            .filter_map(DataCell::value)
            .cloned()
            .collect()
    }
}
