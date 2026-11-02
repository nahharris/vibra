//! The dispatcher of the internal corpus runner.

use crate::format::ToolingV1FormatHandler;
use crate::graph::StaticV1SourceGraphHandler;
use crate::profile::ConformanceProfile;
use crate::project::StaticV1ProjectHandler;
use crate::reader::ReaderV1Handler;
use crate::resolve::StaticV1ResolveHandler;
use crate::runner::ProfileDispatcher;
use crate::types::{InterpreterV1Handler, StaticV1TypeHandler};
use crate::workspace_index::ToolingV1IndexHandler;
use crate::workspace_query::ToolingV1QueryHandler;
use crate::workspace_semantic::{
    InterpreterV1WorkspaceRunHandler, InterpreterV1WorkspaceTestHandler,
    StaticV1WorkspaceCheckHandler,
};

/// Every real handler the workspace has, registered where its profile puts it.
///
/// The `interpreter-v1` handlers also run the WebAssembly backend on each
/// accepted executable case, because both backends receive the one checked
/// program (`docs/spec/07-diagnostics-and-conformance.md`, "Differential
/// execution"). The `vibra-conformance` binary and the host tests that hold the
/// corpus to the parity inventory build the same dispatcher, so the two cannot
/// disagree about which handlers exist.
#[must_use]
pub fn standard_dispatcher() -> ProfileDispatcher {
    ProfileDispatcher::new()
        .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler)
        .with_handler(ConformanceProfile::StaticV1, StaticV1ProjectHandler)
        .with_additional_handler(
            ConformanceProfile::StaticV1,
            StaticV1SourceGraphHandler,
        )
        .with_additional_handler(ConformanceProfile::StaticV1, StaticV1ResolveHandler)
        .with_additional_handler(ConformanceProfile::StaticV1, StaticV1TypeHandler)
        .with_additional_handler(
            ConformanceProfile::StaticV1,
            StaticV1WorkspaceCheckHandler,
        )
        .with_handler(ConformanceProfile::InterpreterV1, InterpreterV1Handler)
        .with_additional_handler(
            ConformanceProfile::InterpreterV1,
            InterpreterV1WorkspaceRunHandler,
        )
        .with_additional_handler(
            ConformanceProfile::InterpreterV1,
            InterpreterV1WorkspaceTestHandler,
        )
        .with_handler(ConformanceProfile::ToolingV1, ToolingV1QueryHandler)
        .with_additional_handler(ConformanceProfile::ToolingV1, ToolingV1FormatHandler)
        .with_additional_handler(ConformanceProfile::ToolingV1, ToolingV1IndexHandler)
}
