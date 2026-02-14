//! Deduplication Change-Estimation Analytical Model (DCAM).
//!
//! Implements the analytical formulas from "Optimizing Deduplication Parameters via
//! a Change-Estimation Analytical Model" (Randall et al., 2025).
//!
//! # Overview
//!
//! DCAM is a mathematical model that predicts deduplication effectiveness **without
//! running the full deduplication process**. It enables rapid parameter optimization
//! by analytically computing the expected storage savings for any combination of
//! Content-Defined Chunking (CDC) parameters.
//!
//! The model addresses a fundamental challenge in deduplication: choosing optimal
//! chunk sizes requires balancing two competing factors:
//!
//! - **Smaller chunks**: Higher deduplication granularity (finds more duplicates) but
//!   more metadata overhead and slower processing
//! - **Larger chunks**: Lower metadata overhead and faster processing but coarser
//!   deduplication (misses small duplicates)
//!
//! DCAM solves this by modeling:
//! 1. **Chunk size distribution** based on FastCDC boundary detection probability
//! 2. **Duplicate detection probability** based on dataset change rate
//! 3. **Metadata overhead** vs. **storage savings** tradeoff
//!
//! # The Analytical Model
//!
//! ## Core Components
//!
//! ### 1. Chunk Boundary Probability
//!
//! FastCDC uses a rolling hash with a fingerprint mask to detect chunk boundaries.
//! The probability of a boundary at any byte position is:
//!
//! ```text
//! p = 1 / 2^f
//! ```
//!
//! Where `f` is the fingerprint size in bits. This creates an **expected chunk size**
//! of `2^f` bytes (e.g., f=13 → 8KB average chunks).
//!
//! ### 2. Expected Chunk Length
//!
//! The actual average chunk size considers min/max constraints:
//!
//! ```text
//! l(θ) = Σ(i=0 to z-m-1) [(1-p)^i · p · (m+i)] + (1-p)^(z-m) · z
//! ```
//!
//! This formula computes the weighted average where:
//! - Each chunk size from `m` (min) to `z-1` (max-1) has probability `(1-p)^i · p`
//! - Chunks that don't hit a boundary before `z` are capped at `z`
//!
//! ### 3. Change Rate Estimation
//!
//! The **change rate** `c` represents the probability that a byte differs from its
//! previous version (or duplicate in the corpus). It is estimated by:
//!
//! ```text
//! c = NDB / (n · l(θ'))
//! ```
//!
//! Where:
//! - `NDB`: Non-duplicate bytes (unique bytes from a dry-run analysis)
//! - `n`: Total file size
//! - `l(θ')`: Expected chunk length for the dry-run parameters
//!
//! **Interpretation**:
//! - `c = 0.0`: Completely redundant data (perfect deduplication opportunity)
//! - `c = 0.1`: 10% of bytes changed (excellent deduplication)
//! - `c = 0.5`: 50% changed (moderate deduplication)
//! - `c = 1.0`: No duplicates (deduplication ineffective)
//!
//! ### 4. Expected Duplicate Bytes
//!
//! For a chunk to be detected as a duplicate, enough consecutive bytes must remain
//! unchanged to preserve the chunk boundary markers. The expected duplicate bytes
//! per chunk is:
//!
//! ```text
//! y(c,θ) = Σ(i=0 to z-m-1) [(1-p)^i · p · (m+i) · (1-c)^(i+m+w)]
//!        + (1-p)^(z-m) · z · (1-c)^(z+w)
//! ```
//!
//! The term `(1-c)^(i+m+w)` models the probability that enough bytes remain unchanged
//! (chunk content `i+m` plus rolling hash window `w`) to produce the same hash.
//!
//! ### 5. Deduplication Ratio Prediction
//!
//! The final deduplication ratio combines savings from duplicate elimination with
//! overhead from metadata:
//!
//! ```text
//! s(n,c,θ) = n · (y/l)           [Total duplicate bytes saved]
//! h(θ) = v · (n/l + 1)            [Metadata overhead in bytes]
//! d(n,c,θ) = s - h                [Net savings (can be negative)]
//! ratio = (n - d) / n             [Final size as fraction of original]
//! ```
//!
//! **Ratio interpretation**:
//! - `< 1.0`: Compression achieved (e.g., 0.6 = 60% of original size, 40% savings)
//! - `= 1.0`: No change (metadata overhead exactly cancels duplicate savings)
//! - `> 1.0`: Expansion (metadata overhead exceeds duplicate savings)
//!
//! # Key Concepts
//!
//! - **Change rate (c)**: Probability that a byte differs from its previous version.
//!   Lower values indicate higher redundancy and better deduplication potential.
//!
//! - **Fingerprint bits (f)**: Controls average chunk size (`2^f` bytes). The primary
//!   tuning parameter for balancing granularity vs. overhead.
//!
//! - **Deduplication ratio**: Final size / Original size. Values < 1.0 indicate
//!   storage savings, > 1.0 indicate expansion due to metadata overhead.
//!
//! - **Metadata overhead (v)**: Per-chunk storage for hash fingerprints and pointers.
//!   Typically 8-16 bytes per chunk.
//!
//! # Parameter Optimization Workflow
//!
//! ## Step 1: Dry-Run Analysis
//!
//! Run CDC on a representative sample of your data to estimate the change rate:
//!
//! ```no_run
//! use hexz_core::algo::dedup::{cdc, dcam::DedupeParams};
//! use std::io::Cursor;
//!
//! // Use baseline parameters for initial analysis
//! let baseline = DedupeParams::lbfs_baseline();
//!
//! // Analyze a representative sample (1-10% of total data is usually sufficient)
//! let sample_data = vec![0u8; 1_000_000]; // 1 MB sample
//! let stats = cdc::analyze_stream(Cursor::new(&sample_data), &baseline)?;
//!
//! println!("Total bytes: {}", stats.total_bytes);
//! println!("Unique bytes: {}", stats.unique_bytes);
//! println!("Duplicate bytes: {}", stats.total_bytes - stats.unique_bytes);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Step 2: Calculate Change Rate
//!
//! Convert the dry-run results into a change rate:
//!
//! ```
//! use hexz_core::algo::dedup::dcam::{DedupeParams, calculate_c};
//!
//! let baseline = DedupeParams::lbfs_baseline();
//! let unique_bytes = 400_000_000;  // From dry run
//! let total_bytes = 1_000_000_000; // From dry run
//!
//! let c = calculate_c(unique_bytes, total_bytes, &baseline);
//! println!("Change rate: {:.4} ({:.1}%)", c, c * 100.0);
//! ```
//!
//! ## Step 3: Evaluate Parameter Candidates
//!
//! Test different parameter sets to find the optimal balance:
//!
//! ```
//! use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio, expected_chunk_length};
//!
//! let file_size = 1_000_000_000; // 1GB
//! let c = 0.15; // From step 2
//!
//! // Define candidates: smaller chunks (f=12), baseline (f=13), larger chunks (f=14, f=15)
//! let candidates = vec![
//!     ("Small (4KB)", DedupeParams { f: 12, ..DedupeParams::lbfs_baseline() }),
//!     ("Baseline (8KB)", DedupeParams::lbfs_baseline()),
//!     ("Medium (16KB)", DedupeParams { f: 14, ..DedupeParams::lbfs_baseline() }),
//!     ("Large (32KB)", DedupeParams { f: 15, ..DedupeParams::lbfs_baseline() }),
//! ];
//!
//! for (name, params) in candidates {
//!     let ratio = predict_ratio(file_size, c, &params);
//!     let avg_chunk = expected_chunk_length(&params);
//!     let savings = (1.0 - ratio) * 100.0;
//!
//!     println!("{}: ratio={:.3} ({:.1}% savings), avg_chunk={:.0} bytes",
//!              name, ratio, savings, avg_chunk);
//! }
//! ```
//!
//! ## Step 4: Select Optimal Parameters
//!
//! Choose parameters based on your priorities:
//!
//! - **Maximum savings**: Choose smallest `ratio` (best compression)
//! - **Balanced**: Choose good savings with reasonable chunk size (8-16KB typical)
//! - **Performance**: Choose larger chunks (16-32KB) if metadata overhead is a concern
//!
//! # Workload-Specific Guidance
//!
//! ## High Redundancy (c < 0.2)
//!
//! Examples: Backups, snapshots, versioned documents
//!
//! - **Recommendation**: Use smaller chunks (f=12 or f=13, ~4-8KB)
//! - **Rationale**: High redundancy means duplicate savings far exceed metadata overhead,
//!   so finer granularity captures more duplicates without penalty
//!
//! ## Moderate Redundancy (0.2 ≤ c ≤ 0.5)
//!
//! Examples: Mixed datasets, partially modified files
//!
//! - **Recommendation**: Use baseline chunks (f=13 or f=14, ~8-16KB)
//! - **Rationale**: Balance between granularity and overhead
//!
//! ## Low Redundancy (c > 0.5)
//!
//! Examples: Compressed data, encrypted data, random data
//!
//! - **Recommendation**: Use larger chunks (f=14 or f=15, ~16-32KB) or skip deduplication
//! - **Rationale**: Limited duplicates mean metadata overhead may exceed savings;
//!   larger chunks minimize overhead
//!
//! # Model Assumptions and Limitations
//!
//! ## Assumptions
//!
//! 1. **Uniform change distribution**: Changes are evenly distributed across the file.
//!    Real data often has localized changes, which can improve deduplication.
//!
//! 2. **Independent chunk boundaries**: FastCDC boundary decisions are independent.
//!    This is a good approximation in practice.
//!
//! 3. **Perfect hash collision resistance**: No false duplicates from hash collisions.
//!    With cryptographic hashes (SHA-256), collision probability is negligible.
//!
//! 4. **Single-version deduplication**: Model assumes comparing against one previous
//!    version. Multi-version deduplication may see higher savings.
//!
//! ## Limitations
//!
//! - **Prediction accuracy**: Typically ±5-10% of actual results. Structured data with
//!   localized changes may deviate more.
//!
//! - **Sample size dependency**: Change rate estimation requires representative samples.
//!   Use at least 1-10% of total data for reliable `c` estimates.
//!
//! - **Does not model**: Compression interactions, filesystem block alignment, or
//!   network transfer costs (DCAM focuses purely on storage size).
//!
//! # Performance Notes
//!
//! - **DCAM computation**: All functions are O(z-m) complexity, typically completing
//!   in microseconds. The summation loops are bounded by max chunk size (~64K iterations).
//!
//! - **Dry-run analysis**: The actual CDC dry-run is O(n) in file size but only needs
//!   to run once on a sample. For a 100MB sample with 8KB chunks, expect ~12,500 chunks
//!   to process, taking milliseconds to seconds depending on I/O.
//!
//! - **Parameter sweep**: Evaluating 10-20 parameter candidates via DCAM is effectively
//!   instantaneous (sub-millisecond total), making it vastly faster than running full
//!   deduplication trials.
//!
//! # Examples
//!
//! ## Complete Optimization Workflow
//!
//! ```
//! use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio, calculate_c};
//!
//! // Step 1: Dry-run results (from cdc::analyze_stream)
//! let file_size = 1_000_000_000; // 1GB
//! let unique_bytes = 500_000_000; // 500MB unique
//!
//! // Step 2: Estimate change rate
//! let params = DedupeParams::lbfs_baseline();
//! let c = calculate_c(unique_bytes, file_size, &params);
//! println!("Change rate: {:.2}%", c * 100.0);
//!
//! // Step 3: Predict dedup ratio for different parameters
//! let ratio = predict_ratio(file_size, c, &params);
//! println!("Baseline (8KB chunks): {:.2}% of original", ratio * 100.0);
//!
//! // Step 4: Try larger chunks (f=15 → 32KB avg)
//! let mut params_large = params;
//! params_large.f = 15;
//! let ratio_large = predict_ratio(file_size, c, &params_large);
//! println!("Large chunks (32KB): {:.2}% of original", ratio_large * 100.0);
//!
//! // Step 5: Choose optimal
//! if ratio < ratio_large {
//!     println!("✓ Baseline parameters are optimal");
//! } else {
//!     println!("✓ Large chunks are optimal");
//! }
//! ```
//!
//! ## Sensitivity Analysis
//!
//! ```
//! use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio};
//!
//! let file_size = 1_000_000_000;
//! let params = DedupeParams::lbfs_baseline();
//!
//! println!("Change Rate vs. Deduplication Ratio:");
//! for c_percent in (0..=100).step_by(10) {
//!     let c = c_percent as f64 / 100.0;
//!     let ratio = predict_ratio(file_size, c, &params);
//!     let savings = (1.0 - ratio) * 100.0;
//!     println!("  c={:.0}%: ratio={:.3} (savings={:.1}%)",
//!              c * 100.0, ratio, savings);
//! }
//! ```
//!
//! # References
//!
//! - Randall et al., "Optimizing Deduplication Parameters via a Change-Estimation
//!   Analytical Model", 2025.
//! - Xia et al., "FastCDC: A Fast and Efficient Content-Defined Chunking Approach
//!   for Data Deduplication", USENIX ATC 2016.
//! - Muthitacharoen et al., "A Low-bandwidth Network File System", SOSP 2001 (LBFS).

/// FastCDC and deduplication parameters.
///
/// This structure encapsulates all parameters needed for Content-Defined Chunking (CDC)
/// and deduplication analysis. The parameters control both the **chunking behavior**
/// (how data is divided into variable-size chunks) and the **analytical prediction**
/// (how DCAM estimates deduplication effectiveness).
///
/// # Parameter Descriptions
///
/// ## f: Fingerprint Bits
///
/// Controls the average chunk size through the boundary detection probability.
///
/// - **Formula**: Average chunk size = `2^f` bytes
/// - **Range**: Typically 12-16 (4KB to 64KB average chunks)
/// - **Default**: 13 (8KB average chunks)
/// - **Effect**: Primary tuning parameter for deduplication granularity
///
/// **Examples**:
/// - `f = 12`: ~4KB chunks (fine granularity, high overhead)
/// - `f = 13`: ~8KB chunks (balanced, LBFS standard)
/// - `f = 14`: ~16KB chunks (coarser, less overhead)
/// - `f = 15`: ~32KB chunks (coarse, minimal overhead)
///
/// ## m: Minimum Chunk Size
///
/// Enforces a lower bound on chunk sizes to prevent excessive metadata overhead
/// from tiny chunks.
///
/// - **Range**: Typically 1KB to 4KB
/// - **Default**: 2048 bytes (2KB)
/// - **Effect**: Prevents pathological cases where very small chunks dominate
///
/// **Guideline**: Set to approximately `2^f / 4` (e.g., for f=13, m=2KB).
///
/// ## z: Maximum Chunk Size
///
/// Enforces an upper bound on chunk sizes to limit worst-case memory usage and
/// ensure reasonable chunk distribution.
///
/// - **Range**: Typically 16KB to 128KB
/// - **Default**: 65536 bytes (64KB)
/// - **Effect**: Caps outlier chunks, improves predictability
///
/// **Guideline**: Set to approximately `2^f * 8` (e.g., for f=13, z=64KB).
///
/// ## w: Rolling Hash Window Size
///
/// The number of bytes in the rolling hash window used for boundary detection.
///
/// - **Range**: Typically 32-64 bytes
/// - **Default**: 48 bytes
/// - **Effect**: Influences hash quality and duplicate detection sensitivity
///
/// **Note**: Larger windows provide better hash distribution but require more
/// unchanged bytes to preserve chunk boundaries during modifications.
///
/// ## v: Metadata Overhead per Chunk
///
/// Storage overhead for each chunk's metadata (hash fingerprint, pointers, etc.).
///
/// - **Range**: Typically 8-20 bytes
/// - **Default**: 8 bytes (64-bit hash + pointer)
/// - **Effect**: Directly impacts the metadata vs. savings tradeoff
///
/// **Typical values**:
/// - 8 bytes: Minimal (64-bit hash reference)
/// - 12 bytes: Moderate (hash + size field)
/// - 16 bytes: Conservative (128-bit hash + metadata)
/// - 20+ bytes: High (includes timestamps, permissions, etc.)
///
/// # Parameter Tradeoffs
///
/// ## Chunk Size (f) Tradeoffs
///
/// ### Smaller Chunks (Lower f, e.g., f=12, ~4KB)
///
/// **Advantages**:
/// - **Finer granularity**: Detects smaller duplicate regions
/// - **Better for small changes**: Modifications affect fewer bytes
/// - **Higher deduplication ratio**: Finds more duplicates in fragmented data
///
/// **Disadvantages**:
/// - **More metadata**: More chunks → more overhead (hash storage, index lookups)
/// - **Slower processing**: More chunk boundaries to compute and hash
/// - **Higher memory usage**: Larger deduplication index
///
/// **Best for**: Highly redundant data with small, scattered changes (backups, snapshots)
///
/// ### Larger Chunks (Higher f, e.g., f=15, ~32KB)
///
/// **Advantages**:
/// - **Less metadata**: Fewer chunks → lower storage overhead
/// - **Faster processing**: Fewer boundaries to detect, fewer hashes to compute
/// - **Lower memory usage**: Smaller deduplication index
///
/// **Disadvantages**:
/// - **Coarser granularity**: Misses small duplicate regions
/// - **Worse for small changes**: Small modifications break entire chunks
/// - **Lower deduplication ratio**: Misses fine-grained duplicates
///
/// **Best for**: Low-redundancy data, performance-critical workloads, or when
/// metadata overhead is a concern
///
/// ## General Tuning Guidelines
///
/// 1. **Start with LBFS baseline** (f=13, m=2KB, z=64KB) - well-tested defaults
/// 2. **High redundancy** (c < 0.2): Try smaller chunks (f=12) for better savings
/// 3. **Low redundancy** (c > 0.5): Try larger chunks (f=14-15) to reduce overhead
/// 4. **Keep m ≈ 2^f / 4** and **z ≈ 2^f * 8** for balanced distribution
/// 5. **Use DCAM** to predict before committing to parameters
///
/// # Examples
///
/// ## Creating Custom Parameters
///
/// ```
/// use hexz_core::algo::dedup::dcam::DedupeParams;
///
/// // High-performance configuration (larger chunks)
/// let high_perf = DedupeParams {
///     f: 15,        // 32KB average chunks
///     m: 8192,      // 8KB minimum
///     z: 262144,    // 256KB maximum
///     w: 48,        // Standard window
///     v: 8,         // Minimal metadata
/// };
///
/// // High-compression configuration (smaller chunks)
/// let high_comp = DedupeParams {
///     f: 12,        // 4KB average chunks
///     m: 1024,      // 1KB minimum
///     z: 32768,     // 32KB maximum
///     w: 48,        // Standard window
///     v: 8,         // Minimal metadata
/// };
/// ```
///
/// ## Comparing Parameter Sets
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
///
/// let baseline = DedupeParams::lbfs_baseline();
/// let custom = DedupeParams { f: 14, ..baseline };
///
/// println!("Baseline avg chunk: {:.0} bytes", expected_chunk_length(&baseline));
/// println!("Custom avg chunk: {:.0} bytes", expected_chunk_length(&custom));
/// ```
///
/// # Performance Characteristics
///
/// ## Chunk Size Impact on Processing Speed
///
/// For a 1GB file:
/// - **f=12 (4KB chunks)**: ~250,000 chunks, ~250K hash computations
/// - **f=13 (8KB chunks)**: ~125,000 chunks, ~125K hash computations
/// - **f=14 (16KB chunks)**: ~62,500 chunks, ~62.5K hash computations
/// - **f=15 (32KB chunks)**: ~31,250 chunks, ~31.25K hash computations
///
/// **Hash computation time** (SHA-256 on modern CPU):
/// - Per chunk: ~1-10 microseconds depending on size
/// - Total for 1GB: ~100ms to 2.5s depending on chunk size
///
/// ## Memory Footprint
///
/// Deduplication index size (assuming 32-byte hash + pointer per unique chunk):
/// - **f=12**: ~250K chunks × 32 bytes = ~8 MB per GB of unique data
/// - **f=13**: ~125K chunks × 32 bytes = ~4 MB per GB of unique data
/// - **f=14**: ~62.5K chunks × 32 bytes = ~2 MB per GB of unique data
/// - **f=15**: ~31.25K chunks × 32 bytes = ~1 MB per GB of unique data
///
/// # Validation
///
/// The parameter set must satisfy:
/// - `m < z` (min < max chunk size)
/// - `f >= 10` (avoid excessively small chunks, < 1KB)
/// - `f <= 20` (avoid excessively large chunks, > 1MB)
/// - `w >= 8` (minimum for hash quality)
///
/// Note: These constraints are not enforced by the type system but are implicit
/// in the DCAM formulas. Invalid parameters will produce nonsensical predictions.
#[derive(Debug, Clone, Copy)]
pub struct DedupeParams {
    /// Fingerprint size in bits ($f$). Controls average chunk size ($2^f$).
    ///
    /// This is the number of bits used in the rolling hash fingerprint to detect
    /// chunk boundaries. A boundary occurs when the hash matches a pattern with
    /// probability `p = 1 / 2^f`.
    ///
    /// **Typical values**: 12-15 (4KB to 32KB average chunks)
    pub f: u32,

    /// Minimum chunk length in bytes ($m$).
    ///
    /// Prevents excessively small chunks that would cause metadata overhead to
    /// dominate. FastCDC will not create chunks smaller than this size.
    ///
    /// **Typical values**: 1024-4096 (1KB to 4KB)
    pub m: u32,

    /// Maximum chunk length in bytes ($z$).
    ///
    /// Caps the largest possible chunk to bound memory usage and ensure reasonable
    /// chunk distribution. FastCDC will force a boundary at this size.
    ///
    /// **Typical values**: 16384-131072 (16KB to 128KB)
    pub z: u32,

    /// Rolling hash window size in bytes ($w$).
    ///
    /// The number of bytes in the sliding window used for the rolling hash function.
    /// Affects both hash quality and the sensitivity of duplicate detection to changes.
    ///
    /// **Typical values**: 32-64 bytes
    pub w: u32,

    /// Per-chunk metadata overhead in bytes ($v$).
    ///
    /// Storage cost for each chunk's metadata (hash fingerprint, reference pointers,
    /// size fields, etc.). Used by DCAM to compute the metadata overhead term.
    ///
    /// **Typical values**: 8-20 bytes
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
    /// These are well-tested parameters originally from the Low-Bandwidth Network File
    /// System (LBFS) and used as the baseline configuration in FastCDC and DCAM research.
    /// They represent a balanced tradeoff suitable for general-purpose deduplication.
    ///
    /// # Parameter Values
    ///
    /// - **f = 13**: 8KB average chunks (`2^13 = 8192` bytes)
    /// - **m = 2048**: 2KB minimum chunk size (prevents tiny chunks)
    /// - **z = 65536**: 64KB maximum chunk size (caps outliers)
    /// - **w = 48**: 48-byte rolling hash window (standard)
    /// - **v = 8**: 8 bytes metadata per chunk (hash + pointer)
    ///
    /// # When to Use
    ///
    /// Use these parameters as a starting point when:
    /// - You have no prior knowledge of your dataset characteristics
    /// - You want a safe, well-tested default configuration
    /// - You need a reference point for comparison with custom parameters
    ///
    /// # Performance Characteristics
    ///
    /// For a 1GB file with LBFS baseline parameters:
    /// - **Expected chunks**: ~125,000 chunks
    /// - **Metadata overhead**: ~1 MB (125K × 8 bytes)
    /// - **Processing time**: ~100-200ms for chunking + hashing (modern CPU)
    /// - **Memory usage**: ~4 MB for deduplication index (32 bytes per unique chunk)
    ///
    /// # Returns
    ///
    /// A `DedupeParams` instance configured with LBFS baseline values, suitable
    /// for general-purpose deduplication across a wide range of workloads.
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
    ///
    /// let params = DedupeParams::lbfs_baseline();
    /// assert_eq!(params.f, 13);
    /// assert_eq!(params.m, 2048);
    ///
    /// let avg_chunk = expected_chunk_length(&params);
    /// println!("Average chunk size: {:.0} bytes", avg_chunk); // ~8192
    /// ```
    ///
    /// # References
    ///
    /// - Muthitacharoen et al., "A Low-bandwidth Network File System", SOSP 2001
    /// - Xia et al., "FastCDC: A Fast and Efficient Content-Defined Chunking
    ///   Approach for Data Deduplication", USENIX ATC 2016
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
    /// In Content-Defined Chunking (CDC), chunk boundaries are detected when the
    /// rolling hash of a byte window matches a specific pattern. With `f` fingerprint
    /// bits, the pattern requires `f` specific bits to be set, giving a boundary
    /// probability of:
    ///
    /// **p = 1 / 2^f**
    ///
    /// This probability determines the **expected chunk size**, which is the inverse
    /// of the boundary probability: `E[chunk_size] = 1/p = 2^f`.
    ///
    /// # Mathematical Interpretation
    ///
    /// The boundary probability `p` models FastCDC's fingerprint matching as a
    /// geometric distribution:
    /// - At each byte position (after min chunk size), there's probability `p`
    ///   of finding a boundary
    /// - The expected number of bytes until the first boundary is `1/p`
    /// - This creates variable-size chunks with exponential distribution centered
    ///   around `2^f`
    ///
    /// # Returns
    ///
    /// Probability (0.0 to 1.0) of a chunk boundary at any given byte position
    /// within the allowed range `[m, z)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::algo::dedup::dcam::DedupeParams;
    ///
    /// let params = DedupeParams::lbfs_baseline(); // f=13
    /// let p = params.p();
    ///
    /// assert!((p - 1.0/8192.0).abs() < 1e-10); // 1/2^13 ≈ 0.000122
    /// println!("Boundary probability: {:.6} ({:.4}%)", p, p * 100.0);
    /// println!("Expected chunk size: {:.0} bytes", 1.0 / p);
    /// ```
    ///
    /// ## Comparing Different Fingerprint Sizes
    ///
    /// ```
    /// use hexz_core::algo::dedup::dcam::DedupeParams;
    ///
    /// for f in 12..=15 {
    ///     let params = DedupeParams { f, ..DedupeParams::lbfs_baseline() };
    ///     let p = params.p();
    ///     println!("f={}: p={:.6}, expected chunk={:.0} bytes",
    ///              f, p, 1.0 / p);
    /// }
    /// // Output:
    /// // f=12: p=0.000244, expected chunk=4096 bytes
    /// // f=13: p=0.000122, expected chunk=8192 bytes
    /// // f=14: p=0.000061, expected chunk=16384 bytes
    /// // f=15: p=0.000031, expected chunk=32768 bytes
    /// ```
    ///
    /// # Performance
    ///
    /// This is a simple arithmetic operation (power and division) with O(1) complexity,
    /// typically completing in nanoseconds.
    ///
    /// # Reference
    ///
    /// Equation 1 in DCAM paper (Randall et al., 2025).
    pub fn p(&self) -> f64 {
        1.0 / 2.0_f64.powi(self.f as i32)
    }
}

/// Calculates the expected (average) chunk length for given parameters.
///
/// This function computes the true expected value of chunk size, accounting for
/// the probability distribution of boundary detection within the constraints of
/// minimum and maximum chunk sizes.
///
/// # Algorithm
///
/// FastCDC generates variable-size chunks through a boundary detection process:
/// 1. After reaching minimum size `m`, check each byte position for a boundary
/// 2. Boundary occurs with probability `p = 1 / 2^f` at each position
/// 3. If no boundary found by maximum size `z`, force a boundary
///
/// This creates a **truncated geometric distribution** where:
/// - Most chunks follow an exponential distribution centered around `2^f`
/// - Minimum size `m` truncates the left tail
/// - Maximum size `z` truncates the right tail
///
/// # Parameters
///
/// - `params`: FastCDC parameters (`f`, `m`, `z`)
///
/// # Returns
///
/// Expected chunk length in bytes (floating-point). For well-chosen parameters
/// (m << 2^f << z), this will be very close to `2^f`. Extreme min/max values
/// will shift the average.
///
/// # Formula
///
/// ```text
/// l(θ) = Σ(i=0 to z-m-1) [(1-p)^i · p · (m+i)] + (1-p)^(z-m) · z
/// ```
///
/// **Components**:
/// - **First term**: Sum over all chunk sizes from `m` to `z-1`
///   - `(1-p)^i`: Probability of no boundary in first `i` bytes after `m`
///   - `p`: Probability of boundary at position `i`
///   - `(m+i)`: Resulting chunk size
/// - **Second term**: Probability mass at maximum chunk size
///   - `(1-p)^(z-m)`: Probability of no boundary before `z`
///   - `z`: Forced chunk size
///
/// # Implementation Notes
///
/// The summation iterates from `i=0` to `i=z-m-1`, which for typical parameters
/// (z=64KB, m=2KB) means ~62,000 iterations. Despite this, the function remains
/// fast (microseconds) because:
/// - Simple floating-point arithmetic per iteration
/// - No I/O or memory allocation
/// - Can be optimized by compiler (tight loop, no branches)
///
/// For very large `z-m` values, consider caching the result or using closed-form
/// approximations (though the current implementation is sufficient for typical use).
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
///
/// let params = DedupeParams::lbfs_baseline(); // f=13, m=2KB, z=64KB
/// let avg_len = expected_chunk_length(&params);
///
/// println!("Average chunk: {:.0} bytes", avg_len); // ~8192
/// assert!((avg_len - 8192.0).abs() < 2500.0); // Within range of 2^13
/// ```
///
/// ## Comparing Different Fingerprint Sizes
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
///
/// let baseline = DedupeParams::lbfs_baseline();
///
/// for f in 12..=15 {
///     let params = DedupeParams { f, ..baseline };
///     let avg = expected_chunk_length(&params);
///     let ideal = 2.0_f64.powi(f as i32);
///     println!("f={}: avg={:.0}B, ideal={:.0}B, diff={:.1}%",
///              f, avg, ideal, (avg - ideal) / ideal * 100.0);
/// }
/// ```
///
/// ## Effect of Min/Max Constraints
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_chunk_length};
///
/// let base = DedupeParams::lbfs_baseline();
///
/// // Tight constraints (shift average)
/// let tight = DedupeParams { m: 4096, z: 16384, ..base };
/// let tight_avg = expected_chunk_length(&tight);
///
/// // Loose constraints (closer to ideal)
/// let loose = DedupeParams { m: 1024, z: 131072, ..base };
/// let loose_avg = expected_chunk_length(&loose);
///
/// println!("Tight constraints: {:.0}B", tight_avg);
/// println!("Loose constraints: {:.0}B", loose_avg);
/// println!("Ideal (2^13): 8192B");
/// ```
///
/// # Performance
///
/// - **Time complexity**: O(z - m) - linear in the range of chunk sizes
/// - **Space complexity**: O(1) - constant memory usage
/// - **Typical performance**: < 10 microseconds for standard parameters
///
/// For typical parameters (z=64KB, m=2KB), this involves ~62,000 iterations
/// of simple floating-point arithmetic. Modern CPUs process this in microseconds.
///
/// # Accuracy
///
/// The result is numerically accurate to within floating-point precision (±1e-15).
/// For practical deduplication, the expected chunk length is accurate enough for
/// ratio predictions (typical error < 0.1%).
///
/// # Reference
///
/// Equation 3 in DCAM paper (Randall et al., 2025).
pub fn expected_chunk_length(params: &DedupeParams) -> f64 {
    assert!(
        params.m < params.z,
        "DedupeParams: min chunk size (m={}) must be less than max chunk size (z={})",
        params.m,
        params.z
    );

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
/// This function models the probability that a chunk will be recognized as a
/// duplicate, weighted by chunk size. It accounts for the fact that for a chunk
/// to be detected as a duplicate, not only must the content be similar, but the
/// chunk boundaries must also be preserved (which requires the rolling hash
/// window to remain unchanged).
///
/// # The Duplicate Detection Model
///
/// For a chunk to be detected as a duplicate:
/// 1. The chunk content must be sufficiently unchanged
/// 2. The **rolling hash window** must also be unchanged (to preserve boundaries)
/// 3. Both conditions together determine if the chunk hash matches
///
/// The probability that a region of length `L` remains unchanged is `(1-c)^L`,
/// where `c` is the change rate. For a chunk of size `s` with window size `w`,
/// the total region that must be unchanged is `s + w` bytes.
///
/// # Parameters
///
/// - `c`: Change rate (probability that a byte differs from previous version),
///   range [0.0, 1.0]. Lower values indicate higher redundancy.
/// - `params`: FastCDC parameters (`f`, `m`, `z`, `w`)
///
/// # Returns
///
/// Expected duplicate bytes per chunk (floating-point). This represents the
/// average number of bytes saved per chunk through deduplication.
///
/// **Interpretation**:
/// - **High value** (close to average chunk size): Excellent deduplication
/// - **Medium value** (50% of chunk size): Moderate deduplication
/// - **Low value** (near zero): Poor deduplication
///
/// # Formula
///
/// ```text
/// y(c,θ) = Σ(i=0 to z-m-1) [(1-p)^i · p · (m+i) · (1-c)^(i+m+w)]
///        + (1-p)^(z-m) · z · (1-c)^(z+w)
/// ```
///
/// **Components**:
/// - `(1-p)^i · p`: Probability of a chunk of size `m+i` (from geometric distribution)
/// - `(m+i)`: Chunk size in bytes
/// - `(1-c)^(i+m+w)`: Probability that the chunk content plus window remains unchanged
///   - `i+m`: Chunk size
///   - `w`: Rolling hash window size
///
/// The term `(1-c)^(i+m+w)` is the **key insight** of DCAM: duplicate detection
/// requires not just unchanged content but also unchanged boundary markers (the
/// rolling hash window).
///
/// # Mathematical Intuition
///
/// ## Why Window Size Matters
///
/// Consider a chunk of size 8KB (8192 bytes) with window size 48:
/// - **Change rate c=0.01** (1% changes):
///   - Probability chunk unchanged: `(1-0.01)^(8192+48) ≈ 7.4e-36`
///   - This seems impossibly low, but remember this is per-chunk probability
///   - Over thousands of chunks, duplicates are still found
///
/// - **Change rate c=0.001** (0.1% changes):
///   - Probability: `(1-0.001)^(8240) ≈ 2.6e-4`
///   - Much higher probability of preservation
///
/// ## Effect of Chunk Size
///
/// Larger chunks require more bytes to remain unchanged:
/// - **4KB chunk**: `(1-c)^4096` chance (smaller exponent, higher probability)
/// - **32KB chunk**: `(1-c)^32768` chance (larger exponent, lower probability)
///
/// This is why smaller chunks can achieve better deduplication in modified data.
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
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_duplicate_bytes, expected_chunk_length};
///
/// let params = DedupeParams::lbfs_baseline();
///
/// // Low change rate → high duplication
/// let c_low = 0.01; // 1% changes
/// let dup_low = expected_duplicate_bytes(c_low, &params);
/// let avg_chunk = expected_chunk_length(&params);
/// println!("1% changes: {:.0} / {:.0} bytes duplicated per chunk ({:.1}%)",
///          dup_low, avg_chunk, dup_low / avg_chunk * 100.0);
///
/// // High change rate → low duplication
/// let c_high = 0.50; // 50% changes
/// let dup_high = expected_duplicate_bytes(c_high, &params);
/// println!("50% changes: {:.0} / {:.0} bytes duplicated per chunk ({:.1}%)",
///          dup_high, avg_chunk, dup_high / avg_chunk * 100.0);
/// ```
///
/// ## Change Rate Sensitivity Analysis
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_duplicate_bytes, expected_chunk_length};
///
/// let params = DedupeParams::lbfs_baseline();
/// let avg_chunk = expected_chunk_length(&params);
///
/// println!("Change Rate vs. Duplicate Detection:");
/// for c_percent in (0..=50).step_by(5) {
///     let c = c_percent as f64 / 100.0;
///     let dup = expected_duplicate_bytes(c, &params);
///     let dup_pct = (dup / avg_chunk) * 100.0;
///     println!("  c={:3}%: {:.1}% of chunks detected as duplicates", c_percent, dup_pct);
/// }
/// ```
///
/// ## Comparing Chunk Sizes
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, expected_duplicate_bytes, expected_chunk_length};
///
/// let c = 0.10; // 10% change rate
///
/// for f in 12..=15 {
///     let params = DedupeParams { f, ..DedupeParams::lbfs_baseline() };
///     let dup = expected_duplicate_bytes(c, &params);
///     let avg = expected_chunk_length(&params);
///     println!("f={} ({:.0}KB chunks): {:.1}% duplicate detection rate",
///              f, avg / 1024.0, dup / avg * 100.0);
/// }
/// // Shows: Smaller chunks have higher duplicate detection at fixed change rate
/// ```
///
/// # Performance
///
/// - **Time complexity**: O(z - m) - linear in chunk size range
/// - **Space complexity**: O(1) - constant memory
/// - **Typical performance**: < 10 microseconds for standard parameters
///
/// Similar to `expected_chunk_length`, this involves ~62,000 iterations for
/// typical parameters, each with a few floating-point operations.
///
/// # Edge Cases
///
/// - **c = 0.0** (no changes): Returns approximately `expected_chunk_length`
///   (all chunks are duplicates)
/// - **c = 1.0** (all changes): Returns ~0.0 (no chunks are duplicates)
/// - **Very small c** (< 0.01): High duplicate detection, deduplication very effective
/// - **Very large c** (> 0.8): Poor duplicate detection, deduplication ineffective
///
/// # Reference
///
/// Equation 9 in DCAM paper (Randall et al., 2025).
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
/// This is the **main DCAM prediction function** that ties together the entire
/// analytical model. It estimates the final storage size as a ratio of the
/// original size, accounting for both:
/// 1. **Storage savings** from eliminating duplicate chunks
/// 2. **Storage overhead** from metadata (hashes, pointers, indices)
///
/// The function answers the critical question: "If I use these CDC parameters,
/// what will my final storage size be?"
///
/// # The Prediction Model
///
/// DCAM predicts storage size through a multi-step calculation:
///
/// ## Step 1: Estimate Number of Chunks
///
/// ```text
/// num_chunks = n / l(θ) + 1
/// ```
///
/// Where `n` is file size and `l(θ)` is expected chunk length.
///
/// ## Step 2: Calculate Duplicate Bytes Saved
///
/// ```text
/// s(n,c,θ) = n · (y(c,θ) / l(θ))
/// ```
///
/// Where:
/// - `y(c,θ)`: Expected duplicate bytes per chunk (from `expected_duplicate_bytes`)
/// - `l(θ)`: Expected chunk length
/// - `s`: Total duplicate bytes that can be eliminated
///
/// ## Step 3: Calculate Metadata Overhead
///
/// ```text
/// h(θ) = v · (n/l + 1)
/// ```
///
/// Where:
/// - `v`: Per-chunk metadata size (hash + pointer)
/// - `n/l + 1`: Number of chunks
/// - `h`: Total metadata storage overhead
///
/// ## Step 4: Compute Net Savings
///
/// ```text
/// d(n,c,θ) = s - h
/// ```
///
/// Net savings = Duplicate bytes eliminated - Metadata overhead.
/// This can be **negative** if overhead exceeds savings!
///
/// ## Step 5: Calculate Final Ratio
///
/// ```text
/// ratio = (n - d) / n = (n - s + h) / n
/// ```
///
/// Final storage size as a fraction of original size.
///
/// # Parameters
///
/// - `file_size`: Original file/dataset size in bytes. Use the total size of
///   the data to be deduplicated, not a sample size.
///
/// - `c`: Change rate (probability a byte differs), range [0.0, 1.0]. Estimate
///   this using `calculate_c()` from a dry-run CDC analysis.
///
/// - `params`: FastCDC parameters to evaluate. Try multiple parameter sets to
///   find the optimal configuration.
///
/// # Returns
///
/// Deduplication ratio (final size / original size) as a floating-point value.
///
/// **Interpretation**:
/// - **< 1.0**: Compression achieved
///   - `0.5` = 50% of original size (50% savings)
///   - `0.3` = 30% of original size (70% savings)
/// - **= 1.0**: No change (metadata overhead exactly cancels savings)
/// - **> 1.0**: Storage expansion (metadata overhead exceeds savings)
///   - `1.1` = 110% of original size (10% expansion)
///
/// # Model Assumptions
///
/// The prediction assumes:
///
/// 1. **Uniform change distribution**: Changes are evenly distributed across
///    the file. Real data often has **localized changes** which can improve
///    deduplication beyond predictions.
///
/// 2. **Independent chunk boundaries**: FastCDC boundary decisions are independent.
///    This is empirically accurate for well-designed rolling hash functions.
///
/// 3. **Perfect hash collision resistance**: No false positives/negatives from
///    hash collisions. With SHA-256, collision probability is negligible
///    (<< 2^-128 for realistic dataset sizes).
///
/// 4. **Single-version deduplication**: Model assumes comparing against one
///    previous version. Multi-version or cross-dataset deduplication may achieve
///    higher savings.
///
/// 5. **No compression interaction**: Model doesn't account for combining
///    deduplication with compression (typically applied separately).
///
/// # Accuracy and Validation
///
/// ## Expected Accuracy
///
/// - **Typical error**: ±5-10% of actual results for general workloads
/// - **Best case**: ±2-5% for uniformly modified data (matches assumptions)
/// - **Worst case**: ±10-20% for highly structured or localized changes
///
/// ## Validation Methodology
///
/// To validate predictions:
/// 1. Run `predict_ratio()` with estimated change rate
/// 2. Run actual deduplication on a representative sample
/// 3. Compare predicted vs. actual ratios
/// 4. If error > 10%, consider:
///    - Larger sample for `c` estimation
///    - Non-uniform change patterns
///    - Interaction with compression or filesystem effects
///
/// # Examples
///
/// ## Basic Prediction
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio, calculate_c};
///
/// let file_size = 1_000_000_000; // 1GB
/// let unique_bytes = 400_000_000; // 400MB unique (from dry run)
///
/// let params = DedupeParams::lbfs_baseline();
/// let c = calculate_c(unique_bytes, file_size, &params);
/// let ratio = predict_ratio(file_size, c, &params);
///
/// println!("Predicted ratio: {:.3}", ratio);
/// println!("Predicted size: {:.0} MB", file_size as f64 * ratio / 1e6);
/// println!("Expected savings: {:.1}%", (1.0 - ratio) * 100.0);
/// ```
///
/// ## Parameter Optimization
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio, expected_chunk_length};
///
/// let file_size = 1_000_000_000;
/// let c = 0.15; // From dry-run analysis
///
/// println!("Parameter Optimization Results:");
/// println!("{:<15} {:<12} {:<12} {:<15}", "Config", "Avg Chunk", "Ratio", "Savings");
/// println!("{:-<54}", "");
///
/// for f in 12..=15 {
///     let params = DedupeParams { f, ..DedupeParams::lbfs_baseline() };
///     let ratio = predict_ratio(file_size, c, &params);
///     let avg_chunk = expected_chunk_length(&params);
///     let savings = (1.0 - ratio) * 100.0;
///
///     println!("{:<15} {:<12.0} {:<12.3} {:<15.1}%",
///              format!("f={} (2^{})", f, f), avg_chunk, ratio, savings);
/// }
///
/// // Choose parameters with best ratio (lowest value)
/// ```
///
/// ## Breakeven Analysis
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio};
///
/// let file_size = 1_000_000_000;
/// let params = DedupeParams::lbfs_baseline();
///
/// // Find change rate where deduplication breaks even (ratio = 1.0)
/// println!("Change Rate Breakeven Analysis:");
/// for c_percent in (0..=100).step_by(10) {
///     let c = c_percent as f64 / 100.0;
///     let ratio = predict_ratio(file_size, c, &params);
///
///     if ratio < 1.0 {
///         println!("  c={:3}%: Dedup beneficial (ratio={:.3})", c_percent, ratio);
///     } else if ratio > 1.0 {
///         println!("  c={:3}%: Dedup harmful (ratio={:.3})", c_percent, ratio);
///     } else {
///         println!("  c={:3}%: Breakeven (ratio={:.3})", c_percent, ratio);
///     }
/// }
/// ```
///
/// ## Sensitivity to Metadata Overhead
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio};
///
/// let file_size = 1_000_000_000;
/// let c = 0.20;
/// let base = DedupeParams::lbfs_baseline();
///
/// println!("Metadata Overhead Sensitivity:");
/// for v in [4, 8, 12, 16, 20, 32] {
///     let params = DedupeParams { v, ..base };
///     let ratio = predict_ratio(file_size, c, &params);
///     println!("  v={}B: ratio={:.3} ({:.1}% savings)",
///              v, ratio, (1.0 - ratio) * 100.0);
/// }
/// // Shows: Higher metadata overhead reduces savings
/// ```
///
/// # Performance
///
/// - **Time complexity**: O(z - m) due to calls to `expected_chunk_length` and
///   `expected_duplicate_bytes`. For typical parameters, this is ~62,000 iterations.
/// - **Space complexity**: O(1) - constant memory usage
/// - **Typical performance**: < 20 microseconds per prediction
///
/// This makes DCAM suitable for **rapid parameter sweeps**: evaluating 100
/// parameter combinations takes ~2 milliseconds, far faster than running actual
/// deduplication trials.
///
/// # Edge Cases and Special Behavior
///
/// ## When Ratio > 1.0 (Expansion)
///
/// If metadata overhead exceeds duplicate savings, the function returns a ratio
/// > 1.0, indicating that deduplication would **increase** storage size. This
/// > can occur with:
/// - Very high change rates (c > 0.8)
/// - Very small chunks (high f, excessive metadata)
/// - Very high metadata overhead (large v)
///
/// **Recommendation**: If predicted ratio > 1.0, skip deduplication for this dataset.
///
/// ## Numerical Stability
///
/// For extreme parameters (e.g., f > 20, causing overflow in `2^f`), numerical
/// instability may occur. The function includes a clamping mechanism to prevent
/// negative size predictions, returning a conservative estimate in edge cases.
///
/// # Reference
///
/// Derived from Equations 5, 10, 11, and 14 in DCAM paper (Randall et al., 2025).
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
/// This function performs the critical **reverse-engineering step** in the DCAM
/// workflow: converting dry-run deduplication results into a change rate `c` that
/// can be used for parameter optimization.
///
/// The change rate `c` quantifies how "different" the dataset is from itself (or
/// from a previous version), which is the fundamental driver of deduplication
/// effectiveness. Once `c` is known, you can use `predict_ratio()` to test many
/// parameter configurations without re-running expensive deduplication.
///
/// # The Change Rate Concept
///
/// The **change rate** `c` represents the probability that any given byte differs
/// from its corresponding byte in a previous version (or duplicate region). It is
/// a single number that characterizes dataset redundancy:
///
/// - **c = 0.0**: No changes (perfect redundancy, 100% duplicates)
/// - **c = 0.1**: 10% of bytes changed (high redundancy, excellent dedup)
/// - **c = 0.5**: 50% of bytes changed (moderate redundancy, moderate dedup)
/// - **c = 1.0**: All bytes unique (no redundancy, no dedup benefit)
///
/// # Algorithm
///
/// The function inverts the DCAM deduplication model to solve for `c`:
///
/// ## Forward Model (what DCAM predicts)
///
/// Given `c` and `θ` (parameters), predict `NDB` (non-duplicate bytes):
/// ```text
/// NDB = n - s(n, c, θ) = n - n·(y(c,θ) / l(θ))
/// ```
///
/// ## Inverse Model (what this function computes)
///
/// Given `NDB` from a dry run, estimate `c`:
/// ```text
/// c ≈ NDB / (n · l(θ'))
/// ```
///
/// Where `θ'` are the parameters used during the dry run.
///
/// **Intuition**: The ratio of unique bytes to total bytes, normalized by expected
/// chunk length, approximates the change rate. Higher `NDB` → higher `c` → less
/// redundancy.
///
/// # Parameters
///
/// - `ndb`: Non-duplicate bytes (unique bytes after deduplication). Obtained from
///   a dry-run CDC analysis using `cdc::analyze_stream()` or similar.
///
/// - `file_size`: Original file/dataset size in bytes. Must be the same size as
///   used in the dry-run analysis.
///
/// - `params`: Parameters used during the dry-run analysis. **Important**: Use the
///   same parameters that were used for the dry run, not the parameters you plan
///   to evaluate.
///
/// # Returns
///
/// Change rate `c` (0.0 to 1.0), clamped to ensure validity. The result is:
/// - Clamped to `[0.0, 1.0]` range (probabilities must be valid)
/// - Returns `1.0` if `ndb >= file_size` (all bytes unique)
/// - Returns `0.0` if `ndb` is very small (nearly perfect deduplication)
///
/// # Dry-Run Workflow
///
/// ## Step 1: Run CDC Analysis
///
/// ```no_run
/// use hexz_core::algo::dedup::{cdc, dcam::DedupeParams};
/// use std::io::Cursor;
///
/// let baseline = DedupeParams::lbfs_baseline();
/// let sample_data = vec![0u8; 1_000_000]; // 1 MB sample
///
/// let stats = cdc::analyze_stream(Cursor::new(&sample_data), &baseline)?;
/// println!("Total: {} bytes", stats.total_bytes);
/// println!("Unique: {} bytes", stats.unique_bytes);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ## Step 2: Calculate Change Rate
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, calculate_c};
///
/// let file_size = 1_000_000_000;  // 1GB
/// let unique_bytes = 300_000_000; // 300MB unique (from step 1)
/// let params = DedupeParams::lbfs_baseline();
///
/// let c = calculate_c(unique_bytes, file_size, &params);
/// println!("Estimated change rate: {:.4} ({:.2}%)", c, c * 100.0);
/// ```
///
/// ## Step 3: Use for Optimization
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, predict_ratio};
///
/// let c = 0.15; // From step 2
/// let file_size = 1_000_000_000;
///
/// // Try different parameters
/// for f in 12..=15 {
///     let params = DedupeParams { f, ..DedupeParams::lbfs_baseline() };
///     let ratio = predict_ratio(file_size, c, &params);
///     println!("f={}: ratio={:.3}", f, ratio);
/// }
/// ```
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, calculate_c};
///
/// let file_size = 1_000_000_000;  // 1GB
/// let unique_bytes = 300_000_000; // 300MB unique
/// let params = DedupeParams::lbfs_baseline();
///
/// let c = calculate_c(unique_bytes, file_size, &params);
/// println!("Change rate: {:.2}%", c * 100.0);
///
/// // Interpret the result
/// if c < 0.2 {
///     println!("✓ Excellent dedup potential (< 20% changes)");
///     println!("  Recommendation: Use small chunks (f=12-13)");
/// } else if c < 0.5 {
///     println!("✓ Good dedup potential (20-50% changes)");
///     println!("  Recommendation: Use baseline chunks (f=13-14)");
/// } else {
///     println!("⚠ Limited dedup potential (> 50% changes)");
///     println!("  Recommendation: Use large chunks (f=14-15) or skip dedup");
/// }
/// ```
///
/// ## Sample Size Considerations
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, calculate_c};
///
/// let full_size: u64 = 10_000_000_000; // 10GB total
/// let params = DedupeParams::lbfs_baseline();
///
/// // Different sample sizes
/// let samples = vec![
///     ("1%",   100_000_000,  30_000_000),   // 100MB sample, 30MB unique
///     ("5%",   500_000_000,  150_000_000),  // 500MB sample, 150MB unique
///     ("10%",  1_000_000_000, 300_000_000), // 1GB sample, 300MB unique
/// ];
///
/// println!("Change Rate Estimates by Sample Size:");
/// for (label, sample_size, unique) in samples {
///     let c = calculate_c(unique, sample_size, &params);
///     println!("  {}: c={:.4}", label, c);
/// }
/// // Larger samples give more stable estimates
/// ```
///
/// ## Validating Estimates
///
/// ```
/// use hexz_core::algo::dedup::dcam::{DedupeParams, calculate_c, predict_ratio};
///
/// let file_size = 1_000_000_000;
/// let unique_bytes = 400_000_000; // From dry run
/// let params = DedupeParams::lbfs_baseline();
///
/// // Calculate change rate
/// let c = calculate_c(unique_bytes, file_size, &params);
///
/// // Predict ratio using same parameters
/// let predicted_ratio = predict_ratio(file_size, c, &params);
///
/// // Compare to actual result
/// let actual_ratio = unique_bytes as f64 / file_size as f64;
/// let error = (predicted_ratio - actual_ratio).abs() / actual_ratio * 100.0;
///
/// println!("Change rate: {:.4}", c);
/// println!("Predicted ratio: {:.3}", predicted_ratio);
/// println!("Actual ratio: {:.3}", actual_ratio);
/// println!("Error: {:.1}%", error);
///
/// if error < 5.0 {
///     println!("✓ Excellent model fit");
/// } else if error < 10.0 {
///     println!("✓ Good model fit");
/// } else {
///     println!("⚠ Poor model fit - consider larger sample or check for non-uniform changes");
/// }
/// ```
///
/// # Sampling Guidelines
///
/// ## Recommended Sample Size
///
/// For reliable change rate estimation:
/// - **Minimum**: 1% of total data (10MB for 1GB, 100MB for 10GB)
/// - **Recommended**: 5-10% of total data
/// - **Diminishing returns**: Beyond 10%, accuracy improvements are minimal
///
/// ## Sample Selection
///
/// For best results:
/// 1. **Random sampling**: If possible, sample random blocks throughout the dataset
/// 2. **Representative data**: Ensure sample includes typical data patterns
/// 3. **Multiple samples**: For critical applications, try 2-3 independent samples
///    and average the change rates
///
/// ## When Estimates May Be Inaccurate
///
/// - **Highly localized changes**: If changes are clustered (e.g., only first 10%
///   of file changed), sample may not be representative
/// - **Structured data**: Files with repeating structures may have non-uniform
///   redundancy patterns
/// - **Tiny samples**: < 1% samples can have high variance
///
/// # Performance
///
/// - **Time complexity**: O(z - m) due to `expected_chunk_length` call
/// - **Space complexity**: O(1)
/// - **Typical performance**: < 10 microseconds
///
/// This function is extremely fast; the bottleneck is always the dry-run CDC
/// analysis, not the change rate calculation.
///
/// # Edge Cases
///
/// ## ndb >= file_size (All Unique)
///
/// Returns `c = 1.0`, indicating no redundancy. This can occur with:
/// - Compressed or encrypted data
/// - Random data
/// - First snapshot (no previous version to deduplicate against)
///
/// ## ndb ≈ 0 (Perfect Deduplication)
///
/// Returns `c ≈ 0.0`, indicating nearly perfect redundancy. This is rare but
/// can occur with:
/// - Exact copies of files
/// - Completely redundant datasets
/// - Synthetic test data with intentional duplication
///
/// ## Invalid Parameters
///
/// If `file_size = 0` or `params` produce `l(θ) = 0`, the function will return
/// an undefined result. Ensure valid inputs before calling.
///
/// # Reference
///
/// Equation 6 in DCAM paper (Randall et al., 2025).
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
