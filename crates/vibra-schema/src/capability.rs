//! The `CommandCapability` record of the `help` payload.

use serde::Serialize;

/// One command line of the support inventory.
///
/// `docs/spec/05-tooling.md`, "Command payloads": `command`, `operations`,
/// `profiles`, `supported`, and the exact published `schemaIds`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCapability {
    /// The command line, such as `check` or `project init`.
    pub command: String,
    /// The operations the command supports; empty when unsupported.
    pub operations: Vec<String>,
    /// The conformance profiles that cover the command; empty when
    /// unsupported.
    pub profiles: Vec<String>,
    /// Whether this tool supports the command.
    pub supported: bool,
    /// The identifiers of the schemas the command's documents use; empty
    /// when unsupported.
    pub schema_ids: Vec<String>,
}
