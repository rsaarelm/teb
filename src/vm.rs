use std::{
    collections::{BTreeSet, HashMap},
    f64,
    str::FromStr,
};

use crate::{Array, Cursor, parse, util};
use anyhow::{Result, anyhow, bail};

#[derive(Clone, Default)]
pub struct Vm {
    bindings: HashMap<String, Formula>,

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

    pub fn run(&mut self, cur: &Cursor, formula: &str) -> Result<Option<Array>> {
        debug_assert!(
            formula.chars().all(|c| !c.is_whitespace()),
            "Formula should not contain whitespace"
        );

        self.work_stack.clear();
        self.input_stack = cur.row_left();

        let formula: Formula = formula.parse()?;
        self.eval(cur, &formula)?;

        // Only return values from the work stack. If there's only input stack
        // left, return none. This lets us not print noise values from things like
        // variable assignment.
        if let Some(ret) = self.work_stack.pop() {
            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }

    fn eval(&mut self, cur: &Cursor, formula: &Formula) -> Result<()> {
        for elt in formula {
            self.step(cur, elt)?;
        }
        Ok(())
    }

    fn step(&mut self, cur: &Cursor, elt: &Element) -> Result<()> {
        use Element::*;
        match elt {
            Num(n) => {
                self.push((*n).into());
            }
            Arr(a) => {
                self.push(a.clone());
            }
            Var(s) => {
                if let Some(fun) = self.bindings.get(s).cloned() {
                    self.eval(cur, &fun)?;
                } else {
                    bail!("Undefined variable: '{s}'");
                }
            }
            Assign(name) => {
                let a = self.pop()?;
                self.bindings.insert(name.clone(), Formula(vec![Arr(a)]));
            }
            Define(name, formula) => {
                self.bindings.insert(name.clone(), formula.clone());
            }
            Fork(f1, f2) => {
                // Compute the second operation first so we can push its
                // result on top of the first one later. Use a cloned VM so we
                // can compute it without disturbing the main stack.

                // XXX: Cloning the full VM is pretty expensive, there are
                // probably cleverer ways to do this.
                let mut scratch = self.clone();
                scratch.eval(cur, f2)?;
                // TODO: Figure out how many outputs the operation produces
                // and grab them.
                let a = scratch.pop()?;

                self.eval(cur, f1)?;
                self.push(a);
            }
            Dip(f) => {
                let a = self.pop()?;
                self.eval(cur, f)?;
                self.push(a);
            }
            Implode => {
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
            Pull(offset) => {
                let column = cur.column_above(*offset);

                let Some(top) = column.last() else {
                    self.push(Array::default());
                    return Ok(());
                };
                // Above-column values must all have same shape.
                if !column.iter().all(|a| a.shape() == top.shape()) {
                    bail!("Cannot pull column, values have different shapes.",);
                }
                let ret = Array::from_iter(column.iter());
                self.push(ret);
            }
            Rearrange(indices) => {
                let mut new_stack = Vec::new();
                let mut pops = BTreeSet::new();
                for i in indices {
                    new_stack.push(self.stack_nth(*i)?.clone());
                    pops.insert(*i);
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

            Reduce(f) => {
                // Reduce array contents in a temporary VM.
                let mut scratch = self.spawn();
                let a = self.pop()?;
                if a.is_scalar() {
                    bail!("Cannot reduce a scalar");
                }
                let n = a.length();
                for cell in a.explode() {
                    scratch.push(cell);
                }
                for _ in 0..(n - 1) {
                    scratch.eval(cur, f)?;
                }
                self.push(scratch.pop()?);
            }

            Un(f) => {
                let f = f.inverted()?;
                self.eval(cur, &f)?;
            }

            Logarithm(base) => {
                if *base <= 0.0 || *base == 1.0 {
                    bail!("Logarithm base must be positive and not equal to 1");
                }
                self.monadic_pervasive(|x| x.log(*base))?;
            }
            Exponential(base) => self.monadic_pervasive(|x| base.powf(x))?,
            Add => self.dyadic_pervasive(|x, y| x + y)?,
            Subtract => self.dyadic_pervasive(|x, y| x - y)?,
            Multiply => self.dyadic_pervasive(|x, y| x * y)?,
            Divide => self.dyadic_pervasive(|x, y| x / y)?,
            Negate => self.monadic_pervasive(|x| -x)?,
            Square => self.monadic_pervasive(|x| x * x)?,
            Sqrt => self.monadic_pervasive(|x| x.sqrt())?,
            Power => self.dyadic_pervasive(|x, y| x.powf(y))?,
            Reciprocal => self.monadic_pervasive(|x| 1.0 / x)?,
            Floor => self.monadic_pervasive(|x| x.floor())?,
            Round => self.monadic_pervasive(|x| x.round())?,
            Ceiling => self.monadic_pervasive(|x| x.ceil())?,
            Length => {
                let a = self.pop()?;
                self.push((a.length() as f64).into());
            }
            Identity => {
                // NB. This isn't equivalent to doing nothing since the
                // pop-push might be moving the value from the input stack
                // (not shown as formula result) to the work stack (shown as
                // result).
                let a = self.pop()?;
                self.push(a);
            }
            First => {
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
            Last => {
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
            Reverse => {
                let a = self.pop()?;
                if a.is_scalar() {
                    self.push(a);
                } else {
                    let mut cells = a.explode();
                    cells.reverse();
                    self.push(cells.iter().collect());
                }
            }
        }
        Ok(())
    }

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

// Keep this alphabetically sorted.
static ALIASES: &[(&str, &str)] = &[
    ("add", "+"),
    ("ceiling", "⌈"),
    ("dip", "⊙"),
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
    ("rearrange", "."),
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
        if let Ok(syms) = util::decipher(ALIASES, word) {
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
        // Common exponents
        ("**2", "²"),
        ("**3", "³"),
        // Multiplication and division ops.
        ("*", "×"),
        ("%", "÷"),
        ("::", "→"),
    ] {
        if let Ok((_, rest)) = parse::literal(s, from) {
            return (to.to_string(), rest);
        }
    }

    parse::char(s)
        .map(|(c, rest)| (c.to_string(), rest))
        .expect("reformat_part: Input is empty")
}

#[derive(Clone, Debug)]
pub struct Formula(pub Vec<Element>);

impl<'a> IntoIterator for &'a Formula {
    type Item = &'a Element;
    type IntoIter = std::slice::Iter<'a, Element>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Formula {
    pub fn inverted(&self) -> Result<Formula> {
        use Element::*;

        Ok(Formula(match self.0[..] {
            [Exponential(n)] => vec![Logarithm(n)],
            [Logarithm(n)] => vec![Exponential(n)],
            [Identity] => vec![Identity],
            [Square] => vec![Sqrt],
            [Sqrt] => vec![Square],
            // TODO: More reverse operations.
            _ => bail!("Cannot invert formula"),
        }))
    }
}

impl FromStr for Formula {
    type Err = anyhow::Error;

    fn from_str(mut s: &str) -> std::result::Result<Self, Self::Err> {
        let mut ret = Vec::new();
        while !s.is_empty() {
            let (token, rest) = parse::element(s).map_err(|_| anyhow!("Bad formula"))?;
            ret.push(token);
            s = rest;
        }
        Ok(Formula(ret))
    }
}

#[derive(Clone, Debug)]
pub enum Element {
    /// Numeric literal, floating point parsing applies, but the token must
    /// begin with a digit. (You can only represent positive reals,
    /// use `123¯` to get -123.)
    Num(f64),

    /// Array literal.
    Arr(Array),

    /// Variable reference. Variables can be any sequence of alphabetical
    /// characters ("abcαβγ"), optionally followed by any sequence of
    /// subscript digits (₁₂₃), then optionally followed by one to three prime
    /// symbols, (′,″,‴).
    Var(String),

    /// Assign to variable, `5→a`
    Assign(String),

    /// User-defined formula, `:Q(.×)`
    Define(String, Formula),

    /// `5 ⊃(2+)(2×) => 7 10`
    Fork(Formula, Formula),

    /// `10 5 ⊙(2×) => 20 5`
    Dip(Formula),
    /// `[1,2,3,4] /+ => 10`
    Reduce(Formula),
    /// `1024 °ₑ₂ => 10`
    Un(Formula),

    /// `1 2 ... 10 ] => [1,2,...,10]`
    Implode,

    /// ```notrust
    ///  1
    ///  2
    ///  ⇓ => [1,2]
    /// ```
    Pull(usize),

    /// `1 2 3 .₁₃ => 1 2 3 3 1`
    Rearrange(Vec<usize>),

    /// `1 ₑ => 2.71828 `, `10 ₑ₂ => 1024`
    Exponential(f64),

    /// Can be obtained by inverting an exponential.
    Logarithm(f64),

    /// `1 2 + => 3`
    Add,
    /// `5 3 - => 2`
    Subtract,
    /// `2 3 × => 6`
    Multiply,
    /// `12 4 ÷ => 3`
    Divide,
    /// `1 ¯ => -1`
    Negate,
    /// `5 ² => 25`
    Square,
    /// `25 √ => 5`
    Sqrt,
    /// `2 5 ⁿ => 32`
    Power,
    /// `5 ⨪ => 0.2`
    Reciprocal,
    /// `3.14 ⌊ => 3`, `2.71⌊ => 2`
    Floor,
    /// `3.14 ⁅ => 3`, `2.71⁅ => 3`
    Round,
    /// `3.14 ⌈ => 4`, `2.71⌈ => 3`
    Ceiling,
    /// `[[1,2][3,4][5,6]] ⧻ => 3`
    Length,
    /// `123 ∘ => 123`
    Identity,
    /// `[1,2,3] ⊢ => 1`
    First,
    /// `[1,2,3] ⊣ => 3`
    Last,
    /// `[[1,2][3,4][5,6]] ⇌ => [[5,6][3,4][1,2]]`
    Reverse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_list_is_sorted() {
        // It must be kept sorted so binary search works on it.
        let mut last = "";
        for (from, _) in ALIASES {
            assert!(
                *from > last,
                "Aliases list is not sorted: '{from}' <= '{last}'"
            );
            last = from;
        }
    }
}
