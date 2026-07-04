use crate::{Array, parse};
use anyhow::{Result, bail};

#[derive(Default)]
pub struct Vm {
    // TODO: Variable bindings.
    /// Stack extracted from spreadsheet, will not be used for return values.
    input_stack: Vec<Array>,
    /// Stack used for intermediate calculations and the return value.
    work_stack: Vec<Array>,

    // This should maybe refer to the spreadsheet as a whole for operations
    // like column-pulling?
    /// Column on the spreadsheet above the current cell, can be pulled into
    /// stack as an array using a special operation.
    above_column: Vec<Array>,
}

impl Vm {
    pub fn init(&mut self, input_stack: Vec<Array>, above_column: Vec<Array>) {
        self.work_stack.clear();
        self.input_stack = input_stack;
        self.above_column = above_column;
    }

    pub fn eval(&mut self, mut formula: &str) -> Result<Option<Array>> {
        debug_assert!(
            formula.chars().all(|c| !c.is_whitespace()),
            "Formula should not contain whitespace"
        );

        while !formula.is_empty() {
            if let Ok((n, rest)) = parse::positive_float(formula) {
                self.push(n.into());
                formula = rest;
                continue;
            }

            // TODO: All the rest of the stuff.

            let (c, rest) = parse::char(formula)?;
            match c {
                '+' => {
                    self.dyadic_pervasive(|x, y| x + y)?;
                }
                '-' => {
                    self.dyadic_pervasive(|x, y| x - y)?;
                }
                '*' => {
                    self.dyadic_pervasive(|x, y| x * y)?;
                }
                '%' => {
                    self.dyadic_pervasive(|x, y| x / y)?;
                }
                _ => bail!("Unknown token: {}", c),
            }
            formula = rest;
        }

        // Only return values from the work stack. If there's only input stack
        // left, return none. This lets us not print noise values from things like
        // variable assignment.
        if let Some(ret) = self.work_stack.pop() {
            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }

    // TODO: How do I abstract these so I can crank in the argument
    // rearranging?

    pub fn monadic_pervasive(&mut self, f: impl Fn(f64) -> f64) -> Result<()> {
        let a = self.pop()?;
        let ret = Array::new(a.shape().to_vec(), a.iter().map(|&x| f(x)));
        self.push(ret);
        Ok(())
    }

    pub fn dyadic_pervasive(&mut self, f: impl Fn(f64, f64) -> f64) -> Result<()> {
        let b = self.pop()?;
        let a = self.pop()?;

        let Some(shape) = a.result_shape(&b) else {
            bail!(
                "Incompatible array shapes for operation: {:?} and {:?}",
                a.shape(),
                b.shape()
            );
        };

        let ret = Array::new(shape, a.zip(&b).map(|(x, y)| f(x, y)));
        self.push(ret);
        Ok(())
    }

    fn push(&mut self, a: Array) {
        self.work_stack.push(a);
    }

    fn pop(&mut self) -> Result<Array> {
        if let Some(a) = self.work_stack.pop() {
            Ok(a)
        } else if let Some(a) = self.input_stack.pop() {
            Ok(a)
        } else {
            bail!("Stack underflow")
        }
    }

    /// Pop at offset from top of stack. pop_at(0) is equivalent to pop(),
    /// pop_at(1) removes and returns the next highest element, etc.
    fn _pop_at(&mut self, i: usize) -> Result<Array> {
        if i < self.work_stack.len() {
            Ok(self.work_stack.remove(self.work_stack.len() - 1 - i))
        } else if i < self.work_stack.len() + self.input_stack.len() {
            let input_index = i - self.work_stack.len();
            Ok(self
                .input_stack
                .remove(self.input_stack.len() - 1 - input_index))
        } else {
            bail!("Stack underflow")
        }
    }
}
