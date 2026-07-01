use anyhow::{Result, bail};

use crate::Array;

pub struct Stack {
    data: Vec<Array>,
}

impl Stack {
    pub fn apply_dyadic(&mut self, f: impl Fn(f64, f64) -> f64) -> Result<()> {
        if self.data.len() < 2 {
            bail!("Stack underflow");
        }

        let Some(result_shape) =
            self.data[self.data.len() - 2].result_shape(&self.data[self.data.len() - 1])
        else {
            bail!("Incompatible shapes");
        };

        let (b, a) = (self.pop()?, self.pop()?);

        let ret = Array::new(result_shape, a.zip(&b).map(|(x, y)| f(x, y)));
        self.data.push(ret);

        Ok(())
    }

    pub fn apply_monadic(&mut self, f: impl Fn(f64) -> f64) -> Result<()> {
        if self.data.is_empty() {
            bail!("Stack underflow");
        }

        let a = self.data.len() - 1;
        for a in self.data[a].iter_mut() {
            *a = f(*a);
        }

        Ok(())
    }

    pub fn pop(&mut self) -> Result<Array> {
        if let Some(array) = self.data.pop() {
            Ok(array)
        } else {
            bail!("Stack underflow");
        }
    }

    pub fn push(&mut self, val: Array) {
        self.data.push(val);
    }

    /// Turn all stack values into a single array, assuming they all have the
    /// same shape.
    pub fn chunk_stacked(&mut self) -> Result<()> {
        if self.data.is_empty() {
            self.data.push(Array::new(vec![0], vec![]));
            return Ok(());
        }

        if !self.data.iter().all(|x| x.shape() == self.data[0].shape()) {
            bail!("Incompatible shapes");
        }

        let result: Array = self.data.iter().collect();

        self.data.clear();
        self.data.push(result);

        Ok(())
    }
}
