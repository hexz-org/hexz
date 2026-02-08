//! Fisher-Yates index shuffler for randomized data loading.
//!
//! Generates a shuffled sequence of block indices so that ML data
//! loaders can access snapshot samples in random order without
//! materializing the entire dataset in memory.

/// Generates a shuffled index sequence using Fisher-Yates algorithm.
///
/// Returns a `Vec<usize>` containing indices `0..count` in random order,
/// seeded deterministically for reproducibility.
pub fn shuffled_indices(count: usize, seed: u64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..count).collect();

    // Simple xorshift64 PRNG for deterministic shuffling
    let mut state = seed;
    for i in (1..indices.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        indices.swap(i, j);
    }

    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shuffled_indices_deterministic() {
        let a = shuffled_indices(100, 42);
        let b = shuffled_indices(100, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn test_shuffled_indices_contains_all() {
        let indices = shuffled_indices(10, 1);
        let mut sorted = indices.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
