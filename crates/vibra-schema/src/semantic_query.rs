//! Wire adapter for the M2 semantic workspace-position query.

use serde::Serialize;
use vibra_diagnostics::LineIndex;
use vibra_workspace::query::{
    ApplicationContract, EffectRowFact, EffectsFact, ImportAlias, LabelledType,
    LocalBinding, QueryIdentity, SemanticFact, SemanticType, WorkspacePositionQuery,
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

/// A semantic identity rendered for a tooling consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryIdentityDocument {
    /// Closed entity kind.
    pub kind: String,
    /// Canonical identity spelling.
    pub canonical: String,
}

/// A closed effect row: the record `{ "roots": [...] }` of the effects
/// chapter. A consumer never assumes the record holds only `roots`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectRowDocument {
    /// The canonical identities of the roots, in canonical order.
    pub roots: Vec<String>,
}

/// A structured primitive or function type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// A function's closed effect row, absent on every other kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<EffectRowDocument>,
}

/// A labelled function slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelledTypeDocument {
    /// Label without its trailing colon.
    pub name: String,
    /// Slot type.
    #[serde(rename = "type")]
    pub value_type: SemanticTypeDocument,
}

/// A visible lexical binder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalBindingDocument {
    /// Local source spelling.
    pub name: String,
    /// Revision-scoped binder identity.
    pub identity: String,
}

/// A visible imported module alias.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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

/// What a contract member call resolved to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDispatchDocument {
    /// The interface's path.
    pub interface: String,
    /// The contract member's name.
    pub member: String,
    /// The type that selected the implementation.
    pub receiver: SemanticTypeDocument,
    /// `static`, `default`, `dynamic`, or `closed`.
    pub selection: String,
    /// Whether the member is selected from the written expected type.
    pub destination: bool,
}

/// The contract of one application: a function call, a declared type's
/// constructor, or an interface's contract member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// For a contract member call, what it resolved to; else null.
    pub dispatch: Option<ContractDispatchDocument>,
}

/// The shape of one `match` arm's pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternFactDocument {
    /// Closed pattern kind.
    pub kind: String,
    /// The type of the value the pattern is matched against.
    pub scrutinee_type: SemanticTypeDocument,
    /// For an `as` pattern, the member type it narrows to; else null.
    pub narrowed: Option<SemanticTypeDocument>,
    /// For an `as` pattern, every member of the scrutinee's union, in order.
    pub union_members: Vec<SemanticTypeDocument>,
}

/// The smallest function body that encloses a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnclosingEffectsDocument {
    /// The function, operation, or test; null for a `lambda`.
    pub identity: Option<QueryIdentityDocument>,
    /// `function`, `operation`, `lambda`, or `test`.
    pub kind: String,
    /// The written or default-empty ceiling; the complete row of an operation.
    pub ceiling: EffectRowDocument,
    /// The performed row the checker computed.
    pub performed: EffectRowDocument,
}

/// The binary target whose entry reaches a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetEffectsDocument {
    /// The entry declaration.
    pub entry: QueryIdentityDocument,
    /// The target `effects` array as a row: the consent that admitted the
    /// entry.
    pub effects: EffectRowDocument,
}

/// What a witness hop applies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WitnessCalleeDocument {
    /// `function`, `operation`, `function-value`, or `contract-member`.
    pub kind: String,
    /// The canonical identity; null for a function value.
    pub identity: Option<String>,
}

/// One call site of a witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WitnessHopDocument {
    /// The call site, carrying its own source identity.
    pub span: SpanDocument,
    /// What it applies.
    pub callee: WitnessCalleeDocument,
}

/// The chain of call sites that carries one performed root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootWitnessDocument {
    /// The performed root's canonical identity.
    pub root: String,
    /// The call sites in the order of the checker's witness.
    pub hops: Vec<WitnessHopDocument>,
}

/// The effect metadata at a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectsDocument {
    /// The smallest enclosing function body, or null outside every function.
    pub enclosing: Option<EnclosingEffectsDocument>,
    /// The callee's complete row, or null outside an application.
    pub application: Option<EffectRowDocument>,
    /// The binary target that reaches the position, or null.
    pub target: Option<TargetEffectsDocument>,
    /// One witness per performed root; present only under
    /// `--expand effect-witnesses`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witnesses: Option<Vec<RootWitnessDocument>>,
}

/// One complete semantic workspace-position result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// The application contract at the position, when it is one.
    pub application: SemanticFactDocument<ApplicationContractDocument>,
    /// The `match` arm pattern at the position, when it is one.
    pub pattern: SemanticFactDocument<PatternFactDocument>,
    /// The effect metadata at the position.
    pub effects: SemanticFactDocument<EffectsDocument>,
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
            pattern: render_fact(query.pattern(), |pattern| PatternFactDocument {
                kind: pattern.kind().to_owned(),
                scrutinee_type: render_type(pattern.scrutinee_type()),
                narrowed: pattern.narrowed().map(render_type),
                union_members: pattern
                    .union_members()
                    .iter()
                    .map(render_type)
                    .collect(),
            }),
            effects: render_fact(query.effects(), render_effects),
        }
    }
}

fn render_row(row: &EffectRowFact) -> EffectRowDocument {
    EffectRowDocument {
        roots: row.roots().to_vec(),
    }
}

fn render_effects(value: &EffectsFact) -> EffectsDocument {
    EffectsDocument {
        enclosing: value.enclosing().map(|enclosing| EnclosingEffectsDocument {
            identity: enclosing.identity().map(render_identity),
            kind: enclosing.kind().to_owned(),
            ceiling: render_row(enclosing.ceiling()),
            performed: render_row(enclosing.performed()),
        }),
        application: value.application().map(render_row),
        target: value.target().map(|target| TargetEffectsDocument {
            entry: render_identity(target.entry()),
            effects: render_row(target.effects()),
        }),
        witnesses: value.witnesses().map(|witnesses| {
            witnesses
                .iter()
                .map(|witness| RootWitnessDocument {
                    root: witness.root().to_owned(),
                    hops: witness
                        .hops()
                        .iter()
                        .map(|hop| WitnessHopDocument {
                            span: SpanDocument {
                                source_id: Some(hop.source_id().to_owned()),
                                start: hop.span().start(),
                                end: hop.span().end(),
                                start_position: hop.start_position().into(),
                                end_position: hop.end_position().into(),
                            },
                            callee: WitnessCalleeDocument {
                                kind: hop.callee().kind().to_owned(),
                                identity: hop.callee().identity().map(str::to_owned),
                            },
                        })
                        .collect(),
                })
                .collect()
        }),
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
        kind: value.kind().as_str().to_owned(),
        name: value.name().to_owned(),
        parameters: value.parameters().iter().map(render_type).collect(),
        result: value.result().map(|result| Box::new(render_type(result))),
        labelled: value.labelled().iter().map(render_labelled).collect(),
        effects: value.effects().map(render_row),
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
        kind: value.kind().as_str().to_owned(),
        callee: value.callee().map(render_identity),
        callee_type: value.callee_type().map(render_type),
        positional: value.positional().iter().map(render_type).collect(),
        labelled: value.labelled().iter().map(render_labelled).collect(),
        result_type: render_type(value.result_type()),
        dispatch: value.dispatch().map(|dispatch| ContractDispatchDocument {
            interface: dispatch.interface().to_owned(),
            member: dispatch.member().to_owned(),
            receiver: render_type(dispatch.receiver()),
            selection: dispatch.selection().as_str().to_owned(),
            destination: dispatch.destination(),
        }),
    }
}
