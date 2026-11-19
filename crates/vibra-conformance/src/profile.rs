//! Profiles, operations, and the handler interface of the corpus runner.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` names six conformance
//! profiles ("Conformance profiles") and ten case operations ("Case operations
//! and inputs"). Both are closed sets spelled as camelCase atoms without the
//! `@`. A handler registers under a profile and returns backend-neutral
//! observations for the cases it can run.

use std::fmt;
use std::str::FromStr;

use vibra_diagnostics::Diagnostic;

use crate::corpus::Case;
use crate::manifest::Verdict;

/// A versioned statement of what a tool supports.
///
/// Profiles are capabilities, not source dialects. A handler registered for a
/// broader profile may run a case requiring a narrower one, and a valid case
/// outside every registered profile is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConformanceProfile {
    /// The source and VIBON readers, recovery, canonical format, and syntax
    /// diagnostics.
    ReaderV2,
    /// `readerV2` plus names, types, guards, effect rows, project and binding
    /// validation.
    StaticV2,
    /// `staticV2` plus reference execution, the operation registry, fake and
    /// system worlds.
    InterpreterV2,
    /// `staticV2` plus the query, edit, schema, CLI, and MCP contracts.
    ToolingV2,
    /// `staticV2` plus verified execution IR, the native runtime, code
    /// generation, linking, launch, and native observations.
    NativeV1,
    /// Everything a release requires.
    FullV2,
}

impl ConformanceProfile {
    /// Every profile, in the order of the specification table.
    pub const ALL: &'static [Self] = &[
        Self::ReaderV2,
        Self::StaticV2,
        Self::InterpreterV2,
        Self::ToolingV2,
        Self::NativeV1,
        Self::FullV2,
    ];

    /// The spelling used by `case.toml` and in reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReaderV2 => "readerV2",
            Self::StaticV2 => "staticV2",
            Self::InterpreterV2 => "interpreterV2",
            Self::ToolingV2 => "toolingV2",
            Self::NativeV1 => "nativeV1",
            Self::FullV2 => "fullV2",
        }
    }

    /// Whether `self` includes everything `required` covers.
    ///
    /// The relation is the "Covers" column of the specification table:
    /// `staticV2` includes `readerV2`; `interpreterV2`, `toolingV2`, and
    /// `nativeV1` each include `staticV2` and nothing beside it; `nativeV1`
    /// does not include `interpreterV2`; `fullV2` includes every profile.
    #[must_use]
    pub const fn supports(self, required: Self) -> bool {
        match self {
            Self::ReaderV2 => matches!(required, Self::ReaderV2),
            Self::StaticV2 => matches!(required, Self::ReaderV2 | Self::StaticV2),
            Self::InterpreterV2 => matches!(
                required,
                Self::ReaderV2 | Self::StaticV2 | Self::InterpreterV2
            ),
            Self::ToolingV2 => {
                matches!(required, Self::ReaderV2 | Self::StaticV2 | Self::ToolingV2)
            }
            Self::NativeV1 => {
                matches!(required, Self::ReaderV2 | Self::StaticV2 | Self::NativeV1)
            }
            Self::FullV2 => true,
        }
    }

    /// A coarse capability depth, used to choose the closest registered
    /// handler when more than one can run a case.
    #[must_use]
    pub const fn depth(self) -> u8 {
        match self {
            Self::ReaderV2 => 0,
            Self::StaticV2 => 1,
            Self::InterpreterV2 | Self::ToolingV2 | Self::NativeV1 => 2,
            Self::FullV2 => 3,
        }
    }
}

impl fmt::Display for ConformanceProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A profile spelling outside the closed set.
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
        Self::ALL
            .iter()
            .copied()
            .find(|profile| profile.as_str() == value)
            .ok_or_else(|| UnknownProfile {
                value: value.to_owned(),
            })
    }
}

/// The closed operation a case selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConformanceOperation {
    /// One `.vib` or `.vibon` document: the lossless tree, recovery, and
    /// diagnostics.
    Reader,
    /// Exactly one `project.vibon`: the typed decode, with no discovery.
    ProjectDecode,
    /// One confined tree with its `project.vibon`: the source graph.
    SourceGraph,
    /// The same tree: resolved identities, imports, and references.
    Resolve,
    /// The same tree: the static result.
    WorkspaceCheck,
    /// The same tree plus one document: the formatted text.
    Format,
    /// A captured workspace, a subject, an offset, and options: the query
    /// result.
    Query,
    /// One pure module, checked as a one-module workspace, whose first
    /// function with no parameters is called.
    Interpret,
    /// The same tree, one binary target: startup, the entry's result, and the
    /// audit trace.
    WorkspaceRun,
    /// The same tree: one suite result.
    WorkspaceTest,
}

impl ConformanceOperation {
    /// Every operation, in the order of the specification table.
    pub const ALL: &'static [Self] = &[
        Self::Reader,
        Self::ProjectDecode,
        Self::SourceGraph,
        Self::Resolve,
        Self::WorkspaceCheck,
        Self::Format,
        Self::Query,
        Self::Interpret,
        Self::WorkspaceRun,
        Self::WorkspaceTest,
    ];

    /// The spelling used by `case.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::ProjectDecode => "projectDecode",
            Self::SourceGraph => "sourceGraph",
            Self::Resolve => "resolve",
            Self::WorkspaceCheck => "workspaceCheck",
            Self::Format => "format",
            Self::Query => "query",
            Self::Interpret => "interpret",
            Self::WorkspaceRun => "workspaceRun",
            Self::WorkspaceTest => "workspaceTest",
        }
    }

    /// Whether this operation executes a program: the operation half of an
    /// executable case ("Differential execution").
    #[must_use]
    pub const fn executes_programs(self) -> bool {
        matches!(
            self,
            Self::Interpret | Self::WorkspaceRun | Self::WorkspaceTest
        )
    }
}

impl fmt::Display for ConformanceOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An operation spelling outside the closed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownOperation {
    value: String,
}

impl UnknownOperation {
    /// The spelling that failed to parse.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for UnknownOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown conformance operation `{}`", self.value)
    }
}

impl std::error::Error for UnknownOperation {}

impl FromStr for ConformanceOperation {
    type Err = UnknownOperation;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|operation| operation.as_str() == value)
            .ok_or_else(|| UnknownOperation {
                value: value.to_owned(),
            })
    }
}

/// The backend-neutral facts a handler returns for one case.
///
/// Every snapshot is the canonical text the handler produced for the
/// observation of that name; `None` when the handler did not produce it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseObservation {
    /// The verdict the handler observed.
    pub verdict: Verdict,
    /// Diagnostics in emission order.
    pub diagnostics: Vec<Diagnostic>,
    /// The `@tree.v2` observation.
    pub tree: Option<String>,
    /// The formatted text.
    pub formatted: Option<String>,
    /// The `@sourceGraph.v1` observation.
    pub graph: Option<String>,
    /// The `@resolved.v2` observation.
    pub resolved: Option<String>,
    /// The `@types.v2` observation.
    pub types: Option<String>,
    /// The `@value.v2` observation of an executed result.
    pub result: Option<String>,
    /// The `@testRun.v2` observation.
    pub test_run: Option<String>,
}

impl CaseObservation {
    /// An observation with `verdict` and nothing else.
    #[must_use]
    pub const fn new(verdict: Verdict) -> Self {
        Self {
            verdict,
            diagnostics: Vec::new(),
            tree: None,
            formatted: None,
            graph: None,
            resolved: None,
            types: None,
            result: None,
            test_run: None,
        }
    }
}

/// A handler failure while running a case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerError {
    message: String,
}

impl HandlerError {
    /// A failure with a human-readable explanation.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HandlerError {}

/// The interface an implementation uses to plug into the runner.
pub trait ProfileHandler: Send + Sync {
    /// Whether this handler runs the declared shape of `case`, normally its
    /// operation. A profile can have several handlers; the dispatcher asks
    /// this before it selects one.
    fn can_run(&self, case: &Case) -> bool;

    /// Runs one case and returns its observations.
    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError>;
}
