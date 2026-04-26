use crate::errors::DariError;

/// Supported on-disk format versions.
///
/// This enum is the single source of truth for every version the binary knows
/// about.  [`TryFrom<u8>`] is used to parse the version byte read from the
/// archive header; any unknown byte produces
/// [`DariError::UnsupportedVersion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    V5 = 5,
    V6 = 6,
}

impl Default for FormatVersion {
    /// The default write version is v5 (current stable format).
    fn default() -> Self {
        FormatVersion::V5
    }
}

impl TryFrom<u8> for FormatVersion {
    type Error = DariError;

    fn try_from(v: u8) -> Result<Self, DariError> {
        match v {
            5 => Ok(FormatVersion::V5),
            6 => Ok(FormatVersion::V6),
            other => Err(DariError::UnsupportedVersion {
                found: other,
                // Maximum version this build knows about.
                // Updated to MAX_SUPPORTED_VERSION once Phase 1 lands.
                max_supported: 6,
            }),
        }
    }
}

impl From<FormatVersion> for u8 {
    fn from(v: FormatVersion) -> u8 {
        v as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v5_roundtrip() {
        let v = FormatVersion::try_from(5u8).unwrap();
        assert_eq!(v, FormatVersion::V5);
        assert_eq!(u8::from(v), 5);
    }

    #[test]
    fn test_v6_roundtrip() {
        let v = FormatVersion::try_from(6u8).unwrap();
        assert_eq!(v, FormatVersion::V6);
        assert_eq!(u8::from(v), 6);
    }

    #[test]
    fn test_unknown_version_returns_error() {
        let err = FormatVersion::try_from(99u8).unwrap_err();
        match err {
            DariError::UnsupportedVersion {
                found,
                max_supported,
            } => {
                assert_eq!(found, 99);
                assert_eq!(max_supported, 6);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn test_default_is_v5() {
        assert_eq!(FormatVersion::default(), FormatVersion::V5);
    }
}
