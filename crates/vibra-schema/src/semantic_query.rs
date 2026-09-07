//! Wire adapter for the M2 semantic workspace-position query.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use vibra_diagnostics::LineIndex;
use vibra_workspace::query::{
    ApplicationContract, ImportAlias, LabelledType, LocalBinding, QueryIdentity,
    SemanticFact, SemanticType, SemanticTypeKind, WorkspacePositionQuery,
};

use crate::{SCHEMA_VERSION, SourcePositionQueryDocument, SpanDocument};

/// The published JSON Schema for [`WorkspacePositionQueryDocument`].
pub const WORKSPACE_POSITION_QUERY_SCHEMA: &str =
    include_str!("../schemas/v1/workspace-position-query.json");

/// One semantic fact with its independent wire status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticFactDocument<T> {
    /// Exact, recovered, or unavailable status.
    pub status: String,
    /// The fact value, or null when unavailable.
    pub value: Option<T>,
}

impl<'de, T> Deserialize<'de> for SemanticFactDocument<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Raw<T> {
            status: String,
            value: Option<T>,
        }

        let raw = Raw::deserialize(deserializer)?;
        match raw.status.as_str() {
            "exact" if raw.value.is_some() => Ok(Self {
                status: raw.status,
                value: raw.value,
            }),
            "recovered" => Ok(Self {
                status: raw.status,
                value: raw.value,
            }),
            "unavailable" if raw.value.is_none() => Ok(Self {
                status: raw.status,
                value: None,
            }),
            "exact" => Err(D::Error::custom(
                "an exact semantic fact must carry a non-null value",
            )),
            "unavailable" => Err(D::Error::custom(
                "an unavailable semantic fact must carry a null value",
            )),
            _ => Err(D::Error::custom("unknown semantic fact status")),
        }
    }
}

/// A semantic identity rendered for a tooling consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryIdentityDocument {
    /// Closed entity kind.
    pub kind: String,
    /// Canonical identity spelling.
    pub canonical: String,
}

/// A structured primitive or function type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticTypeDocument {
    /// `primitive` or `function`.
    pub kind: String,
    /// Primitive name or `fn`.
    pub name: String,
    /// Ordered function parameters.
    pub parameters: Vec<Self>,
    /// Function result, or null for a primitive.
    pub result: Option<Box<Self>>,
    /// Ordered labelled function slots.
    pub labelled: Vec<LabelledTypeDocument>,
}

/// A labelled function slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabelledTypeDocument {
    /// Label without its trailing colon.
    pub name: String,
    /// Slot type.
    #[serde(rename = "type")]
    pub value_type: SemanticTypeDocument,
}

/// A visible lexical binder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalBindingDocument {
    /// Local source spelling.
    pub name: String,
    /// Revision-scoped binder identity.
    pub identity: String,
}

/// A visible imported module alias.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportAliasDocument {
    /// Local alias.
    pub alias: String,
    /// Canonical module identity when resolved.
    pub module: Option<String>,
    /// Source identity of the import declaration.
    pub source_id: String,
    /// Import declaration span.
    pub span: SpanDocument,
}

/// A supported M2 function application contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationContractDocument {
    /// Closed application kind.
    pub kind: String,
    /// Resolved callee identity when available.
    pub callee: Option<QueryIdentityDocument>,
    /// Callable type when available.
    pub callee_type: Option<SemanticTypeDocument>,
    /// Ordered positional operand types.
    pub positional: Vec<SemanticTypeDocument>,
    /// Ordered labelled operand types.
    pub labelled: Vec<LabelledTypeDocument>,
    /// Exact application result type.
    pub result_type: SemanticTypeDocument,
}

/// One complete semantic workspace-position result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePositionQueryDocument {
    /// Major schema version.
    pub schema_version: u32,
    /// Immutable snapshot revision.
    pub workspace_revision: String,
    /// Source identity owning the query.
    pub source_id: String,
    /// Queried byte offset.
    pub offset: usize,
    /// Revision-scoped selected-node locator.
    pub node_id: String,
    /// Complete M1 structural result.
    pub structural: SourcePositionQueryDocument,
    /// Semantic token role.
    pub role: SemanticFactDocument<String>,
    /// Semantic lexical context.
    pub context: SemanticFactDocument<String>,
    /// Resolved identity, when available.
    pub identity: SemanticFactDocument<QueryIdentityDocument>,
    /// Expected type, when available.
    pub expected_type: SemanticFactDocument<SemanticTypeDocument>,
    /// Observed type, when available.
    pub observed_type: SemanticFactDocument<SemanticTypeDocument>,
    /// Visible lexical locals.
    pub visible_locals: SemanticFactDocument<Vec<LocalBindingDocument>>,
    /// Visible imported aliases.
    pub visible_imports: SemanticFactDocument<Vec<ImportAliasDocument>>,
    /// Candidate declaration identities.
    pub declaration_candidates: SemanticFactDocument<Vec<QueryIdentityDocument>>,
    /// Supported function application contract.
    pub application: SemanticFactDocument<ApplicationContractDocument>,
}

impl WorkspacePositionQueryDocument {
    /// Renders a workspace query against its source line index.
    #[must_use]
    pub fn render(query: &WorkspacePositionQuery, index: &LineIndex<'_>) -> Self {
        Self::render_with_source(query, index)
    }

    /// Renders a workspace query while retaining its captured source identity.
    #[must_use]
    pub fn render_with_source(
        query: &WorkspacePositionQuery,
        index: &LineIndex<'_>,
    ) -> Self {
        let structural = SourcePositionQueryDocument::render_with_source(
            query.structural(),
            index,
            Some(query.source_id()),
        );
        Self {
            schema_version: SCHEMA_VERSION,
            workspace_revision: query.workspace_revision().as_str().to_owned(),
            source_id: query.source_id().to_owned(),
            offset: query.structural().offset(),
            node_id: query.node_id().to_owned(),
            structural,
            role: render_fact(query.role(), Clone::clone),
            context: render_fact(query.context(), Clone::clone),
            identity: render_fact(query.identity(), render_identity),
            expected_type: render_fact(query.expected_type(), render_type),
            observed_type: render_fact(query.observed_type(), render_type),
            visible_locals: render_fact(query.visible_locals(), |locals| {
                locals.iter().map(render_local).collect()
            }),
            visible_imports: render_fact(query.visible_imports(), |imports| {
                imports
                    .iter()
                    .map(|import| render_import(import, index))
                    .collect()
            }),
            declaration_candidates: render_fact(
                query.declaration_candidates(),
                |identities| identities.iter().map(render_identity).collect(),
            ),
            application: render_fact(query.application(), |application| {
                render_application(application)
            }),
        }
    }
}

fn render_fact<T, U>(
    fact: &SemanticFact<T>,
    render: impl Fn(&T) -> U,
) -> SemanticFactDocument<U> {
    SemanticFactDocument {
        status: fact.status().as_str().to_owned(),
        value: fact.value().map(render),
    }
}

fn render_identity(identity: &QueryIdentity) -> QueryIdentityDocument {
    QueryIdentityDocument {
        kind: identity.kind().to_owned(),
        canonical: identity.canonical().to_owned(),
    }
}

fn render_type(value: &SemanticType) -> SemanticTypeDocument {
    SemanticTypeDocument {
        kind: match value.kind() {
            SemanticTypeKind::Primitive => "primitive".to_owned(),
            SemanticTypeKind::Function => "function".to_owned(),
        },
        name: value.name().to_owned(),
        parameters: value.parameters().iter().map(render_type).collect(),
        result: value.result().map(|result| Box::new(render_type(result))),
        labelled: value.labelled().iter().map(render_labelled).collect(),
    }
}

fn render_labelled(value: &LabelledType) -> LabelledTypeDocument {
    LabelledTypeDocument {
        name: value.name().to_owned(),
        value_type: render_type(value.value_type()),
    }
}

fn render_local(value: &LocalBinding) -> LocalBindingDocument {
    LocalBindingDocument {
        name: value.name().to_owned(),
        identity: value.identity().to_owned(),
    }
}

fn render_import(value: &ImportAlias, index: &LineIndex<'_>) -> ImportAliasDocument {
    ImportAliasDocument {
        alias: value.alias().to_owned(),
        module: value.module().map(str::to_owned),
        source_id: value.source_id().to_owned(),
        span: SpanDocument::render_with_source(
            value.span(),
            index,
            Some(value.source_id()),
        ),
    }
}

fn render_application(value: &ApplicationContract) -> ApplicationContractDocument {
    ApplicationContractDocument {
        kind: "@function".to_owned(),
        callee: value.callee().map(render_identity),
        callee_type: value.callee_type().map(render_type),
        positional: value.positional().iter().map(render_type).collect(),
        labelled: value.labelled().iter().map(render_labelled).collect(),
        result_type: render_type(value.result_type()),
    }
}
