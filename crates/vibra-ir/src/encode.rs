//! The canonical encodings of `docs/spec/06-runtime.md`, "Canonical value
//! encoding": `@type.v2`, `@value.v2`, the signature record, and the
//! implementation record, written as canonical bytes
//! (`docs/spec/04-programs-and-packages.md`, "Decoded values and canonical
//! bytes"): UTF-8 on one line, one space between elements and after each
//! `:`, no trailing newline, and an empty compound as `{}`, `[]`, or `[:]`.
//!
//! This crate writes the bytes itself; no reader exists here.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::rc::Rc;

use crate::{
    Binder, Bound, EntityRef, ImplementationRecord, InterfaceKey, Library,
    LibraryTypes, NamedTypes, Row, SignatureRecord, Type, TypeBody, TypeDefinition,
    TypeId, Value,
};

/// The bits of the one canonical `F32` NaN: positive, quiet, zero payload.
pub const CANONICAL_NAN_F32: u32 = 0x7FC0_0000;

/// The bits of the one canonical `F64` NaN: positive, quiet, zero payload.
pub const CANONICAL_NAN_F64: u64 = 0x7FF8_0000_0000_0000;

/// Why a value or a type has no encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// A function, a closure, a procedure, or a plan, which have no
    /// encoding, named by what was found.
    NoEncoding(String),
    /// A binder reference, `Self`, or an association slot where the encoding
    /// is closed.
    OpenForm(String),
    /// The value does not fit its type.
    Mismatch {
        /// The type expected.
        expected: String,
        /// What was found instead.
        found: String,
    },
    /// A declared type the table does not define.
    UnknownType(String),
    /// A value of `Never`, which has none.
    NeverValue,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEncoding(what) => write!(formatter, "{what} has no value encoding"),
            Self::OpenForm(what) => write!(formatter, "{what} in a closed encoding"),
            Self::Mismatch { expected, found } => {
                write!(formatter, "expected a value of {expected}, found {found}")
            }
            Self::UnknownType(path) => {
                write!(formatter, "the type `@{path}` is not defined")
            }
            Self::NeverValue => formatter.write_str("`Never` has no value"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// The declared types an encoding of a value looks through: their bodies
/// fix field and case order, and the library bindings say which declared
/// type is `Str`, `Bytes`, or `Dict`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeTable {
    library: LibraryTypes,
    types: BTreeMap<TypeId, TypeDefinition>,
}

impl TypeTable {
    /// The table of `definitions` under the bindings `library`. A later
    /// definition of one identity replaces an earlier one.
    #[must_use]
    pub fn new(
        library: LibraryTypes,
        definitions: impl IntoIterator<Item = TypeDefinition>,
    ) -> Self {
        Self {
            library,
            types: definitions
                .into_iter()
                .map(|definition| (definition.id().clone(), definition))
                .collect(),
        }
    }

    /// The library bindings.
    #[must_use]
    pub const fn library(&self) -> &LibraryTypes {
        &self.library
    }

    /// The definitions, by identity.
    #[must_use]
    pub const fn types(&self) -> &BTreeMap<TypeId, TypeDefinition> {
        &self.types
    }

    /// The definition of `id`.
    #[must_use]
    pub fn definition(&self, id: &TypeId) -> Option<&TypeDefinition> {
        self.types.get(id)
    }
}

/// Appends the canonical string literal of `text`.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            other => write_scalar(out, other),
        }
    }
    out.push('"');
}

/// Appends the canonical character literal of `character`.
fn write_char(out: &mut String, character: char) {
    out.push('\'');
    match character {
        '\'' => out.push_str("\\'"),
        other => write_scalar(out, other),
    }
    out.push('\'');
}

/// The escapes strings and characters share: `\\`, `\n`, `\r`, `\t`, and
/// `\u{H}` with the fewest upper-case digits for any other scalar below
/// U+0020 and for U+007F.
fn write_scalar(out: &mut String, character: char) {
    match character {
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        control if u32::from(control) < 0x20 || control == '\u{7F}' => {
            let _ = write!(out, "\\u{{{:X}}}", u32::from(control));
        }
        other => out.push(other),
    }
}

fn write_atom(out: &mut String, path: &str) {
    out.push('@');
    out.push_str(path);
}

/// Appends `items`, each written by `write`, separated by single spaces.
fn write_spaced<T>(
    out: &mut String,
    items: impl IntoIterator<Item = T>,
    mut write: impl FnMut(&mut String, T) -> Result<(), EncodeError>,
) -> Result<(), EncodeError> {
    for (index, item) in items.into_iter().enumerate() {
        if index != 0 {
            out.push(' ');
        }
        write(out, item)?;
    }
    Ok(())
}

/// Appends a dict from `Str` names to `T`s in canonical name order: `[:]`
/// when it is empty.
fn write_type_dict(
    out: &mut String,
    named: &NamedTypes,
    open: bool,
) -> Result<(), EncodeError> {
    if named.is_empty() {
        out.push_str("[:]");
        return Ok(());
    }
    out.push('[');
    write_spaced(out, named.entries(), |out, (name, ty)| {
        write_string(out, name);
        out.push_str(": ");
        write_type(out, ty, open)
    })?;
    out.push(']');
    Ok(())
}

fn write_type_list(
    out: &mut String,
    types: &[Type],
    open: bool,
) -> Result<(), EncodeError> {
    out.push('[');
    write_spaced(out, types, |out, ty| write_type(out, ty, open))?;
    out.push(']');
    Ok(())
}

fn write_entity(out: &mut String, entity: &EntityRef) {
    out.push_str("path: ");
    write_atom(out, entity.path());
    out.push_str(" seed: ");
    write_string(out, entity.seed());
}

fn write_row(out: &mut String, row: &Row) {
    out.push_str("{roots: [");
    for (index, root) in row.roots().iter().enumerate() {
        if index != 0 {
            out.push(' ');
        }
        out.push('{');
        write_entity(out, root);
        out.push('}');
    }
    out.push_str("]}");
}

/// Appends the type encoding of `ty`; with `open`, the open type encoding,
/// which also has the three references.
fn write_type(out: &mut String, ty: &Type, open: bool) -> Result<(), EncodeError> {
    if let Some(atom) = ty.wire_atom() {
        write_atom(out, atom);
        return Ok(());
    }
    match ty {
        Type::AtomSingleton(atom) => {
            out.push_str("{kind: @atomSingleton atom: ");
            write_atom(out, atom);
            out.push('}');
        }
        Type::List(element) => {
            out.push_str("{kind: @list arguments: [");
            write_type(out, element, open)?;
            out.push_str("]}");
        }
        Type::Tuple(values) => {
            out.push_str("{kind: @tuple arguments: ");
            write_type_list(out, values, open)?;
            out.push('}');
        }
        Type::Record(fields) => {
            out.push_str("{kind: @record fields: {");
            write_spaced(out, fields.entries(), |out, (name, value)| {
                out.push_str(name);
                out.push_str(": ");
                write_type(out, value, open)
            })?;
            out.push_str("}}");
        }
        Type::Fn(signature) => {
            out.push_str("{kind: @fn parameters: ");
            write_type_list(out, &signature.parameters, open)?;
            out.push_str(" labels: ");
            write_type_dict(out, &signature.labels, open)?;
            out.push_str(" result: ");
            write_type(out, &signature.result, open)?;
            out.push('}');
        }
        Type::Proc(signature) => {
            out.push_str("{kind: @proc parameters: ");
            write_type_list(out, &signature.parameters, open)?;
            out.push_str(" labels: ");
            write_type_dict(out, &signature.labels, open)?;
            out.push_str(" result: ");
            write_type(out, &signature.result, open)?;
            out.push_str(" effects: ");
            write_row(out, &signature.effects);
            out.push('}');
        }
        Type::Intent(intent) => {
            out.push_str("{kind: @intent result: ");
            write_type(out, &intent.result, open)?;
            out.push_str(" effects: ");
            write_row(out, &intent.effects);
            out.push('}');
        }
        Type::Declared(id, arguments) => {
            out.push_str("{kind: @declared ");
            write_entity(out, id.entity());
            out.push_str(" arguments: ");
            write_type_list(out, arguments, open)?;
            out.push('}');
        }
        Type::Interface(id, arguments, choices) => {
            out.push_str("{kind: @interface ");
            write_entity(out, id.entity());
            out.push_str(" arguments: ");
            write_type_list(out, arguments, open)?;
            out.push_str(" choices: ");
            write_type_dict(out, choices, open)?;
            out.push('}');
        }
        Type::Binder(index) if open => {
            let _ = write!(out, "{{kind: @binder index: {index}u64}}");
        }
        Type::SelfType if open => out.push_str("{kind: @self}"),
        Type::Associated(name) if open => {
            out.push_str("{kind: @associated name: ");
            write_string(out, name);
            out.push('}');
        }
        other => return Err(EncodeError::OpenForm(other.to_string())),
    }
    Ok(())
}

/// The canonical bytes of the closed type encoding of `ty`.
///
/// # Errors
///
/// The type holds a binder reference, `Self`, or an association slot.
pub fn encode_type(ty: &Type) -> Result<String, EncodeError> {
    let mut out = String::new();
    write_type(&mut out, ty, false)?;
    Ok(out)
}

/// The canonical bytes of the `@type.v2` document of `ty`.
///
/// # Errors
///
/// As [`encode_type`].
pub fn type_document(ty: &Type) -> Result<String, EncodeError> {
    Ok(format!(
        "{{format: @type.v2 typeInfo: {}}}",
        encode_type(ty)?
    ))
}

/// The canonical bytes of the open type encoding of `ty`, as a signature or
/// implementation record writes it.
///
/// # Errors
///
/// None for a type of a record that was built; the result keeps the
/// writer's signature.
pub fn encode_open_type(ty: &Type) -> Result<String, EncodeError> {
    let mut out = String::new();
    write_type(&mut out, ty, true)?;
    Ok(out)
}

impl InterfaceKey {
    /// The canonical bytes of this interface key: `{entity arguments}`.
    ///
    /// # Errors
    ///
    /// As [`encode_open_type`].
    pub fn encode(&self) -> Result<String, EncodeError> {
        let mut out = String::new();
        write_key(&mut out, self)?;
        Ok(out)
    }
}

fn write_key(out: &mut String, key: &InterfaceKey) -> Result<(), EncodeError> {
    out.push('{');
    write_entity(out, key.interface.entity());
    out.push_str(" arguments: ");
    write_type_list(out, &key.arguments, true)?;
    out.push('}');
    Ok(())
}

fn write_bound(out: &mut String, bound: &Bound) -> Result<(), EncodeError> {
    out.push_str("{contract: ");
    write_key(out, &bound.contract)?;
    out.push_str(" choices: ");
    write_type_dict(out, &bound.choices, true)?;
    out.push('}');
    Ok(())
}

fn write_binders(out: &mut String, binders: &[Binder]) -> Result<(), EncodeError> {
    out.push('[');
    write_spaced(out, binders, |out, binder| {
        out.push_str("{name: ");
        write_string(out, binder.name());
        if let Some(bound) = binder.bound() {
            out.push_str(" bound: ");
            write_bound(out, bound)?;
        }
        out.push('}');
        Ok(())
    })?;
    out.push(']');
    Ok(())
}

impl SignatureRecord {
    /// The canonical bytes of this signature record:
    /// `{binders receiver typeInfo}`, without `receiver` when there is none.
    ///
    /// # Errors
    ///
    /// None for a record that was built, which holds only valid open forms;
    /// the result keeps the writer's signature.
    pub fn encode(&self) -> Result<String, EncodeError> {
        let mut out = String::from("{binders: ");
        write_binders(&mut out, self.binders())?;
        if let Some(receiver) = self.receiver() {
            let _ = write!(out, " receiver: {receiver}u64");
        }
        out.push_str(" typeInfo: ");
        write_type(&mut out, self.type_info(), true)?;
        out.push('}');
        Ok(out)
    }
}

impl ImplementationRecord {
    /// The canonical bytes of this implementation record:
    /// `{binders contract receiver choices}`.
    ///
    /// # Errors
    ///
    /// As [`SignatureRecord::encode`].
    pub fn encode(&self) -> Result<String, EncodeError> {
        let mut out = String::from("{binders: ");
        write_binders(&mut out, self.binders())?;
        out.push_str(" contract: ");
        write_key(&mut out, self.contract())?;
        out.push_str(" receiver: ");
        write_type(&mut out, self.receiver(), true)?;
        out.push_str(" choices: ");
        write_type_dict(&mut out, self.choices(), true)?;
        out.push('}');
        Ok(out)
    }
}

/// The shape of a value, for an error message.
fn describe(value: &Value) -> String {
    match value {
        Value::I8(_) => "an `I8`",
        Value::I16(_) => "an `I16`",
        Value::I32(_) => "an `I32`",
        Value::I64(_) => "an `I64`",
        Value::U8(_) => "a `U8`",
        Value::U16(_) => "a `U16`",
        Value::U32(_) => "a `U32`",
        Value::U64(_) => "a `U64`",
        Value::F32(_) => "an `F32`",
        Value::F64(_) => "an `F64`",
        Value::Char(_) => "a `Char`",
        Value::Atom(_) => "an atom",
        Value::Void => "`void`",
        Value::Str(_) => "a string",
        Value::Bytes(_) => "bytes",
        Value::Tuple(_) => "a tuple",
        Value::List(_) => "a list",
        Value::Dict(_) => "a dict",
        Value::Record(_) => "a record",
        Value::Variant { .. } => "a variant",
        Value::Wrapper(_) => "a wrapper",
        Value::Interface { .. } => "an interface value",
        Value::Function { .. } => "a function",
        Value::Closure { .. } => "a closure",
    }
    .to_owned()
}

fn mismatch(ty: &Type, value: &Value) -> EncodeError {
    EncodeError::Mismatch {
        expected: ty.to_string(),
        found: describe(value),
    }
}

/// The canonical text of a finite float written by Rust's shortest
/// round-trip formatting, which is the canonical text of the runtime
/// chapter: decimal notation with a fractional digit for zero and for a
/// magnitude in [1e-4, 1e16), scientific notation otherwise.
fn write_float(out: &mut String, finite: impl fmt::Debug, suffix: &str) {
    let _ = write!(out, "{finite:?}{suffix}");
}

fn write_non_finite(out: &mut String, nan: bool, negative: bool) {
    out.push_str("{kind: @nonFinite value: ");
    out.push_str(match (nan, negative) {
        (true, _) => "@nan",
        (false, false) => "@positiveInfinity",
        (false, true) => "@negativeInfinity",
    });
    out.push('}');
}

/// A part of a value encoding still to write. The encoder keeps these on a
/// work list, so that no host stack grows with the depth of the value.
enum Piece<'v> {
    /// Literal text.
    Text(&'static str),
    /// A value at its closed type.
    Value(&'v Value, Rc<Type>),
    /// `name: v`, a field of a record or a case.
    Field(String, &'v Value, Rc<Type>),
    /// `{kind: @tuple values: [k v]}`, an entry of a dict.
    Entry(&'v Value, Rc<Type>, &'v Value, Rc<Type>),
}

/// Queues `children`, separated by spaces, and then `closing`.
fn nest<'v>(
    pending: &mut Vec<Piece<'v>>,
    children: Vec<Piece<'v>>,
    closing: &'static str,
) {
    pending.push(Piece::Text(closing));
    let count = children.len();
    for (index, child) in children.into_iter().rev().enumerate() {
        pending.push(child);
        if index.saturating_add(1) < count {
            pending.push(Piece::Text(" "));
        }
    }
}

/// The fields of a record or a case in the given order, each at its type.
fn field_pieces<'v>(
    declared: &[(String, Type)],
    written: &'v [(String, Value)],
    ty: &Type,
    value: &Value,
) -> Result<Vec<Piece<'v>>, EncodeError> {
    if declared.len() != written.len()
        || !declared.iter().all(|(name, _)| {
            written.iter().filter(|(field, _)| field == name).count() == 1
        })
    {
        return Err(mismatch(ty, value));
    }
    declared
        .iter()
        .map(|(name, field_type)| {
            let field = written
                .iter()
                .find_map(|(field, held)| (field == name).then_some(held))
                .ok_or_else(|| mismatch(ty, value))?;
            Ok(Piece::Field(
                name.clone(),
                field,
                Rc::new(field_type.clone()),
            ))
        })
        .collect()
}

/// Appends the value encoding of `value` at the closed type `ty`.
fn write_value(
    out: &mut String,
    value: &Value,
    ty: &Type,
    table: &TypeTable,
) -> Result<(), EncodeError> {
    let mut pending = vec![Piece::Value(value, Rc::new(ty.clone()))];
    while let Some(piece) = pending.pop() {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Value(value, ty) => {
                write_node(out, value, &ty, table, &mut pending)?
            }
            Piece::Field(name, value, ty) => {
                out.push_str(&name);
                out.push_str(": ");
                pending.push(Piece::Value(value, ty));
            }
            Piece::Entry(key, key_type, item, item_type) => {
                out.push_str("{kind: @tuple values: [");
                pending.extend([
                    Piece::Text("]}"),
                    Piece::Value(item, item_type),
                    Piece::Text(" "),
                    Piece::Value(key, key_type),
                ]);
            }
        }
    }
    Ok(())
}

/// Appends the start of the encoding of `value` at `ty` and queues the
/// rest: its children and its closing text.
fn write_node<'v>(
    out: &mut String,
    value: &'v Value,
    ty: &Type,
    table: &TypeTable,
    pending: &mut Vec<Piece<'v>>,
) -> Result<(), EncodeError> {
    if let Value::Function { .. } | Value::Closure { .. } = value {
        return Err(EncodeError::NoEncoding(describe(value)));
    }
    match (ty, value) {
        (Type::Binder(_) | Type::SelfType | Type::Associated(_), _) => {
            return Err(EncodeError::OpenForm(ty.to_string()));
        }
        (Type::Never, _) => return Err(EncodeError::NeverValue),
        (Type::Fn(_) | Type::Proc(_) | Type::Intent(_), _) => {
            return Err(EncodeError::NoEncoding(ty.to_string()));
        }
        (Type::I8, Value::I8(number)) => {
            let _ = write!(out, "{number}i8");
        }
        (Type::I16, Value::I16(number)) => {
            let _ = write!(out, "{number}i16");
        }
        (Type::I32, Value::I32(number)) => {
            let _ = write!(out, "{number}i32");
        }
        (Type::I64, Value::I64(number)) => {
            let _ = write!(out, "{number}i64");
        }
        (Type::U8, Value::U8(number)) => {
            let _ = write!(out, "{number}u8");
        }
        (Type::U16, Value::U16(number)) => {
            let _ = write!(out, "{number}u16");
        }
        (Type::U32, Value::U32(number)) => {
            let _ = write!(out, "{number}u32");
        }
        (Type::U64, Value::U64(number)) => {
            let _ = write!(out, "{number}u64");
        }
        (Type::F32, Value::F32(bits)) => {
            let number = f32::from_bits(*bits);
            if number.is_finite() {
                write_float(out, number, "f32");
            } else {
                write_non_finite(out, number.is_nan(), number.is_sign_negative());
            }
        }
        (Type::F64, Value::F64(bits)) => {
            let number = f64::from_bits(*bits);
            if number.is_finite() {
                write_float(out, number, "f64");
            } else {
                write_non_finite(out, number.is_nan(), number.is_sign_negative());
            }
        }
        (Type::Char, Value::Char(character)) => write_char(out, *character),
        (Type::Atom, Value::Atom(atom)) => write_atom(out, atom),
        (Type::AtomSingleton(singleton), Value::Atom(atom)) if singleton == atom => {
            write_atom(out, atom);
        }
        (Type::Void, Value::Void) => out.push_str("void"),
        (Type::Any | Type::Interface(..), Value::Interface { held_type, held }) => {
            if held_type.is_interface_value() || held_type.depth() > Type::MAX_DEPTH {
                return Err(mismatch(ty, value));
            }
            out.push_str("{kind: @interface held: {format: @value.v2 typeInfo: ");
            write_type(out, held_type, false)?;
            out.push_str(" value: ");
            pending.push(Piece::Text("}}"));
            pending.push(Piece::Value(held, Rc::new(held_type.clone())));
        }
        (Type::List(element), Value::List(items)) => {
            out.push_str("{kind: @list values: [");
            let element = Rc::new(Type::clone(element));
            let children = items
                .iter()
                .map(|item| Piece::Value(item, Rc::clone(&element)))
                .collect();
            nest(pending, children, "]}");
        }
        (Type::Tuple(types), Value::Tuple(values)) if types.len() == values.len() => {
            out.push_str("{kind: @tuple values: [");
            let children = values
                .iter()
                .zip(types)
                .map(|(value, ty)| Piece::Value(value, Rc::new(ty.clone())))
                .collect();
            nest(pending, children, "]}");
        }
        (Type::Record(fields), Value::Record(written)) => {
            let children = field_pieces(fields.entries(), written, ty, value)?;
            out.push_str("{kind: @record fields: {");
            nest(pending, children, "}}");
        }
        (Type::Declared(id, arguments), _) => {
            write_declared(out, (id, arguments), ty, value, table, pending)?;
        }
        _ => return Err(mismatch(ty, value)),
    }
    Ok(())
}

fn write_declared<'v>(
    out: &mut String,
    (id, arguments): (&TypeId, &[Type]),
    ty: &Type,
    value: &'v Value,
    table: &TypeTable,
    pending: &mut Vec<Piece<'v>>,
) -> Result<(), EncodeError> {
    let library = table.library().library_of(id);
    match (library, value) {
        (Some(Library::Str), Value::Str(text)) => {
            write_string(out, text);
            return Ok(());
        }
        (Some(Library::Bytes), Value::Bytes(bytes)) => {
            out.push_str("{kind: @bytes values: [");
            write_spaced(out, bytes, |out, byte| {
                let _ = write!(out, "{byte}u8");
                Ok(())
            })?;
            out.push_str("]}");
            return Ok(());
        }
        (Some(Library::Dict), Value::Dict(entries)) => {
            let [key, item] = arguments else {
                return Err(mismatch(ty, value));
            };
            out.push_str("{kind: @dict entries: [");
            let (key, item) = (Rc::new(key.clone()), Rc::new(item.clone()));
            let children = entries
                .iter()
                .map(|(entry_key, entry_value)| {
                    Piece::Entry(
                        entry_key,
                        Rc::clone(&key),
                        entry_value,
                        Rc::clone(&item),
                    )
                })
                .collect();
            nest(pending, children, "]}");
            return Ok(());
        }
        (Some(Library::Str | Library::Bytes | Library::Dict), _) => {
            return Err(mismatch(ty, value));
        }
        _ => {}
    }
    let definition = table
        .definition(id)
        .ok_or_else(|| EncodeError::UnknownType(id.path().to_owned()))?;
    if definition.parameters().len() != arguments.len() {
        return Err(mismatch(ty, value));
    }
    let instantiate = |fields: &[(String, Type)]| {
        fields
            .iter()
            .map(|(name, field)| (name.clone(), field.instantiate(arguments)))
            .collect::<Vec<_>>()
    };
    let mut type_info = String::new();
    write_type(&mut type_info, ty, false)?;
    match (definition.body(), value) {
        (TypeBody::Tuple(types), Value::Tuple(values))
            if types.len() == values.len() =>
        {
            let _ = write!(out, "{{kind: @tuple typeInfo: {type_info} values: [");
            let children = values
                .iter()
                .zip(types)
                .map(|(value, component)| {
                    Piece::Value(value, Rc::new(component.instantiate(arguments)))
                })
                .collect();
            nest(pending, children, "]}");
        }
        (TypeBody::Record(fields), Value::Record(written)) => {
            let children = field_pieces(&instantiate(fields), written, ty, value)?;
            let _ = write!(out, "{{kind: @record typeInfo: {type_info} fields: {{");
            nest(pending, children, "}}");
        }
        (TypeBody::Wrapper(representation), Value::Wrapper(inner)) => {
            let _ = write!(out, "{{kind: @wrapper typeInfo: {type_info} value: ");
            pending.push(Piece::Text("}"));
            pending.push(Piece::Value(
                inner,
                Rc::new(representation.instantiate(arguments)),
            ));
        }
        (TypeBody::Variant(_), Value::Variant { case, fields }) => {
            let declared = definition.case(case).ok_or_else(|| mismatch(ty, value))?;
            let _ = write!(out, "{{kind: @enum typeInfo: {type_info} variant: ");
            write_atom(out, case);
            if declared.fields.is_empty() {
                if !fields.is_empty() {
                    return Err(mismatch(ty, value));
                }
                out.push('}');
            } else {
                let children =
                    field_pieces(&instantiate(&declared.fields), fields, ty, value)?;
                out.push_str(" payload: {");
                nest(pending, children, "}}");
            }
        }
        _ => return Err(mismatch(ty, value)),
    }
    Ok(())
}

fn write_document(
    out: &mut String,
    value: &Value,
    ty: &Type,
    table: &TypeTable,
) -> Result<(), EncodeError> {
    out.push_str("{format: @value.v2 typeInfo: ");
    write_type(out, ty, false)?;
    out.push_str(" value: ");
    write_value(out, value, ty, table)?;
    out.push('}');
    Ok(())
}

/// The canonical bytes of the value encoding of `value` at the closed type
/// `ty`.
///
/// # Errors
///
/// The value holds a function or a closure, at any depth, or the type is or
/// holds a callable type; the type is open or `Never`; or the value does
/// not fit the type.
pub fn encode_value(
    value: &Value,
    ty: &Type,
    table: &TypeTable,
) -> Result<String, EncodeError> {
    let mut out = String::new();
    write_value(&mut out, value, ty, table)?;
    Ok(out)
}

/// The canonical bytes of the `@value.v2` document of `value` at `ty`:
/// `{format: @value.v2 typeInfo: T value: v}`.
///
/// # Errors
///
/// As [`encode_value`].
pub fn value_document(
    value: &Value,
    ty: &Type,
    table: &TypeTable,
) -> Result<String, EncodeError> {
    let mut out = String::new();
    write_document(&mut out, value, ty, table)?;
    Ok(out)
}
