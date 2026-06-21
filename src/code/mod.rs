//! Lossless, path-addressed Vibra source manipulation.

mod cli;
mod edit;
mod form;
mod path;
mod query;
mod semantic;
mod source;

pub use cli::{run_pipeline, CodeRunOptions};
pub use edit::{AppliedChangeSet, ChangeSet, Edit};
pub use form::{Entry, Form};
pub use path::{Path, Segment};
pub use query::{Pattern, Query, QueryMatch};
pub use semantic::{SemanticFact, SemanticIndex, SemanticKind};
pub use source::{
    CodeError, CodeErrorKind, DocumentSnapshot, Node, NodeKind, NodeLocator, SourceDatabase,
};
