use std::collections::HashMap;

use crate::{Array, parse};
use anyhow::{Result, bail};

#[derive(Clone, Default)]
pub struct Vm {
    bindings: HashMap<char, Array>,

    /// Stack extracted from spreadsheet, will not be used for return values.
    input_stack: Vec<Array>,
    /// Stack used for intermediate calculations and the return value.
    work_stack: Vec<Array>,

    // This should maybe refer to the spreadsheet as a whole for operations
    // like column-pulling?
    /// Column on the spreadsheet above the current cell, can be pulled into
    /// stack as an array using a special operation.
    above_column: Vec<Array>,

    // XXX: Having the registers in object state is error-prone, should move
    // to some sort of disposable context object instead.
    shift_1: Option<usize>,
    shift_2: Option<usize>,
}

impl Vm {
    pub fn init(&mut self, input_stack: Vec<Array>, above_column: Vec<Array>) {
        self.work_stack.clear();
        self.input_stack = input_stack;
        self.above_column = above_column;
    }

    /// Make a clean copy that shares bindings.
    fn spawn(&self) -> Self {
        let mut ret = self.clone();
        ret.init(Default::default(), Default::default());
        ret
    }

    pub fn run(&mut self, mut formula: &str) -> Result<Option<Array>> {
        debug_assert!(
            formula.chars().all(|c| !c.is_whitespace()),
            "Formula should not contain whitespace"
        );

        while !formula.is_empty() {
            // Reset the shift registers before each top-level operation.
            self.shift_1 = None;
            self.shift_2 = None;
            let (op, rest) = operation(formula)?;
            formula = rest;
            self.eval(op)?;
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

    fn eval(&mut self, op: Operation) -> Result<()> {
        use Operation::*;

        match op {
            Shift1(p1, op) => {
                self.shift_1 = Some(p1 - 1);
                self.shift_2 = None;
                return self.eval(*op);
            }

            Shift2(p1, p2, op) => {
                self.shift_1 = Some(p1 - 1);
                self.shift_2 = Some(p2 - 1);
                return self.eval(*op);
            }

            Number(n) => {
                self.push(n.into());
            }

            Var(c) => {
                if let Some(a) = self.bindings.get(&c) {
                    self.push(a.clone());
                } else {
                    bail!("Undefined variable: '{c}'");
                }
            }

            AssignTo(c) => {
                let a = self.pop_monadic()?;
                self.bindings.insert(c, a);
            }

            // Modifiers
            Fork(op1, op2) => {
                // Compute the second operation first so we can push its
                // result on top of the first one later. Use a cloned VM so we
                // can compute it without disturbing the main stack.

                // XXX: Cloning the full VM is pretty expensive, there are
                // probably cleverer ways to do this.
                let mut scratch = self.clone();
                scratch.eval(*op2)?;
                // XXX: Is it okay to always grab just one output?
                let a = scratch.pop()?;

                self.eval(*op1)?;
                self.push(a);
            }
            Reduce(op) => {
                // Reduce array contents in a temporary VM.
                let mut scratch = self.spawn();
                let a = self.pop_monadic()?;
                if a.is_scalar() {
                    bail!("Cannot reduce a scalar");
                }
                for cell in a.explode() {
                    scratch.push(cell);
                }
                while scratch.work_stack.len() > 1 {
                    let old_len = scratch.work_stack.len();
                    scratch.eval(*op.clone())?;
                    if scratch.work_stack.len() >= old_len {
                        bail!("Reduce operation did not reduce stack size");
                    }
                }
                self.push(scratch.pop()?);
            }

            // Crunch input and work stacks into a single array.
            ImplodeStack => {
                let Some(top) = self.peek() else {
                    self.push(Array::default());
                    return Ok(());
                };
                if !self.stack_values().all(|a| a.shape() == top.shape()) {
                    bail!("Cannot implode stack, values have different shapes.",);
                }
                // Stack values must all have the same shape.
                let ret = Array::from_iter(self.stack_values());
                self.work_stack.clear();
                self.input_stack.clear();
                self.push(ret);
            }

            InsertColumn => {
                let Some(top) = self.above_column.last() else {
                    self.push(Array::default());
                    return Ok(());
                };
                // Above-column values must all have same shape.
                if !self.above_column.iter().all(|a| a.shape() == top.shape()) {
                    bail!("Cannot insert column, values have different shapes.",);
                }
                let ret = Array::from_iter(self.above_column.iter());
                self.push(ret);
            }

            // Functions
            F('+') => {
                self.dyadic_pervasive(|x, y| x + y)?;
            }
            F('-') => {
                self.dyadic_pervasive(|x, y| x - y)?;
            }
            F('*') => {
                self.dyadic_pervasive(|x, y| x * y)?;
            }
            F('%') => {
                self.dyadic_pervasive(|x, y| x / y)?;
            }
            // Array length
            F('#') => {
                let a = self.pop_monadic()?;
                self.push((a.length() as f64).into());
            }
            // First
            F('⊢') => {
                let a = self.pop_monadic()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let cells = a.explode();
                    if let Some(first) = cells.first() {
                        self.push(first.to_owned());
                    } else {
                        bail!("Cannot take first of empty array");
                    }
                }
            }
            // Last
            F('⊣') => {
                let a = self.pop_monadic()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let cells = a.explode();
                    if let Some(first) = cells.last() {
                        self.push(first.to_owned());
                    } else {
                        bail!("Cannot take last of empty array");
                    }
                }
            }

            F(c) => {
                bail!("Unknown function: '{c}'");
            }
        }

        Ok(())
    }

    // TODO: How do I abstract these so I can crank in the argument
    // rearranging?

    pub fn monadic_pervasive(&mut self, f: impl Fn(f64) -> f64) -> Result<()> {
        let a = self.pop_monadic()?;
        let ret = Array::new(a.shape().to_vec(), a.iter().map(|&x| f(x)));
        self.push(ret);
        Ok(())
    }

    pub fn dyadic_pervasive(&mut self, f: impl Fn(f64, f64) -> f64) -> Result<()> {
        let (a, b) = self.pop_dyadic()?;

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

    fn peek(&self) -> Option<&Array> {
        if let Some(a) = self.work_stack.last() {
            Some(a)
        } else if let Some(a) = self.input_stack.last() {
            Some(a)
        } else {
            None
        }
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

    /// Pop arguments for monadic op using shift value if available.
    fn pop_monadic(&mut self) -> Result<Array> {
        if self.shift_2.is_some() {
            bail!("Monadic operation cannot use two shift values");
        }
        if let Some(n) = self.shift_1 {
            self.pop_at(n)
        } else {
            self.pop()
        }
    }

    /// Pop arguments for dyadic op using shift value if available.
    fn pop_dyadic(&mut self) -> Result<(Array, Array)> {
        // This gets tricky, both values refer to the stack before any pops,
        // and might even refer to the same value.
        let p1 = self.shift_1.unwrap_or(1);
        let p2 = self.shift_2.unwrap_or(0);

        let a = self.stack_nth(p1)?.clone();
        let b = self.stack_nth(p2)?.clone();

        // Pop the higher first so we don't invalidate the lower index.
        self.pop_at(p1.max(p2))?;
        // Check that they are actually two different values, if they're the
        // same we only need to remove it once.
        if p1 != p2 {
            self.pop_at(p1.min(p2))?;
        }

        Ok((a, b))
    }

    fn stack_nth(&self, i: usize) -> Result<&Array> {
        if i < self.work_stack.len() {
            Ok(&self.work_stack[self.work_stack.len() - 1 - i])
        } else if i < self.work_stack.len() + self.input_stack.len() {
            let input_index = i - self.work_stack.len();
            Ok(&self.input_stack[self.input_stack.len() - 1 - input_index])
        } else {
            bail!("Stack underflow")
        }
    }

    /// Pop at offset from top of stack. pop_at(0) is equivalent to pop(),
    /// pop_at(1) removes and returns the next highest element, etc.
    fn pop_at(&mut self, i: usize) -> Result<Array> {
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

    fn stack_values(&self) -> impl Iterator<Item = &Array> + '_ {
        self.input_stack.iter().chain(self.work_stack.iter())
    }
}

#[derive(Clone, Debug)]
enum Operation {
    /// Call a function.
    F(char),
    /// Refer a variable,
    Var(char),
    /// Push a number to stack.
    Number(f64),
    /// Assign to variable
    AssignTo(char),
    /// Reduce array with inner operation.
    Reduce(Box<Operation>),
    /// Execute two operations with the same inputs.
    Fork(Box<Operation>, Box<Operation>),
    /// Turn stack into array
    ImplodeStack,
    /// Insert column from above into stack.
    InsertColumn,
    /// Shift first (deeper in the stack) argument of operation.
    Shift1(usize, Box<Operation>),
    /// Shift both arguments of operation.
    Shift2(usize, usize, Box<Operation>),
}

fn operation(s: &str) -> Result<(Operation, &str)> {
    use Operation::*;

    let (inner, rest) = inner_operation(s)?;

    // See if we have indexing digits after the operation.
    if let Ok((p1, rest)) = parse::subscript_digit(rest) {
        if let Ok((p2, rest)) = parse::subscript_digit(rest) {
            Ok((Shift2(p1, p2, Box::new(inner)), rest))
        } else {
            Ok((Shift1(p1, Box::new(inner)), rest))
        }
    } else {
        Ok((inner, rest))
    }
}

/// Parse the next operation from input, simple ones are usually one
/// character, modifiers create multi-char operations.
fn inner_operation(s: &str) -> Result<(Operation, &str)> {
    use Operation::*;

    let Some(c) = s.chars().next() else {
        bail!("Empty input");
    };

    let rest = &s[c.len_utf8()..];

    // Commas can serve as separators within a formula.
    if c.is_whitespace() || c == ',' {
        return operation(rest);
    }

    if let Ok((n, rest)) = parse::positive_float(s) {
        return Ok((Number(n), rest));
    }

    // Rewrite the above as a match statement:
    match c {
        c if c.is_ascii_alphabetic() => Ok((Var(c), rest)),
        '→' => {
            let Ok((Var(c), rest)) = operation(rest) else {
                bail!("Expected variable after assignment operator");
            };
            Ok((AssignTo(c), rest))
        }
        '/' => {
            let (op, rest) = operation(rest)?;
            Ok((Reduce(Box::new(op)), rest))
        }
        '⊃' => {
            let (op1, rest) = operation(rest)?;
            let (op2, rest) = operation(rest)?;
            Ok((Fork(Box::new(op1), Box::new(op2)), rest))
        }
        ']' => Ok((ImplodeStack, rest)),
        '⇓' => Ok((InsertColumn, rest)),
        // Anything we don't know and can't rule off with blanket rules is assumed
        // to be a function call, the intepreter can figure it out.
        _ => Ok((F(c), rest)),
    }
}
