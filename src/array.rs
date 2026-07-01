use std::{
    iter,
    ops::{Deref, DerefMut},
};

pub struct Array {
    shape: Shape,
    data: Vec<f64>,
}

impl Array {
    pub fn new(shape: impl Into<Shape>, data: impl IntoIterator<Item = f64>) -> Self {
        let shape = shape.into();

        let len = shape.num_cells();
        let data: Vec<f64> = data.into_iter().take(len).collect();
        assert_eq!(data.len(), len, "Insufficient data for array");
        Array { shape, data }
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn rank(&self) -> usize {
        self.shape.rank()
    }

    pub fn cell_len(&self, cell_rank: usize) -> usize {
        assert!(
            cell_rank <= self.shape.len(),
            "Cell rank is greater than array rank"
        );
        self.shape[cell_rank..].iter().product()
    }

    pub fn cell_shape(&self, cell_rank: usize) -> &[usize] {
        assert!(
            cell_rank <= self.shape.len(),
            "Cell rank is greater than array rank"
        );
        &self.shape[..cell_rank]
    }

    pub fn zip(&self, other: &Array) -> impl Iterator<Item = (f64, f64)> {
        // Both operands have scalar rank.
        // TODO: Abstract pairs method to handle higher-rank operands.

        // Array shapes must share a prefix
        let shared_rank = self
            .shape()
            .effective_rank()
            .min(other.shape().effective_rank());
        assert_eq!(
            &self.shape[..shared_rank],
            &other.shape[..shared_rank],
            "Array shapes do not match"
        );

        // Iterate over the arrays of both, looping the shorter one if
        // necessary.
        let n = self.data.len().max(other.data.len());
        self.data
            .iter()
            .cycle()
            .zip(other.data.iter().cycle())
            .take(n)
            .map(|(&a, &b)| (a, b))
    }
}

impl Deref for Array {
    type Target = [f64];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for Array {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    pub fn new(dims: Vec<usize>) -> Self {
        assert!(
            dims.iter().all(|&dim| dim > 0),
            "Array dimensions must be greater than zero"
        );
        Shape { dims }
    }

    pub fn rank(&self) -> usize {
        self.len()
    }

    pub fn num_cells(&self) -> usize {
        self.dims.iter().product()
    }

    /// Strip high dimensions of size 1, the array is structurally identical
    /// to an array with these dimensions removed.
    pub fn effective_rank(&self) -> usize {
        self.rank() - self.dims.iter().rev().take_while(|&&dim| dim == 1).count()
    }

    pub fn coords(&self) -> impl Iterator<Item = Vec<usize>> + '_ {
        let mut indices = vec![0; self.len()];
        let mut end = false;
        iter::from_fn(move || {
            if end {
                return None;
            }
            let current = indices.clone();

            // With empty frame_shape, return a single empty vector then exit.
            if self.is_empty() {
                end = true;
                return Some(current);
            }

            for i in 0..self.len() {
                indices[i] += 1;
                if indices[i] < self[i] {
                    break;
                } else {
                    indices[i] = 0;
                    if i == self.len() - 1 {
                        end = true;
                        break;
                    }
                }
            }
            Some(current)
        })
    }
}

impl From<Vec<usize>> for Shape {
    fn from(dims: Vec<usize>) -> Self {
        Shape::new(dims)
    }
}

impl Deref for Shape {
    type Target = [usize];

    fn deref(&self) -> &Self::Target {
        &self.dims
    }
}
