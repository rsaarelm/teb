use std::ops::{Deref, DerefMut};

#[derive(Clone, PartialEq, Debug)]
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

    pub fn is_scalar(&self) -> bool {
        self.shape.is_empty()
    }

    /// Length along the top-level dimension, or 1 if the array is a scalar.
    pub fn length(&self) -> usize {
        self.shape.last().copied().unwrap_or(1)
    }

    /// Strip high dimensions of size 1, the array is structurally identical
    /// to an array with these dimensions removed.
    fn effective_rank(&self) -> usize {
        self.rank() - self.shape.iter().rev().take_while(|&&dim| dim == 1).count()
    }

    /// Number of scalars in a cell of this array of the given rank.
    pub fn cell_size(&self, cell_rank: usize) -> usize {
        assert!(
            cell_rank <= self.shape.len(),
            "Cell rank is greater than array rank"
        );
        self.shape[0..cell_rank].iter().product()
    }

    pub fn cell_shape(&self, cell_rank: usize) -> &[usize] {
        assert!(
            cell_rank <= self.shape.len(),
            "Cell rank is greater than array rank"
        );
        &self.shape[..cell_rank]
    }

    /// If the array represents a scalar value (single value, no shape),
    /// return that value.
    pub fn as_scalar(&self) -> Option<f64> {
        if self.data.len() == 1 && self.shape.is_empty() {
            Some(self.data[0])
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &f64> {
        self.data.iter()
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

    pub fn is_compatible_with(&self, other: &Array) -> bool {
        let shared_rank = self.effective_rank().min(other.effective_rank());
        self.shape[..shared_rank] == other.shape[..shared_rank]
    }

    pub fn result_shape(&self, other: &Array) -> Option<Vec<usize>> {
        if self.is_compatible_with(other) {
            if self.rank() >= other.rank() {
                Some(self.shape.clone())
            } else {
                Some(other.shape.clone())
            }
        } else {
            None
        }
    }

    /// Append the data of another array to this, assuming other is either the
    /// shape of this whole array (rank goes up by 1) or the shape of a
    /// top-level cell of this array.
    pub fn append(&mut self, other: &Array) {
        if other.shape() == self.shape() {
            // It's an exact copy, increase our rank.
            self.shape.push(2);
        } else if other.shape() == self.cell_shape(self.rank() - 1) {
            // It's a cell of our array, increment the size of the last dimension.
            let last_dim = self.shape.last_mut().unwrap();
            *last_dim += 1;
        } else {
            panic!("Incompatible shapes");
        }
        // XXX: Should we do something clever if this or the other has
        // trailing ones in their shape?
        self.data.extend_from_slice(&other.data);
    }

    /// Explode the array into cells along its highest dimension.
    pub fn explode(&self) -> Vec<Array> {
        assert!(self.rank() > 0, "Cannot explode a scalar array");
        let size = self.cell_size(self.rank() - 1);
        let n = self.length();
        let mut result = Vec::with_capacity(n);
        for i in 0..n {
            result.push(Array::new(
                self.cell_shape(self.rank() - 1).to_vec(),
                (self.data[i * size..(i + 1) * size]).to_vec(),
            ));
        }
        result
    }
}

impl From<f64> for Array {
    fn from(value: f64) -> Self {
        Array::new(vec![], vec![value])
    }
}

impl<'a> FromIterator<&'a Array> for Array {
    fn from_iter<I: IntoIterator<Item = &'a Array>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let Some(mut seed) = iter.next().cloned() else {
            return Array::default();
        };

        for next in iter {
            seed.append(next);
        }
        seed
    }
}

impl Default for Array {
    fn default() -> Self {
        Array::new(vec![0], vec![])
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
