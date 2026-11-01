//! Wire adapter for the M2 semantic workspace-position query.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::{Map, Value};
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

/// A closed effect row: the record `{ "roots": [...] }` of the effects
/// chapter. A consumer never assumes the record holds only `roots`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectRowDocument {
    /// The canonical identities of the roots, in canonical order.
    pub roots: Vec<String>,
}

impl<'de> Deserialize<'de> for EffectRowDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_row(Value::deserialize(deserializer)?).map_err(D::Error::custom)
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
    /// A function's closed effect row, absent on every other kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<EffectRowDocument>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalBindingDocument {
    /// Local source spelling.
    pub name: String,
    /// Revision-scoped binder identity.
    pub identity: String,
}

impl<'de> Deserialize<'de> for LocalBindingDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_local(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// A visible imported module alias.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for ImportAliasDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_import(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// What a contract member call resolved to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// For a contract member call, what it resolved to; else null.
    pub dispatch: Option<ContractDispatchDocument>,
}

/// The shape of one `match` arm's pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

impl<'de> Deserialize<'de> for PatternFactDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_pattern(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ApplicationContractDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_application(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// The smallest function body that encloses a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

impl<'de> Deserialize<'de> for EnclosingEffectsDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_enclosing(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// The binary target whose entry reaches a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetEffectsDocument {
    /// The entry declaration.
    pub entry: QueryIdentityDocument,
    /// The target `effects` array as a row: the consent that admitted the
    /// entry.
    pub effects: EffectRowDocument,
}

impl<'de> Deserialize<'de> for TargetEffectsDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_target(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// What a witness hop applies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WitnessCalleeDocument {
    /// `function`, `operation`, `function-value`, or `contract-member`.
    pub kind: String,
    /// The canonical identity; null for a function value.
    pub identity: Option<String>,
}

/// One call site of a witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WitnessHopDocument {
    /// The call site, carrying its own source identity.
    pub span: SpanDocument,
    /// What it applies.
    pub callee: WitnessCalleeDocument,
}

/// The chain of call sites that carries one performed root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootWitnessDocument {
    /// The performed root's canonical identity.
    pub root: String,
    /// The call sites in the order of the checker's witness.
    pub hops: Vec<WitnessHopDocument>,
}

/// The effect metadata at a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

impl<'de> Deserialize<'de> for EffectsDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        parse_effects(Value::deserialize(deserializer)?).map_err(D::Error::custom)
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
    /// The application contract at the position, when it is one.
    pub application: SemanticFactDocument<ApplicationContractDocument>,
    /// The `match` arm pattern at the position, when it is one.
    pub pattern: SemanticFactDocument<PatternFactDocument>,
    /// The effect metadata at the position.
    pub effects: SemanticFactDocument<EffectsDocument>,
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
    "bool", "void", "never", "char", "str", "bytes", "atom", "i8", "i16", "i32", "i64",
    "u8", "u16", "u32", "u64", "f32", "f64",
];

const APPLICATION_KINDS: &[&str] = &["@function", "@constructor", "@contract"];

const SELECTION_NAMES: &[&str] = &["static", "default", "dynamic", "closed"];

const ENCLOSING_KINDS: &[&str] = &["function", "operation", "lambda", "test"];

const WITNESS_CALLEE_KINDS: &[&str] =
    &["function", "operation", "function-value", "contract-member"];

const PATTERN_KINDS: &[&str] = &[
    "@wildcard",
    "@binding",
    "@literal",
    "@variant",
    "@record",
    "@tuple",
    "@wrapper",
    "@array",
    "@as",
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
    "let-else-fallback",
    "return-operand",
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

fn parse_local(value: Value) -> Result<LocalBindingDocument, String> {
    let object = object(value, "local binding")?;
    ensure_keys(&object, &["name", "identity"])?;
    let name = required_string(&object, "name")?;
    if name.is_empty() {
        return Err("local binding name cannot be empty".to_owned());
    }
    let identity = required_string(&object, "identity")?;
    if !valid_binder_identity(&identity) {
        return Err("local binding identity has an invalid spelling".to_owned());
    }
    Ok(LocalBindingDocument { name, identity })
}

fn parse_import(value: Value) -> Result<ImportAliasDocument, String> {
    let object = object(value, "import alias")?;
    ensure_keys(&object, &["alias", "module", "sourceId", "span"])?;
    let alias = required_string(&object, "alias")?;
    if alias.is_empty() {
        return Err("import alias cannot be empty".to_owned());
    }
    let module_value = required(&object, "module")?;
    let module = if module_value.is_null() {
        None
    } else {
        let module = module_value
            .as_str()
            .ok_or_else(|| "import module must be a string or null".to_owned())?;
        if module.is_empty() {
            return Err("import module cannot be empty".to_owned());
        }
        Some(module.to_owned())
    };
    let source_id = required_string(&object, "sourceId")?;
    if source_id.is_empty() {
        return Err("import sourceId cannot be empty".to_owned());
    }
    let span: SpanDocument = serde_json::from_value(required(&object, "span")?.clone())
        .map_err(|error| format!("invalid import span: {error}"))?;
    validate_span(&span, Some(&source_id))?;
    Ok(ImportAliasDocument {
        alias,
        module,
        source_id,
        span,
    })
}

fn parse_type(value: Value) -> Result<SemanticTypeDocument, String> {
    let object = object(value, "semantic type")?;
    ensure_keys(
        &object,
        &["kind", "name", "parameters", "result", "labelled", "effects"],
    )?;
    let kind = required_string(&object, "kind")?;
    // Only a function carries the closed row its type is compared by.
    let effects = match (kind == "function", object.get("effects")) {
        (true, Some(row)) => Some(
            serde_json::from_value(row.clone())
                .map_err(|error| format!("invalid function effect row: {error}"))?,
        ),
        (true, None) => {
            return Err("function semantic types require an effects row".to_owned());
        }
        (false, None) => None,
        (false, Some(_)) => {
            return Err("only function types carry an effects row".to_owned());
        }
    };
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
        // Every other shape has no result. A structural shape is named by
        // its form; a declared type, an interface, a parameter, and an atom
        // carry their own name.
        "declared" | "interface" | "param" | "atom" | "tuple" | "array" | "dict"
        | "record" | "enum" | "union" => {
            if result.is_some() {
                return Err("only function types carry a result".to_owned());
            }
            if name.is_empty() {
                return Err("a semantic type needs a name".to_owned());
            }
            if matches!(
                kind.as_str(),
                "tuple" | "array" | "dict" | "record" | "enum" | "union"
            ) && name != kind
            {
                return Err(format!("a {kind} type must use name {kind}"));
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
        effects,
    })
}

fn parse_row(value: Value) -> Result<EffectRowDocument, String> {
    let object = object(value, "effect row")?;
    ensure_keys(&object, &["roots"])?;
    let roots = required(&object, "roots")?
        .as_array()
        .ok_or_else(|| "effect row roots must be an array".to_owned())?
        .iter()
        .map(|root| {
            root.as_str()
                .filter(|root| !root.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "an effect root must be a non-empty string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A row is a set: a duplicate root is malformed.
    let mut distinct = roots.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() != roots.len() {
        return Err("effect row roots must be distinct".to_owned());
    }
    Ok(EffectRowDocument { roots })
}

fn parse_enclosing(value: Value) -> Result<EnclosingEffectsDocument, String> {
    let object = object(value, "enclosing effects")?;
    ensure_keys(&object, &["identity", "kind", "ceiling", "performed"])?;
    let kind = required_string(&object, "kind")?;
    if !ENCLOSING_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown enclosing kind {kind:?}"));
    }
    let identity_value = required(&object, "identity")?;
    let identity = if identity_value.is_null() {
        None
    } else {
        Some(parse_identity(identity_value.clone())?)
    };
    // A `lambda` has no identity, and neither has a member written in an
    // `impl` block; an operation and a test always have one.
    let identity_fits = match kind.as_str() {
        "lambda" => identity.is_none(),
        "operation" | "test" => identity.is_some(),
        _ => true,
    };
    if !identity_fits {
        return Err(
            "a lambda has no identity, and an operation and a test have one"
                .to_owned(),
        );
    }
    Ok(EnclosingEffectsDocument {
        identity,
        kind,
        ceiling: parse_row(required(&object, "ceiling")?.clone())?,
        performed: parse_row(required(&object, "performed")?.clone())?,
    })
}

fn parse_target(value: Value) -> Result<TargetEffectsDocument, String> {
    let object = object(value, "target effects")?;
    ensure_keys(&object, &["entry", "effects"])?;
    Ok(TargetEffectsDocument {
        entry: parse_identity(required(&object, "entry")?.clone())?,
        effects: parse_row(required(&object, "effects")?.clone())?,
    })
}

fn parse_hop(value: Value) -> Result<WitnessHopDocument, String> {
    let object = object(value, "witness hop")?;
    ensure_keys(&object, &["span", "callee"])?;
    let span: SpanDocument = serde_json::from_value(required(&object, "span")?.clone())
        .map_err(|error| format!("invalid hop span: {error}"))?;
    validate_span(&span, None)?;
    if span.source_id.is_none() {
        return Err("a hop span carries its source identity".to_owned());
    }
    let callee = object_of(required(&object, "callee")?, "witness callee")?;
    ensure_keys(&callee, &["kind", "identity"])?;
    let kind = required_string(&callee, "kind")?;
    if !WITNESS_CALLEE_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown witness callee kind {kind:?}"));
    }
    let identity_value = required(&callee, "identity")?;
    let identity = if identity_value.is_null() {
        None
    } else {
        Some(
            identity_value
                .as_str()
                .filter(|identity| !identity.is_empty())
                .ok_or_else(|| "a witness identity must be a non-empty string".to_owned())?
                .to_owned(),
        )
    };
    // A function value has no identity; so does a member written in an `impl`
    // block, but an operation and a contract member always have one.
    let identity_fits = match kind.as_str() {
        "function-value" => identity.is_none(),
        "operation" | "contract-member" => identity.is_some(),
        _ => true,
    };
    if !identity_fits {
        return Err(
            "a function value has no identity, and an operation and a contract member have one"
                .to_owned(),
        );
    }
    Ok(WitnessHopDocument {
        span,
        callee: WitnessCalleeDocument { kind, identity },
    })
}

fn object_of(value: &Value, name: &str) -> Result<Map<String, Value>, String> {
    object(value.clone(), name)
}

fn parse_effects(value: Value) -> Result<EffectsDocument, String> {
    let object = object(value, "effects")?;
    ensure_keys(
        &object,
        &["enclosing", "application", "target", "witnesses"],
    )?;
    let optional = |key: &str| -> Result<Option<Value>, String> {
        let value = required(&object, key)?;
        Ok((!value.is_null()).then(|| value.clone()))
    };
    let enclosing = optional("enclosing")?.map(parse_enclosing).transpose()?;
    let application = optional("application")?.map(parse_row).transpose()?;
    let target = optional("target")?.map(parse_target).transpose()?;
    let witnesses = object
        .get("witnesses")
        .map(|witnesses| {
            witnesses
                .as_array()
                .ok_or_else(|| "effect witnesses must be an array".to_owned())?
                .iter()
                .map(|witness| {
                    let witness = object_of(witness, "root witness")?;
                    ensure_keys(&witness, &["root", "hops"])?;
                    Ok(RootWitnessDocument {
                        root: required_string(&witness, "root")?,
                        hops: required(&witness, "hops")?
                            .as_array()
                            .ok_or_else(|| "witness hops must be an array".to_owned())?
                            .iter()
                            .cloned()
                            .map(parse_hop)
                            .collect::<Result<Vec<_>, _>>()?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    // One witness for each performed root of the enclosing body, in the
    // order of the row, and none outside every function.
    if let Some(witnesses) = &witnesses {
        let performed = enclosing
            .as_ref()
            .map(|enclosing| enclosing.performed.roots.as_slice())
            .unwrap_or_default();
        if witnesses.len() != performed.len()
            || witnesses
                .iter()
                .zip(performed)
                .any(|(witness, root)| &witness.root != root)
        {
            return Err(
                "witnesses must hold one record for each performed root, in order"
                    .to_owned(),
            );
        }
    }
    Ok(EffectsDocument {
        enclosing,
        application,
        target,
        witnesses,
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
            "dispatch",
        ],
    )?;
    let kind = required_string(&object, "kind")?;
    if !APPLICATION_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown application kind {kind:?}"));
    }
    let dispatch_value = required(&object, "dispatch")?;
    let dispatch = if dispatch_value.is_null() {
        None
    } else {
        Some(parse_dispatch(dispatch_value.clone())?)
    };
    // Exactly a contract member call resolves to an implementation.
    if (kind == "@contract") != dispatch.is_some() {
        return Err("dispatch belongs to exactly a @contract application".to_owned());
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
        dispatch,
    })
}

fn parse_dispatch(value: Value) -> Result<ContractDispatchDocument, String> {
    let object = object(value, "contract dispatch")?;
    ensure_keys(
        &object,
        &[
            "interface",
            "member",
            "receiver",
            "selection",
            "destination",
        ],
    )?;
    let selection = required_string(&object, "selection")?;
    if !SELECTION_NAMES.contains(&selection.as_str()) {
        return Err(format!("unknown dispatch selection {selection:?}"));
    }
    Ok(ContractDispatchDocument {
        interface: required_string(&object, "interface")?,
        member: required_string(&object, "member")?,
        receiver: serde_json::from_value(required(&object, "receiver")?.clone())
            .map_err(|error| format!("invalid dispatch receiver: {error}"))?,
        selection,
        destination: required(&object, "destination")?
            .as_bool()
            .ok_or_else(|| "dispatch destination must be a boolean".to_owned())?,
    })
}

fn parse_pattern(value: Value) -> Result<PatternFactDocument, String> {
    let object = object(value, "pattern fact")?;
    ensure_keys(
        &object,
        &["kind", "scrutineeType", "narrowed", "unionMembers"],
    )?;
    let kind = required_string(&object, "kind")?;
    if !PATTERN_KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown pattern kind {kind:?}"));
    }
    let narrowed_value = required(&object, "narrowed")?;
    let narrowed = if narrowed_value.is_null() {
        None
    } else {
        Some(
            serde_json::from_value(narrowed_value.clone())
                .map_err(|error| format!("invalid narrowed type: {error}"))?,
        )
    };
    let union_members = required(&object, "unionMembers")?
        .as_array()
        .ok_or_else(|| "pattern union members must be an array".to_owned())?
        .iter()
        .cloned()
        .map(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("invalid union member type: {error}"))
        })
        .collect::<Result<Vec<SemanticTypeDocument>, _>>()?;
    // Only an `as` pattern narrows a union.
    if (kind == "@as") != narrowed.is_some()
        || (kind != "@as" && !union_members.is_empty())
    {
        return Err("only an @as pattern narrows a union".to_owned());
    }
    Ok(PatternFactDocument {
        kind,
        scrutinee_type: serde_json::from_value(
            required(&object, "scrutineeType")?.clone(),
        )
        .map_err(|error| format!("invalid scrutinee type: {error}"))?,
        narrowed,
        union_members,
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
            "pattern",
            "effects",
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
    let pattern: SemanticFactDocument<PatternFactDocument> =
        serde_json::from_value(required(&object, "pattern")?.clone())
            .map_err(|error| format!("invalid pattern fact: {error}"))?;
    let effects: SemanticFactDocument<EffectsDocument> =
        serde_json::from_value(required(&object, "effects")?.clone())
            .map_err(|error| format!("invalid effects fact: {error}"))?;
    let (node_start, node_end) = parse_node_id(&node_id, &source_id)?;
    validate_structural(&structural, &source_id, offset, node_start, node_end)?;
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
        pattern,
        effects,
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

fn parse_node_id(value: &str, source_id: &str) -> Result<(usize, usize), String> {
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
    let start = start
        .parse::<usize>()
        .map_err(|_| "nodeId start is too large".to_owned())?;
    let end = end
        .parse::<usize>()
        .map_err(|_| "nodeId end is too large".to_owned())?;
    Ok((start, end))
}

fn validate_structural(
    structural: &SourcePositionQueryDocument,
    source_id: &str,
    offset: usize,
    node_start: usize,
    node_end: usize,
) -> Result<(), String> {
    if structural.schema_version != SCHEMA_VERSION {
        return Err(
            "structural schemaVersion must match the workspace envelope".to_owned()
        );
    }
    if structural.offset != offset {
        return Err("structural offset must match the workspace offset".to_owned());
    }
    if structural.source_id.as_deref() != Some(source_id) {
        return Err("structural sourceId must match sourceId".to_owned());
    }
    if structural.span.source_id.as_deref() != Some(source_id) {
        return Err("structural span sourceId must match sourceId".to_owned());
    }
    validate_span(&structural.span, Some(source_id))?;
    if structural.span.start != node_start || structural.span.end != node_end {
        return Err("nodeId must identify the structural span exactly".to_owned());
    }
    if !matches!(structural.mode.as_str(), "source" | "data") {
        return Err("structural mode has an unknown value".to_owned());
    }
    if !matches!(
        structural.syntax_kind.as_str(),
        "root"
            | "list"
            | "atom"
            | "whitespace"
            | "line-comment"
            | "open-paren"
            | "close-paren"
            | "error"
    ) {
        return Err("structural syntaxKind has an unknown value".to_owned());
    }
    if !matches!(
        structural.category.as_str(),
        "module"
            | "declaration"
            | "type"
            | "pattern"
            | "expression"
            | "declaration-attribute"
            | "effect-row"
            | "data-field"
            | "trivia"
            | "recovery"
    ) {
        return Err("structural category has an unknown value".to_owned());
    }
    if !matches!(
        structural.status.as_str(),
        "exact" | "recovered" | "unavailable"
    ) {
        return Err("structural status has an unknown value".to_owned());
    }
    Ok(())
}

fn validate_span(span: &SpanDocument, source_id: Option<&str>) -> Result<(), String> {
    if span.start > span.end {
        return Err("span start cannot exceed end".to_owned());
    }
    if span
        .source_id
        .as_deref()
        .is_some_and(|value| value.is_empty())
    {
        return Err("span sourceId cannot be empty".to_owned());
    }
    if let Some(source_id) = source_id
        && span.source_id.as_deref() != Some(source_id)
    {
        return Err("span sourceId does not match its owning source".to_owned());
    }
    if span.start_position.line == 0
        || span.start_position.column == 0
        || span.end_position.line == 0
        || span.end_position.column == 0
    {
        return Err("span positions are one-based".to_owned());
    }
    Ok(())
}

fn valid_binder_identity(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("binder:") else {
        return false;
    };
    let Some((source_id, range)) = rest.rsplit_once(':') else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    !source_id.is_empty()
        && !start.is_empty()
        && !end.is_empty()
        && start.bytes().all(|byte| byte.is_ascii_digit())
        && end.bytes().all(|byte| byte.is_ascii_digit())
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
