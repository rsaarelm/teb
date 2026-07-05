use anyhow::Result;

use crate::{Array, Table, Vm};

#[derive(Clone, Debug, Default)]
pub struct Spreadsheet {
    cells: Vec<Vec<Cell>>,
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
                Cell::Output(Some(value), _) => table.assign(i, j, value),
                Cell::Output(None, _) => table.clear_output(i, j),
                _ => {}
            }
            if let Cell::Output(Some(value), _) = &self.cells[i][j] {
                table.assign(i, j, value);
            }
        }
    }

    /// Evaluate spreadsheet formulas left-to-right and top-to-bottom.
    pub fn eval(&mut self, vm: &mut Vm) -> Result<()> {
        for (i, j) in self.posns().collect::<Vec<_>>() {
            // XXX: Awkward borrow checker dance, should rethink Cell type...
            let Some(formula) = (match &self.cells[i][j] {
                Cell::Output(_, formula) => Some(formula.clone()),
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
        let left_vals = self.cells[i]
            .iter()
            .filter_map(Cell::value)
            .cloned()
            .collect::<Vec<_>>();
        let top_vals = self
            .cells
            .iter()
            .take(i)
            .filter_map(|row| row.get(j).and_then(Cell::value))
            .cloned()
            .collect::<Vec<_>>();

        vm.init(left_vals, top_vals);
        vm.run(formula)
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
                .map(|cell| {
                    if let Some(input) = cell.input() {
                        Cell::Input(input.into())
                    } else if let Some(formula) = cell.formula() {
                        Cell::Output(Default::default(), formula.to_owned())
                    } else {
                        Cell::Empty
                    }
                })
                .collect::<Vec<_>>();
            cells.push(row);
        }

        // For output cells with empty formulas, copy the last non-empty
        // formula from the same column above.
        for j in 0..width {
            let mut last_formula = None;
            for i in 0..cells.len() {
                if let Cell::Output(_, formula) = &cells[i][j] {
                    if !formula.is_empty() {
                        last_formula = Some(formula.clone());
                    } else if let Some(last_formula) = &last_formula {
                        cells[i][j] = Cell::Output(Default::default(), last_formula.clone());
                    }
                }
            }
        }

        Spreadsheet { cells }
    }
}

#[derive(Clone, Debug, Default)]
enum Cell {
    #[default]
    Empty,
    Input(Array),
    // Value and formula.
    Output(Option<Array>, String),
}

impl Cell {
    fn set_output(&mut self, value: Option<Array>) {
        // XXX: Inefficient cloning of formula, should shift to struct-style
        // enum.
        if let Cell::Output(_, formula) = self {
            *self = Cell::Output(value, formula.clone());
        }
    }

    fn value(&self) -> Option<&Array> {
        match self {
            Cell::Input(value) => Some(value),
            Cell::Output(Some(value), _) => Some(value),
            _ => None,
        }
    }
}
