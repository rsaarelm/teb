use std::{
    collections::{BTreeSet, HashMap},
    f64,
};

use crate::{Array, Cursor, parse};
use anyhow::{Result, bail};

#[derive(Clone, Default)]
pub struct Vm {
    bindings: HashMap<char, Array>,

    /// Stack extracted from spreadsheet, will not be used for return values.
    input_stack: Vec<Array>,
    /// Stack used for intermediate calculations and the return value.
    work_stack: Vec<Array>,
}

impl Vm {
    /// Make a clean copy that shares bindings.
    fn spawn(&self) -> Self {
        let mut ret = self.clone();
        ret.input_stack.clear();
        ret.work_stack.clear();
        ret
    }

    pub fn run(&mut self, cur: &Cursor, mut formula: &str) -> Result<Option<Array>> {
        debug_assert!(
            formula.chars().all(|c| !c.is_whitespace()),
            "Formula should not contain whitespace"
        );

        self.work_stack.clear();
        self.input_stack = cur.row_left();

        while !formula.is_empty() {
            let (op, rest) = operation(formula)?;
            formula = rest;
            self.eval(cur, op)?;
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

    fn eval(&mut self, cur: &Cursor, op: Operation) -> Result<()> {
        use Operation::*;

        match op {
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
                let a = self.pop()?;
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
                scratch.eval(cur, *op2)?;
                // XXX: Is it okay to always grab just one output?
                let a = scratch.pop()?;

                self.eval(cur, *op1)?;
                self.push(a);
            }
            Reduce(op) => {
                // Reduce array contents in a temporary VM.
                let mut scratch = self.spawn();
                let a = self.pop()?;
                if a.is_scalar() {
                    bail!("Cannot reduce a scalar");
                }
                for cell in a.explode() {
                    scratch.push(cell);
                }
                while scratch.work_stack.len() > 1 {
                    let old_len = scratch.work_stack.len();
                    scratch.eval(cur, *op.clone())?;
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

            InsertColumn(offset) => {
                let column = cur.column_above(offset);

                let Some(top) = column.last() else {
                    self.push(Array::default());
                    return Ok(());
                };
                // Above-column values must all have same shape.
                if !column.iter().all(|a| a.shape() == top.shape()) {
                    bail!("Cannot insert column, values have different shapes.",);
                }
                let ret = Array::from_iter(column.iter());
                self.push(ret);
            }

            Rearrange(indices) => {
                let mut new_stack = Vec::new();
                let mut pops = BTreeSet::new();
                for i in indices {
                    new_stack.push(self.stack_nth(i)?.clone());
                    pops.insert(i);
                }
                // Pop from largest to smallest so we don't mess up the stack
                // order.
                for i in pops.into_iter().rev() {
                    self.pop_at(i)?;
                }
                // Push the new stuff in.
                for a in new_stack {
                    self.push(a);
                }
            }

            Exp(base) => {
                self.monadic_pervasive(|x| base.powf(x))?;
            }

            Log(base) => {
                if base <= 0.0 || base == 1.0 {
                    bail!("Logarithm base must be positive and not equal to 1");
                }
                self.monadic_pervasive(|x| x.log(base))?;
            }

            // Functions
            F('+') => {
                self.dyadic_pervasive(|x, y| x + y)?;
            }
            F('-') => {
                self.dyadic_pervasive(|x, y| x - y)?;
            }
            F('×') => {
                self.dyadic_pervasive(|x, y| x * y)?;
            }
            F('÷') => {
                self.dyadic_pervasive(|x, y| x / y)?;
            }
            // Negate
            F('¯') => {
                self.monadic_pervasive(|x| -x)?;
            }
            F('²') => {
                self.monadic_pervasive(|x| x * x)?;
            }
            F('√') => {
                self.monadic_pervasive(|x| x.sqrt())?;
            }
            // a bⁿ, raise a to b:th power
            F('ⁿ') => {
                self.dyadic_pervasive(|x, y| x.powf(y))?;
            }
            // Reciprocal
            F('⨪') => self.monadic_pervasive(|x| 1.0 / x)?,
            F('⌊') => {
                self.monadic_pervasive(|x| x.floor())?;
            }
            F('⁅') => {
                self.monadic_pervasive(|x| x.round())?;
            }
            F('⌈') => {
                self.monadic_pervasive(|x| x.ceil())?;
            }
            // Array length
            F('⧻') => {
                let a = self.pop()?;
                self.push((a.length() as f64).into());
            }
            // Identity
            F('∘') => {
                // NB. This isn't equivalent to doing nothing since the
                // pop-push might be moving the value from the input stack
                // (not shown as formula result) to the work stack (shown as
                // result).
                let a = self.pop()?;
                self.push(a);
            }
            // First
            F('⊢') => {
                let a = self.pop()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let cells = a.explode();
                    if let Some(first) = cells.first() {
                        self.push(first.to_owned());
                    } else {
                        bail!("first: Cannot take first of empty array");
                    }
                }
            }
            // Last
            F('⊣') => {
                let a = self.pop()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let cells = a.explode();
                    if let Some(first) = cells.last() {
                        self.push(first.to_owned());
                    } else {
                        bail!("last: Cannot take last of empty array");
                    }
                }
            }
            // Reverse array
            F('⇌') => {
                let a = self.pop()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let mut cells = a.explode();
                    cells.reverse();
                    self.push(cells.iter().collect());
                }
            }

            // Take n rows of array
            F('↙') => {
                let n = self.pop()?;
                let a = self.pop()?;

                let Some(n) = n.as_scalar().map(|n| n as i32) else {
                    // It can probably be given semantics for array args
                    // too...
                    bail!("take: Operation requires a scalar argument");
                };
                if n.abs() as usize > a.length() {
                    bail!("take: Not enough elements");
                }
                if a.is_scalar() {
                    bail!("take: Cannot take rows of a scalar");
                }

                let rows = a.explode();

                let a = if n < 0 {
                    // take last abs(n) rows as one array
                    Array::from_iter(rows.iter().skip(rows.len() - n.abs() as usize))
                } else {
                    // take first n rows as one array
                    Array::from_iter(rows.iter().take(n as usize))
                };
                self.push(a);
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
        let a = self.pop()?;
        let ret = Array::new(a.shape().to_vec(), a.iter().map(|&x| f(x)));
        self.push(ret);
        Ok(())
    }

    pub fn dyadic_pervasive(&mut self, f: impl Fn(f64, f64) -> f64) -> Result<()> {
        let (b, a) = (self.pop()?, self.pop()?);

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

    /// Exponential with base, raise base to value at top of stack.
    Exp(f64),

    /// Take logarithm with base from top of stack.
    Log(f64),

    /// Assign to variable
    AssignTo(char),
    /// Reduce array with inner operation.
    Reduce(Box<Operation>),
    /// Execute two operations with the same inputs.
    Fork(Box<Operation>, Box<Operation>),
    /// Turn stack into array
    ImplodeStack,
    /// Insert column from above into stack. You can displace to the left by
    /// offset.
    InsertColumn(usize),
    /// Pop named stack indices and push them in in the given pattern.
    Rearrange(Vec<usize>),
}

impl Operation {
    pub fn invert(&self) -> Result<Operation> {
        // Inversion is sort of hairy, add cases as we go.
        use Operation::*;
        match self {
            F('∘') => Ok(F('∘')),

            F('²') => Ok(F('√')),
            F('√') => Ok(F('²')),

            Log(base) => Ok(Exp(*base)),
            Exp(base) => Ok(Log(*base)),
            _ => bail!("Cannot invert operation: '{self:?}'"),
        }
    }
}

/// Parse the next operation from input, simple ones are usually one
/// character, modifiers create multi-char operations.
fn operation(s: &str) -> Result<(Operation, &str)> {
    use Operation::*;

    let Some(c) = s.chars().next() else {
        bail!("Empty input");
    };

    let rest = &s[c.len_utf8()..];

    // Commas can serve as separators within a formula.
    if c.is_whitespace() || c == ',' {
        return operation(rest);
    }

    if c.is_ascii_digit() {
        if let Ok((n, rest)) = parse::positive_float(s) {
            return Ok((Number(n), rest));
        }
    }

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
        '⇓' => {
            if let Ok((subscript, rest)) = parse::subscript_number(rest) {
                Ok((InsertColumn(subscript as usize), rest))
            } else {
                Ok((InsertColumn(0), rest))
            }
        }
        '.' => {
            // Rearrange operation, read subsequent subscripts as stack
            // indices.
            let mut indices = Vec::new();
            let mut rest = rest;
            while let Ok((n, r)) = parse::subscript_digit(rest) {
                if n == 0 {
                    bail!("Stack indices are 1-based, cannot use 0");
                }
                indices.push(n - 1);
                rest = r;
            }
            if indices.is_empty() {
                // Default behavior is to duplicate the top item.
                indices.push(0);
                indices.push(0);
            }
            Ok((Rearrange(indices), rest))
        }
        // Inverse modifier.
        '°' => {
            let (op, rest) = operation(rest)?;
            Ok((op.invert()?, rest))
        }

        'ₑ' => {
            if let Ok((base, rest)) = parse::subscript_number(rest) {
                Ok((Exp(base as f64), rest))
            } else {
                Ok((Exp(f64::consts::E), rest))
            }
        }

        // Anything we don't know and can't rule off with blanket rules is assumed
        // to be a function call, the intepreter can figure it out.
        _ => Ok((F(c), rest)),
    }
}

// Keep this alphabetically sorted.
static ALIASES: &[(&str, &str)] = &[
    ("add", "+"),
    ("ceiling", "⌈"),
    ("divide", "÷"),
    ("exponential", "ₑ"),
    ("first", "⊢"),
    ("floor", "⌊"),
    ("flor", "⌊"), // floor
    ("flr", "⌊"),  // floor
    ("fork", "⊃"),
    ("fst", "⊢"), // first
    ("id", "∘"),  // identity
    ("identity", "∘"),
    ("implode", "]"),
    ("last", "⊣"),
    ("length", "⧻"),
    ("lst", "⊣"), // last
    ("multiply", "×"),
    ("negate", "¯"),
    ("power", "ⁿ"),
    ("pull", "⇓"),
    ("reciprocal", "⨪"),
    ("reduce", "/"),
    ("round", "⁅"),
    ("sqrt", "√"),
    ("subtract", "-"),
    ("un", "°"),
];

/// Reformat ASCII notation into canonical unicode symbols in formula code.
pub fn prettify_formula(s: &str) -> String {
    let mut ret = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        let (part, r) = reformat_part(rest);
        ret.push_str(&part);
        rest = r;
    }
    ret
}

fn reformat_part(s: &str) -> (String, &str) {
    // If we see text, try to decipher it into a command sequence.
    if let Ok((word, rest)) = parse::word(s) {
        if let Ok(syms) = parse::decipher(ALIASES, word) {
            return (syms.join(""), rest);
        } else {
            // It didn't resolve into functions, assume it's a variable name
            // or something and return it as-is.
            return (word.to_string(), rest);
        }
    }

    // ASCII subscript to unicode subscript.
    if let Ok((subscript, rest)) = parse::ascii_subscript(s) {
        return (subscript, rest);
    }

    // Some hardcoded substitutions.
    for (from, to) in [
        // ASCII primes to unicode, make sure to match the longest string
        // first.
        ("'''", "‴"),
        ("''", "″"),
        ("'", "′"),
        // Multiplication and division ops.
        ("*", "×"),
        ("%", "÷"),
        ("::", "→"),
    ] {
        if let Ok(rest) = parse::literal(s, from) {
            return (to.to_string(), rest);
        }
    }

    parse::char(s)
        .map(|(c, rest)| (c.to_string(), rest))
        .expect("reformat_part: Input is empty")
}
