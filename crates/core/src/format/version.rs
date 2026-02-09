use std::fmt;

/// Current format version of Strata snapshots.
///
/// Incremented whenever the on-disk format changes in a way that affects readers.
pub const CURRENT_VERSION: u32 = 1;

/// Minimum supported version (oldest version we can read).
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Maximum supported version (newest version we can read).
pub const MAX_SUPPORTED_VERSION: u32 = 1;

/// Compatibility status of a snapshot version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCompatibility {
    /// Fully supported version.
    Full,
    /// Newer version than we support, but we might be able to read it (with warnings).
    /// This happens if the major version is the same but minor is higher, or if we define
    /// forward compatibility rules. For now, strict versioning.
    Degraded,
    /// Incompatible version (too old or too new).
    Incompatible,
}

impl VersionCompatibility {
    /// Returns true if the version is compatible (Full or Degraded).
    pub fn is_compatible(&self) -> bool {
        match self {
            VersionCompatibility::Full | VersionCompatibility::Degraded => true,
            VersionCompatibility::Incompatible => false,
        }
    }
}

/// Checks compatibility of a given format version.
pub fn check_version(version: u32) -> VersionCompatibility {
    if version < MIN_SUPPORTED_VERSION {
        VersionCompatibility::Incompatible
    } else if version > MAX_SUPPORTED_VERSION {
        // For now, strict versioning. Future: Check major/minor.
        VersionCompatibility::Incompatible
    } else {
        VersionCompatibility::Full
    }
}

/// Returns a human-readable message describing compatibility.
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
                    "Version {} is too new (max supported: {}). Please upgrade Strata.",
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
