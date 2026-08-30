//! Capability profiles used by the conformance corpus.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A backend capability advertised by a conformance runner.
///
/// Profiles are capabilities, not source dialects. A handler registered for a
/// broader profile may run a case requiring a narrower profile, but a smaller
/// profile must report a broader case as unavailable.
#[derive(
    Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
pub enum ConformanceProfile {
    /// Reader, recovery CST, formatter, and syntax diagnostics.
    #[serde(rename = "reader-v1")]
    ReaderV1,
    /// Reader plus names, types, effects, and project checking.
    #[serde(rename = "static-v1")]
    StaticV1,
    /// Static profile plus reference execution and external registries.
    #[serde(rename = "interpreter-v1")]
    InterpreterV1,
    /// Static profile plus schemas, queries, plans, CLI, and MCP.
    #[serde(rename = "tooling-v1")]
    ToolingV1,
    /// Static profile plus deterministic Wasm and runtime.
    #[serde(rename = "wasm-v1")]
    WasmV1,
    /// All profiles, the standard library, projects, and release gates.
    #[serde(rename = "full-v1")]
    FullV1,
}

impl ConformanceProfile {
    /// Every standard profile in specification order.
    pub const ALL: &'static [Self] = &[
        Self::ReaderV1,
        Self::StaticV1,
        Self::InterpreterV1,
        Self::ToolingV1,
        Self::WasmV1,
        Self::FullV1,
    ];

    /// The stable machine spelling used by `case.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReaderV1 => "reader-v1",
            Self::StaticV1 => "static-v1",
            Self::InterpreterV1 => "interpreter-v1",
            Self::ToolingV1 => "tooling-v1",
            Self::WasmV1 => "wasm-v1",
            Self::FullV1 => "full-v1",
        }
    }

    /// Whether `self` provides the capability required by `required`.
    #[must_use]
    pub const fn supports(self, required: Self) -> bool {
        match self {
            Self::ReaderV1 => matches!(required, Self::ReaderV1),
            Self::StaticV1 => matches!(required, Self::ReaderV1 | Self::StaticV1),
            Self::InterpreterV1 => {
                matches!(
                    required,
                    Self::ReaderV1 | Self::StaticV1 | Self::InterpreterV1
                )
            }
            Self::ToolingV1 => {
                matches!(required, Self::ReaderV1 | Self::StaticV1 | Self::ToolingV1)
            }
            Self::WasmV1 => {
                matches!(required, Self::ReaderV1 | Self::StaticV1 | Self::WasmV1)
            }
            Self::FullV1 => true,
        }
    }

    /// A coarse capability depth used to choose the closest registered
    /// handler when more than one handler can execute a case.
    #[must_use]
    pub const fn depth(self) -> u8 {
        match self {
            Self::ReaderV1 => 0,
            Self::StaticV1 => 1,
            Self::InterpreterV1 | Self::ToolingV1 | Self::WasmV1 => 2,
            Self::FullV1 => 3,
        }
    }
}

impl fmt::Display for ConformanceProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A profile spelling that is not part of the closed v1 profile set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownProfile {
    value: String,
}

impl UnknownProfile {
    /// The spelling that failed to parse.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for UnknownProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown conformance profile `{}`", self.value)
    }
}

impl std::error::Error for UnknownProfile {}

impl FromStr for ConformanceProfile {
    type Err = UnknownProfile;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let profile = match value {
            "reader-v1" => Self::ReaderV1,
            "static-v1" => Self::StaticV1,
            "interpreter-v1" => Self::InterpreterV1,
            "tooling-v1" => Self::ToolingV1,
            "wasm-v1" => Self::WasmV1,
            "full-v1" => Self::FullV1,
            _ => {
                return Err(UnknownProfile {
                    value: value.to_owned(),
                });
            }
        };
        Ok(profile)
    }
}
