//! Checked types (`docs/spec/02-type-system.md`) and the identities they
//! carry.
//!
//! [`Type`] has a case for every compiler-owned type, the structural tuple
//! and record types, the three callable types, declared types, interface
//! value types, and three open forms. Every other type, `Bool`, `Str`,
//! `Bytes`, `Dict`, `Option`, and `Result` among them, is a
//! [`Type::Declared`] whose identity the checker binds through its role
//! ([`crate::LibraryTypes`]). Structural equality is type equality: record
//! fields, callable labels, and interface choices are stored in canonical
//! name order, so two types that compare their named parts as sets are equal
//! exactly when they are `==`.

use std::fmt;

use crate::IrError;

/// Whether `text` is an identifier of the source chapter: an ASCII letter,
/// then ASCII letters and digits.
pub(crate) fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

/// Whether `text` is the part of an atom after its `@`: identifiers joined
/// by single dots.
pub(crate) fn is_atom_path(text: &str) -> bool {
    !text.is_empty() && text.split('.').all(is_identifier)
}

/// Whether `text` is a declaration identity: `sha256:` and 64 lower-case
/// hexadecimal digits.
fn is_identity(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// An entity reference record: a canonical atom path and the declaration
/// identity of the type chapter's "Declared types". This crate stores both
/// and never computes the identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityRef {
    path: String,
    seed: String,
}

impl EntityRef {
    /// A reference to the entity at `path` (an atom path without its `@`,
    /// such as `std.option.Option`) whose identity is `seed`.
    ///
    /// # Errors
    ///
    /// The path is not an atom path, or the seed is not `sha256:` followed by
    /// 64 lower-case hexadecimal digits.
    pub fn new(
        path: impl Into<String>,
        seed: impl Into<String>,
    ) -> Result<Self, IrError> {
        let path = path.into();
        let seed = seed.into();
        if !is_atom_path(&path) {
            return Err(IrError::InvalidIdentity(format!(
                "`{path}` is not an atom path"
            )));
        }
        if !is_identity(&seed) {
            return Err(IrError::InvalidIdentity(format!(
                "`{seed}` is not a declaration identity"
            )));
        }
        Ok(Self { path, seed })
    }

    /// The canonical atom path, without its `@`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The declaration identity, `sha256:` and 64 hexadecimal digits.
    #[must_use]
    pub fn seed(&self) -> &str {
        &self.seed
    }

    /// The last part of the path: the entity's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or(&self.path)
    }
}

/// The identity of a declared type or an interface: its entity reference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(EntityRef);

impl TypeId {
    /// The identity of the type or interface at `path` with the identity
    /// `seed`.
    ///
    /// # Errors
    ///
    /// As [`EntityRef::new`].
    pub fn new(
        path: impl Into<String>,
        seed: impl Into<String>,
    ) -> Result<Self, IrError> {
        EntityRef::new(path, seed).map(Self)
    }

    /// The entity reference.
    #[must_use]
    pub const fn entity(&self) -> &EntityRef {
        &self.0
    }

    /// The canonical atom path, without its `@`.
    #[must_use]
    pub fn path(&self) -> &str {
        self.0.path()
    }

    /// The declaration identity.
    #[must_use]
    pub fn seed(&self) -> &str {
        self.0.seed()
    }

    /// The declaration's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.0.name()
    }
}

impl From<EntityRef> for TypeId {
    fn from(entity: EntityRef) -> Self {
        Self(entity)
    }
}

/// A row: a closed set of operation interfaces, sorted by the UTF-8 bytes of
/// their paths. An omitted row is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Row(Vec<EntityRef>);

impl Row {
    /// The empty row.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// The row of `roots`, sorted by path; a root named twice counts once.
    ///
    /// # Errors
    ///
    /// Two roots have one path and different identities.
    pub fn new(roots: impl IntoIterator<Item = EntityRef>) -> Result<Self, IrError> {
        let mut roots = roots.into_iter().collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        for pair in roots.windows(2) {
            if let [left, right] = pair
                && left.path() == right.path()
            {
                return Err(IrError::Duplicate {
                    what: "row root",
                    name: left.path().to_owned(),
                });
            }
        }
        Ok(Self(roots))
    }

    /// The roots in canonical order.
    #[must_use]
    pub fn roots(&self) -> &[EntityRef] {
        &self.0
    }
}

/// Named types in canonical name order with distinct names: the fields of a
/// structural record, the labels of a callable, the choices of an interface
/// value or a bound. Comparing two of them compares the sets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NamedTypes(Vec<(String, Type)>);

impl NamedTypes {
    /// No names.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// `entries` in canonical name order: the UTF-8 bytes of the names.
    ///
    /// # Errors
    ///
    /// A name is not an identifier, or two entries share a name.
    pub fn new(
        entries: impl IntoIterator<Item = (String, Type)>,
    ) -> Result<Self, IrError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        for (name, _) in &entries {
            if !is_identifier(name) {
                return Err(IrError::InvalidName {
                    what: "field, label, or choice",
                    name: name.clone(),
                });
            }
        }
        for pair in entries.windows(2) {
            if let [left, right] = pair
                && left.0 == right.0
            {
                return Err(IrError::Duplicate {
                    what: "field, label, or choice",
                    name: left.0.clone(),
                });
            }
        }
        Ok(Self(entries))
    }

    /// The entries in canonical name order.
    #[must_use]
    pub fn entries(&self) -> &[(String, Type)] {
        &self.0
    }

    /// The type named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Type> {
        self.0
            .iter()
            .find_map(|(entry, value)| (entry == name).then_some(value))
    }

    /// The names in canonical order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(name, _)| name.as_str())
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn map(&self, mut function: impl FnMut(&Type) -> Type) -> Self {
        Self(
            self.0
                .iter()
                .map(|(name, value)| (name.clone(), function(value)))
                .collect(),
        )
    }
}

/// The type of a pure function: positional parameter types, labelled
/// parameter types, and a result. A `Fn` has no row, and defaults belong to
/// the function value, not to its type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnType {
    /// Required positional parameter types, in order.
    pub parameters: Vec<Type>,
    /// Labelled parameter types, in canonical label order.
    pub labels: NamedTypes,
    /// The result type.
    pub result: Type,
}

impl FnType {
    /// A function type without labels.
    #[must_use]
    pub const fn new(parameters: Vec<Type>, result: Type) -> Self {
        Self {
            parameters,
            labels: NamedTypes::empty(),
            result,
        }
    }

    /// The type of every argument slot: the positional parameters, then the
    /// labelled ones in canonical label order.
    #[must_use]
    pub fn slot_types(&self) -> Vec<Type> {
        self.parameters
            .iter()
            .cloned()
            .chain(self.labels.entries().iter().map(|(_, value)| value.clone()))
            .collect()
    }

    /// The number of argument slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.parameters.len().saturating_add(self.labels.len())
    }
}

/// The type of a procedure: its input vector, its result, and its row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProcType {
    /// Required positional parameter types, in order.
    pub parameters: Vec<Type>,
    /// Labelled parameter types, in canonical label order.
    pub labels: NamedTypes,
    /// The result of the plan an application builds.
    pub result: Type,
    /// The row.
    pub effects: Row,
}

/// The type of a plan: its result and its row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IntentType {
    /// The result of awaiting the plan.
    pub result: Type,
    /// The row.
    pub effects: Row,
}

/// A checked type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// `I8`.
    I8,
    /// `I16`.
    I16,
    /// `I32`.
    I32,
    /// `I64`.
    I64,
    /// `U8`.
    U8,
    /// `U16`.
    U16,
    /// `U32`.
    U32,
    /// `U64`.
    U64,
    /// `F32`, IEEE 754 binary32.
    F32,
    /// `F64`, IEEE 754 binary64.
    F64,
    /// `Char`, the Unicode scalar values.
    Char,
    /// `Atom`, every atom value.
    Atom,
    /// `Void`, whose one value is `void`.
    Void,
    /// `Never`, which has no value.
    Never,
    /// `Any`, the interface value type of the predeclared empty interface.
    Any,
    /// The singleton type of one atom, written without its `@`.
    AtomSingleton(String),
    /// `List(T)`.
    List(Box<Type>),
    /// A structural tuple; `{}` is the empty tuple.
    Tuple(Vec<Type>),
    /// A structural record, with at least one field.
    Record(NamedTypes),
    /// `Fn({…} R)`.
    Fn(Box<FnType>),
    /// `Proc({…} R {Row})`.
    Proc(Box<ProcType>),
    /// `Intent(R {Row})`.
    Intent(Box<IntentType>),
    /// A declared type applied to its arguments in parameter order.
    Declared(TypeId, Vec<Type>),
    /// An interface value type: the interface, its positional arguments, and
    /// one choice for every associated type.
    Interface(TypeId, Vec<Type>, NamedTypes),
    /// A reference to the binder at this position of the enclosing signature
    /// or implementation record. Open.
    Binder(usize),
    /// The receiver slot, `Self`, inside an interface member. Open.
    SelfType,
    /// The association slot: a bare associated type name of the member's own
    /// interface. Open.
    Associated(String),
}

/// Dropping a type moves its components to a work list, so that a type a
/// host builds deeper than [`Type::MAX_DEPTH`], which the checked IR
/// rejects, is dropped without a host stack that grows with its depth.
impl Drop for Type {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.take_components(&mut pending);
        while let Some(mut ty) = pending.pop() {
            ty.take_components(&mut pending);
        }
    }
}

impl Type {
    fn take_components(&mut self, pending: &mut Vec<Self>) {
        let mut named = |named: &mut NamedTypes| {
            pending.extend(named.0.drain(..).map(|(_, ty)| ty));
        };
        match self {
            Self::List(element) => {
                pending.push(std::mem::replace(&mut **element, Self::Void))
            }
            Self::Tuple(values) | Self::Declared(_, values) => pending.append(values),
            Self::Record(fields) => named(fields),
            Self::Fn(signature) => {
                named(&mut signature.labels);
                pending.append(&mut signature.parameters);
                pending.push(std::mem::replace(&mut signature.result, Self::Void));
            }
            Self::Proc(signature) => {
                named(&mut signature.labels);
                pending.append(&mut signature.parameters);
                pending.push(std::mem::replace(&mut signature.result, Self::Void));
            }
            Self::Intent(intent) => {
                pending.push(std::mem::replace(&mut intent.result, Self::Void));
            }
            Self::Interface(_, arguments, choices) => {
                named(choices);
                pending.append(arguments);
            }
            _ => {}
        }
    }
}

/// Where open forms are valid while a type is checked or encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenScope<'a> {
    /// No open form: a value, an activation, a `@type.v2` document.
    Closed,
    /// Binder references below this count: a declaration's signature, a
    /// type's body, an implementation record, or a body under one.
    Binders(usize),
    /// Binder references below `binders`, `Self`, and the association slots
    /// of the interface that declares `associated`: a member of an interface.
    InterfaceMember {
        /// The number of binders in scope.
        binders: usize,
        /// The interface's associated type names.
        associated: &'a [String],
    },
}

impl OpenScope<'_> {
    /// The number of binders in scope.
    #[must_use]
    pub const fn binders(&self) -> usize {
        match self {
            Self::Closed => 0,
            Self::Binders(count) | Self::InterfaceMember { binders: count, .. } => {
                *count
            }
        }
    }
}

/// What replaces the open forms of a type.
#[derive(Clone, Copy, Debug, Default)]
pub struct Substitution<'a> {
    /// The type of each binder, by position. A binder past the end is kept.
    pub binders: &'a [Type],
    /// The receiver type, when `Self` is replaced.
    pub self_type: Option<&'a Type>,
    /// The choices, when association slots are replaced.
    pub associated: Option<&'a NamedTypes>,
}

impl Type {
    /// The deepest a type of the checked IR nests: a type written in a
    /// module set, an expression, or a value, and a type the check of a
    /// body works out. Within it the host may recurse over a type; the
    /// checked IR rejects a deeper one before any such recursion.
    pub const MAX_DEPTH: usize = 64;

    /// The compiler-owned scalar types, in the order of the chapter's wire
    /// atoms.
    pub const SCALARS: [Self; 14] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::F32,
        Self::F64,
        Self::Char,
        Self::Atom,
        Self::Void,
        Self::Never,
    ];

    /// A structural record type of `fields`.
    ///
    /// # Errors
    ///
    /// A field name is not an identifier or is repeated, or there is no field.
    pub fn record(
        fields: impl IntoIterator<Item = (String, Self)>,
    ) -> Result<Self, IrError> {
        let fields = NamedTypes::new(fields)?;
        if fields.is_empty() {
            return Err(IrError::InvalidType(
                "a record type has at least one field".to_owned(),
            ));
        }
        Ok(Self::Record(fields))
    }

    /// `List(element)`.
    #[must_use]
    pub fn list(element: Self) -> Self {
        Self::List(Box::new(element))
    }

    /// `Fn(signature)`.
    #[must_use]
    pub fn function(signature: FnType) -> Self {
        Self::Fn(Box::new(signature))
    }

    /// The wire atom of a compiler-owned scalar type or `Any`, without its
    /// `@`, such as `u8`.
    #[must_use]
    pub const fn wire_atom(&self) -> Option<&'static str> {
        Some(match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Char => "char",
            Self::Atom => "atom",
            Self::Void => "void",
            Self::Never => "never",
            Self::Any => "any",
            _ => return None,
        })
    }

    /// Whether this is one of the eight integer types.
    #[must_use]
    pub const fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }

    /// Whether this is an interface value type: `Any` or an interface.
    #[must_use]
    pub const fn is_interface_value(&self) -> bool {
        matches!(self, Self::Any | Self::Interface(..))
    }

    /// Whether this type holds no open form.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.validate(OpenScope::Closed).is_ok()
    }

    /// Whether an association slot occurs in this type.
    #[must_use]
    pub fn mentions_associated(&self) -> bool {
        let mut found = false;
        self.visit(&mut |ty| found |= matches!(ty, Self::Associated(_)));
        found
    }

    /// Whether any open form occurs in this type.
    #[must_use]
    pub fn is_open(&self) -> bool {
        let mut found = false;
        self.visit(&mut |ty| {
            found |=
                matches!(ty, Self::Binder(_) | Self::SelfType | Self::Associated(_));
        });
        found
    }

    /// Calls `visit` on this type and on every type nested in it, outermost
    /// first and then in [`Self::components`] order.
    pub fn visit(&self, visit: &mut impl FnMut(&Self)) {
        let mut pending = vec![self];
        while let Some(ty) = pending.pop() {
            visit(ty);
            pending.extend(ty.components().into_iter().rev());
        }
    }

    /// The number of levels this type nests: one for a type with no
    /// component. It is measured without recursing.
    #[must_use]
    pub fn depth(&self) -> usize {
        let mut deepest = 0;
        let mut pending = vec![(self, 1_usize)];
        while let Some((ty, depth)) = pending.pop() {
            deepest = deepest.max(depth);
            let below = depth.saturating_add(1);
            pending.extend(ty.components().into_iter().map(|inner| (inner, below)));
        }
        deepest
    }

    /// Checks that this type nests at most [`Self::MAX_DEPTH`] levels, the
    /// bound under which every other operation on a type may recurse over
    /// it.
    ///
    /// # Errors
    ///
    /// [`IrError::InvalidType`] for a deeper type.
    pub fn check_depth(&self) -> Result<(), IrError> {
        if self.depth() <= Self::MAX_DEPTH {
            Ok(())
        } else {
            Err(IrError::InvalidType(format!(
                "a type nested more than {} levels deep",
                Self::MAX_DEPTH
            )))
        }
    }

    /// The types this type is directly built from, in a fixed order.
    #[must_use]
    pub fn components(&self) -> Vec<&Self> {
        match self {
            Self::List(element) => vec![element],
            Self::Tuple(values) | Self::Declared(_, values) => values.iter().collect(),
            Self::Record(fields) => {
                fields.entries().iter().map(|(_, value)| value).collect()
            }
            Self::Fn(signature) => signature
                .parameters
                .iter()
                .chain(signature.labels.entries().iter().map(|(_, value)| value))
                .chain(std::iter::once(&signature.result))
                .collect(),
            Self::Proc(signature) => signature
                .parameters
                .iter()
                .chain(signature.labels.entries().iter().map(|(_, value)| value))
                .chain(std::iter::once(&signature.result))
                .collect(),
            Self::Intent(intent) => vec![&intent.result],
            Self::Interface(_, arguments, choices) => arguments
                .iter()
                .chain(choices.entries().iter().map(|(_, value)| value))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Checks the shape of this type where `scope` says which open forms are
    /// valid: it nests at most [`Self::MAX_DEPTH`] levels, a record has a
    /// field, an atom is spelled as one, a binder is in range, and `Self` and
    /// association slots appear only in an interface member.
    ///
    /// # Errors
    ///
    /// The first violation found, a type nested too deep before any other.
    pub fn validate(&self, scope: OpenScope<'_>) -> Result<(), IrError> {
        self.check_depth()?;
        let mut pending = vec![self];
        while let Some(ty) = pending.pop() {
            match ty {
                Self::AtomSingleton(atom) if !is_atom_path(atom) => {
                    return Err(IrError::InvalidType(format!(
                        "`@{atom}` is not an atom"
                    )));
                }
                Self::Record(fields) if fields.is_empty() => {
                    return Err(IrError::InvalidType(
                        "a record type has at least one field".to_owned(),
                    ));
                }
                Self::Binder(index) if *index >= scope.binders() => {
                    return Err(IrError::MisplacedOpenForm(format!(
                        "binder {index} with {} binders in scope",
                        scope.binders()
                    )));
                }
                Self::SelfType
                    if !matches!(scope, OpenScope::InterfaceMember { .. }) =>
                {
                    return Err(IrError::MisplacedOpenForm(
                        "`Self` outside a member of an interface".to_owned(),
                    ));
                }
                Self::Associated(name) => match scope {
                    OpenScope::InterfaceMember { associated, .. }
                        if associated.iter().any(|declared| declared == name) => {}
                    _ => {
                        return Err(IrError::MisplacedOpenForm(format!(
                            "association slot `{name}` outside the interface that declares it"
                        )));
                    }
                },
                _ => {}
            }
            pending.extend(ty.components().into_iter().rev());
        }
        Ok(())
    }

    /// This type with its open forms replaced as `substitution` says.
    #[must_use]
    pub fn substitute(&self, substitution: &Substitution<'_>) -> Self {
        let go = |ty: &Self| ty.substitute(substitution);
        match self {
            Self::Binder(index) => substitution
                .binders
                .get(*index)
                .cloned()
                .unwrap_or_else(|| self.clone()),
            Self::SelfType => substitution
                .self_type
                .cloned()
                .unwrap_or_else(|| self.clone()),
            Self::Associated(name) => substitution
                .associated
                .and_then(|choices| choices.get(name))
                .cloned()
                .unwrap_or_else(|| self.clone()),
            Self::List(element) => Self::List(Box::new(go(element))),
            Self::Tuple(values) => Self::Tuple(values.iter().map(go).collect()),
            Self::Record(fields) => Self::Record(fields.map(go)),
            Self::Fn(signature) => Self::Fn(Box::new(FnType {
                parameters: signature.parameters.iter().map(go).collect(),
                labels: signature.labels.map(go),
                result: go(&signature.result),
            })),
            Self::Proc(signature) => Self::Proc(Box::new(ProcType {
                parameters: signature.parameters.iter().map(go).collect(),
                labels: signature.labels.map(go),
                result: go(&signature.result),
                effects: signature.effects.clone(),
            })),
            Self::Intent(intent) => Self::Intent(Box::new(IntentType {
                result: go(&intent.result),
                effects: intent.effects.clone(),
            })),
            Self::Declared(id, arguments) => {
                Self::Declared(id.clone(), arguments.iter().map(go).collect())
            }
            Self::Interface(id, arguments, choices) => Self::Interface(
                id.clone(),
                arguments.iter().map(go).collect(),
                choices.map(go),
            ),
            _ => self.clone(),
        }
    }

    /// This type with each binder replaced by the type at its position.
    #[must_use]
    pub fn instantiate(&self, binders: &[Self]) -> Self {
        self.substitute(&Substitution {
            binders,
            ..Substitution::default()
        })
    }

    /// This type with every open form replaced by `Never`: each binder,
    /// `Self`, and each association slot. It is the closed type of a
    /// constant written at this type. A value of it holds nothing at an open
    /// position, so it is a value of every instantiation of this type
    /// (`docs/spec/02-type-system.md`, "Constant expressions and constant
    /// patterns").
    #[must_use]
    pub fn at_never(&self) -> Self {
        let mut count = 0;
        let mut open_self = false;
        let mut associated = Vec::new();
        self.visit(&mut |ty| match ty {
            Self::Binder(index) => count = count.max(index.saturating_add(1)),
            Self::SelfType => open_self = true,
            Self::Associated(name) => {
                if !associated.contains(name) {
                    associated.push(name.clone());
                }
            }
            _ => {}
        });
        if count == 0 && !open_self && associated.is_empty() {
            return self.clone();
        }
        let binders = vec![Self::Never; count];
        let choices =
            NamedTypes::new(associated.into_iter().map(|name| (name, Self::Never)))
                .ok();
        self.substitute(&Substitution {
            binders: &binders,
            self_type: Some(&Self::Never),
            associated: choices.as_ref(),
        })
    }
}

fn write_list(formatter: &mut fmt::Formatter<'_>, values: &[Type]) -> fmt::Result {
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            formatter.write_str(" ")?;
        }
        write!(formatter, "{value}")?;
    }
    Ok(())
}

fn write_named(formatter: &mut fmt::Formatter<'_>, named: &NamedTypes) -> fmt::Result {
    for (index, (name, value)) in named.entries().iter().enumerate() {
        if index != 0 {
            formatter.write_str(" ")?;
        }
        write!(formatter, "{name}: {value}")?;
    }
    Ok(())
}

fn write_inputs(
    formatter: &mut fmt::Formatter<'_>,
    parameters: &[Type],
    labels: &NamedTypes,
) -> fmt::Result {
    formatter.write_str("{")?;
    write_list(formatter, parameters)?;
    if !parameters.is_empty() && !labels.is_empty() {
        formatter.write_str(" ")?;
    }
    write_named(formatter, labels)?;
    formatter.write_str("}")
}

impl fmt::Display for Row {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("{")?;
        for (index, root) in self.0.iter().enumerate() {
            if index != 0 {
                formatter.write_str(" ")?;
            }
            write!(formatter, "@{}", root.path())?;
        }
        formatter.write_str("}")
    }
}

/// Types are displayed in their source spelling, with a binder as `#index`.
impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I8 => formatter.write_str("I8"),
            Self::I16 => formatter.write_str("I16"),
            Self::I32 => formatter.write_str("I32"),
            Self::I64 => formatter.write_str("I64"),
            Self::U8 => formatter.write_str("U8"),
            Self::U16 => formatter.write_str("U16"),
            Self::U32 => formatter.write_str("U32"),
            Self::U64 => formatter.write_str("U64"),
            Self::F32 => formatter.write_str("F32"),
            Self::F64 => formatter.write_str("F64"),
            Self::Char => formatter.write_str("Char"),
            Self::Atom => formatter.write_str("Atom"),
            Self::Void => formatter.write_str("Void"),
            Self::Never => formatter.write_str("Never"),
            Self::Any => formatter.write_str("Any"),
            Self::AtomSingleton(atom) => write!(formatter, "@{atom}"),
            Self::List(element) => write!(formatter, "List({element})"),
            Self::Tuple(values) => {
                formatter.write_str("{")?;
                write_list(formatter, values)?;
                formatter.write_str("}")
            }
            Self::Record(fields) => {
                formatter.write_str("{")?;
                write_named(formatter, fields)?;
                formatter.write_str("}")
            }
            Self::Fn(signature) => {
                formatter.write_str("Fn(")?;
                write_inputs(formatter, &signature.parameters, &signature.labels)?;
                write!(formatter, " {})", signature.result)
            }
            Self::Proc(signature) => {
                formatter.write_str("Proc(")?;
                write_inputs(formatter, &signature.parameters, &signature.labels)?;
                write!(formatter, " {} {})", signature.result, signature.effects)
            }
            Self::Intent(intent) => {
                write!(formatter, "Intent({} {})", intent.result, intent.effects)
            }
            Self::Declared(id, arguments) => {
                formatter.write_str(id.name())?;
                if !arguments.is_empty() {
                    formatter.write_str("(")?;
                    write_list(formatter, arguments)?;
                    formatter.write_str(")")?;
                }
                Ok(())
            }
            Self::Interface(id, arguments, choices) => {
                formatter.write_str(id.name())?;
                if !arguments.is_empty() || !choices.is_empty() {
                    formatter.write_str("(")?;
                    write_list(formatter, arguments)?;
                    if !arguments.is_empty() && !choices.is_empty() {
                        formatter.write_str(" ")?;
                    }
                    write_named(formatter, choices)?;
                    formatter.write_str(")")?;
                }
                Ok(())
            }
            Self::Binder(index) => write!(formatter, "#{index}"),
            Self::SelfType => formatter.write_str("Self"),
            Self::Associated(name) => formatter.write_str(name),
        }
    }
}
