//! The fan and power modes the EC cycles through on Fn+K.

use std::fmt;

/// Mode as stored in EC RAM at [`crate::ec::MODE_OFFSET`].
///
/// Fn+K cycles Balanced, Performance, Silent and back to Balanced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Performance,
    Balanced,
    Silent,
    Unknown(u8),
}

impl Mode {
    pub fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Performance,
            1 => Self::Balanced,
            2 => Self::Silent,
            other => Self::Unknown(other),
        }
    }

    pub fn raw(self) -> u8 {
        match self {
            Self::Performance => 0,
            Self::Balanced => 1,
            Self::Silent => 2,
            Self::Unknown(raw) => raw,
        }
    }

    /// Freedesktop icon name for the desktop banner.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Performance => "power-profile-performance-symbolic",
            Self::Balanced => "power-profile-balanced-symbolic",
            Self::Silent => "power-profile-power-saver-symbolic",
            Self::Unknown(_) => "dialog-information-symbolic",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Performance => f.write_str("Performance"),
            Self::Balanced => f.write_str("Balanced"),
            Self::Silent => f.write_str("Silent"),
            Self::Unknown(raw) => write!(f, "Unknown mode {raw}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trip() {
        for raw in 0..=255u8 {
            assert_eq!(Mode::from_raw(raw).raw(), raw);
        }
    }

    #[test]
    fn known_modes() {
        assert_eq!(Mode::from_raw(0), Mode::Performance);
        assert_eq!(Mode::from_raw(1), Mode::Balanced);
        assert_eq!(Mode::from_raw(2), Mode::Silent);
        assert_eq!(Mode::from_raw(7).to_string(), "Unknown mode 7");
    }
}
