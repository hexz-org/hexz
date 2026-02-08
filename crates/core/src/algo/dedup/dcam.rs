//! Deduplication Change-Estimation Analytical Model (DCAM).
//!
//! Implements the analytical formulas from "Optimizing Deduplication Parameters via
//! a Change-Estimation Analytical Model" (Randall et al., 2025).
//!
//! # Overview
//!
//! DCAM predicts how well deduplication will work for a given dataset and CDC
//! parameter set. It models:
//! - Chunk size distribution based on FastCDC parameters
//! - Duplicate detection probability based on change rate
//! - Metadata overhead vs storage savings tradeoff
//!
//! # Key Concepts
//!
//! - **Change rate (c)**: Probability that a byte differs from its previous version
//! - **Fingerprint bits (f)**: Controls average chunk size (2^f bytes)
//! - **Deduplication ratio**: (Final size) / (Original size), where <1.0 means savings
//!
//! # Workflow
//!
//! 1. Run a "dry run" CDC analysis to estimate `c` (change rate)
//! 2. Use DCAM formulas to predict dedup ratio for different parameter sets
//! 3. Choose optimal parameters (minimize ratio while keeping reasonable chunk sizes)
//! 4. Apply chosen parameters during actual snapshot creation
//!
//! # Examples
//!
//! ```
//! use strata_core::algo::dedup::dcam::{DedupeParams, predict_ratio, calculate_c};
//!
//! // Estimate change rate from dry run
//! let file_size = 1_000_000_000; // 1GB
//! let unique_bytes = 500_000_000; // 500MB unique
//! let params = DedupeParams::lbfs_baseline();
//! let c = calculate_c(unique_bytes, file_size, &params);
//!
//! // Predict dedup ratio for different parameters
//! let ratio = predict_ratio(file_size, c, &params);
//! println!("Predicted compression: {:.2}% of original", ratio * 100.0);
//!
//! // Try larger chunks (f=15 → 32KB avg)
//! let mut params_large = params;
//! params_large.f = 15;
//! let ratio_large = predict_ratio(file_size, c, &params_large);
//! println!("With larger chunks: {:.2}%", ratio_large * 100.0);
//! ```
//!
//! # References
//!
//! Randall et al., "Optimizing Deduplication Parameters via a Change-Estimation
//! Analytical Model", 2025.

/// FastCDC and deduplication parameters.
///
/// These parameters control chunk size distribution and are used by both FastCDC
/// (for actual chunking) and DCAM (for analytical prediction).
///
/// # Parameters
///
/// - **f**: Fingerprint bits. Average chunk size is 2^f bytes. Typical: 13-15 (8KB-32KB)
/// - **m**: Minimum chunk size. Prevents tiny chunks. Typical: 2KB-4KB
/// - **z**: Maximum chunk size. Bounds worst-case chunk size. Typical: 64KB-128KB
/// - **w**: Rolling hash window size. Typical: 48 bytes
/// - **v**: Per-chunk metadata overhead (hash + pointer). Typical: 8-16 bytes
///
/// # Tradeoffs
///
/// - **Larger chunks (higher f)**:
///   - Pro: Less metadata overhead, faster processing
///   - Con: Coarser deduplication (misses small duplicates)
/// - **Smaller chunks (lower f)**:
///   - Pro: Finer deduplication granularity
///   - Con: More metadata, slower processing
#[derive(Debug, Clone, Copy)]
pub struct DedupeParams {
    /// Fingerprint size in bits ($f$). Controls average chunk size ($2^f$).
    pub f: u32,

    /// Minimum chunk length in bytes ($m$).
    pub m: u32,

    /// Maximum chunk length in bytes ($z$).
    pub z: u32,

    /// Rolling hash window size in bytes ($w$).
    pub w: u32,

    /// Per-chunk metadata overhead in bytes ($v$).
    pub v: u32,
}

impl Default for DedupeParams {
    /// Returns default parameters (same as LBFS baseline).
    fn default() -> Self {
        Self::lbfs_baseline()
    }
}

impl DedupeParams {
    /// LBFS Baseline parameters used as the reference point in the DCAM paper.
    ///
    /// These are well-tested parameters from the Low-Bandwidth Network File System:
    /// - **f = 13**: 8KB average chunks (2^13 = 8192)
    /// - **m = 2KB**: Minimum chunk size
    /// - **z = 64KB**: Maximum chunk size
    /// - **w = 48**: Rolling hash window
    /// - **v = 8**: Hash + pointer overhead per chunk
    ///
    /// # Returns
    ///
    /// A parameter set suitable for general-purpose deduplication.
    pub fn lbfs_baseline() -> Self {
        Self {
            f: 13,
            m: 2048,
            z: 65536,
            w: 48,
            v: 8, // 8 bytes hash/pointer overhead
        }
    }

    /// Calculates the chunk boundary probability.
    ///
    /// In FastCDC, a chunk boundary occurs when the rolling hash matches a pattern.
    /// With `f` fingerprint bits, the probability is:
    ///
    /// **p = 1 / 2^f**
    ///
    /// # Returns
    ///
    /// Probability (0.0 to 1.0) of a boundary at any given position.
    ///
    /// # Examples
    ///
    /// ```
    /// use strata_core::algo::dedup::dcam::DedupeParams;
    ///
    /// let params = DedupeParams::lbfs_baseline(); // f=13
    /// let p = params.p();
    /// assert!((p - 1.0/8192.0).abs() < 1e-10); // 1/2^13 ≈ 0.000122
    /// ```
    ///
    /// # Reference
    ///
    /// Equation 1 in DCAM paper.
    pub fn p(&self) -> f64 {
        1.0 / 2.0_f64.powi(self.f as i32)
    }
}

/// Calculates the expected (average) chunk length for given parameters.
///
/// This function computes the weighted average chunk size considering:
/// - Chunks that hit a natural boundary (probability `p`)
/// - Chunks that reach maximum size (probability `(1-p)^(z-m)`)
///
/// # Parameters
///
/// - `params`: FastCDC parameters (f, m, z)
///
/// # Returns
///
/// Expected chunk length in bytes. Typically close to 2^f but affected by
/// min/max constraints.
///
/// # Formula
///
/// ```text
/// l(θ) = Σ(i=0 to z-m-1) [(1-p)^i * p * (m+i)] + (1-p)^(z-m) * z
/// ```
///
/// Where:
/// - First term: Sum of probabilities for each possible chunk size from m to z-1
/// - Second term: Probability of reaching maximum chunk size z
///
/// # Examples
///
/// ```
/// use strata_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
///
/// let params = DedupeParams::lbfs_baseline(); // f=13, m=2KB, z=64KB
/// let avg_len = expected_chunk_length(&params);
/// println!("Average chunk: {:.0} bytes", avg_len); // ~8192
/// ```
///
/// # Reference
///
/// Equation 3 in DCAM paper.
pub fn expected_chunk_length(params: &DedupeParams) -> f64 {
    let p = params.p();
    let m = params.m as f64;
    let z = params.z as f64;

    // Sum from i=0 to z-m-1 of: (1-p)^i * p * (m+i)
    let term1: f64 = (0..(params.z - params.m))
        .map(|i| {
            let i_f = i as f64;
            (1.0 - p).powf(i_f) * p * (m + i_f)
        })
        .sum();

    // Term for max chunk size: (1-p)^(z-m) * z
    let term2 = (1.0 - p).powf(z - m) * z;

    term1 + term2
}

/// Calculates the expected number of duplicate bytes in a chunk.
///
/// This function models how many bytes in a chunk are likely to be duplicates
/// based on the change rate `c`. A lower change rate means more duplicates.
///
/// # Parameters
///
/// - `c`: Change rate (probability that a byte differs from previous version)
/// - `params`: FastCDC parameters (f, m, z, w)
///
/// # Returns
///
/// Expected duplicate bytes per chunk. Higher values mean better deduplication.
///
/// # Formula
///
/// ```text
/// y(c,θ) = Σ(i=0 to z-m-1) [(1-p)^i * p * (m+i) * (1-c)^(i+m+w)]
///        + (1-p)^(z-m) * z * (1-c)^(z+w)
/// ```
///
/// The term `(1-c)^(i+m+w)` models the probability that enough bytes remain
/// unchanged for the chunk to be detected as a duplicate.
///
/// # Examples
///
/// ```
/// use strata_core::algo::dedup::dcam::{DedupeParams, expected_duplicate_bytes};
///
/// let params = DedupeParams::lbfs_baseline();
///
/// // Low change rate → high duplication
/// let dup_low = expected_duplicate_bytes(0.01, &params);
/// println!("1% changes: {:.0} dup bytes/chunk", dup_low);
///
/// // High change rate → low duplication
/// let dup_high = expected_duplicate_bytes(0.50, &params);
/// println!("50% changes: {:.0} dup bytes/chunk", dup_high);
/// ```
///
/// # Reference
///
/// Equation 9 in DCAM paper.
pub fn expected_duplicate_bytes(c: f64, params: &DedupeParams) -> f64 {
    let p = params.p();
    let m = params.m as f64;
    let z = params.z as f64;
    let w = params.w as f64;

    // Sum from i=0 to z-m-1
    let term1: f64 = (0..(params.z - params.m))
        .map(|i| {
            let i_f = i as f64;
            // (1-p)^i * p * (m+i) * (1-c)^(i+m+w)
            (1.0 - p).powf(i_f) * p * (m + i_f) * (1.0 - c).powf(i_f + m + w)
        })
        .sum();

    // Max chunk term: (1-p)^(z-m) * z * (1-c)^(z+w)
    let term2 = (1.0 - p).powf(z - m) * z * (1.0 - c).powf(z + w);

    term1 + term2
}

/// Predicts the deduplication ratio for a file/dataset.
///
/// This is the main DCAM prediction function. It estimates the final compressed
/// size as a ratio of the original size, accounting for both duplicate elimination
/// and metadata overhead.
///
/// # Parameters
///
/// - `file_size`: Original file size in bytes
/// - `c`: Change rate (0.0 to 1.0), estimated from dry-run analysis
/// - `params`: FastCDC parameters to evaluate
///
/// # Returns
///
/// Deduplication ratio where:
/// - **< 1.0**: Storage savings (e.g., 0.5 = 50% of original size)
/// - **= 1.0**: No change (overhead equals savings)
/// - **> 1.0**: Storage increase (overhead exceeds savings)
///
/// # Formula
///
/// ```text
/// s(n,c,θ) = n * (y/l)           [Total duplicate bytes]
/// h(θ) = v * (n/l + 1)            [Metadata overhead]
/// d(n,c,θ) = s - h                [Net savings]
/// ratio = (n - d) / n             [Final ratio]
/// ```
///
/// Where:
/// - `n`: File size
/// - `y`: Expected duplicate bytes per chunk
/// - `l`: Expected chunk length
/// - `v`: Per-chunk metadata bytes
///
/// # Examples
///
/// ```
/// use strata_core::algo::dedup::dcam::{DedupeParams, predict_ratio, calculate_c};
///
/// let file_size = 1_000_000_000; // 1GB
/// let unique_bytes = 400_000_000; // 400MB unique (600MB duplicates)
///
/// let params = DedupeParams::lbfs_baseline();
/// let c = calculate_c(unique_bytes, file_size, &params);
/// let ratio = predict_ratio(file_size, c, &params);
///
/// println!("Predicted: {:.1}% of original size", ratio * 100.0);
/// println!("Expected savings: {:.1}%", (1.0 - ratio) * 100.0);
/// ```
///
/// # Note
///
/// The prediction assumes:
/// - Uniform distribution of changes across the file
/// - Independent chunk boundaries (FastCDC assumption)
/// - Perfect hash collision resistance (no false duplicates)
///
/// Real-world results may vary by ±5-10% depending on data structure.
///
/// # Reference
///
/// Derived from Equations 5, 10, 11, and 14 in DCAM paper.
pub fn predict_ratio(file_size: u64, c: f64, params: &DedupeParams) -> f64 {
    let l = expected_chunk_length(params);
    let n = file_size as f64;

    // Equation 10: Expected duplicate bytes in the file s(n, c, theta)
    // s = n * (y / l)
    let y = expected_duplicate_bytes(c, params);
    let s = n * (y / l);

    // Equation 5: Total overhead h(theta)
    // h = v * (n/l + 1)
    let num_chunks = (n / l) + 1.0;
    let h = (params.v as f64) * num_chunks;

    // Equation 11: Net duplicate bytes d(n, c, theta)
    // d = s - h
    // This represents the "savings" in bytes (Duplicate Content - Metadata Overhead).
    let d = s - h;

    // Deduplication Ratio = (Original Size - Net Savings) / Original Size
    // Ratio = (n - d) / n
    // Note: The paper defines Ratio = (NDB + Overhead) / Original.
    // Since NDB (Non-Duplicate Bytes) approx n - s,
    // Ratio = (n - s + h) / n = (n - (s - h)) / n = (n - d) / n.

    let predicted_size = n - d;

    // Clamp to avoid negative size predictions in extreme overhead cases
    if predicted_size < 0.0 {
        // If overhead exceeds savings significantly, ratio > 1.0
        return (n + h) / n;
    }

    predicted_size / n
}

/// Estimates the change rate from empirical deduplication results.
///
/// This function reverse-engineers the change rate `c` from a dry-run CDC analysis.
/// The change rate represents how "different" the dataset is from itself (or a
/// previous version), which drives deduplication effectiveness.
///
/// # Parameters
///
/// - `ndb`: Non-duplicate bytes (unique bytes after deduplication)
/// - `file_size`: Original file size in bytes
/// - `params`: Parameters used during the dry-run analysis
///
/// # Returns
///
/// Change rate `c` (0.0 to 1.0) where:
/// - **c = 0.0**: No changes (100% duplicates, perfect dedup)
/// - **c = 0.5**: 50% change rate (moderate dedup)
/// - **c = 1.0**: All unique (no duplicates)
///
/// # Formula
///
/// ```text
/// c = NDB / (n * l(θ'))
/// ```
///
/// Where:
/// - `NDB`: Non-duplicate bytes (from dry run)
/// - `n`: File size
/// - `l(θ')`: Expected chunk length for dry-run parameters
///
/// # Workflow
///
/// 1. Run `analyze_stream()` to get unique_bytes (NDB)
/// 2. Call `calculate_c()` to estimate change rate
/// 3. Use `predict_ratio()` with different parameter sets to find optimal
///
/// # Examples
///
/// ```
/// use strata_core::algo::dedup::dcam::{DedupeParams, calculate_c};
///
/// let file_size = 1_000_000_000;  // 1GB
/// let unique = 300_000_000;       // 300MB unique
/// let params = DedupeParams::lbfs_baseline();
///
/// let c = calculate_c(unique, file_size, &params);
/// println!("Change rate: {:.2}%", c * 100.0);
///
/// if c < 0.2 {
///     println!("Excellent dedup potential!");
/// } else if c < 0.5 {
///     println!("Good dedup potential");
/// } else {
///     println!("Limited dedup potential");
/// }
/// ```
///
/// # Reference
///
/// Equation 6 in DCAM paper.
pub fn calculate_c(ndb: u64, file_size: u64, params: &DedupeParams) -> f64 {
    let n = file_size as f64;
    let l = expected_chunk_length(params);

    if ndb >= file_size {
        return 1.0;
    }

    let c = (ndb as f64) / (n * l);

    // Clamp between 0.0 and 1.0
    c.clamp(0.0, 1.0)
}
