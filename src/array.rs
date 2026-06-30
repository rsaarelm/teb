use std::{
    iter,
    ops::{Deref, DerefMut},
};

pub struct Array {
    shape: Vec<usize>,
    data: Vec<f64>,
}

// Use slice::chunks(n) to iterate cell chunks

pub fn cell_coords(frame_shape: &[usize]) -> impl Iterator<Item = Vec<usize>> + '_ {
    let mut indices = vec![0; frame_shape.len()];
    let mut end = false;
    iter::from_fn(move || {
        if end {
            return None;
        }
        let current = indices.clone();

        // With empty frame_shape, return a single empty vector then exit.
        if frame_shape.is_empty() {
            end = true;
            return Some(current);
        }

        for i in 0..frame_shape.len() {
            indices[i] += 1;
            if indices[i] < frame_shape[i] {
                break;
            } else {
                indices[i] = 0;
                if i == frame_shape.len() - 1 {
                    end = true;
                    break;
                }
            }
        }
        Some(current)
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_cell_scan() {
        // Scalar
        let scalar = Array::new(vec![], vec![42.0]);

        assert_eq!(scalar.rank(), 0);
        assert_eq!(
            scalar.cells(0).collect::<Vec<_>>(),
            vec![(vec![], &[42.0][..])]
        );

        // Vector
        let vector = Array::new(vec![3], vec![1.0, 2.0, 3.0]);
        assert_eq!(vector.rank(), 1);
        assert_eq!(
            vector.cells(0).collect::<Vec<_>>(),
            vec![
                (vec![0], &[1.0][..]),
                (vec![1], &[2.0][..]),
                (vec![2], &[3.0][..])
            ]
        );
        assert_eq!(
            vector.cells(1).collect::<Vec<_>>(),
            vec![(vec![], &[1.0, 2.0, 3.0][..])]
        );

        // Matrix
        let matrix = Array::new(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(matrix.rank(), 2);
        assert_eq!(
            matrix.cells(0).collect::<Vec<_>>(),
            vec![
                (vec![0, 0], &[1.0][..]),
                (vec![1, 0], &[2.0][..]),
                (vec![0, 1], &[3.0][..]),
                (vec![1, 1], &[4.0][..]),
                (vec![0, 2], &[5.0][..]),
                (vec![1, 2], &[6.0][..])
            ]
        );

        assert_eq!(
            matrix.cells(1).collect::<Vec<_>>(),
            vec![
                (vec![0], &[1.0, 2.0][..]),
                (vec![1], &[3.0, 4.0][..]),
                (vec![2], &[5.0, 6.0][..])
            ]
        );

        assert_eq!(
            matrix.cells(2).collect::<Vec<_>>(),
            vec![(vec![], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0][..])]
        );
    }
}
