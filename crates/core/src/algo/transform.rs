//! Byte-level transforms for XOR delta compression.
//!
//! Provides byte-shuffle/unshuffle and XOR operations used by checkpoint
//! delta encoding. These transforms improve compression ratios by grouping
//! similar bytes together before compression.

/// Byte-unshuffle: inverse of byte_shuffle.
///
/// Groups bytes by their position within each element. For `element_size=4`:
/// input  `[A0 B0 C0 D0  A1 B1 C1 D1]` (interleaved)
/// output `[A0 A1  B0 B1  C0 C1  D0 D1]` (grouped by byte lane)
///
/// The `scratch` buffer is reused across calls to avoid repeated allocation.
pub fn byte_unshuffle(data: &mut [u8], element_size: usize, scratch: &mut Vec<u8>) {
    if element_size <= 1 || data.len() < element_size {
        return;
    }
    let n = data.len();
    scratch.resize(n, 0);
    scratch.copy_from_slice(data);
    let count = n / element_size;
    let tail = n % element_size;
    for i in 0..count {
        for j in 0..element_size {
            data[i * element_size + j] = scratch[j * count + i];
        }
    }
    // Copy tail bytes verbatim
    if tail > 0 {
        data[count * element_size..].copy_from_slice(&scratch[count * element_size..]);
    }
}

/// XOR `src` into `dst` in-place: `dst[i] ^= src[i]`.
///
/// Both slices must have the same length.
///
/// # Panics
///
/// Panics if `dst.len() != src.len()`.
pub fn xor_in_place(dst: &mut [u8], src: &[u8]) {
    assert_eq!(dst.len(), src.len(), "xor_in_place: length mismatch");
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d ^= s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_unshuffle_identity_element_size_1() {
        let mut data = vec![1, 2, 3, 4];
        let mut scratch = Vec::new();
        byte_unshuffle(&mut data, 1, &mut scratch);
        assert_eq!(data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_byte_unshuffle_element_size_2() {
        // Shuffled layout (byte lanes grouped): [A0 A1 B0 B1]
        // Unshuffled (interleaved): [A0 B0 A1 B1]
        let mut data = vec![0x10, 0x20, 0x11, 0x21];
        let mut scratch = Vec::new();
        byte_unshuffle(&mut data, 2, &mut scratch);
        assert_eq!(data, vec![0x10, 0x11, 0x20, 0x21]);
    }

    #[test]
    fn test_byte_unshuffle_with_tail() {
        // 5 bytes with element_size=2: 2 complete elements + 1 tail byte
        let mut data = vec![0x10, 0x20, 0x11, 0x21, 0xFF];
        let mut scratch = Vec::new();
        byte_unshuffle(&mut data, 2, &mut scratch);
        assert_eq!(data, vec![0x10, 0x11, 0x20, 0x21, 0xFF]);
    }

    #[test]
    fn test_xor_in_place() {
        let mut dst = vec![0xFF, 0x00, 0xAA];
        let src = vec![0xFF, 0xFF, 0x55];
        xor_in_place(&mut dst, &src);
        assert_eq!(dst, vec![0x00, 0xFF, 0xFF]);
    }

    #[test]
    #[should_panic(expected = "length mismatch")]
    fn test_xor_in_place_length_mismatch() {
        let mut dst = vec![0xFF, 0x00];
        let src = vec![0xFF];
        xor_in_place(&mut dst, &src);
    }
}
