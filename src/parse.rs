use std::{rc::Rc, str::FromStr};

use anyhow::{Result, bail};

#[derive(Clone, Default)]
struct Cell {
    text: String,
    value: Option<f64>,
    formula: Option<Formula>,
}

impl Cell {
    pub fn uses_scientific_notation(&self) -> bool {
        // We only care about non-formula values.
        self.formula.is_none()
            && self.value.is_some()
            && (self.text.contains('e') || self.text.contains('E'))
    }

    pub fn is_formula_cell(&self) -> bool {
        self.formula.is_some()
    }

    /// Rewrite the value of a formula cell.
    pub fn assign(&mut self, value: impl AsRef<str>) {
        // If a formula exists, replace text up to the first comma.
        // Non-formula cells should not be rewritten at all.
        if self.formula.is_none() {
            panic!("Trying to rewrite a non-formula cell.");
        }
        let formula_start = self.text.find(',').unwrap_or(self.text.len());
        self.text.replace_range(..formula_start, value.as_ref());
    }
}

impl FromStr for Cell {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let text = s.trim().to_string();

        let mut ret = Cell {
            text: text.clone(),
            value: None,
            formula: None,
        };

        // Only a number.
        if let Ok(num) = text.parse::<f64>() {
            ret.value = Some(num);
            return Ok(ret);
        }

        // Formula with optional number value before it.
        if let Some((val, form)) = text.split_once(',') {
            let mut value = None;
            if !val.is_empty() {
                // If there's a non-number prefix, this isn't a valid formula
                // cell after all, revert back to treating it as text.
                let Ok(num) = val.parse::<f64>() else {
                    return Ok(ret);
                };
                value = Some(num);
            }

            let formula = formula(form)?;

            return Ok(Cell {
                text,
                value,
                formula: Some(formula),
            });
        }
        Ok(ret)
    }
}

type Formula = Vec<Token>;

pub fn formula(s: &str) -> Result<Formula> {
    let mut tokens = Vec::new();
    let mut rest = s;

    while !rest.is_empty() {
        match token(rest) {
            Ok((token, new_rest)) => {
                tokens.push(token);
                rest = new_rest;
            }
            Err(_) => break,
        }
    }

    if tokens.is_empty() {
        bail!("Empty formula");
    }

    Ok(tokens)
}

#[derive(Clone)]
pub enum Token {
    Literal(f64),
    ChunkStacked,
    PullColumn,
    Variable(char),
    AssignTo(char),
    MonadicOp(Rc<dyn Fn(f64) -> f64>),
    DyadicOp(Rc<dyn Fn(f64, f64) -> f64>),
}
use Token::*;

pub fn token(s: &str) -> Result<(Token, &str)> {
    if s.is_empty() {
        bail!("No input");
    }

    match s.chars().next().unwrap() {
        c if c.is_whitespace() => bail!("Whitespace is not allowed"),

        '0'..='9' => {
            // TODO: Support decimal dots and e notation.
            let (num_str, rest) = s.split_at(s.find(|c: char| !c.is_digit(10)).unwrap_or(s.len()));
            let num = num_str.parse::<f64>()?;
            Ok((Literal(num), rest))
        }

        '+' => Ok((DyadicOp(Rc::new(|x, y| x + y)), &s[1..])),
        '-' => Ok((DyadicOp(Rc::new(|x, y| x - y)), &s[1..])),
        '*' | '×' => Ok((DyadicOp(Rc::new(|x, y| x * y)), &s[1..])),
        '%' | '÷' => Ok((DyadicOp(Rc::new(|x, y| x / y)), &s[1..])),

        '‾' => Ok((MonadicOp(Rc::new(|x| -x)), &s[1..])),

        '⁅' => Ok((MonadicOp(Rc::new(|x| x.round())), &s[1..])),
        '√' => Ok((MonadicOp(Rc::new(|x| x.sqrt())), &s[1..])),
        '²' => Ok((MonadicOp(Rc::new(|x| x * x)), &s[1..])),

        // TODO: Trailing subscripts to rearrange stack inputs for ops,
        // 8 0 6 0 +₄₂
        // Needs lookahead parsing, two for dyads, one for monads.
        // Make monadic/dyadic op parse into its own function so it can be
        // matched in one go and then we can move to the lookahead part.

        // Variable assignment.
        '→' => {
            let Ok((Variable(var), rest)) = token(&s[1..]) else {
                bail!("Bad variable name");
            };
            Ok((AssignTo(var), rest))
        }

        // Special forms.
        ']' => Ok((ChunkStacked, &s[1..])),
        '↓' => Ok((PullColumn, &s[1..])),

        // TODO: Sum, product, count etc. with combinators or something.

        // Put variable matching last so any reserved letters are caught
        // earlier.

        // XXX: Should variables just be "is_alphabetic" instead of limited to
        // ASCII? Greek letters are definitely useful, but some of them are
        // used by builtin ops like Σ.
        'a'..='z' | 'A'..='Z' => Ok((Variable(s.chars().next().unwrap()), &s[1..])),

        c => bail!("Unrecognized token {c}"),
    }
}

pub fn subscript_digit(s: &str) -> Result<(u32, &str)> {
    if s.is_empty() {
        bail!("No input");
    }

    let c = s.chars().next().unwrap();
    let digit = match c {
        '₀' => 0,
        '₁' => 1,
        '₂' => 2,
        '₃' => 3,
        '₄' => 4,
        '₅' => 5,
        '₆' => 6,
        '₇' => 7,
        '₈' => 8,
        '₉' => 9,
        _ => bail!("Not a subscript digit: {c}"),
    };

    Ok((digit, &s[c.len_utf8()..]))
}

/// If the nonempty lines in input all share the exact same prefix made of
/// spaces and tabs, return that prefix. If nonempty lines have inconsistent
/// indentation, return an error.
pub fn indent_prefix(text: &str) -> Result<String> {
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
                bail!("Inconsistent indentation on line {}", i + 1);
            }
        } else {
            prefix = Some(line_prefix);
        }
    }

    Ok(prefix.unwrap_or_default())
}
