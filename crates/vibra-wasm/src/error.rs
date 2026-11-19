//! What the emitter reports when it does not produce a module.
//!
//! The backend runs the pure part of the language under the transition
//! profile (`docs/spec/06-runtime.md`, "Wasm transition"), and it is
//! fail-closed: a checked node it does not lower is [`EmitError::Unsupported`],
//! which names the node and where it was written, and the whole module fails,
//! so a program is never partly executed and never approximated. The other
//! errors are a program larger than the 32-bit indices of a module and a
//! checked-IR shape that violates an invariant the checker guarantees, which
//! is a toolchain defect or a hand-built program. An error is a value, never a
//! partial module.

use std::fmt;

use vibra_ir::SourceOrigin;

/// Why a module was not emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitError {
    /// A node or declaration the transition profile does not run.
    Unsupported {
        /// The kind of the node, in camelCase: the name of a variant of
        /// [`vibra_ir::Expr`], or of the declaration form that stopped the
        /// emit, such as `entryGuard`.
        node: &'static str,
        /// Where it was written.
        origin: SourceOrigin,
    },
    /// A count or an index does not fit the 32 bits of a module.
    ModuleSize,
    /// A checked-IR shape the checker never builds.
    Malformed {
        /// What was malformed, for a person.
        what: String,
        /// Where it was written, when that is known.
        origin: Option<SourceOrigin>,
    },
}

impl EmitError {
    /// An unsupported `node` written at `origin`.
    pub(crate) fn unsupported(node: &'static str, origin: &SourceOrigin) -> Self {
        Self::Unsupported {
            node,
            origin: origin.clone(),
        }
    }

    /// A malformed shape, described by `what`, written at `origin`.
    pub(crate) fn malformed(what: impl Into<String>, origin: &SourceOrigin) -> Self {
        Self::Malformed {
            what: what.into(),
            origin: Some(origin.clone()),
        }
    }

    /// The node kind of an unsupported emit, `None` for any other error.
    #[must_use]
    pub const fn unsupported_node(&self) -> Option<&'static str> {
        match self {
            Self::Unsupported { node, .. } => Some(node),
            Self::ModuleSize | Self::Malformed { .. } => None,
        }
    }
}

impl fmt::Display for EmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { node, origin } => write!(
                formatter,
                "the WebAssembly transition profile does not run `{node}` \
                 (written in {} at {}..{})",
                origin.source_id(),
                origin.span().start(),
                origin.span().end()
            ),
            Self::ModuleSize => {
                formatter.write_str("the program does not fit the indices of a module")
            }
            Self::Malformed { what, .. } => {
                write!(formatter, "a malformed checked program: {what}")
            }
        }
    }
}

impl std::error::Error for EmitError {}
