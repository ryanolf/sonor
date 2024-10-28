use std::fmt;

/// This enum describes how Sonos repeats the current playlist.
#[derive(Debug, Default)]
pub enum RepeatMode {
    /// The playlist doesn't get repeated.
    #[default]
    None,
    /// Only one song gets played on and on.
    One,
    /// The whole playlist is repeated.
    All,
}

impl fmt::Display for RepeatMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug)]
pub struct ParseRepeatModeError;
impl std::error::Error for ParseRepeatModeError {}
impl std::fmt::Display for ParseRepeatModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "provided string was not `NONE` or `ONE` or `ALL`".fmt(f)
    }
}

impl std::str::FromStr for RepeatMode {
    type Err = ParseRepeatModeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "none" => Ok(RepeatMode::None),
            "one" => Ok(RepeatMode::One),
            "all" => Ok(RepeatMode::All),
            _ => Err(ParseRepeatModeError),
        }
    }
}
