use std::ops::{Deref, DerefMut};

pub struct Array {
    shape: Vec<usize>,
    data: Vec<f64>,
}

impl Array {
    pub fn new(shape: Vec<usize>, data: impl IntoIterator<Item = f64>) -> Self {
        let len = shape.iter().product::<usize>();
        let data: Vec<f64> = data.into_iter().take(len).collect();
        assert_eq!(data.len(), len, "Insufficient data for array");
        Array { shape, data }
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    pub fn rank(&self) -> usize {
        self.shape.len()
    }

    /// Strip high dimensions of size 1, the array is structurally identical
    /// to an array with these dimensions removed.
    pub fn effective_rank(&self) -> usize {
        self.rank() - self.shape.iter().rev().take_while(|&&dim| dim == 1).count()
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
        let shared_rank = self.effective_rank().min(other.effective_rank());
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
