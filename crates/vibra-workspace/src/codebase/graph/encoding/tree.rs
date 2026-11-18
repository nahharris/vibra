//! Canonical checked-definition nodes and typed IR projection.

use std::collections::BTreeMap;
use vibra_ir::{
    CallTarget, Constant, EffectRow, Expr, FunctionSignature, Pattern, Type, Value,
    canonical_f32_text, canonical_f64_text,
};

use super::super::{EdgeKind, GraphEncodingError, ObjectKind};
use super::canonical_frame;
fn row(value: &EffectRow) -> String {
    use vibra_syntax::canonical_string_spelling as quote;
    let mut ordered_roots = value.roots().iter().collect::<Vec<_>>();
    ordered_roots
        .sort_by(|left, right| left.id().as_bytes().cmp(right.id().as_bytes()));
    let roots = ordered_roots
        .iter()
        .map(|root| format!("(record kind: @reference identity: {})", quote(root.id())))
        .collect::<Vec<_>>();
    format!(
        "(record roots: (array{}{}) tail: @none)",
        if roots.is_empty() { "" } else { " " },
        roots.join(" "),
    )
}

/// Closed semantic node tree used by `@checked-definition.v1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) enum DefinitionNode {
    /// Ordered source list.
    List(Vec<Self>),
    /// Syntax atom that is not another node category.
    Atom(String),
    /// Decoded atom literal value.
    AtomValue(String),
    /// User-visible source label.
    Label(String),
    /// Decoded Unicode string literal.
    String(String),
    /// Decoded Unicode character literal.
    Character(char),
    /// Exact byte sequence as lowercase hexadecimal.
    Bytes(Vec<u8>),
    /// Checked integer literal.
    Integer { ty: String, value: String },
    /// Checked float literal in canonical round-tripping spelling.
    Float { ty: String, value: String },
    /// Void literal.
    Void,
    /// Resolver-confirmed nonlocal identity.
    Reference(String),
    /// Normalized lexical or generic binder.
    Binder { kind: String, index: u64 },
}

impl DefinitionNode {
    fn canonical(&self) -> String {
        use vibra_syntax::canonical_string_spelling as quote;
        match self {
            Self::List(items) => format!(
                "(record kind: @list items: (array{}{}))",
                if items.is_empty() { "" } else { " " },
                items
                    .iter()
                    .map(Self::canonical)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Atom(value) => {
                format!("(record kind: @atom value: {})", quote(value))
            }
            Self::AtomValue(value) => {
                format!("(record kind: @atom-value value: {})", quote(value))
            }
            Self::Label(name) => {
                format!("(record kind: @label name: {})", quote(name))
            }
            Self::String(value) => {
                format!("(record kind: @string value: {})", quote(value))
            }
            Self::Character(value) => format!(
                "(record kind: @character value: {})",
                quote(&value.to_string())
            ),
            Self::Bytes(value) => format!(
                "(record kind: @bytes value: {})",
                quote(
                    &value
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                )
            ),
            Self::Integer { ty, value } => format!(
                "(record kind: @integer type: @{} value: {})",
                ty,
                quote(value)
            ),
            Self::Float { ty, value } => format!(
                "(record kind: @float type: @{} value: {})",
                ty,
                quote(value)
            ),
            Self::Void => "(record kind: @void)".to_owned(),
            Self::Reference(identity) => {
                format!("(record kind: @reference identity: {})", quote(identity))
            }
            Self::Binder { kind, index } => format!(
                "(record kind: @binder binder-kind: @{} index: {}u64)",
                kind, index
            ),
        }
    }
}

/// One resolver-confirmed dependency found while projecting a checked tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) struct PendingReference {
    /// Canonical entity identity or closed compiler/host identity.
    pub identity: String,
    /// Semantic edge kind.
    pub kind: EdgeKind,
    /// Owner-relative path for the occurrence.
    pub part: Vec<String>,
    /// Checked call facts used to select eligible implementation objects.
    pub dispatch: Option<ContractDispatch>,
}

/// Typed facts from one checked contract call, retained for implementation
/// eligibility without resolving or checking the call again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codebase::graph) struct ContractDispatch {
    /// The exact target selected by the checker.
    pub target: CallTarget,
    /// Checked operand types in evaluation order.
    pub actual_operand_types: Vec<Type>,
}

pub(in crate::codebase::graph) fn atom(value: impl Into<String>) -> DefinitionNode {
    DefinitionNode::Atom(value.into())
}

pub(in crate::codebase::graph) fn string(value: impl Into<String>) -> DefinitionNode {
    DefinitionNode::String(value.into())
}

pub(in crate::codebase::graph) fn array(
    values: impl IntoIterator<Item = DefinitionNode>,
) -> DefinitionNode {
    DefinitionNode::List(values.into_iter().collect())
}

pub(in crate::codebase::graph) fn tagged(
    name: impl Into<String>,
    fields: impl IntoIterator<Item = (String, DefinitionNode)>,
) -> DefinitionNode {
    let mut values = vec![atom(name)];
    values.extend(fields.into_iter().map(|(field_name, value)| {
        DefinitionNode::List(vec![atom(field_name), value])
    }));
    DefinitionNode::List(values)
}

fn field(name: &str, value: DefinitionNode) -> (String, DefinitionNode) {
    (name.to_owned(), value)
}

fn u64_node(value: usize) -> Result<DefinitionNode, GraphEncodingError> {
    let value = u64::try_from(value).map_err(|_| {
        GraphEncodingError::MalformedInput("integer exceeds u64".to_owned())
    })?;
    Ok(DefinitionNode::Integer {
        ty: "u64".to_owned(),
        value: value.to_string(),
    })
}

/// Canonical VIBON for an implementation receiver or applied interface key.
pub(in crate::codebase::graph) fn normalized_type_text(
    value: &Type,
    binders: &BTreeMap<String, u64>,
) -> Result<String, GraphEncodingError> {
    use vibra_syntax::canonical_string_spelling as quote;
    fn list(items: impl IntoIterator<Item = String>) -> String {
        let values = items.into_iter().collect::<Vec<_>>();
        if values.is_empty() {
            "(array)".to_owned()
        } else {
            format!("(array {})", values.join(" "))
        }
    }
    fn signature(
        value: &FunctionSignature,
        binders: &BTreeMap<String, u64>,
    ) -> Result<String, GraphEncodingError> {
        let positional = value
            .parameters()
            .iter()
            .map(|ty| normalized_type(ty, binders))
            .collect::<Result<Vec<_>, _>>()?;
        let mut labelled_parameters = value.labelled().iter().collect::<Vec<_>>();
        labelled_parameters
            .sort_by(|left, right| left.name().as_bytes().cmp(right.name().as_bytes()));
        let labelled = labelled_parameters
            .into_iter()
            .map(|parameter| {
                Ok(format!(
                    "(record name: {} type: {})",
                    quote(parameter.name()),
                    normalized_type(&parameter.value_type(), binders)?
                ))
            })
            .collect::<Result<Vec<_>, GraphEncodingError>>()?;
        let variadic = match value.variadic() {
            Some(ty) => format!(
                "(record kind: @present type: {})",
                normalized_type(ty, binders)?
            ),
            None => "(record kind: @absent)".to_owned(),
        };
        Ok(format!(
            "(record positional: {} result: {} labelled: {} variadic: {} effects: {})",
            list(positional),
            normalized_type(&value.result(), binders)?,
            list(labelled),
            variadic,
            row(value.effects())
        ))
    }
    fn normalized_type(
        value: &Type,
        binders: &BTreeMap<String, u64>,
    ) -> Result<String, GraphEncodingError> {
        Ok(match value {
            Type::Bool
            | Type::Void
            | Type::Never
            | Type::Char
            | Type::Str
            | Type::Bytes
            | Type::Atom
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::F32
            | Type::F64
            | Type::Any => {
                format!("(record kind: @primitive name: @{})", value.as_str())
            }
            Type::AtomSingleton(name) => {
                format!("(record kind: @atom-singleton name: {})", quote(name))
            }
            Type::Declared(id) => format!(
                "(record kind: @declared identity: {} arguments: (array))",
                quote(id.id())
            ),
            Type::Applied(id, arguments) => format!(
                "(record kind: @declared identity: {} arguments: {})",
                quote(id.id()),
                list(
                    arguments
                        .iter()
                        .map(|ty| normalized_type(ty, binders))
                        .collect::<Result<Vec<_>, _>>()?
                )
            ),
            Type::Interface(id, arguments) => format!(
                "(record kind: @interface identity: {} arguments: {})",
                quote(id.id()),
                list(
                    arguments
                        .iter()
                        .map(|ty| normalized_type(ty, binders))
                        .collect::<Result<Vec<_>, _>>()?
                )
            ),
            Type::Param(name) => format!(
                "(record kind: @parameter binder-kind: @type index: {}u64)",
                binders.get(name).copied().ok_or_else(|| {
                    GraphEncodingError::IncompleteProjection(format!(
                        "generic type parameter `{name}` has no binder"
                    ))
                })?
            ),
            Type::Array(element) => format!(
                "(record kind: @array element: {})",
                normalized_type(element, binders)?
            ),
            Type::Dict(key, value) => format!(
                "(record kind: @dict key: {} value: {})",
                normalized_type(key, binders)?,
                normalized_type(value, binders)?
            ),
            Type::Tuple(items) => format!(
                "(record kind: @tuple items: {})",
                list(
                    items
                        .iter()
                        .map(|ty| normalized_type(ty, binders))
                        .collect::<Result<Vec<_>, _>>()?
                )
            ),
            Type::Record(fields) | Type::Enum(fields) => {
                let is_enum = matches!(value, Type::Enum(_));
                let fields = fields
                    .iter()
                    .map(|(name, ty)| {
                        Ok(format!(
                            "(record name: {} type: {})",
                            quote(name),
                            normalized_type(ty, binders)?
                        ))
                    })
                    .collect::<Result<Vec<_>, GraphEncodingError>>()?;
                format!(
                    "(record kind: @{} {}: {})",
                    if is_enum { "enum" } else { "record" },
                    if is_enum { "variants" } else { "fields" },
                    list(fields)
                )
            }
            Type::Union(items) => format!(
                "(record kind: @union members: {})",
                list(
                    items
                        .iter()
                        .map(|ty| normalized_type(ty, binders))
                        .collect::<Result<Vec<_>, _>>()?
                )
            ),
            Type::Function(value) => {
                format!("(record kind: @function {})", signature(value, binders)?)
            }
        })
    }
    let form = normalized_type(value, binders)?;
    Ok(format!("(record format: @normalized-type.v1 form: {form})"))
}

/// Mutable lexical and reference state used while visiting one checked body.
pub(in crate::codebase::graph) struct IrProjection<'a> {
    /// Checked module function identities in index order.
    pub functions: &'a [String],
    /// Checked module global identities in index order.
    pub globals: &'a [String],
    /// Generic type binders for this complete owning object.
    pub type_binders: &'a BTreeMap<String, u64>,
    /// Canonical identities of primitive-backed nominal roles.
    pub role_type_identities: &'a BTreeMap<String, String>,
    /// Canonical identity of the object whose form is being projected.
    pub owner_identity: String,
    /// Checked prelude boolean value identities.
    pub booleans: &'a BTreeMap<bool, String>,
    /// Dependency occurrences collected by the projection.
    pub references: &'a mut Vec<PendingReference>,
    /// Next global binder ordinal.
    pub next_binder: &'a mut u64,
    /// Activation slot to normalized lexical binder ordinal.
    pub slots: BTreeMap<usize, u64>,
    /// Closure environment slot to normalized captured binder ordinal.
    pub captures: BTreeMap<usize, u64>,
    /// Owner-relative path for visited references.
    pub part: Vec<String>,
}

impl IrProjection<'_> {
    fn reference(&mut self, identity: &str, kind: EdgeKind) -> DefinitionNode {
        self.references.push(PendingReference {
            identity: identity.to_owned(),
            kind,
            part: self.part.clone(),
            dispatch: None,
        });
        DefinitionNode::Reference(identity.to_owned())
    }

    fn dispatch_reference(
        &mut self,
        identity: &str,
        target: CallTarget,
        actual_operand_types: Vec<Type>,
    ) -> DefinitionNode {
        self.references.push(PendingReference {
            identity: identity.to_owned(),
            kind: EdgeKind::Contract,
            part: self.part.clone(),
            dispatch: Some(ContractDispatch {
                target,
                actual_operand_types,
            }),
        });
        DefinitionNode::Reference(identity.to_owned())
    }

    fn binder(&mut self, slot: usize) -> Result<DefinitionNode, GraphEncodingError> {
        let index = if let Some(index) = self.slots.get(&slot) {
            *index
        } else {
            let index = *self.next_binder;
            *self.next_binder = index.checked_add(1).ok_or_else(|| {
                GraphEncodingError::MalformedInput(
                    "binder count exceeds u64".to_owned(),
                )
            })?;
            self.slots.insert(slot, index);
            index
        };
        Ok(DefinitionNode::Binder {
            kind: "value".to_owned(),
            index,
        })
    }

    pub(in crate::codebase::graph) fn type_node(
        &mut self,
        value: &Type,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        let role = match value {
            Type::Bool => Some("bool"),
            Type::Str => Some("str"),
            Type::Bytes => Some("bytes"),
            _ => None,
        };
        if let Some(role) = role {
            let identity = self.role_type_identities.get(role).ok_or_else(|| {
                GraphEncodingError::IncompleteProjection(format!(
                    "checked primitive role `{role}` has no resolved nominal identity"
                ))
            })?;
            self.references.push(PendingReference {
                identity: identity.clone(),
                kind: EdgeKind::TypeReference,
                part: self.part.clone(),
                dispatch: None,
            });
        }
        let primitive = |name: &str| tagged("primitive", [field("name", atom(name))]);
        Ok(match value {
            Type::Bool => primitive("bool"), Type::Void => primitive("void"),
            Type::Never => primitive("never"), Type::Char => primitive("char"),
            Type::Str => primitive("str"), Type::Bytes => primitive("bytes"),
            Type::Atom => primitive("atom"), Type::I8 => primitive("i8"),
            Type::I16 => primitive("i16"), Type::I32 => primitive("i32"),
            Type::I64 => primitive("i64"), Type::U8 => primitive("u8"),
            Type::U16 => primitive("u16"), Type::U32 => primitive("u32"),
            Type::U64 => primitive("u64"), Type::F32 => primitive("f32"),
            Type::F64 => primitive("f64"), Type::Any => primitive("any"),
            Type::AtomSingleton(name) => tagged("atom-singleton", [field("name", string(name.clone()))]),
            Type::Param(name) => DefinitionNode::Binder {
                kind: "type".to_owned(),
                index: self.type_binders.get(name).copied()
                    .ok_or_else(|| GraphEncodingError::IncompleteProjection(format!(
                        "checked type parameter `{name}` in `{}` part `{}` has no binder ordinal",
                        self.owner_identity,
                        self.part.join("."),
                    )))?,
            },
            Type::Declared(id) => {
                let identity = self.reference(id.id(), EdgeKind::TypeReference);
                tagged("declared", [field("identity", identity)])
            }
            Type::Applied(id, arguments) | Type::Interface(id, arguments) => {
                let interface = matches!(value, Type::Interface(..));
                let identity = self.reference(id.id(), EdgeKind::TypeReference);
                let arguments = arguments.iter().map(|arg| self.type_node(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                tagged(if interface { "interface" } else { "applied" }, [
                    field("identity", identity),
                    field("arguments", array(arguments)),
                ])
            }
            Type::Array(element) => tagged("array", [field("element", self.type_node(element)?)]),
            Type::Dict(key, value) => tagged("dict", [
                field("key", self.type_node(key)?), field("value", self.type_node(value)?),
            ]),
            Type::Tuple(items) => tagged("tuple", [field("items", array(
                items.iter().map(|item| self.type_node(item)).collect::<Result<Vec<_>, _>>()?,
            ))]),
            Type::Record(fields) | Type::Enum(fields) => {
                let is_enum = matches!(value, Type::Enum(_));
                let fields = fields.iter().map(|(name, ty)| Ok(tagged("member", [
                    field("name", string(name.clone())), field("type", self.type_node(ty)?),
                ]))).collect::<Result<Vec<_>, GraphEncodingError>>()?;
                tagged(if is_enum { "enum" } else { "record" }, [field("members", array(fields))])
            }
            Type::Union(items) => tagged("union", [field("members", array(
                items.iter().map(|item| self.type_node(item)).collect::<Result<Vec<_>, _>>()?,
            ))]),
            Type::Function(signature) => tagged("function", [
                field("signature", self.signature_node(signature)?),
            ]),
        })
    }

    pub(in crate::codebase::graph) fn row_node(
        &mut self,
        row: &EffectRow,
    ) -> DefinitionNode {
        let mut ordered_roots = row.roots().iter().collect::<Vec<_>>();
        ordered_roots
            .sort_by(|left, right| left.id().as_bytes().cmp(right.id().as_bytes()));
        let roots = ordered_roots
            .iter()
            .map(|root| {
                self.references.push(PendingReference {
                    identity: root.id().to_owned(),
                    kind: EdgeKind::EffectOwner,
                    part: self.part.clone(),
                    dispatch: None,
                });
                DefinitionNode::Reference(root.id().to_owned())
            })
            .collect::<Vec<_>>();
        tagged(
            "effects",
            [field("roots", array(roots)), field("tail", atom("none"))],
        )
    }

    pub(in crate::codebase::graph) fn signature_node(
        &mut self,
        signature: &FunctionSignature,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        let positional = signature
            .parameters()
            .iter()
            .map(|ty| self.type_node(ty))
            .collect::<Result<Vec<_>, _>>()?;
        let labelled = signature
            .labelled()
            .iter()
            .map(|parameter| {
                let default = parameter
                    .default()
                    .map_or(Ok(DefinitionNode::Void), |value| {
                        self.constant_node(value)
                    })?;
                Ok(tagged(
                    "labelled",
                    [
                        field(
                            "name",
                            DefinitionNode::Label(parameter.name().to_owned()),
                        ),
                        field("type", self.type_node(&parameter.value_type())?),
                        field("default", default),
                    ],
                ))
            })
            .collect::<Result<Vec<_>, GraphEncodingError>>()?;
        let variadic = match signature.variadic() {
            None => tagged("absent", []),
            Some(ty) => tagged("present", [field("type", self.type_node(ty)?)]),
        };
        Ok(tagged(
            "signature",
            [
                field("positional", array(positional)),
                field("labelled", array(labelled)),
                field("variadic", variadic),
                field("result", self.type_node(&signature.result())?),
                field("effects", self.row_node(signature.effects())),
            ],
        ))
    }

    pub(in crate::codebase::graph) fn constant_node(
        &mut self,
        value: &Constant,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        Ok(match value {
            Constant::Primitive(value) => self.value_node(value, None)?,
            Constant::Tuple {
                value_type,
                components,
            } => tagged(
                "tuple",
                [
                    field("type", self.type_node(value_type)?),
                    field(
                        "components",
                        array(
                            components
                                .iter()
                                .map(|item| self.constant_node(item))
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                    ),
                ],
            ),
            Constant::Record {
                value_type,
                fields: members,
            } => tagged(
                "record",
                [
                    field("type", self.type_node(value_type)?),
                    field(
                        "fields",
                        array(
                            members
                                .iter()
                                .map(|(name, value)| {
                                    Ok(tagged(
                                        "field",
                                        [
                                            field("name", string(name.clone())),
                                            field("value", self.constant_node(value)?),
                                        ],
                                    ))
                                })
                                .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                        ),
                    ),
                ],
            ),
            Constant::Variant {
                value_type,
                variant,
                payload,
            } => tagged(
                "variant",
                [
                    field("type", self.type_node(value_type)?),
                    field("name", string(variant.clone())),
                    field(
                        "payload",
                        payload.as_deref().map_or(Ok(DefinitionNode::Void), |v| {
                            self.constant_node(v)
                        })?,
                    ),
                ],
            ),
            Constant::Wrap { value_type, value } => tagged(
                "wrap",
                [
                    field("type", self.type_node(value_type)?),
                    field("value", self.constant_node(value)?),
                ],
            ),
            Constant::Member {
                value_type,
                index,
                value,
            } => tagged(
                "member",
                [
                    field("type", self.type_node(value_type)?),
                    field("index", u64_node(*index)?),
                    field("value", self.constant_node(value)?),
                ],
            ),
        })
    }

    fn value_node(
        &mut self,
        value: &Value,
        origin: Option<&vibra_ir::SourceOrigin>,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        if let Value::Bool(boolean) = value {
            let identity = self
                .booleans
                .get(boolean)
                .ok_or_else(|| {
                    GraphEncodingError::IncompleteProjection(format!(
                        "boolean {boolean} has no prelude entity identity"
                    ))
                })?
                .clone();
            let _ = origin;
            return Ok(self.reference(&identity, EdgeKind::ValueReference));
        }
        Ok(match value {
            Value::Void => DefinitionNode::Void,
            Value::Bool(_) => {
                return Err(GraphEncodingError::IncompleteProjection(
                    "boolean literal identity was not projected".to_owned(),
                ));
            }
            Value::Char(value) => DefinitionNode::Character(*value),
            Value::Str(value) => DefinitionNode::String(value.clone()),
            Value::Bytes(value) => DefinitionNode::Bytes(value.clone()),
            Value::Atom(value) => DefinitionNode::AtomValue(value.clone()),
            Value::I8(value) => DefinitionNode::Integer {
                ty: "i8".to_owned(),
                value: value.to_string(),
            },
            Value::I16(value) => DefinitionNode::Integer {
                ty: "i16".to_owned(),
                value: value.to_string(),
            },
            Value::I32(value) => DefinitionNode::Integer {
                ty: "i32".to_owned(),
                value: value.to_string(),
            },
            Value::I64(value) => DefinitionNode::Integer {
                ty: "i64".to_owned(),
                value: value.to_string(),
            },
            Value::U8(value) => DefinitionNode::Integer {
                ty: "u8".to_owned(),
                value: value.to_string(),
            },
            Value::U16(value) => DefinitionNode::Integer {
                ty: "u16".to_owned(),
                value: value.to_string(),
            },
            Value::U32(value) => DefinitionNode::Integer {
                ty: "u32".to_owned(),
                value: value.to_string(),
            },
            Value::U64(value) => DefinitionNode::Integer {
                ty: "u64".to_owned(),
                value: value.to_string(),
            },
            Value::F32(bits) => DefinitionNode::Float {
                ty: "f32".to_owned(),
                value: canonical_f32_text(f32::from_bits(*bits)),
            },
            Value::F64(bits) => DefinitionNode::Float {
                ty: "f64".to_owned(),
                value: canonical_f64_text(f64::from_bits(*bits)),
            },
        })
    }

    pub(in crate::codebase::graph) fn expression(
        &mut self,
        expression: &Expr,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        let expr =
            |tag: &str, fields: Vec<(String, DefinitionNode)>| {
                let mut values = vec![atom("expr"), atom(tag)];
                values.extend(fields.into_iter().map(|(name, value)| {
                    DefinitionNode::List(vec![atom(name), value])
                }));
                DefinitionNode::List(values)
            };
        Ok(match expression {
            Expr::Literal { value, origin } => expr(
                "literal",
                vec![field("value", self.value_node(value, Some(origin))?)],
            ),
            Expr::External {
                intrinsic,
                arguments,
                result,
                ..
            } => {
                let identity = format!("@compiler:{}", intrinsic.symbol());
                let reference = self.reference(&identity, EdgeKind::ValueReference);
                expr(
                    "external",
                    vec![
                        field("identity", reference),
                        field(
                            "arguments",
                            array(
                                arguments
                                    .iter()
                                    .map(|arg| self.expression(arg))
                                    .collect::<Result<Vec<_>, _>>()?,
                            ),
                        ),
                        field("result", self.type_node(result)?),
                    ],
                )
            }
            Expr::Host {
                row,
                arguments,
                result,
                ..
            } => {
                let identity = format!("@host:{}", row.symbol);
                let reference = self.reference(&identity, EdgeKind::ValueReference);
                expr(
                    "host",
                    vec![
                        field("identity", reference),
                        field(
                            "arguments",
                            array(
                                arguments
                                    .iter()
                                    .map(|arg| self.expression(arg))
                                    .collect::<Result<Vec<_>, _>>()?,
                            ),
                        ),
                        field("result", self.type_node(result)?),
                    ],
                )
            }
            Expr::Default { value_type, .. } => {
                expr("default", vec![field("type", self.type_node(value_type)?)])
            }
            Expr::Sequence { expressions, .. } => expr(
                "sequence",
                vec![field(
                    "expressions",
                    array(
                        expressions
                            .iter()
                            .map(|item| self.expression(item))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                )],
            ),
            Expr::Variable {
                slot, value_type, ..
            } => expr(
                "variable",
                vec![
                    field("binder", self.binder(*slot)?),
                    field("type", self.type_node(value_type)?),
                ],
            ),
            Expr::Global {
                index, value_type, ..
            } => {
                let identity = self
                    .globals
                    .get(*index)
                    .ok_or_else(|| {
                        GraphEncodingError::IncompleteProjection(format!(
                            "checked global index {index} is outside its scope"
                        ))
                    })?
                    .clone();
                let reference = self.reference(&identity, EdgeKind::ValueReference);
                expr(
                    "global",
                    vec![
                        field("identity", reference),
                        field("type", self.type_node(value_type)?),
                    ],
                )
            }
            Expr::Function {
                function,
                signature,
                ..
            } => {
                let identity = self
                    .functions
                    .get(*function)
                    .ok_or_else(|| {
                        GraphEncodingError::IncompleteProjection(format!(
                            "checked function index {function} is outside its scope"
                        ))
                    })?
                    .clone();
                let reference = self.reference(&identity, EdgeKind::ValueReference);
                expr(
                    "function",
                    vec![
                        field("identity", reference),
                        field("signature", self.signature_node(signature)?),
                    ],
                )
            }
            Expr::Closure {
                signature,
                parameters,
                captures,
                capture_types,
                body,
                ..
            } => {
                let capture_nodes = captures
                    .iter()
                    .map(|item| self.expression(item))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut capture_binders = BTreeMap::new();
                for index in 0..capture_types.len() {
                    let binder = *self.next_binder;
                    *self.next_binder = binder.checked_add(1).ok_or_else(|| {
                        GraphEncodingError::MalformedInput(
                            "binder count exceeds u64".to_owned(),
                        )
                    })?;
                    capture_binders.insert(index, binder);
                }
                let mut closure_slots = BTreeMap::new();
                for slot in 0..signature.fixed_parameter_count() {
                    let binder = *self.next_binder;
                    *self.next_binder = binder.checked_add(1).ok_or_else(|| {
                        GraphEncodingError::MalformedInput(
                            "binder count exceeds u64".to_owned(),
                        )
                    })?;
                    closure_slots.insert(slot, binder);
                }
                let parameter_nodes = parameters
                    .iter()
                    .map(|ty| self.type_node(ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let body_node = {
                    let mut nested = IrProjection {
                        functions: self.functions,
                        globals: self.globals,
                        type_binders: self.type_binders,
                        role_type_identities: self.role_type_identities,
                        owner_identity: self.owner_identity.clone(),
                        booleans: self.booleans,
                        references: &mut *self.references,
                        next_binder: &mut *self.next_binder,
                        slots: closure_slots,
                        captures: capture_binders,
                        part: self.part.clone(),
                    };
                    nested.expression(body)?
                };
                expr(
                    "closure",
                    vec![
                        field("signature", self.signature_node(signature)?),
                        field("parameters", array(parameter_nodes)),
                        field("captures", array(capture_nodes)),
                        field(
                            "capture-types",
                            array(
                                capture_types
                                    .iter()
                                    .map(|ty| self.type_node(ty))
                                    .collect::<Result<Vec<_>, _>>()?,
                            ),
                        ),
                        field("body", body_node),
                    ],
                )
            }
            Expr::Captured {
                slot, value_type, ..
            } => {
                let index = self.captures.get(slot).copied().ok_or_else(|| {
                    GraphEncodingError::IncompleteProjection(format!(
                        "closure capture slot {slot} has no normalized binder"
                    ))
                })?;
                expr(
                    "captured",
                    vec![
                        field(
                            "binder",
                            DefinitionNode::Binder {
                                kind: "value".to_owned(),
                                index,
                            },
                        ),
                        field("type", self.type_node(value_type)?),
                    ],
                )
            }
            Expr::Let {
                slot, value, body, ..
            } => {
                let value = self.expression(value)?;
                let binder =
                    slot.map_or(Ok(DefinitionNode::Void), |slot| self.binder(slot))?;
                expr(
                    "let",
                    vec![
                        field("binder", binder),
                        field("value", value),
                        field("body", self.expression(body)?),
                    ],
                )
            }
            Expr::Match {
                scrutinee,
                arms,
                value_type,
                ..
            } => {
                let scrutinee = self.expression(scrutinee)?;
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let pattern = self.pattern(&arm.pattern)?;
                        let body = self.expression(&arm.body)?;
                        Ok(tagged(
                            "arm",
                            [field("pattern", pattern), field("body", body)],
                        ))
                    })
                    .collect::<Result<Vec<_>, GraphEncodingError>>()?;
                expr(
                    "match",
                    vec![
                        field("scrutinee", scrutinee),
                        field("arms", array(arms)),
                        field("type", self.type_node(value_type)?),
                    ],
                )
            }
            Expr::Widen {
                value,
                value_type,
                member,
                ..
            } => expr(
                "widen",
                vec![
                    field("value", self.expression(value)?),
                    field("type", self.type_node(value_type)?),
                    field(
                        "member",
                        member.map_or(Ok(DefinitionNode::Void), |index| {
                            u64_node(index)
                        })?,
                    ),
                ],
            ),
            Expr::Try {
                value,
                value_type,
                exit_type,
                ..
            } => expr(
                "try",
                vec![
                    field("value", self.expression(value)?),
                    field("type", self.type_node(value_type)?),
                    field("exit-type", self.type_node(exit_type)?),
                ],
            ),
            Expr::Return { value, .. } => {
                expr("return", vec![field("value", self.expression(value)?)])
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => expr(
                "if",
                vec![
                    field("condition", self.expression(condition)?),
                    field("then", self.expression(then_branch)?),
                    field("else", self.expression(else_branch)?),
                ],
            ),
            Expr::Call {
                target,
                arguments: operands,
                result,
                tail,
                ..
            } => {
                let target = match target {
                    CallTarget::Direct(index) => {
                        let identity = self.functions.get(*index).ok_or_else(|| GraphEncodingError::IncompleteProjection(format!("contract callee function index {index} is outside its scope")))?.clone();
                        self.reference(&identity, EdgeKind::Call)
                    }
                    CallTarget::Indirect { callee, .. } => self.expression(callee)?,
                    CallTarget::Contract {
                        interface,
                        member,
                        arguments: interface_arguments,
                        member_types,
                        destination,
                        signature,
                        closed,
                        ..
                    } => {
                        let identity = format!("{}.{}", interface.id(), member);
                        let actual_operand_types =
                            operands.iter().map(Expr::result_type).collect();
                        let reference = self.dispatch_reference(
                            &identity,
                            target.clone(),
                            actual_operand_types,
                        );
                        tagged(
                            "contract",
                            [
                                field("identity", reference),
                                field(
                                    "arguments",
                                    array(
                                        interface_arguments
                                            .iter()
                                            .map(|ty| self.type_node(ty))
                                            .collect::<Result<Vec<_>, _>>()?,
                                    ),
                                ),
                                field(
                                    "member-types",
                                    array(
                                        member_types
                                            .iter()
                                            .map(|ty| self.type_node(ty))
                                            .collect::<Result<Vec<_>, _>>()?,
                                    ),
                                ),
                                field(
                                    "destination",
                                    destination
                                        .as_ref()
                                        .map_or(Ok(DefinitionNode::Void), |ty| {
                                            self.type_node(ty)
                                        })?,
                                ),
                                field("signature", self.signature_node(signature)?),
                                field(
                                    "closed",
                                    closed.map_or_else(
                                        || atom("none"),
                                        |value| atom(value.symbol()),
                                    ),
                                ),
                            ],
                        )
                    }
                };
                expr(
                    "call",
                    vec![
                        field("target", target),
                        field(
                            "arguments",
                            array(
                                operands
                                    .iter()
                                    .map(|arg| self.expression(arg))
                                    .collect::<Result<Vec<_>, _>>()?,
                            ),
                        ),
                        field("result", self.type_node(result)?),
                        field("tail", atom(if *tail { "tail" } else { "ordinary" })),
                    ],
                )
            }
            Expr::Record {
                value_type, fields, ..
            } => expr(
                "record",
                vec![
                    field("type", self.type_node(value_type)?),
                    field(
                        "fields",
                        array(
                            fields
                                .iter()
                                .map(|(name, value)| {
                                    Ok(tagged(
                                        "field",
                                        [
                                            field("name", string(name.clone())),
                                            field("value", self.expression(value)?),
                                        ],
                                    ))
                                })
                                .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                        ),
                    ),
                ],
            ),
            Expr::Variant {
                value_type,
                variant,
                payload,
                ..
            } => expr(
                "variant",
                vec![
                    field("type", self.type_node(value_type)?),
                    field("name", string(variant.clone())),
                    field(
                        "payload",
                        payload
                            .as_deref()
                            .map_or(Ok(DefinitionNode::Void), |value| {
                                self.expression(value)
                            })?,
                    ),
                ],
            ),
            Expr::Wrap {
                value_type, value, ..
            } => expr(
                "wrap",
                vec![
                    field("type", self.type_node(value_type)?),
                    field("value", self.expression(value)?),
                ],
            ),
            Expr::Project {
                record,
                field: member,
                value_type,
                ..
            } => expr(
                "project",
                vec![
                    field("record", self.expression(record)?),
                    field("field", string(member.clone())),
                    field("type", self.type_node(value_type)?),
                ],
            ),
            Expr::Tuple {
                value_type,
                components,
                ..
            } => expr(
                "tuple",
                vec![
                    field("type", self.type_node(value_type)?),
                    field(
                        "components",
                        array(
                            components
                                .iter()
                                .map(|item| self.expression(item))
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                    ),
                ],
            ),
            Expr::TupleProject {
                tuple,
                index,
                value_type,
                ..
            } => expr(
                "tuple-project",
                vec![
                    field("tuple", self.expression(tuple)?),
                    field("index", u64_node(*index)?),
                    field("type", self.type_node(value_type)?),
                ],
            ),
            Expr::Array {
                value_type,
                elements,
                ..
            } => expr(
                "array",
                vec![
                    field("type", self.type_node(value_type)?),
                    field(
                        "elements",
                        array(
                            elements
                                .iter()
                                .map(|item| self.expression(item))
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                    ),
                ],
            ),
            Expr::Dict {
                value_type,
                entries,
                key_order,
                ..
            } => expr(
                "dict",
                vec![
                    field("type", self.type_node(value_type)?),
                    field(
                        "entries",
                        array(
                            entries
                                .iter()
                                .map(|(key, value)| {
                                    Ok(tagged(
                                        "entry",
                                        [
                                            field("key", self.expression(key)?),
                                            field("value", self.expression(value)?),
                                        ],
                                    ))
                                })
                                .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                        ),
                    ),
                    field(
                        "key-order",
                        key_order.as_ref().map_or(Ok(DefinitionNode::Void), |id| {
                            Ok(self.reference(id.id(), EdgeKind::Contract))
                        })?,
                    ),
                ],
            ),
            Expr::Lookup {
                collection,
                key,
                key_order,
                value_type,
                ..
            } => expr(
                "lookup",
                vec![
                    field("collection", self.expression(collection)?),
                    field("key", self.expression(key)?),
                    field(
                        "key-order",
                        key_order.as_ref().map_or(Ok(DefinitionNode::Void), |id| {
                            Ok(self.reference(id.id(), EdgeKind::Contract))
                        })?,
                    ),
                    field("type", self.type_node(value_type)?),
                ],
            ),
        })
    }

    pub(in crate::codebase::graph) fn pattern(
        &mut self,
        pattern: &Pattern,
    ) -> Result<DefinitionNode, GraphEncodingError> {
        let pattern_node =
            |tag: &str, fields: Vec<(String, DefinitionNode)>| {
                let mut values = vec![atom("pattern"), atom(tag)];
                values.extend(fields.into_iter().map(|(name, value)| {
                    DefinitionNode::List(vec![atom(name), value])
                }));
                DefinitionNode::List(values)
            };
        Ok(match pattern {
            Pattern::Wildcard => pattern_node("wildcard", Vec::new()),
            Pattern::Bind { slot, value_type } => pattern_node(
                "bind",
                vec![
                    field("binder", self.binder(*slot)?),
                    field("type", self.type_node(value_type)?),
                ],
            ),
            Pattern::Literal(value) => pattern_node(
                "literal",
                vec![field("value", self.value_node(value, None)?)],
            ),
            Pattern::Variant { variant, payload } => pattern_node(
                "variant",
                vec![
                    field("name", string(variant.clone())),
                    field(
                        "payload",
                        payload
                            .as_deref()
                            .map_or(Ok(DefinitionNode::Void), |value| {
                                self.pattern(value)
                            })?,
                    ),
                ],
            ),
            Pattern::Record(fields) => pattern_node(
                "record",
                vec![field(
                    "fields",
                    array(
                        fields
                            .iter()
                            .map(|(name, pattern)| {
                                Ok(tagged(
                                    "field",
                                    [
                                        field("name", string(name.clone())),
                                        field("pattern", self.pattern(pattern)?),
                                    ],
                                ))
                            })
                            .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                    ),
                )],
            ),
            Pattern::Tuple(items) => pattern_node(
                "tuple",
                vec![field(
                    "items",
                    array(
                        items
                            .iter()
                            .map(|item| self.pattern(item))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                )],
            ),
            Pattern::Wrap(value) => {
                pattern_node("wrap", vec![field("value", self.pattern(value)?)])
            }
            Pattern::Array(items) => pattern_node(
                "array",
                vec![field(
                    "items",
                    array(
                        items
                            .iter()
                            .map(|item| self.pattern(item))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                )],
            ),
            Pattern::Member {
                index,
                member,
                pattern,
            } => pattern_node(
                "member",
                vec![
                    field("index", u64_node(*index)?),
                    field("type", self.type_node(member)?),
                    field("pattern", self.pattern(pattern)?),
                ],
            ),
        })
    }
}

/// Canonical `@checked-definition.v1` frame for one owner object.
pub(in crate::codebase::graph) fn encode_definition(
    kind: ObjectKind,
    form: &DefinitionNode,
) -> Result<Vec<u8>, GraphEncodingError> {
    let record = format!(
        "(record format: @checked-definition.v1 kind: @{} form: {})",
        kind.as_str(),
        form.canonical()
    );
    canonical_frame(&record)
}

#[cfg(test)]
mod normalized_type_tests {
    use super::{DefinitionNode, IrProjection, normalized_type_text, row};
    use std::collections::BTreeMap;
    use vibra_ir::{EffectRoot, EffectRow, FunctionSignature, LabelledParameter, Type};

    #[test]
    fn normalized_function_type_sorts_labelled_slots_by_exact_name() {
        let left = Type::Function(Box::new(FunctionSignature::with_labelled(
            Vec::new(),
            vec![
                LabelledParameter::new("zeta", Type::I32, None),
                LabelledParameter::new("alpha", Type::Str, None),
            ],
            Type::Void,
        )));
        let right = Type::Function(Box::new(FunctionSignature::with_labelled(
            Vec::new(),
            vec![
                LabelledParameter::new("alpha", Type::Str, None),
                LabelledParameter::new("zeta", Type::I32, None),
            ],
            Type::Void,
        )));
        let binders = BTreeMap::new();

        assert_eq!(
            normalized_type_text(&left, &binders)
                .expect("left normalized function type"),
            normalized_type_text(&right, &binders)
                .expect("right normalized function type"),
        );
    }

    #[test]
    fn atom_literal_has_its_own_node_kind() {
        assert_eq!(
            DefinitionNode::AtomValue("ready".to_owned()).canonical(),
            "(record kind: @atom-value value: \"ready\")",
        );
        assert_ne!(
            DefinitionNode::Atom("ready".to_owned()).canonical(),
            DefinitionNode::AtomValue("ready".to_owned()).canonical(),
        );
    }

    #[test]
    fn graph_effect_rows_sort_by_identity_instead_of_runtime_path_order() {
        let effect_row = EffectRow::new([
            EffectRoot::new("@vibra-stdlib@0.3.0/std.io.stdout", "std.io.stdout"),
            EffectRoot::new("@demo@0.1.0/app.main.zebra", "zebra"),
        ]);
        assert_eq!(
            row(&effect_row),
            "(record roots: (array (record kind: @reference identity: \"@demo@0.1.0/app.main.zebra\") (record kind: @reference identity: \"@vibra-stdlib@0.3.0/std.io.stdout\")) tail: @none)",
        );

        let no_names = Vec::new();
        let no_binders = BTreeMap::new();
        let no_roles = BTreeMap::<String, String>::new();
        let booleans = BTreeMap::new();
        let mut references = Vec::new();
        let mut next_binder = 0;
        let mut projection = IrProjection {
            functions: &no_names,
            globals: &no_names,
            type_binders: &no_binders,
            role_type_identities: &no_roles,
            owner_identity: "@graph-test".to_owned(),
            booleans: &booleans,
            references: &mut references,
            next_binder: &mut next_binder,
            slots: BTreeMap::new(),
            captures: BTreeMap::new(),
            part: Vec::new(),
        };
        let _ = projection.row_node(&effect_row);
        drop(projection);
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.identity.as_str())
                .collect::<Vec<_>>(),
            [
                "@demo@0.1.0/app.main.zebra",
                "@vibra-stdlib@0.3.0/std.io.stdout"
            ],
        );
    }
}
