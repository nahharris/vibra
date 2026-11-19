//! Schema-typed VIBON: closed record schemas, the reader's validation order,
//! decoded values, and canonical bytes.
//!
//! `docs/spec/04-programs-and-packages.md`, "Schemas", fixes the order in
//! which a reader validates a document and reports one fault once:
//!
//! 1. lexical and grammatical faults;
//! 2. a repeated record field, `@data.duplicateField`;
//! 3. format selection, `@data.invalidValue` at the `format` value;
//! 4. shape, `@data.invalidShape`;
//! 5. value, `@data.invalidValue`;
//! 6. a repeated dict key, compared as a typed value, `@data.duplicateKey`.
//!
//! The first step that finds a fault ends the reading: its faults are the
//! document's diagnostics and no later step runs. Within a step every fault
//! is reported, each once.
//!
//! "Decoded values and canonical bytes" fixes the decoded value and the
//! canonical bytes: one line, no comment, fields in schema order, defaults
//! omitted, every number suffixed, dict entries in the key type's order. The
//! canonical bytes of a [`TypedDocument`] are a function of its decoded value
//! and its schema alone.
//!
//! An opaque slot holds data no schema describes. Its decoded value is its
//! canonical bytes under the opaque rules of the same section: record fields
//! in the byte order of their names, numbers with their minimal digits and the
//! suffix they were written with, and dict entries in the byte order of their
//! canonical key bytes.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Write as _;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::data::{
    DataEntry, DataField, DataNode, DataValue, FloatLiteral, IntegerLiteral,
};
use crate::document::parse_data_for_schema;
use crate::literal::{
    FloatSuffix, IntegerSuffix, add_to_decimal, canonical_character, canonical_string,
    decimal_value, integer_value,
};

/// A value constraint over the spelling of a string or an atom: `None` when
/// the spelling is admitted, otherwise the reason it is not.
pub type SpellingRule = fn(&str) -> Option<&'static str>;

/// A schema default: the decoded value the reader supplies for an absent
/// optional field.
pub type DefaultValue = fn() -> TypedValue;

/// The type of one slot of a schema.
#[derive(Clone, Debug)]
pub enum Slot {
    /// `true` or `false`.
    Bool,
    /// A character.
    Char,
    /// A string, with an optional constraint on its decoded scalars.
    Str(Option<SpellingRule>),
    /// An integer of one exact width.
    Integer(IntegerSuffix),
    /// A float of one exact width.
    Float(FloatSuffix),
    /// `void`.
    Void,
    /// An atom, with an optional constraint on its spelling, `@` included.
    Atom(Option<SpellingRule>),
    /// An atom of a closed set, each spelled with its `@`.
    AtomOf(Vec<&'static str>),
    /// A list of one element type with at least `min` elements.
    List {
        /// The element type.
        element: Box<Slot>,
        /// The fewest elements the slot admits.
        min: usize,
    },
    /// A dict from one key type to one value type.
    Dict {
        /// The key type.
        key: Box<Slot>,
        /// The value type.
        value: Box<Slot>,
    },
    /// A closed record.
    Record(RecordSchema),
    /// A closed record selected by the atom in one of its fields.
    Variant(VariantSchema),
    /// Data no schema describes.
    Opaque,
}

impl Slot {
    /// A list of `element` values, possibly empty.
    #[must_use]
    pub fn list(element: Self) -> Self {
        Self::List {
            element: Box::new(element),
            min: 0,
        }
    }

    /// A list of `element` values with at least one element.
    #[must_use]
    pub fn non_empty_list(element: Self) -> Self {
        Self::List {
            element: Box::new(element),
            min: 1,
        }
    }

    /// A dict from `key` to `value`.
    #[must_use]
    pub fn dict(key: Self, value: Self) -> Self {
        Self::Dict {
            key: Box::new(key),
            value: Box::new(value),
        }
    }
}

/// Whether a field must be written.
#[derive(Clone, Copy, Debug)]
pub enum Presence {
    /// The field must be written.
    Required,
    /// The field may be omitted, and is then absent.
    Optional,
    /// The field may be omitted, and the reader then supplies the default.
    Default(DefaultValue),
}

/// One field of a closed record.
#[derive(Clone, Debug)]
pub struct FieldSchema {
    name: &'static str,
    slot: Slot,
    presence: Presence,
}

impl FieldSchema {
    /// A required field.
    #[must_use]
    pub const fn required(name: &'static str, slot: Slot) -> Self {
        Self {
            name,
            slot,
            presence: Presence::Required,
        }
    }

    /// An optional field with no default.
    #[must_use]
    pub const fn optional(name: &'static str, slot: Slot) -> Self {
        Self {
            name,
            slot,
            presence: Presence::Optional,
        }
    }

    /// An optional field whose absence means `default`.
    #[must_use]
    pub const fn with_default(
        name: &'static str,
        slot: Slot,
        default: DefaultValue,
    ) -> Self {
        Self {
            name,
            slot,
            presence: Presence::Default(default),
        }
    }

    /// The field name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// A closed record: its fields in canonical order.
#[derive(Clone, Debug)]
pub struct RecordSchema {
    fields: Vec<FieldSchema>,
    min_written: usize,
}

impl RecordSchema {
    /// A record of `fields`, in canonical order.
    #[must_use]
    pub const fn new(fields: Vec<FieldSchema>) -> Self {
        Self {
            fields,
            min_written: 0,
        }
    }

    /// A record of `fields` that writes at least one of them: fewer is
    /// `@data.invalidValue` at the record.
    #[must_use]
    pub const fn at_least_one(fields: Vec<FieldSchema>) -> Self {
        Self {
            fields,
            min_written: 1,
        }
    }

    /// The fields in canonical order.
    #[must_use]
    pub fn fields(&self) -> &[FieldSchema] {
        &self.fields
    }
}

/// A closed record whose fields depend on the atom in its tag field.
///
/// Each case is a whole record, its tag field included, so the canonical
/// field order is the case's own. A tag outside the cases is
/// `@data.invalidValue` at the tag, and no other field of that record is
/// checked: no record schema was selected.
#[derive(Clone, Debug)]
pub struct VariantSchema {
    tag: &'static str,
    cases: Vec<(&'static str, RecordSchema)>,
}

impl VariantSchema {
    /// A variant tagged by the field `tag`, with one record per tag atom.
    #[must_use]
    pub const fn new(
        tag: &'static str,
        cases: Vec<(&'static str, RecordSchema)>,
    ) -> Self {
        Self { tag, cases }
    }
}

/// A compiler-owned document format: its version atom and its closed record.
#[derive(Clone, Debug)]
pub struct Schema {
    format: &'static str,
    root: RecordSchema,
}

impl Schema {
    /// The format `format` (spelled with its `@`), whose root record has a
    /// first field `format` followed by `fields`.
    #[must_use]
    pub fn new(format: &'static str, fields: Vec<FieldSchema>) -> Self {
        let mut all = Vec::with_capacity(fields.len() + 1);
        all.push(FieldSchema::required("format", Slot::AtomOf(vec![format])));
        all.extend(fields);
        Self {
            format,
            root: RecordSchema::new(all),
        }
    }

    /// The format atom, `@` included.
    #[must_use]
    pub const fn format(&self) -> &'static str {
        self.format
    }

    /// The root record, its `format` field first.
    #[must_use]
    pub const fn root(&self) -> &RecordSchema {
        &self.root
    }
}

/// A decoded value of a schema slot.
#[derive(Clone, Debug, PartialEq)]
pub enum TypedValue {
    /// A boolean.
    Bool(bool),
    /// A character.
    Char(char),
    /// A string, as its scalars.
    Str(String),
    /// An integer and its width.
    Integer(i128, IntegerSuffix),
    /// A finite float and its width. An `F32` value is held widened.
    Float(f64, FloatSuffix),
    /// `void`.
    Void,
    /// An atom, spelled with its `@`.
    Atom(String),
    /// A list, in order.
    List(Vec<TypedNode>),
    /// A dict, its entries in the key type's order.
    Dict(Vec<TypedEntry>),
    /// A record, its fields in schema order; an absent field without a
    /// default is not held.
    Record(Vec<TypedField>),
    /// Opaque data, as its canonical bytes.
    Opaque(String),
}

/// A decoded value and the bytes it was read from.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedNode {
    value: TypedValue,
    span: ByteSpan,
}

impl TypedNode {
    /// The decoded value.
    #[must_use]
    pub const fn value(&self) -> &TypedValue {
        &self.value
    }

    /// The bytes the value was read from. A default the reader supplied has
    /// the span of the record it was supplied to.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The field `name` of a record, whether written or defaulted.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&TypedField> {
        match &self.value {
            TypedValue::Record(fields) => {
                fields.iter().find(|field| field.name == name)
            }
            _ => None,
        }
    }

    /// The value of the field `name` of a record.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Self> {
        self.field(name).map(TypedField::value)
    }

    /// The scalars of a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            TypedValue::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The spelling of an atom, `@` included.
    #[must_use]
    pub fn as_atom(&self) -> Option<&str> {
        match &self.value {
            TypedValue::Atom(text) => Some(text),
            _ => None,
        }
    }

    /// The elements of a list.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Self]> {
        match &self.value {
            TypedValue::List(elements) => Some(elements),
            _ => None,
        }
    }

    /// The entries of a dict, in dict order.
    #[must_use]
    pub fn as_entries(&self) -> Option<&[TypedEntry]> {
        match &self.value {
            TypedValue::Dict(entries) => Some(entries),
            _ => None,
        }
    }

    /// The value of an integer.
    #[must_use]
    pub const fn as_integer(&self) -> Option<i128> {
        match &self.value {
            TypedValue::Integer(value, _) => Some(*value),
            _ => None,
        }
    }
}

/// One field of a decoded record.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedField {
    name: &'static str,
    name_span: Option<ByteSpan>,
    value: TypedNode,
    is_default: bool,
}

impl TypedField {
    /// The field name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Where the name was written; `None` for a default the reader supplied.
    #[must_use]
    pub const fn name_span(&self) -> Option<ByteSpan> {
        self.name_span
    }

    /// The field's value.
    #[must_use]
    pub const fn value(&self) -> &TypedNode {
        &self.value
    }

    /// Whether the value equals the field's schema default, whatever the
    /// document spelled. Canonical bytes omit such a field.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }
}

/// One entry of a decoded dict.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedEntry {
    key: TypedNode,
    value: TypedNode,
}

impl TypedEntry {
    /// The key.
    #[must_use]
    pub const fn key(&self) -> &TypedNode {
        &self.key
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> &TypedNode {
        &self.value
    }

    /// The bytes from the key's first byte to the value's last.
    #[must_use]
    pub fn span(&self) -> ByteSpan {
        self.key.span.join(self.value.span)
    }
}

/// A document validated against its schema.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedDocument {
    format: &'static str,
    root: TypedNode,
}

impl TypedDocument {
    /// The format atom the document was read under.
    #[must_use]
    pub const fn format(&self) -> &'static str {
        self.format
    }

    /// The decoded root record.
    #[must_use]
    pub const fn root(&self) -> &TypedNode {
        &self.root
    }

    /// The canonical bytes: one line, no comment, no trailing newline.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        write_typed(&self.root, &mut out);
        out
    }
}

/// The result of reading one document under a schema.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    document: Option<TypedDocument>,
    diagnostics: Vec<Diagnostic>,
}

impl Decoded {
    /// The validated document, when the reading found no fault.
    #[must_use]
    pub const fn document(&self) -> Option<&TypedDocument> {
        self.document.as_ref()
    }

    /// The faults of the first step that found any, sorted by primary start
    /// and end and then registry order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The validated document and the diagnostics.
    #[must_use]
    pub fn into_parts(self) -> (Option<TypedDocument>, Vec<Diagnostic>) {
        (self.document, self.diagnostics)
    }
}

/// Reads `bytes` as a document of `schema`, in the six-step order of
/// "Schemas".
#[must_use]
pub fn decode(schema: &Schema, source_id: Option<&str>, bytes: &[u8]) -> Decoded {
    let document = parse_data_for_schema(source_id, bytes);
    let finish = |diagnostics: Vec<Diagnostic>, typed: Option<TypedNode>| {
        let mut diagnostics = diagnostics
            .into_iter()
            .map(|diagnostic| match source_id {
                Some(source_id) => diagnostic.with_source_id(source_id),
                None => diagnostic,
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
                diagnostic.code(),
            )
        });
        Decoded {
            document: typed.filter(|_| diagnostics.is_empty()).map(|root| {
                TypedDocument {
                    format: schema.format,
                    root,
                }
            }),
            diagnostics,
        }
    };
    // Steps 1 and 2: the grammar, then repeated fields.
    let (fields, grammar): (Vec<_>, Vec<_>) = document
        .diagnostics()
        .iter()
        .cloned()
        .partition(|diagnostic| {
            diagnostic.code() == DiagnosticCode::DataDuplicateField
        });
    if !grammar.is_empty() {
        return finish(grammar, None);
    }
    if !fields.is_empty() {
        return finish(fields, None);
    }
    let Some(root) = document.value() else {
        return finish(Vec::new(), None);
    };
    // Step 3: format selection.
    if let Some(fault) = select_format(schema, root) {
        return finish(vec![fault], None);
    }
    // Steps 4 to 6, gathered in one walk and reported by the first step that
    // found a fault.
    let mut walk = Walk::default();
    let typed = walk.record(&schema.root, root);
    let faults = [walk.shape, walk.value, walk.keys]
        .into_iter()
        .find(|faults| !faults.is_empty())
        .unwrap_or_default();
    finish(faults, typed)
}

/// Step 3: the root must be a record whose `format` field holds the schema's
/// format atom.
fn select_format(schema: &Schema, root: &DataNode) -> Option<Diagnostic> {
    let DataValue::Record(fields) = root.value() else {
        return Some(Diagnostic::new(
            DiagnosticCode::DataInvalidShape,
            root.span(),
            "a compiler-owned document is a record",
        ));
    };
    let Some(format) = fields.iter().find(|field| field.name() == "format") else {
        return Some(Diagnostic::new(
            DiagnosticCode::DataInvalidShape,
            root.span(),
            "the required field `format` is missing",
        ));
    };
    match format.value().value() {
        DataValue::Atom(atom) if atom == schema.format => None,
        _ => Some(Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            format.value().span(),
            format!(
                "this reader implements the format `{}` and no other",
                schema.format
            ),
        )),
    }
}

/// The faults of steps 4, 5, and 6, gathered in one walk.
#[derive(Debug, Default)]
struct Walk {
    shape: Vec<Diagnostic>,
    value: Vec<Diagnostic>,
    keys: Vec<Diagnostic>,
}

impl Walk {
    fn shape_fault(&mut self, span: ByteSpan, message: impl Into<String>) {
        self.shape.push(Diagnostic::new(
            DiagnosticCode::DataInvalidShape,
            span,
            message,
        ));
    }

    fn value_fault(&mut self, span: ByteSpan, message: impl Into<String>) {
        self.value.push(Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            span,
            message,
        ));
    }

    /// A value of the wrong form. An unsuffixed number in a slot with no
    /// number type is a value fault, as "Schemas" states; anything else is a
    /// shape fault.
    fn wrong_form(&mut self, node: &DataNode, expected: &str) -> Option<TypedValue> {
        match node.value() {
            DataValue::Integer(literal) if literal.suffix().is_none() => self
                .value_fault(
                    node.span(),
                    format!("an unsuffixed number in a slot of type {expected}"),
                ),
            DataValue::Float(literal) if literal.suffix().is_none() => self
                .value_fault(
                    node.span(),
                    format!("an unsuffixed number in a slot of type {expected}"),
                ),
            _ => self.shape_fault(node.span(), format!("the slot holds {expected}")),
        }
        None
    }

    fn typed(&mut self, slot: &Slot, node: &DataNode) -> Option<TypedNode> {
        let value = self.slot(slot, node)?;
        Some(TypedNode {
            value,
            span: node.span(),
        })
    }

    fn slot(&mut self, slot: &Slot, node: &DataNode) -> Option<TypedValue> {
        match (slot, node.value()) {
            (Slot::Bool, DataValue::Bool(value)) => Some(TypedValue::Bool(*value)),
            (Slot::Bool, _) => self.wrong_form(node, "a boolean"),
            (Slot::Char, DataValue::Character(value)) => Some(TypedValue::Char(*value)),
            (Slot::Char, _) => self.wrong_form(node, "a character"),
            (Slot::Void, DataValue::Void) => Some(TypedValue::Void),
            (Slot::Void, _) => self.wrong_form(node, "`void`"),
            (Slot::Str(rule), DataValue::String(text)) => {
                self.spelling(*rule, text, node.span())?;
                Some(TypedValue::Str(text.clone()))
            }
            (Slot::Str(_), _) => self.wrong_form(node, "a string"),
            (Slot::Atom(rule), DataValue::Atom(text)) => {
                self.spelling(*rule, text, node.span())?;
                Some(TypedValue::Atom(text.clone()))
            }
            (Slot::AtomOf(set), DataValue::Atom(text)) => {
                if set.contains(&text.as_str()) {
                    Some(TypedValue::Atom(text.clone()))
                } else {
                    self.value_fault(
                        node.span(),
                        format!("the atom is not one of {}", set.join(" ")),
                    );
                    None
                }
            }
            (Slot::Atom(_) | Slot::AtomOf(_), _) => self.wrong_form(node, "an atom"),
            (Slot::Integer(suffix), DataValue::Integer(literal)) => {
                self.integer(*suffix, literal, node.span())
            }
            (Slot::Integer(suffix), DataValue::Float(_)) => {
                self.value_fault(
                    node.span(),
                    format!("a float in a slot of type {suffix}"),
                );
                None
            }
            (Slot::Integer(suffix), _) => {
                self.shape_fault(
                    node.span(),
                    format!("the slot holds a {suffix} integer"),
                );
                None
            }
            (Slot::Float(suffix), DataValue::Float(literal)) => {
                self.float(*suffix, literal.suffix(), literal.body(), node.span())
            }
            (Slot::Float(suffix), DataValue::Integer(literal)) => {
                if literal.suffix().is_some() {
                    self.value_fault(
                        node.span(),
                        format!("an integer in a slot of type {suffix}"),
                    );
                    return None;
                }
                self.float(*suffix, None, literal.body(), node.span())
            }
            (Slot::Float(suffix), _) => {
                self.shape_fault(
                    node.span(),
                    format!("the slot holds a {suffix} float"),
                );
                None
            }
            (Slot::List { element, min }, DataValue::List(elements)) => {
                let mut typed = Some(Vec::with_capacity(elements.len()));
                for element_node in elements {
                    let element_value = self.typed(element, element_node);
                    typed = typed.zip(element_value).map(|(mut list, value)| {
                        list.push(value);
                        list
                    });
                }
                if elements.len() < *min {
                    self.value_fault(
                        node.span(),
                        format!("the list holds at least {min} element(s)"),
                    );
                    return None;
                }
                typed.map(TypedValue::List)
            }
            (Slot::List { .. }, _) => self.wrong_form(node, "a list"),
            (Slot::Dict { key, value }, DataValue::Dict(entries)) => {
                self.dict(key, value, entries)
            }
            (Slot::Dict { .. }, _) => self.wrong_form(node, "a dict"),
            (Slot::Record(schema), DataValue::Record(_)) => {
                self.record(schema, node).map(|typed| typed.value)
            }
            (Slot::Record(_), _) => self.wrong_form(node, "a record"),
            (Slot::Variant(variant), DataValue::Record(fields)) => {
                self.variant(variant, node, fields)
            }
            (Slot::Variant(_), _) => self.wrong_form(node, "a record"),
            (Slot::Opaque, _) => write_opaque(node, &mut self.value, &mut self.keys)
                .map(TypedValue::Opaque),
        }
    }

    fn spelling(
        &mut self,
        rule: Option<SpellingRule>,
        text: &str,
        span: ByteSpan,
    ) -> Option<()> {
        match rule.and_then(|rule| rule(text)) {
            Some(reason) => {
                self.value_fault(span, reason);
                None
            }
            None => Some(()),
        }
    }

    fn integer(
        &mut self,
        suffix: IntegerSuffix,
        literal: &IntegerLiteral,
        span: ByteSpan,
    ) -> Option<TypedValue> {
        if literal.suffix().is_some_and(|written| written != suffix) {
            self.value_fault(span, format!("the slot holds a {suffix} integer"));
            return None;
        }
        let value = literal
            .body()
            .parse::<i128>()
            .ok()
            .filter(|value| in_range(*value, suffix));
        if value.is_none() {
            self.value_fault(span, format!("the value is out of range for {suffix}"));
        }
        value.map(|value| TypedValue::Integer(value, suffix))
    }

    fn float(
        &mut self,
        suffix: FloatSuffix,
        written: Option<FloatSuffix>,
        body: &str,
        span: ByteSpan,
    ) -> Option<TypedValue> {
        if written.is_some_and(|written| written != suffix) {
            self.value_fault(span, format!("the slot holds a {suffix} float"));
            return None;
        }
        let value = match suffix {
            FloatSuffix::F32 => body.parse::<f32>().ok().map(f64::from),
            FloatSuffix::F64 => body.parse::<f64>().ok(),
        }
        .filter(|value| value.is_finite());
        if value.is_none() {
            self.value_fault(span, format!("the value is out of range for {suffix}"));
        }
        value.map(|value| TypedValue::Float(value, suffix))
    }

    fn dict(
        &mut self,
        key_slot: &Slot,
        value_slot: &Slot,
        entries: &[DataEntry],
    ) -> Option<TypedValue> {
        let mut typed = Some(Vec::with_capacity(entries.len()));
        let mut accepted: Vec<TypedNode> = Vec::new();
        for entry in entries {
            let key = self.typed(key_slot, entry.key());
            let value = self.typed(value_slot, entry.value());
            if let Some(key) = &key {
                // Step 6: keys are compared as typed values, after steps 3 to
                // 5 accepted them.
                if accepted.iter().any(|earlier| {
                    compare(&earlier.value, &key.value) == Ordering::Equal
                }) {
                    self.keys.push(Diagnostic::new(
                        DiagnosticCode::DataDuplicateKey,
                        key.span,
                        "a dict's keys are unique as decoded values",
                    ));
                } else {
                    accepted.push(key.clone());
                }
            }
            typed = typed.zip(key.zip(value)).map(|(mut list, (key, value))| {
                list.push(TypedEntry { key, value });
                list
            });
        }
        let mut entries = typed?;
        entries.sort_by(|left, right| compare(&left.key.value, &right.key.value));
        Some(TypedValue::Dict(entries))
    }

    fn variant(
        &mut self,
        variant: &VariantSchema,
        node: &DataNode,
        fields: &[DataField],
    ) -> Option<TypedValue> {
        let Some(tag) = fields.iter().find(|field| field.name() == variant.tag) else {
            self.shape_fault(
                node.span(),
                format!("the required field `{}` is missing", variant.tag),
            );
            return None;
        };
        let DataValue::Atom(atom) = tag.value().value() else {
            return self.wrong_form(tag.value(), "an atom");
        };
        let Some((_, schema)) = variant.cases.iter().find(|(case, _)| case == atom)
        else {
            let cases = variant
                .cases
                .iter()
                .map(|(case, _)| *case)
                .collect::<Vec<_>>();
            self.value_fault(
                tag.value().span(),
                format!("the atom is not one of {}", cases.join(" ")),
            );
            return None;
        };
        self.record(schema, node).map(|typed| typed.value)
    }

    fn record(&mut self, schema: &RecordSchema, node: &DataNode) -> Option<TypedNode> {
        let DataValue::Record(written) = node.value() else {
            return self.wrong_form(node, "a record").map(|value| TypedNode {
                value,
                span: node.span(),
            });
        };
        let mut typed = Some(Vec::with_capacity(schema.fields.len()));
        for field in &schema.fields {
            let present = written
                .iter()
                .find(|candidate| candidate.name() == field.name);
            let decoded = match (present, field.presence) {
                (Some(present), presence) => {
                    self.typed(&field.slot, present.value()).map(|value| {
                        let is_default = match presence {
                            Presence::Default(default) => {
                                same_canonical(&value.value, &default())
                            }
                            Presence::Required | Presence::Optional => false,
                        };
                        Some(TypedField {
                            name: field.name,
                            name_span: Some(present.name_span()),
                            value,
                            is_default,
                        })
                    })
                }
                (None, Presence::Required) => {
                    self.shape_fault(
                        node.span(),
                        format!("the required field `{}` is missing", field.name),
                    );
                    None
                }
                (None, Presence::Optional) => Some(None),
                (None, Presence::Default(default)) => Some(Some(TypedField {
                    name: field.name,
                    name_span: None,
                    value: TypedNode {
                        value: default(),
                        span: node.span(),
                    },
                    is_default: true,
                })),
            };
            typed = typed.zip(decoded).map(|(mut fields, decoded)| {
                fields.extend(decoded);
                fields
            });
        }
        for field in written {
            if !schema.fields.iter().any(|known| known.name == field.name()) {
                self.shape_fault(
                    field.name_span(),
                    format!("the record has no field `{}`", field.name()),
                );
                typed = None;
            }
        }
        if written.len() < schema.min_written {
            self.value_fault(
                node.span(),
                format!("the record writes at least {} field(s)", schema.min_written),
            );
            typed = None;
        }
        typed.map(|fields| TypedNode {
            value: TypedValue::Record(fields),
            span: node.span(),
        })
    }
}

fn in_range(value: i128, suffix: IntegerSuffix) -> bool {
    let (low, high) = match suffix {
        IntegerSuffix::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        IntegerSuffix::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        IntegerSuffix::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        IntegerSuffix::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        IntegerSuffix::U8 => (0, i128::from(u8::MAX)),
        IntegerSuffix::U16 => (0, i128::from(u16::MAX)),
        IntegerSuffix::U32 => (0, i128::from(u32::MAX)),
        IntegerSuffix::U64 => (0, i128::from(u64::MAX)),
    };
    (low..=high).contains(&value)
}

fn same_canonical(left: &TypedValue, right: &TypedValue) -> bool {
    let mut left_bytes = String::new();
    let mut right_bytes = String::new();
    write_value(left, &mut left_bytes);
    write_value(right, &mut right_bytes);
    left_bytes == right_bytes
}

/// The order of two decoded values of one slot: the canonical key order of
/// the type chapter ("Dict keys, equality, and order"). `false` before
/// `true`; numeric order; scalar order for characters and strings; the UTF-8
/// bytes of the spelling for atoms; element-wise for lists and field-wise in
/// canonical order for records; byte order for opaque data.
#[must_use]
pub fn compare(left: &TypedValue, right: &TypedValue) -> Ordering {
    match (left, right) {
        (TypedValue::Bool(left), TypedValue::Bool(right)) => left.cmp(right),
        (TypedValue::Char(left), TypedValue::Char(right)) => left.cmp(right),
        (TypedValue::Str(left), TypedValue::Str(right))
        | (TypedValue::Atom(left), TypedValue::Atom(right))
        | (TypedValue::Opaque(left), TypedValue::Opaque(right)) => {
            left.as_bytes().cmp(right.as_bytes())
        }
        (TypedValue::Integer(left, _), TypedValue::Integer(right, _)) => {
            left.cmp(right)
        }
        (TypedValue::Float(left, _), TypedValue::Float(right, _)) => {
            left.total_cmp(right)
        }
        (TypedValue::Void, TypedValue::Void) => Ordering::Equal,
        (TypedValue::List(left), TypedValue::List(right)) => {
            for (left, right) in left.iter().zip(right) {
                let order = compare(&left.value, &right.value);
                if order != Ordering::Equal {
                    return order;
                }
            }
            left.len().cmp(&right.len())
        }
        (TypedValue::Dict(left), TypedValue::Dict(right)) => {
            for (left, right) in left.iter().zip(right) {
                let order = compare(&left.key.value, &right.key.value)
                    .then_with(|| compare(&left.value.value, &right.value.value));
                if order != Ordering::Equal {
                    return order;
                }
            }
            left.len().cmp(&right.len())
        }
        (TypedValue::Record(left), TypedValue::Record(right)) => {
            for (left, right) in left.iter().zip(right) {
                let order = left
                    .name
                    .cmp(right.name)
                    .then_with(|| compare(&left.value.value, &right.value.value));
                if order != Ordering::Equal {
                    return order;
                }
            }
            left.len().cmp(&right.len())
        }
        _ => rank(left).cmp(&rank(right)),
    }
}

const fn rank(value: &TypedValue) -> u8 {
    match value {
        TypedValue::Bool(_) => 0,
        TypedValue::Char(_) => 1,
        TypedValue::Str(_) => 2,
        TypedValue::Integer(..) => 3,
        TypedValue::Float(..) => 4,
        TypedValue::Void => 5,
        TypedValue::Atom(_) => 6,
        TypedValue::List(_) => 7,
        TypedValue::Dict(_) => 8,
        TypedValue::Record(_) => 9,
        TypedValue::Opaque(_) => 10,
    }
}

fn write_typed(node: &TypedNode, out: &mut String) {
    write_value(&node.value, out);
}

fn write_value(value: &TypedValue, out: &mut String) {
    match value {
        TypedValue::Bool(true) => out.push_str("true"),
        TypedValue::Bool(false) => out.push_str("false"),
        TypedValue::Char(value) => out.push_str(&canonical_character(*value)),
        TypedValue::Str(value) => out.push_str(&canonical_string(value)),
        TypedValue::Integer(value, suffix) => {
            let _ = write!(out, "{value}{suffix}");
        }
        TypedValue::Float(value, suffix) => {
            let text = match suffix {
                // The value was parsed as an `F32` and widened exactly, so
                // narrowing it back is exact.
                FloatSuffix::F32 => float_text(&format!("{:e}", *value as f32)),
                FloatSuffix::F64 => float_text(&format!("{value:e}")),
            };
            let _ = write!(out, "{text}{suffix}");
        }
        TypedValue::Void => out.push_str("void"),
        TypedValue::Atom(value) | TypedValue::Opaque(value) => out.push_str(value),
        TypedValue::List(elements) => {
            out.push('[');
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                write_typed(element, out);
            }
            out.push(']');
        }
        TypedValue::Dict(entries) if entries.is_empty() => out.push_str("[:]"),
        TypedValue::Dict(entries) => {
            out.push('[');
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                write_typed(&entry.key, out);
                out.push_str(": ");
                write_typed(&entry.value, out);
            }
            out.push(']');
        }
        TypedValue::Record(fields) => {
            out.push('{');
            let mut first = true;
            for field in fields.iter().filter(|field| !field.is_default) {
                if !first {
                    out.push(' ');
                }
                first = false;
                out.push_str(field.name);
                out.push_str(": ");
                write_typed(&field.value, out);
            }
            out.push('}');
        }
    }
}

/// The canonical text of a finite float from its shortest round-trip
/// scientific form (`1.5e-7`, `-0e0`): the runtime chapter's "Canonical
/// text". Zero and a magnitude in [1e-4, 1e16) use decimal notation with at
/// least one fractional digit; any other magnitude uses scientific notation
/// with one integral digit and no plus sign.
fn float_text(scientific: &str) -> String {
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific, "0"));
    let exponent = exponent.parse::<i64>().unwrap_or_default();
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits = mantissa.replace('.', "");
    decimal_text(sign, &digits, exponent)
}

/// The canonical text of `sign` `d.ddd` × 10^`exponent`, `digits` holding
/// at least one digit and no leading zero unless it is the single digit
/// `0`.
fn decimal_text(sign: &str, digits: &str, exponent: i64) -> String {
    let zero = digits.bytes().all(|digit| digit == b'0');
    if zero || (-4..=15).contains(&exponent) {
        if zero {
            return format!("{sign}0.0");
        }
        let point = exponent + 1;
        let length = i64::try_from(digits.len()).unwrap_or(i64::MAX);
        if point <= 0 {
            let zeros = usize::try_from(-point).unwrap_or_default();
            return format!("{sign}0.{}{digits}", "0".repeat(zeros));
        }
        if point >= length {
            let zeros = usize::try_from(point - length).unwrap_or_default();
            return format!("{sign}{digits}{}.0", "0".repeat(zeros));
        }
        let split = usize::try_from(point).unwrap_or_default();
        let (whole, fraction) = digits.split_at(split.min(digits.len()));
        return format!("{sign}{whole}.{fraction}");
    }
    scientific_text(sign, digits, &exponent.to_string())
}

/// `sign` `d.ddd` `e` `exponent`, with one integral digit and no plus sign.
fn scientific_text(sign: &str, digits: &str, exponent: &str) -> String {
    let mut characters = digits.chars();
    let first = characters.next().unwrap_or('0');
    let rest = characters.as_str();
    if rest.is_empty() {
        format!("{sign}{first}e{exponent}")
    } else {
        format!("{sign}{first}.{rest}e{exponent}")
    }
}

/// The canonical text of an unsuffixed float in opaque data: its exact
/// decimal value, in the notation of [`float_text`], its exponent exact at
/// every length.
fn unsuffixed_float_text(literal: &FloatLiteral) -> String {
    let decimal = decimal_value(literal.body());
    let (sign, unsigned) = match decimal.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", decimal.as_str()),
    };
    let Some((digits, power)) = unsigned.split_once('e') else {
        return format!("{sign}0.0");
    };
    let exponent = add_to_decimal(power, false, digits.len().saturating_sub(1));
    match exponent.parse::<i64>() {
        Ok(exponent) => decimal_text(sign, digits, exponent),
        // Beyond 64 bits the exponent is far outside the decimal range.
        Err(_) => scientific_text(sign, digits, &exponent),
    }
}

/// The canonical bytes of a number in opaque data: its minimal digits and
/// the suffix it was written with, or `None` when it lies outside the range
/// of its written type.
fn opaque_number(value: &DataValue) -> Option<String> {
    match value {
        DataValue::Integer(literal) => {
            let digits = integer_value(literal.body());
            match literal.suffix() {
                Some(suffix) => digits
                    .parse::<i128>()
                    .ok()
                    .filter(|value| in_range(*value, suffix))
                    .map(|_| format!("{digits}{suffix}")),
                None => Some(digits),
            }
        }
        DataValue::Float(literal) => match literal.suffix() {
            Some(FloatSuffix::F32) => literal
                .body()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| format!("{}f32", float_text(&format!("{value:e}")))),
            Some(FloatSuffix::F64) => literal
                .body()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| format!("{}f64", float_text(&format!("{value:e}")))),
            None => Some(unsuffixed_float_text(literal)),
        },
        _ => None,
    }
}

/// Writes opaque data as its canonical bytes, without recursion. A number
/// outside the range of its written type is reported at the number into
/// `values` (step 5), and the result is `None`; a repeated dict key (equal
/// canonical key bytes) is reported at the later key into `keys` (step 6).
///
/// A dict's keys are encoded first, each into its own buffer, as work items
/// of their own; then the entries are sorted by those bytes and emitted.
fn write_opaque(
    root: &DataNode,
    values: &mut Vec<Diagnostic>,
    keys: &mut Vec<Diagnostic>,
) -> Option<String> {
    enum Work<'a> {
        Node(&'a DataNode),
        Text(String),
        /// Starts the canonical bytes of a dict key in a buffer of its own.
        BeginKey,
        /// Ends a key's bytes and gives them to the innermost pending dict.
        EndKey,
        /// Sorts the innermost pending dict by its key bytes and emits it.
        EmitDict,
    }
    /// A dict whose keys are being encoded.
    struct Pending<'a> {
        entries: &'a [DataEntry],
        keys: Vec<String>,
    }
    let mut buffers = vec![String::new()];
    let mut pending: Vec<Pending<'_>> = Vec::new();
    let mut in_range = true;
    let mut work = vec![Work::Node(root)];
    while let Some(step) = work.pop() {
        let mut parts: Vec<Work<'_>> = Vec::new();
        let mut text = String::new();
        match step {
            Work::Text(written) => text = written,
            Work::BeginKey => buffers.push(String::new()),
            Work::EndKey => {
                if buffers.len() > 1
                    && let Some(key) = buffers.pop()
                    && let Some(dict) = pending.last_mut()
                {
                    dict.keys.push(key);
                }
            }
            Work::EmitDict => {
                let Some(dict) = pending.pop() else {
                    continue;
                };
                let mut seen = BTreeSet::new();
                let mut keyed = Vec::with_capacity(dict.entries.len());
                for (entry, key) in dict.entries.iter().zip(dict.keys) {
                    if seen.insert(key.clone()) {
                        keyed.push((key, entry.value()));
                    } else {
                        keys.push(Diagnostic::new(
                            DiagnosticCode::DataDuplicateKey,
                            entry.key().span(),
                            "a dict's keys are unique as decoded values",
                        ));
                    }
                }
                keyed.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                parts.push(Work::Text("[".to_owned()));
                for (index, (key, value)) in keyed.into_iter().enumerate() {
                    let separator = if index > 0 { " " } else { "" };
                    parts.push(Work::Text(format!("{separator}{key}: ")));
                    parts.push(Work::Node(value));
                }
                parts.push(Work::Text("]".to_owned()));
            }
            Work::Node(node) => match node.value() {
                DataValue::String(value) => text = canonical_string(value),
                DataValue::Character(value) => text = canonical_character(*value),
                DataValue::Bool(true) => text = "true".to_owned(),
                DataValue::Bool(false) => text = "false".to_owned(),
                DataValue::Void => text = "void".to_owned(),
                DataValue::Atom(value) => text.clone_from(value),
                number @ (DataValue::Integer(_) | DataValue::Float(_)) => {
                    match opaque_number(number) {
                        Some(canonical) => text = canonical,
                        None => {
                            in_range = false;
                            values.push(Diagnostic::new(
                                DiagnosticCode::DataInvalidValue,
                                node.span(),
                                "the value is out of range for its written type",
                            ));
                        }
                    }
                }
                DataValue::Tuple(elements) | DataValue::List(elements) => {
                    let (open, close) = if matches!(node.value(), DataValue::Tuple(_)) {
                        ("{", "}")
                    } else {
                        ("[", "]")
                    };
                    parts.push(Work::Text(open.to_owned()));
                    for (index, element) in elements.iter().enumerate() {
                        if index > 0 {
                            parts.push(Work::Text(" ".to_owned()));
                        }
                        parts.push(Work::Node(element));
                    }
                    parts.push(Work::Text(close.to_owned()));
                }
                DataValue::Record(fields) => {
                    let mut sorted = fields.iter().collect::<Vec<_>>();
                    sorted.sort_by(|left, right| {
                        left.name().as_bytes().cmp(right.name().as_bytes())
                    });
                    parts.push(Work::Text("{".to_owned()));
                    for (index, field) in sorted.into_iter().enumerate() {
                        let separator = if index > 0 { " " } else { "" };
                        parts
                            .push(Work::Text(format!("{separator}{}: ", field.name())));
                        parts.push(Work::Node(field.value()));
                    }
                    parts.push(Work::Text("}".to_owned()));
                }
                DataValue::Dict(entries) if entries.is_empty() => {
                    text = "[:]".to_owned()
                }
                DataValue::Dict(entries) => {
                    pending.push(Pending {
                        entries,
                        keys: Vec::with_capacity(entries.len()),
                    });
                    for entry in entries {
                        parts.extend([
                            Work::BeginKey,
                            Work::Node(entry.key()),
                            Work::EndKey,
                        ]);
                    }
                    parts.push(Work::EmitDict);
                }
            },
        }
        if let Some(buffer) = buffers.last_mut() {
            buffer.push_str(&text);
        }
        work.extend(parts.into_iter().rev());
    }
    in_range.then(|| buffers.into_iter().next().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{decimal_text, float_text};

    #[test]
    fn floats_follow_the_runtime_chapters_canonical_text() {
        assert_eq!(float_text(&format!("{:e}", 100.0_f64)), "100.0");
        assert_eq!(float_text(&format!("{:e}", -0.0_f64)), "-0.0");
        assert_eq!(float_text(&format!("{:e}", 0.0025_f64)), "0.0025");
        assert_eq!(float_text(&format!("{:e}", 1e20_f64)), "1e20");
        assert_eq!(float_text(&format!("{:e}", 1.5e-7_f64)), "1.5e-7");
        assert_eq!(float_text(&format!("{:e}", 1e16_f64)), "1e16");
        assert_eq!(float_text(&format!("{:e}", 1e-4_f64)), "0.0001");
        assert_eq!(float_text(&format!("{:e}", 0.1_f32)), "0.1");
        assert_eq!(decimal_text("", "15", 0), "1.5");
    }
}
