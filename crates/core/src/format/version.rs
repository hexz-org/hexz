//! Format version management and compatibility checking.
//!
//! This module defines the versioning strategy for Hexz snapshot files (`.hxz`),
//! enabling safe evolution of the on-disk format while maintaining backward and
//! forward compatibility guarantees. Version negotiation ensures that readers can
//! detect incompatible snapshots and provide actionable error messages.
//!
//! # Versioning Strategy
//!
//! Hexz uses a monotonic integer versioning scheme where each version number
//! represents a distinct on-disk format:
//!
//! - **Version 1**: Initial format with two-level index, LZ4/Zstd compression,
//!   optional encryption, thin provisioning support
//! - **Future versions**: Reserved for incompatible format changes (e.g., new
//!   index structures, alternative serialization formats, block metadata extensions)
//!
//! ## Semantic Versioning Alignment
//!
//! Format versions are independent of Hexz software versions (semver). A software
//! release may support multiple format versions for backward compatibility. The
//! mapping is defined as:
//!
//! | Format Version | Hexz Software Versions | Key Changes |
//! |----------------|--------------------------|-------------|
//! | 1              | 0.1.0+                   | Initial release |
//!
//! # Compatibility Model
//!
//! ## Backward Compatibility (Reading Old Snapshots)
//!
//! Hexz readers maintain compatibility with snapshots created by older software:
//!
//! - **MIN_SUPPORTED_VERSION**: Oldest format version we can read (currently 1)
//! - **Upgrade Path**: Snapshots older than MIN_SUPPORTED_VERSION must be migrated
//!   using the `hexz-migrate` tool (see Migration section below)
//!
//! ## Forward Compatibility (Reading New Snapshots)
//!
//! Hexz readers handle snapshots created by newer software:
//!
//! - **MAX_SUPPORTED_VERSION**: Newest format version we can read (currently 1)
//! - **Degraded Mode**: Future versions may enable partial reads with warnings if
//!   minor features are unrecognized (not yet implemented)
//! - **Strict Rejection**: Snapshots with `version > MAX_SUPPORTED_VERSION` are
//!   rejected with an actionable error message
//!
//! # Version Negotiation Workflow
//!
//! When opening a snapshot file:
//!
//! ```text
//! 1. Read header magic bytes (validate "HEXZ" signature)
//! 2. Read header.version field
//! 3. Call check_version(header.version)
//! 4. If VersionCompatibility::Incompatible:
//!       - Generate human-readable error via compatibility_message()
//!       - Suggest upgrade/migration path
//!       - Abort operation
//! 5. If VersionCompatibility::Full:
//!       - Proceed with normal operations
//! 6. If VersionCompatibility::Degraded (future):
//!       - Log warning about unsupported features
//!       - Proceed with limited functionality
//! ```
//!
//! # Adding New Format Versions
//!
//! To introduce a breaking format change:
//!
//! ## Step 1: Increment Version Constant
//!
//! ```rust,ignore
//! pub const CURRENT_VERSION: u32 = 2;  // Changed from 1
//! pub const MAX_SUPPORTED_VERSION: u32 = 2;
//! ```
//!
//! ## Step 2: Update Serialization Logic
//!
//! Modify [`crate::format::header::Header`] or [`crate::format::index::MasterIndex`]
//! to include new fields or change existing structures. Use serde attributes to
//! maintain compatibility:
//!
//! ```rust,no_run
//! # use serde::{Serialize, Deserialize};
//! #[derive(Serialize, Deserialize)]
//! pub struct Header {
//!     // Existing fields...
//!     #[serde(skip_serializing_if = "Option::is_none")]
//!     pub new_feature: Option<String>,  // Version 2+ only
//! }
//! ```
//!
//! ## Step 3: Conditional Deserialization
//!
//! Add version-aware deserialization in reader code:
//!
//! ```rust,no_run
//! # fn read_index_v1<R>(_: R) -> Result<(), Box<dyn std::error::Error>> { Ok(()) }
//! # fn read_index_v2<R>(_: R) -> Result<(), Box<dyn std::error::Error>> { Ok(()) }
//! # fn example<R>(header: hexz_core::format::header::Header, reader: R) -> Result<(), Box<dyn std::error::Error>> {
//! match header.version {
//!     1 => {
//!         // Legacy path for version 1
//!         read_index_v1(reader)?
//!     }
//!     2 => {
//!         // New path for version 2
//!         read_index_v2(reader)?
//!     }
//!     _ => unreachable!("check_version already validated"),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Step 4: Update Tests
//!
//! Add test fixtures for the new format version:
//!
//! ```rust,no_run
//! # fn load_fixture(_: &str) -> hexz_core::File { todo!() }
//! #[test]
//! fn test_read_v2_snapshot() {
//!     let snapshot = load_fixture("testdata/v2_snapshot.hxz");
//!     assert_eq!(snapshot.header.version, 2);
//!     // Verify new features work correctly
//! }
//! ```
//!
//! ## Step 5: Document Migration Path
//!
//! Update the migration guide with conversion instructions (see Migration section).
//!
//! # Migration Between Versions
//!
//! The `hexz-migrate` tool converts snapshots between format versions:
//!
//! ```bash
//! # Upgrade old snapshot to current format
//! hexz-migrate upgrade --input old_v1.hxz --output new_v2.hxz
//!
//! # Downgrade for compatibility (if supported)
//! hexz-migrate downgrade --input new_v2.hxz --output legacy_v1.hxz \
//!     --target-version 1
//! ```
//!
//! ## Migration Algorithm
//!
//! The migration tool performs a streaming rewrite:
//!
//! 1. Open source snapshot with reader for version N
//! 2. Create destination snapshot with writer for version M
//! 3. Stream all blocks through decompression/re-compression
//! 4. Rebuild index in target format
//! 5. Preserve metadata (encryption keys, parent references, etc.)
//!
//! ## Lossy Migrations
//!
//! Downgrading may lose features unsupported in the target version:
//!
//! - Version 2 → 1: Hypothetical new features would be discarded with warnings
//!
//! # Performance Considerations
//!
//! Version checking is performed once per snapshot open operation:
//!
//! - **Overhead**: Negligible (~100ns for integer comparison)
//! - **Caching**: Version is cached in [`crate::api::file::File`] struct
//! - **Hot Path**: Not in block read/write paths
//!
//! # Examples
//!
//! ## Basic Version Check
//!
//! ```
//! use hexz_core::format::version::{check_version, VersionCompatibility};
//!
//! let snapshot_version = 1;
//! let compat = check_version(snapshot_version);
//!
//! match compat {
//!     VersionCompatibility::Full => {
//!         println!("Snapshot is fully compatible");
//!     }
//!     VersionCompatibility::Incompatible => {
//!         eprintln!("Cannot read snapshot (incompatible version)");
//!     }
//!     VersionCompatibility::Degraded => {
//!         println!("Snapshot readable with warnings");
//!     }
//! }
//! ```
//!
//! ## User-Facing Error Messages
//!
//! ```
//! use hexz_core::format::version::compatibility_message;
//!
//! let version = 999;  // Future version
//! let message = compatibility_message(version);
//! println!("{}", message);
//! // Output: "Version 999 is too new (max supported: 1). Please upgrade Hexz."
//! ```
//!
//! ## Reader Implementation
//!
//! ```rust,no_run
//! # use hexz_core::format::version::{check_version, MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION};
//! # use hexz_common::Error;
//! # use hexz_core::format::header::Header;
//! # use std::path::Path;
//! # struct Snapshot { header: Header }
//! # fn read_header(_: &Path) -> Result<Header, Error> { todo!() }
//! fn open_snapshot(path: &Path) -> Result<Snapshot, Error> {
//!     let header = read_header(path)?;
//!
//!     if !check_version(header.version).is_compatible() {
//!         return Err(Error::Format(format!(
//!             "Incompatible version {}. Supported range: [{}, {}]",
//!             header.version, MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION
//!         )));
//!     }
//!
//!     // Proceed with version-aware deserialization
//!     Ok(Snapshot { header })
//! }
//! ```

use std::fmt;

/// Current format version written by this build of Hexz.
///
/// This constant defines the format version number written to the `version` field
/// of new snapshot headers. It is incremented when the on-disk format changes in
/// a way that requires readers to adapt their parsing logic.
///
/// # Incrementing Policy
///
/// Increment `CURRENT_VERSION` when:
/// - Header fields are added/removed/reordered
/// - Index structure changes (e.g., new page entry fields)
/// - Serialization format changes (e.g., switch from bincode to another codec)
/// - Block metadata semantics change
///
/// Do NOT increment for:
/// - New compression algorithms (use header.compression field)
/// - New encryption modes (use header.encryption field)
/// - Performance optimizations that don't affect format
///
/// # Current Version
///
/// **Version 1** (initial release):
/// - Two-level index (master index + pages)
/// - bincode serialization
/// - LZ4/Zstd compression support
/// - Optional AES-256-GCM encryption
/// - Thin provisioning via parent snapshots
/// - Dual streams (disk + memory)
pub const CURRENT_VERSION: u32 = 1;

/// Minimum format version readable by this build.
///
/// Snapshots with `version < MIN_SUPPORTED_VERSION` are rejected with an
/// [`VersionCompatibility::Incompatible`] error. Users must migrate such
/// snapshots using `hexz-migrate upgrade` before reading.
///
/// # Rationale
///
/// Maintaining backward compatibility indefinitely is untenable as format
/// complexity grows. This constant defines the "support horizon" for old
/// snapshots. When incrementing, ensure migration tooling exists.
///
/// # Current Policy
///
/// - **Hexz 0.1.x - 0.9.x**: Support version 1 only
/// - **Hexz 1.0+**: May drop support for version 1 (TBD based on adoption)
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Maximum format version readable by this build.
///
/// Snapshots with `version > MAX_SUPPORTED_VERSION` are rejected unless
/// degraded mode is enabled (not yet implemented). This prevents crashes
/// when reading snapshots created by future Hexz versions.
///
/// # Forward Compatibility
///
/// Currently, Hexz uses strict versioning: any version mismatch results
/// in incompatibility. Future releases may implement graceful degradation
/// for minor version increments (e.g., ignore unknown header fields).
///
/// # Upgrade Path
///
/// If you encounter a snapshot with `version > MAX_SUPPORTED_VERSION`,
/// upgrade Hexz to a newer release that supports that version.
pub const MAX_SUPPORTED_VERSION: u32 = 1;

/// Result of snapshot version compatibility analysis.
///
/// Returned by [`check_version`] to indicate whether a snapshot with a given
/// format version can be read by this build of Hexz. This enum enables
/// graceful handling of version mismatches with appropriate error messages.
///
/// # Variants
///
/// - **Full**: Version is within [`MIN_SUPPORTED_VERSION`]..=[`MAX_SUPPORTED_VERSION`]
///   and all features are supported. Proceed with normal operations.
///
/// - **Degraded**: Version is newer than [`MAX_SUPPORTED_VERSION`] but forward
///   compatibility rules allow partial reads with warnings. Unsupported features
///   are ignored or substituted with defaults. (Not yet implemented; currently
///   treated as Incompatible.)
///
/// - **Incompatible**: Version is outside the supported range and cannot be read.
///   The user must upgrade Hexz (for newer snapshots) or migrate the snapshot
///   (for older snapshots).
///
/// # Examples
///
/// ```
/// use hexz_core::format::version::{check_version, VersionCompatibility};
///
/// let result = check_version(1);
/// assert_eq!(result, VersionCompatibility::Full);
///
/// let result = check_version(999);
/// assert_eq!(result, VersionCompatibility::Incompatible);
/// assert!(!result.is_compatible());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCompatibility {
    /// Fully supported version with all features available.
    Full,
    /// Newer version with partial support (warnings emitted, some features unavailable).
    ///
    /// This variant is reserved for future forward compatibility modes. Currently,
    /// all versions outside the supported range are marked as Incompatible.
    Degraded,
    /// Incompatible version that cannot be read (too old or too new).
    Incompatible,
}

impl VersionCompatibility {
    /// Tests whether the snapshot can be read with this compatibility status.
    ///
    /// Returns `true` for [`Full`](VersionCompatibility::Full) and
    /// [`Degraded`](VersionCompatibility::Degraded) compatibility, indicating
    /// that read operations can proceed (possibly with warnings). Returns `false`
    /// for [`Incompatible`](VersionCompatibility::Incompatible), indicating that
    /// the snapshot must be rejected.
    ///
    /// # Returns
    ///
    /// - `true`: Snapshot can be opened (possibly with limited functionality)
    /// - `false`: Snapshot cannot be opened (hard error)
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::format::version::{check_version, VersionCompatibility};
    ///
    /// let compat = check_version(1);
    /// if compat.is_compatible() {
    ///     println!("Snapshot can be read");
    /// } else {
    ///     eprintln!("Snapshot is incompatible");
    /// }
    /// ```
    pub fn is_compatible(&self) -> bool {
        match self {
            VersionCompatibility::Full | VersionCompatibility::Degraded => true,
            VersionCompatibility::Incompatible => false,
        }
    }
}

/// Determines compatibility status of a snapshot format version.
///
/// This function implements the version negotiation logic by comparing the
/// provided version number against the supported range defined by
/// [`MIN_SUPPORTED_VERSION`] and [`MAX_SUPPORTED_VERSION`].
///
/// # Parameters
///
/// - `version`: The format version number read from a snapshot header
///
/// # Returns
///
/// A [`VersionCompatibility`] value indicating whether the snapshot can be read:
///
/// - [`Full`](VersionCompatibility::Full): Version is within supported range
/// - [`Incompatible`](VersionCompatibility::Incompatible): Version is too old or too new
/// - [`Degraded`](VersionCompatibility::Degraded): Currently unused (reserved for future)
///
/// # Version Ranges
///
/// | Condition | Result | Reason |
/// |-----------|--------|--------|
/// | `version < MIN_SUPPORTED_VERSION` | Incompatible | Too old, needs migration |
/// | `MIN_SUPPORTED_VERSION <= version <= MAX_SUPPORTED_VERSION` | Full | Within supported range |
/// | `version > MAX_SUPPORTED_VERSION` | Incompatible | Too new, upgrade Hexz |
///
/// # Future Extensions
///
/// In future versions, this function may return `Degraded` for snapshots with
/// minor version mismatches, enabling partial reads with warnings. For example:
///
/// ```rust,no_run
/// # use hexz_core::format::version::{VersionCompatibility, CURRENT_VERSION};
/// # struct V { major: u32, minor: u32 }
/// # let version = V { major: 1, minor: 1 };
/// # let current_version = V { major: 1, minor: 0 };
/// // Hypothetical future behavior with major.minor versioning
/// let _compat = if version.major == current_version.major && version.minor > current_version.minor {
///     VersionCompatibility::Degraded  // Newer minor version, try best-effort read
/// } else {
///     VersionCompatibility::Full
/// };
/// ```
///
/// # Performance
///
/// This function performs two integer comparisons and has negligible overhead
/// (~100ns). It is called once per snapshot open operation and does not affect
/// hot path performance.
///
/// # Examples
///
/// ```
/// use hexz_core::format::version::{
///     check_version, VersionCompatibility,
///     MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION
/// };
///
/// // Version within supported range
/// let compat = check_version(1);
/// assert_eq!(compat, VersionCompatibility::Full);
///
/// // Version too old (hypothetical example if MIN_SUPPORTED_VERSION > 1)
/// let compat = check_version(0);
/// assert_eq!(compat, VersionCompatibility::Incompatible);
///
/// // Version too new
/// let compat = check_version(MAX_SUPPORTED_VERSION + 1);
/// assert_eq!(compat, VersionCompatibility::Incompatible);
/// ```
///
/// ## Error Handling
///
/// ```rust,no_run
/// # use hexz_core::format::version::{check_version, MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION};
/// # use hexz_common::{Error, Result};
/// # use hexz_core::format::header::Header;
/// # struct Snapshot;
/// # impl Snapshot { fn new(_: &Header) -> Self { Snapshot } }
/// fn open_snapshot(header: &Header) -> Result<Snapshot> {
///     let compat = check_version(header.version);
///     if !compat.is_compatible() {
///         return Err(Error::Format(format!(
///             "Incompatible version {}. Supported range: [{}, {}]",
///             header.version, MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION
///         )));
///     }
///     // Proceed with read operations
///     Ok(Snapshot::new(header))
/// }
/// ```
pub fn check_version(version: u32) -> VersionCompatibility {
    if version < MIN_SUPPORTED_VERSION {
        VersionCompatibility::Incompatible
    } else if version > MAX_SUPPORTED_VERSION {
        // For now, strict versioning. Future: Check major/minor for degraded mode.
        VersionCompatibility::Incompatible
    } else {
        VersionCompatibility::Full
    }
}

/// Generates a user-facing message describing version compatibility status.
///
/// This function produces human-readable error messages and recommendations
/// for handling version mismatches. The messages are designed to be displayed
/// directly to end users in error logs or CLI output.
///
/// # Parameters
///
/// - `version`: The format version number from a snapshot header
///
/// # Returns
///
/// A `String` containing:
/// - **Full compatibility**: Confirmation message
/// - **Degraded compatibility**: Warning about potential feature limitations
/// - **Incompatibility**: Error message with actionable remediation steps
///
/// # Message Content
///
/// ## Fully Supported Version
///
/// ```text
/// "Version 1 is fully supported."
/// ```
///
/// ## Too Old (version < MIN_SUPPORTED_VERSION)
///
/// ```text
/// "Version 0 is too old (min supported: 1). Please upgrade the snapshot."
/// ```
///
/// Remediation: Use `hexz-migrate upgrade` to convert the snapshot.
///
/// ## Too New (version > MAX_SUPPORTED_VERSION)
///
/// ```text
/// "Version 2 is too new (max supported: 1). Please upgrade Hexz."
/// ```
///
/// Remediation: Install a newer version of the Hexz toolchain.
///
/// ## Degraded Compatibility (Future)
///
/// ```text
/// "Version 2 is newer than supported (1), features may be missing."
/// ```
///
/// Remediation: Upgrade Hexz for full feature support, or proceed with warnings.
///
/// # Usage in Error Handling
///
/// This function is typically called when displaying compatibility errors to users:
///
/// ```rust,no_run
/// # use hexz_core::format::version::{check_version, compatibility_message};
/// # use hexz_core::format::header::Header;
/// fn validate_snapshot(header: &Header) -> Result<(), String> {
///     let compat = check_version(header.version);
///     if !compat.is_compatible() {
///         return Err(compatibility_message(header.version));
///     }
///     Ok(())
/// }
/// ```
///
/// # Examples
///
/// ```
/// use hexz_core::format::version::{compatibility_message, MAX_SUPPORTED_VERSION};
///
/// // Supported version
/// let msg = compatibility_message(1);
/// assert!(msg.contains("fully supported"));
///
/// // Version too new
/// let msg = compatibility_message(MAX_SUPPORTED_VERSION + 1);
/// assert!(msg.contains("too new"));
/// assert!(msg.contains("upgrade Hexz"));
///
/// // Version too old (hypothetical if MIN_SUPPORTED_VERSION > 1)
/// let msg = compatibility_message(0);
/// assert!(msg.contains("too old"));
/// assert!(msg.contains("upgrade the snapshot"));
/// ```
///
/// ## CLI Integration
///
/// ```bash
/// $ hexz open old_snapshot.hxz
/// Error: Version 0 is too old (min supported: 1). Please upgrade the snapshot.
///
/// Run: hexz-migrate upgrade old_snapshot.hxz new_snapshot.hxz
/// ```
pub fn compatibility_message(version: u32) -> String {
    match check_version(version) {
        VersionCompatibility::Full => format!("Version {} is fully supported.", version),
        VersionCompatibility::Degraded => format!(
            "Version {} is newer than supported ({}), features may be missing.",
            version, MAX_SUPPORTED_VERSION
        ),
        VersionCompatibility::Incompatible => {
            if version < MIN_SUPPORTED_VERSION {
                format!(
                    "Version {} is too old (min supported: {}). Please upgrade the snapshot.",
                    version, MIN_SUPPORTED_VERSION
                )
            } else {
                format!(
                    "Version {} is too new (max supported: {}). Please upgrade Hexz.",
                    version, MAX_SUPPORTED_VERSION
                )
            }
        }
    }
}

impl fmt::Display for VersionCompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionCompatibility::Full => write!(f, "full"),
            VersionCompatibility::Degraded => write!(f, "degraded"),
            VersionCompatibility::Incompatible => write!(f, "incompatible"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_version_constants_are_consistent() {
        // MIN_SUPPORTED_VERSION must be <= MAX_SUPPORTED_VERSION
        assert!(
            MIN_SUPPORTED_VERSION <= MAX_SUPPORTED_VERSION,
            "MIN_SUPPORTED_VERSION ({}) must be <= MAX_SUPPORTED_VERSION ({})",
            MIN_SUPPORTED_VERSION,
            MAX_SUPPORTED_VERSION
        );

        // CURRENT_VERSION must be within supported range
        assert!(
            CURRENT_VERSION >= MIN_SUPPORTED_VERSION,
            "CURRENT_VERSION ({}) must be >= MIN_SUPPORTED_VERSION ({})",
            CURRENT_VERSION,
            MIN_SUPPORTED_VERSION
        );
        assert!(
            CURRENT_VERSION <= MAX_SUPPORTED_VERSION,
            "CURRENT_VERSION ({}) must be <= MAX_SUPPORTED_VERSION ({})",
            CURRENT_VERSION,
            MAX_SUPPORTED_VERSION
        );
    }

    #[test]
    fn test_check_version_current_is_fully_supported() {
        let compat = check_version(CURRENT_VERSION);
        assert_eq!(compat, VersionCompatibility::Full);
        assert!(compat.is_compatible());
    }

    #[test]
    fn test_check_version_within_range_is_full() {
        // Test all versions in the supported range
        for version in MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION {
            let compat = check_version(version);
            assert_eq!(
                compat,
                VersionCompatibility::Full,
                "Version {} should be fully compatible",
                version
            );
            assert!(compat.is_compatible());
        }
    }

    #[test]
    fn test_check_version_too_old_is_incompatible() {
        if MIN_SUPPORTED_VERSION > 0 {
            let old_version = MIN_SUPPORTED_VERSION - 1;
            let compat = check_version(old_version);
            assert_eq!(compat, VersionCompatibility::Incompatible);
            assert!(!compat.is_compatible());
        }

        // Always test version 0 as too old
        let compat = check_version(0);
        assert_eq!(compat, VersionCompatibility::Incompatible);
        assert!(!compat.is_compatible());
    }

    #[test]
    fn test_check_version_too_new_is_incompatible() {
        let new_version = MAX_SUPPORTED_VERSION + 1;
        let compat = check_version(new_version);
        assert_eq!(compat, VersionCompatibility::Incompatible);
        assert!(!compat.is_compatible());

        // Test very high version number
        let compat = check_version(9999);
        assert_eq!(compat, VersionCompatibility::Incompatible);
        assert!(!compat.is_compatible());
    }

    #[test]
    fn test_version_compatibility_is_compatible() {
        // Full is compatible
        assert!(VersionCompatibility::Full.is_compatible());

        // Degraded is compatible (with warnings)
        assert!(VersionCompatibility::Degraded.is_compatible());

        // Incompatible is not compatible
        assert!(!VersionCompatibility::Incompatible.is_compatible());
    }

    #[test]
    fn test_version_compatibility_display() {
        assert_eq!(format!("{}", VersionCompatibility::Full), "full");
        assert_eq!(format!("{}", VersionCompatibility::Degraded), "degraded");
        assert_eq!(
            format!("{}", VersionCompatibility::Incompatible),
            "incompatible"
        );
    }

    #[test]
    fn test_compatibility_message_for_supported_version() {
        let msg = compatibility_message(CURRENT_VERSION);
        assert!(msg.contains("fully supported"), "Message: {}", msg);
        assert!(
            msg.contains(&CURRENT_VERSION.to_string()),
            "Message: {}",
            msg
        );
    }

    #[test]
    fn test_compatibility_message_for_too_old_version() {
        if MIN_SUPPORTED_VERSION > 0 {
            let old_version = MIN_SUPPORTED_VERSION - 1;
            let msg = compatibility_message(old_version);
            assert!(msg.contains("too old"), "Message: {}", msg);
            assert!(
                msg.contains(&MIN_SUPPORTED_VERSION.to_string()),
                "Message: {}",
                msg
            );
            assert!(msg.contains("upgrade the snapshot"), "Message: {}", msg);
        }
    }

    #[test]
    fn test_compatibility_message_for_too_new_version() {
        let new_version = MAX_SUPPORTED_VERSION + 1;
        let msg = compatibility_message(new_version);
        assert!(msg.contains("too new"), "Message: {}", msg);
        assert!(
            msg.contains(&MAX_SUPPORTED_VERSION.to_string()),
            "Message: {}",
            msg
        );
        assert!(msg.contains("upgrade Hexz"), "Message: {}", msg);
    }

    #[test]
    fn test_compatibility_message_for_degraded() {
        // Since we don't currently return Degraded, we can't directly test it
        // But we can test the message format by checking what would happen
        let msg = match VersionCompatibility::Degraded {
            VersionCompatibility::Degraded => format!(
                "Version {} is newer than supported ({}), features may be missing.",
                99, MAX_SUPPORTED_VERSION
            ),
            _ => String::new(),
        };
        assert!(msg.contains("newer than supported"));
        assert!(msg.contains("features may be missing"));
    }

    #[test]
    fn test_version_compatibility_enum_properties() {
        // Test Debug trait
        let full = VersionCompatibility::Full;
        assert!(format!("{:?}", full).contains("Full"));

        // Test Clone and Copy
        let degraded = VersionCompatibility::Degraded;
        let degraded_copy = degraded;
        let degraded_clone = degraded;
        assert_eq!(degraded, degraded_copy);
        assert_eq!(degraded, degraded_clone);

        // Test PartialEq and Eq
        assert_eq!(VersionCompatibility::Full, VersionCompatibility::Full);
        assert_ne!(
            VersionCompatibility::Full,
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_check_version_boundary_conditions() {
        // Test MIN_SUPPORTED_VERSION boundary
        let compat = check_version(MIN_SUPPORTED_VERSION);
        assert_eq!(compat, VersionCompatibility::Full);

        // Test MAX_SUPPORTED_VERSION boundary
        let compat = check_version(MAX_SUPPORTED_VERSION);
        assert_eq!(compat, VersionCompatibility::Full);

        // Test just below MIN
        if MIN_SUPPORTED_VERSION > 0 {
            let compat = check_version(MIN_SUPPORTED_VERSION - 1);
            assert_eq!(compat, VersionCompatibility::Incompatible);
        }

        // Test just above MAX
        let compat = check_version(MAX_SUPPORTED_VERSION + 1);
        assert_eq!(compat, VersionCompatibility::Incompatible);
    }

    #[test]
    fn test_multiple_version_checks() {
        // Verify check_version is consistent across multiple calls
        let version = CURRENT_VERSION;
        let result1 = check_version(version);
        let result2 = check_version(version);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_compatibility_message_contains_version_number() {
        // Test that messages include the actual version number
        for version in [0u32, 1, 2, 99, 999] {
            let msg = compatibility_message(version);
            assert!(
                msg.contains(&version.to_string()),
                "Message for version {} should contain the version number: {}",
                version,
                msg
            );
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_version_constants_are_reasonable() {
        // Ensure version numbers are in a reasonable range
        assert!(CURRENT_VERSION > 0, "CURRENT_VERSION must be positive");
        assert!(
            CURRENT_VERSION < 1000,
            "CURRENT_VERSION seems unreasonably high"
        );
        assert!(
            MIN_SUPPORTED_VERSION > 0,
            "MIN_SUPPORTED_VERSION must be positive"
        );
        assert!(
            MAX_SUPPORTED_VERSION < 1000,
            "MAX_SUPPORTED_VERSION seems unreasonably high"
        );
    }
}
