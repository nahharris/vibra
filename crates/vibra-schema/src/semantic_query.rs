//! Wire adapter for the M2 semantic workspace-position query.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::{Map, Value};
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
    T: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = object(value, "semantic fact").map_err(D::Error::custom)?;
        ensure_keys(&object, &["status", "value"]).map_err(D::Error::custom)?;
        let status = required_string(&object, "status").map_err(D::Error::custom)?;
        let value_field = object
            .get("value")
            .ok_or_else(|| D::Error::custom("semantic fact requires a value field"))?;
        let parsed = if value_field.is_null() {
            None
        } else {
            Some(
                serde_json::from_value(value_field.clone())
                    .map_err(|error| D::Error::custom(error.to_string()))?,
            )
        };
        match status.as_str() {
            "exact" if parsed.is_some() => Ok(Self {
                status,
                value: parsed,
            }),
            "recovered" => Ok(Self {
                status,
                value: parsed,
            }),
            "unavailable" if parsed.is_none() => Ok(Self {
                status,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryIdentityDocument {
    /// Closed entity kind.
    pub kind: String,
    /// Canonical identity spelling.
    pub canonical: String,
}

impl<'de> Deserialize<'de> for QueryIdentityDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_identity(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// A structured primitive or function type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for SemanticTypeDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_type(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// A labelled function slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabelledTypeDocument {
    /// Label without its trailing colon.
    pub name: String,
    /// Slot type.
    #[serde(rename = "type")]
    pub value_type: SemanticTypeDocument,
}

impl<'de> Deserialize<'de> for LabelledTypeDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_labelled(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for ApplicationContractDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_application(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// One complete semantic workspace-position result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for WorkspacePositionQueryDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_workspace(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

const IDENTITY_KINDS: &[&str] = &[
    "module",
    "type",
    "interface",
    "effect",
    "value",
    "function",
    "test",
    "field",
    "variant",
    "operation",
    "binder",
];

const PRIMITIVE_NAMES: &[&str] = &[
    "bool", "void", "char", "str", "bytes", "atom", "i8", "i16", "i32", "i64", "u8",
    "u16", "u32", "u64", "f32", "f64",
];

const ROLE_NAMES: &[&str] = &[
    "@atom-value",
    "@entity-reference",
    "@code-reference",
    "@literal",
    "@local-binding",
    "@discard",
    "@declaration",
    "@application",
    "@unknown",
];

const CONTEXT_NAMES: &[&str] = &[
    "module",
    "initializer",
    "function",
    "parameter",
    "lambda",
    "let-value",
    "let-body",
    "branch",
    "argument",
    "result",
    "trivia",
    "recovery",
];

fn object(value: Value, name: &str) -> Result<Map<String, Value>, String> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| format!("{name} must be an object"))
}

fn ensure_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    if let Some(unknown) = object
        .keys()
        .find(|key| !allowed.iter().any(|allowed| allowed == key))
    {
        return Err(format!("unknown field {unknown:?}"));
    }
    Ok(())
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("missing required field {key:?}"))
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    let value = required(object, key)?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("field {key:?} must be a string"))
}

fn required_usize(object: &Map<String, Value>, key: &str) -> Result<usize, String> {
    let value = required(object, key)?;
    let number = value
        .as_u64()
        .ok_or_else(|| format!("field {key:?} must be a non-negative integer"))?;
    usize::try_from(number).map_err(|_| format!("field {key:?} is too large"))
}

fn parse_identity(value: Value) -> Result<QueryIdentityDocument, String> {
    let object = object(value, "identity")?;
    ensure_keys(&object, &["kind", "canonical"])?;
    let kind = required_string(&object, "kind")?;
    if !IDENTITY_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown identity kind {kind:?}"));
    }
    let canonical = required_string(&object, "canonical")?;
    if canonical.is_empty() {
        return Err("identity canonical spelling cannot be empty".to_owned());
    }
    Ok(QueryIdentityDocument { kind, canonical })
}

fn parse_labelled(value: Value) -> Result<LabelledTypeDocument, String> {
    let object = object(value, "labelled type")?;
    ensure_keys(&object, &["name", "type"])?;
    let name = required_string(&object, "name")?;
    if name.is_empty() {
        return Err("labelled type name cannot be empty".to_owned());
    }
    let value_type = serde_json::from_value(required(&object, "type")?.clone())
        .map_err(|error| format!("invalid labelled type: {error}"))?;
    Ok(LabelledTypeDocument { name, value_type })
}

fn parse_type(value: Value) -> Result<SemanticTypeDocument, String> {
    let object = object(value, "semantic type")?;
    ensure_keys(
        &object,
        &["kind", "name", "parameters", "result", "labelled"],
    )?;
    let kind = required_string(&object, "kind")?;
    let name = required_string(&object, "name")?;
    let parameters_value = required(&object, "parameters")?;
    let parameters = parameters_value
        .as_array()
        .ok_or_else(|| "semantic type parameters must be an array".to_owned())?
        .iter()
        .cloned()
        .map(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("invalid parameter type: {error}"))
        })
        .collect::<Result<Vec<SemanticTypeDocument>, _>>()?;
    let result_value = required(&object, "result")?;
    let result = if result_value.is_null() {
        None
    } else {
        Some(Box::new(
            serde_json::from_value(result_value.clone())
                .map_err(|error| format!("invalid result type: {error}"))?,
        ))
    };
    let labelled_value = required(&object, "labelled")?;
    let labelled = labelled_value
        .as_array()
        .ok_or_else(|| "semantic type labelled slots must be an array".to_owned())?
        .iter()
        .cloned()
        .map(parse_labelled)
        .collect::<Result<Vec<_>, _>>()?;

    match kind.as_str() {
        "primitive" => {
            if !PRIMITIVE_NAMES.contains(&name.as_str()) {
                return Err(format!("unknown primitive type {name:?}"));
            }
            if !parameters.is_empty() || result.is_some() || !labelled.is_empty() {
                return Err("primitive types cannot carry function fields".to_owned());
            }
        }
        "function" => {
            if name != "fn" {
                return Err("function semantic types must use name fn".to_owned());
            }
            if result.is_none() {
                return Err("function semantic types require a result".to_owned());
            }
        }
        _ => return Err(format!("unknown semantic type kind {kind:?}")),
    }
    Ok(SemanticTypeDocument {
        kind,
        name,
        parameters,
        result,
        labelled,
    })
}

fn parse_application(value: Value) -> Result<ApplicationContractDocument, String> {
    let object = object(value, "application contract")?;
    ensure_keys(
        &object,
        &[
            "kind",
            "callee",
            "calleeType",
            "positional",
            "labelled",
            "resultType",
        ],
    )?;
    let kind = required_string(&object, "kind")?;
    if kind != "@function" {
        return Err(format!("unknown application kind {kind:?}"));
    }
    let callee_value = required(&object, "callee")?;
    let callee = if callee_value.is_null() {
        None
    } else {
        Some(parse_identity(callee_value.clone())?)
    };
    let callee_type_value = required(&object, "calleeType")?;
    let callee_type = if callee_type_value.is_null() {
        None
    } else {
        Some(
            serde_json::from_value(callee_type_value.clone())
                .map_err(|error| format!("invalid callee type: {error}"))?,
        )
    };
    let positional_value = required(&object, "positional")?;
    let positional = positional_value
        .as_array()
        .ok_or_else(|| "application positional types must be an array".to_owned())?
        .iter()
        .cloned()
        .map(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("invalid positional type: {error}"))
        })
        .collect::<Result<Vec<SemanticTypeDocument>, _>>()?;
    let labelled_value = required(&object, "labelled")?;
    let labelled = labelled_value
        .as_array()
        .ok_or_else(|| "application labelled types must be an array".to_owned())?
        .iter()
        .cloned()
        .map(parse_labelled)
        .collect::<Result<Vec<_>, _>>()?;
    let result_type = serde_json::from_value(required(&object, "resultType")?.clone())
        .map_err(|error| format!("invalid application result type: {error}"))?;
    Ok(ApplicationContractDocument {
        kind,
        callee,
        callee_type,
        positional,
        labelled,
        result_type,
    })
}

fn parse_workspace(value: Value) -> Result<WorkspacePositionQueryDocument, String> {
    let object = object(value, "workspace position query")?;
    ensure_keys(
        &object,
        &[
            "schemaVersion",
            "workspaceRevision",
            "sourceId",
            "offset",
            "nodeId",
            "structural",
            "role",
            "context",
            "identity",
            "expectedType",
            "observedType",
            "visibleLocals",
            "visibleImports",
            "declarationCandidates",
            "application",
        ],
    )?;
    let schema_version = required(&object, "schemaVersion")?
        .as_u64()
        .ok_or_else(|| "schemaVersion must be an integer".to_owned())?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(format!("unsupported schemaVersion {schema_version}"));
    }
    let workspace_revision = required_string(&object, "workspaceRevision")?;
    if !valid_revision(&workspace_revision) {
        return Err(
            "workspaceRevision must be sha256:<64 lowercase hex digits>".to_owned()
        );
    }
    let source_id = required_string(&object, "sourceId")?;
    if source_id.is_empty() {
        return Err("sourceId cannot be empty".to_owned());
    }
    let offset = required_usize(&object, "offset")?;
    let node_id = required_string(&object, "nodeId")?;
    validate_node_id(&node_id, &source_id)?;
    let structural: SourcePositionQueryDocument =
        serde_json::from_value(required(&object, "structural")?.clone())
            .map_err(|error| format!("invalid structural query: {error}"))?;
    let role: SemanticFactDocument<String> =
        serde_json::from_value(required(&object, "role")?.clone())
            .map_err(|error| format!("invalid role fact: {error}"))?;
    if role
        .value
        .as_deref()
        .is_some_and(|value| !ROLE_NAMES.contains(&value))
    {
        return Err("role fact contains an unknown role".to_owned());
    }
    let context: SemanticFactDocument<String> =
        serde_json::from_value(required(&object, "context")?.clone())
            .map_err(|error| format!("invalid context fact: {error}"))?;
    if context
        .value
        .as_deref()
        .is_some_and(|value| !CONTEXT_NAMES.contains(&value))
    {
        return Err("context fact contains an unknown context".to_owned());
    }
    let identity: SemanticFactDocument<QueryIdentityDocument> =
        serde_json::from_value(required(&object, "identity")?.clone())
            .map_err(|error| format!("invalid identity fact: {error}"))?;
    let expected_type: SemanticFactDocument<SemanticTypeDocument> =
        serde_json::from_value(required(&object, "expectedType")?.clone())
            .map_err(|error| format!("invalid expected type fact: {error}"))?;
    let observed_type: SemanticFactDocument<SemanticTypeDocument> =
        serde_json::from_value(required(&object, "observedType")?.clone())
            .map_err(|error| format!("invalid observed type fact: {error}"))?;
    let visible_locals: SemanticFactDocument<Vec<LocalBindingDocument>> =
        serde_json::from_value(required(&object, "visibleLocals")?.clone())
            .map_err(|error| format!("invalid visible locals fact: {error}"))?;
    let visible_imports: SemanticFactDocument<Vec<ImportAliasDocument>> =
        serde_json::from_value(required(&object, "visibleImports")?.clone())
            .map_err(|error| format!("invalid visible imports fact: {error}"))?;
    let declaration_candidates: SemanticFactDocument<Vec<QueryIdentityDocument>> =
        serde_json::from_value(required(&object, "declarationCandidates")?.clone())
            .map_err(|error| format!("invalid declaration candidates fact: {error}"))?;
    let application: SemanticFactDocument<ApplicationContractDocument> =
        serde_json::from_value(required(&object, "application")?.clone())
            .map_err(|error| format!("invalid application fact: {error}"))?;
    if structural
        .source_id
        .as_deref()
        .is_some_and(|value| value != source_id)
    {
        return Err("structural sourceId must match sourceId".to_owned());
    }
    Ok(WorkspacePositionQueryDocument {
        schema_version: u32::try_from(schema_version)
            .map_err(|_| "schemaVersion is too large".to_owned())?,
        workspace_revision,
        source_id,
        offset,
        node_id,
        structural,
        role,
        context,
        identity,
        expected_type,
        observed_type,
        visible_locals,
        visible_imports,
        declaration_candidates,
        application,
    })
}

fn valid_revision(value: &str) -> bool {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_node_id(value: &str, source_id: &str) -> Result<(), String> {
    let prefix = format!("{source_id}#");
    let Some(range) = value.strip_prefix(&prefix) else {
        return Err("nodeId must begin with sourceId#".to_owned());
    };
    let Some((start, end)) = range.split_once('-') else {
        return Err("nodeId must contain a start-end range".to_owned());
    };
    if start.is_empty()
        || end.is_empty()
        || !start.bytes().all(|byte| byte.is_ascii_digit())
        || !end.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("nodeId range must contain decimal offsets".to_owned());
    }
    Ok(())
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
