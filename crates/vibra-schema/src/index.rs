//! The JSON contract for the `@index.v1` document.
//!
//! `docs/spec/05-tooling.md`, "Index records", defines the document as
//! canonical VIBON. This is its JSON wire form for a tooling consumer: the
//! same records, in the same order, with identities as atom spellings without
//! `@` and canonical type encodings as VIBON text.

use serde::{Deserialize, Serialize};
use vibra_workspace::index::{
    IndexDeclaration, IndexDocument, IndexImplementation, IndexSource,
};

use crate::SCHEMA_VERSION;

/// The published JSON Schema for [`IndexDocumentJson`].
pub const INDEX_SCHEMA: &str = include_str!("../schemas/v1/index.json");

/// One source span of the indexed snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexSourceDocument {
    /// The project-relative source identity.
    pub source_id: String,
    /// The first byte of the span.
    pub start: usize,
    /// The byte after the span.
    pub end: usize,
}

impl IndexSourceDocument {
    fn render(source: &IndexSource) -> Self {
        Self {
            source_id: source.source_id().to_owned(),
            start: source.start(),
            end: source.end(),
        }
    }
}

/// The checked facts of one declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexFactsDocument {
    /// The canonical VIBON of the signature.
    pub signature: String,
    /// The performed effect roots, sorted.
    pub effects: Vec<String>,
    /// The canonical VIBON of each error type the result can carry.
    pub errors: Vec<String>,
    /// The identities of the declarations the body applies, sorted.
    pub applications: Vec<String>,
}

/// One module-level or nested declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexDeclarationDocument {
    /// The canonical atom identity.
    pub id: String,
    /// The declaration kind.
    pub kind: String,
    /// The owning module's identity.
    pub module: String,
    /// The owning type, interface, or effect root of a nested member.
    pub owner: Option<String>,
    /// `public` or `private`.
    pub visibility: String,
    /// The declaration's span.
    pub source: IndexSourceDocument,
    /// The checked facts, or null for a declaration that did not check.
    pub facts: Option<IndexFactsDocument>,
    /// The formatter-normalized source of the declaration.
    pub text: String,
}

impl IndexDeclarationDocument {
    fn render(declaration: &IndexDeclaration) -> Self {
        Self {
            id: declaration.id().to_owned(),
            kind: declaration.kind().to_owned(),
            module: declaration.module().to_owned(),
            owner: declaration.owner().map(str::to_owned),
            visibility: declaration.visibility().to_owned(),
            source: IndexSourceDocument::render(declaration.source()),
            facts: declaration.facts().map(|facts| IndexFactsDocument {
                signature: facts.signature().to_owned(),
                effects: facts.effects().to_vec(),
                errors: facts.errors().to_vec(),
                applications: facts.applications().to_vec(),
            }),
            text: declaration.text().to_owned(),
        }
    }
}

/// One member written in an `impl` block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexImplementationMemberDocument {
    /// The identity of the contract member it implements.
    pub contract: String,
    /// The member's span.
    pub source: IndexSourceDocument,
    /// The canonical VIBON of its checked signature.
    pub signature: String,
    /// The formatter-normalized source of the member.
    pub text: String,
}

/// One `impl` block, identified by its receiver and applied interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexImplementationDocument {
    /// The canonical VIBON of the receiver type.
    pub receiver: String,
    /// The canonical VIBON of the applied interface target.
    pub interface: String,
    /// The block's span.
    pub source: IndexSourceDocument,
    /// The formatter-normalized source of the block.
    pub text: String,
    /// The members the block writes.
    pub members: Vec<IndexImplementationMemberDocument>,
}

impl IndexImplementationDocument {
    fn render(implementation: &IndexImplementation) -> Self {
        Self {
            receiver: implementation.receiver().to_owned(),
            interface: implementation.interface().to_owned(),
            source: IndexSourceDocument::render(implementation.source()),
            text: implementation.text().to_owned(),
            members: implementation
                .members()
                .iter()
                .map(|member| IndexImplementationMemberDocument {
                    contract: member.contract().to_owned(),
                    source: IndexSourceDocument::render(member.source()),
                    signature: member.signature().to_owned(),
                    text: member.text().to_owned(),
                })
                .collect(),
        }
    }
}

/// One resolved written name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexReferenceDocument {
    /// The identity of the enclosing declaration.
    pub from: String,
    /// The exact spelling.
    pub written: String,
    /// The name's span.
    pub source: IndexSourceDocument,
    /// The resolved identity, or null for a name left to the checker.
    pub to: Option<String>,
}

/// The `@index.v1` document rendered for a tooling consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexDocumentJson {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The workspace revision the records belong to.
    pub workspace_revision: String,
    /// Declaration records, sorted by identity.
    pub declarations: Vec<IndexDeclarationDocument>,
    /// Implementation records, sorted by receiver and then interface.
    pub implementations: Vec<IndexImplementationDocument>,
    /// Reference records, sorted by source identity, start, and end.
    pub references: Vec<IndexReferenceDocument>,
}

impl IndexDocumentJson {
    /// Renders a workspace index, keeping its record order.
    #[must_use]
    pub fn render(document: &IndexDocument) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            workspace_revision: document.revision().to_owned(),
            declarations: document
                .declarations()
                .iter()
                .map(IndexDeclarationDocument::render)
                .collect(),
            implementations: document
                .implementations()
                .iter()
                .map(IndexImplementationDocument::render)
                .collect(),
            references: document
                .references()
                .iter()
                .map(|reference| IndexReferenceDocument {
                    from: reference.from().to_owned(),
                    written: reference.written().to_owned(),
                    source: IndexSourceDocument::render(reference.source()),
                    to: reference.to().map(str::to_owned),
                })
                .collect(),
        }
    }
}
