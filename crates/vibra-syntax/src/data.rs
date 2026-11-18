//! The VIBON data grammar.
//!
//! `docs/spec/04-programs-and-packages.md`, "VIBON data documents", defines
//! the grammar:
//!
//! ```text
//! data   = string | character | boolean | integer | float | void | atom
//!        | tuple | record | list | dict ;
//! tuple  = "{", { data }, "}" ;
//! record = "{", field, { field }, "}" ;
//! field  = identifier, ":", data ;
//! list   = "[", { data }, "]" ;
//! dict   = "[", entry, { entry }, "]" | "[", ":", "]" ;
//! entry  = data, ":", data ;
//! ```
//!
//! This module shapes each bracket form into its production as the form
//! closes, so validation is as iterative as bracket matching. It reports the
//! grammar-only steps of the validation order of "Schemas": a form outside
//! the data grammar (`@syntax.invalidForm`), a repeated record field
//! (`@data.duplicateField`, compared as written), and a repeated dict key
//! (`@data.duplicateKey`, compared as decoded values with the written suffix
//! as part of a number's value, because a grammar-only reader has no schema).
//!
//! A form the grammar does not admit keeps its tokens inside an `@error`
//! node. A valid value in a place that admits no value, such as a second
//! document value, keeps its node and is reported.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::cst::{CstNode, GroupKind, Shaper, SyntaxKind, error_around};
use crate::lexer::{Token, TokenKind, is_atom};
use crate::literal::{
    FloatSuffix, IntegerSuffix, decimal_value, decode_character, decode_string,
    integer_value, scan_number, split_number,
};

/// An integer literal, with its spelling kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerLiteral {
    text: String,
    suffix: Option<IntegerSuffix>,
}

impl IntegerLiteral {
    /// The literal as written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The written suffix, if any.
    #[must_use]
    pub const fn suffix(&self) -> Option<IntegerSuffix> {
        self.suffix
    }

    /// The literal without its suffix.
    #[must_use]
    pub fn body(&self) -> &str {
        split_number(&self.text).0
    }
}

/// A float literal, with its spelling kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloatLiteral {
    text: String,
    suffix: Option<FloatSuffix>,
}

impl FloatLiteral {
    /// The literal as written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The written suffix, if any.
    #[must_use]
    pub const fn suffix(&self) -> Option<FloatSuffix> {
        self.suffix
    }

    /// The literal without its suffix.
    #[must_use]
    pub fn body(&self) -> &str {
        split_number(&self.text).0
    }
}

/// A generic VIBON value, before any schema types it.
#[derive(Debug)]
pub enum DataValue {
    /// A string, decoded to its scalars.
    String(String),
    /// A character, decoded to its scalar.
    Character(char),
    /// `true` or `false`.
    Bool(bool),
    /// An integer literal.
    Integer(IntegerLiteral),
    /// A float literal.
    Float(FloatLiteral),
    /// `void`.
    Void,
    /// An atom, as written, `@` included.
    Atom(String),
    /// A tuple, `{}` included.
    Tuple(Vec<DataNode>),
    /// A record, its fields in written order.
    Record(Vec<DataField>),
    /// A list.
    List(Vec<DataNode>),
    /// A dict, its entries in written order.
    Dict(Vec<DataEntry>),
}

/// One field of a record.
#[derive(Debug)]
pub struct DataField {
    name: String,
    name_span: ByteSpan,
    value: DataNode,
}

impl DataField {
    /// The field name as written.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the field name is.
    #[must_use]
    pub const fn name_span(&self) -> ByteSpan {
        self.name_span
    }

    /// The field's value.
    #[must_use]
    pub const fn value(&self) -> &DataNode {
        &self.value
    }
}

/// One entry of a dict.
#[derive(Debug)]
pub struct DataEntry {
    key: DataNode,
    value: DataNode,
}

impl DataEntry {
    /// The key.
    #[must_use]
    pub const fn key(&self) -> &DataNode {
        &self.key
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> &DataNode {
        &self.value
    }
}

/// A VIBON value and the bytes it was read from.
#[derive(Debug)]
pub struct DataNode {
    value: DataValue,
    span: ByteSpan,
}

impl DataNode {
    /// A value read from `span`.
    #[must_use]
    pub const fn new(value: DataValue, span: ByteSpan) -> Self {
        Self { value, span }
    }

    /// The value.
    #[must_use]
    pub const fn value(&self) -> &DataValue {
        &self.value
    }

    /// The bytes the value was read from.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Whether two values are equal as decoded values: a string or character
    /// as its scalars, a number as its written suffix and its value, an atom
    /// as its parts, a record as its set of named fields, a dict as its set
    /// of entries, and a tuple or list element by element.
    #[must_use]
    pub fn decoded_eq(&self, other: &Self) -> bool {
        let mut interner = Interner::default();
        interner.tree(self) == interner.tree(other)
    }

    const fn is_compound(&self) -> bool {
        matches!(
            self.value,
            DataValue::Tuple(_)
                | DataValue::Record(_)
                | DataValue::List(_)
                | DataValue::Dict(_)
        )
    }

    /// The child values in order: a record's field values, and a dict's key
    /// and value of each entry.
    fn children(&self) -> Vec<&Self> {
        match &self.value {
            DataValue::Tuple(elements) | DataValue::List(elements) => {
                elements.iter().collect()
            }
            DataValue::Record(fields) => {
                fields.iter().map(|field| &field.value).collect()
            }
            DataValue::Dict(entries) => entries
                .iter()
                .flat_map(|entry| [&entry.key, &entry.value])
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl Drop for DataNode {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        take_children(&mut self.value, &mut stack);
        while let Some(mut node) = stack.pop() {
            take_children(&mut node.value, &mut stack);
        }
    }
}

fn take_children(value: &mut DataValue, stack: &mut Vec<DataNode>) {
    match value {
        DataValue::Tuple(elements) | DataValue::List(elements) => {
            stack.append(elements)
        }
        DataValue::Record(fields) => {
            stack.extend(fields.drain(..).map(|field| field.value))
        }
        DataValue::Dict(entries) => {
            for entry in entries.drain(..) {
                stack.push(entry.key);
                stack.push(entry.value);
            }
        }
        _ => {}
    }
}

/// A self-delimiting text of a length and a string.
fn counted(tag: &str, text: &str) -> String {
    format!("{tag}{}:{text}", text.len())
}

fn scalar_key(value: &DataValue) -> String {
    match value {
        DataValue::String(text) => counted("s", text),
        DataValue::Character(scalar) => format!("c{};", u32::from(*scalar)),
        DataValue::Bool(true) => "b1".to_owned(),
        DataValue::Bool(false) => "b0".to_owned(),
        DataValue::Void => "v".to_owned(),
        DataValue::Atom(text) => counted("a", text),
        DataValue::Integer(literal) => {
            let suffix = literal.suffix.map_or("", IntegerSuffix::as_str);
            counted(&format!("i{suffix}/"), &integer_value(literal.body()))
        }
        DataValue::Float(literal) => {
            let body = literal.body();
            let value = match literal.suffix {
                Some(FloatSuffix::F32) => body.parse::<f32>().map_or_else(
                    |_| body.to_owned(),
                    |value| value.to_bits().to_string(),
                ),
                Some(FloatSuffix::F64) => body.parse::<f64>().map_or_else(
                    |_| body.to_owned(),
                    |value| value.to_bits().to_string(),
                ),
                None => decimal_value(body),
            };
            let suffix = literal.suffix.map_or("", FloatSuffix::as_str);
            counted(&format!("f{suffix}/"), &value)
        }
        DataValue::Tuple(_)
        | DataValue::Record(_)
        | DataValue::List(_)
        | DataValue::Dict(_) => String::new(),
    }
}

fn list_signature(tag: char, ids: &[usize]) -> String {
    let mut signature = format!("{tag}{}(", ids.len());
    for id in ids {
        signature.push_str(&format!("{id},"));
    }
    signature.push(')');
    signature
}

fn record_signature<'a>(fields: impl Iterator<Item = (&'a str, usize)>) -> String {
    let mut named = fields.collect::<Vec<_>>();
    named.sort_unstable();
    let mut signature = format!("r{}(", named.len());
    for (name, id) in named {
        signature.push_str(&counted("", name));
        signature.push_str(&format!("={id},"));
    }
    signature.push(')');
    signature
}

fn dict_signature(entries: impl Iterator<Item = (usize, usize)>) -> String {
    let mut pairs = entries.collect::<Vec<_>>();
    pairs.sort_unstable();
    let mut signature = format!("d{}(", pairs.len());
    for (key, value) in pairs {
        signature.push_str(&format!("{key}={value},"));
    }
    signature.push(')');
    signature
}

/// Numbers decoded values so that two values get one number exactly when
/// they are equal as decoded values.
///
/// A value's signature names its scalar, or its kind and the numbers of its
/// children, so interning is linear in the size of a tree and exact: two
/// signatures are compared in full, never by a hash alone.
#[derive(Debug, Default)]
pub(crate) struct Interner {
    numbers: BTreeMap<String, usize>,
}

impl Interner {
    fn number(&mut self, signature: String) -> usize {
        let next = self.numbers.len();
        *self.numbers.entry(signature).or_insert(next)
    }

    /// The number of a whole tree, walked iteratively.
    pub(crate) fn tree(&mut self, root: &DataNode) -> usize {
        enum Work<'a> {
            Visit(&'a DataNode),
            Combine(&'a DataNode),
        }
        let mut work = vec![Work::Visit(root)];
        let mut numbers: Vec<usize> = Vec::new();
        while let Some(step) = work.pop() {
            match step {
                Work::Visit(node) if !node.is_compound() => {
                    numbers.push(self.number(scalar_key(&node.value)));
                }
                Work::Visit(node) => {
                    work.push(Work::Combine(node));
                    work.extend(node.children().into_iter().rev().map(Work::Visit));
                }
                Work::Combine(node) => {
                    let count = node.children().len();
                    let ids = numbers.split_off(numbers.len().saturating_sub(count));
                    let signature =
                        match &node.value {
                            DataValue::Tuple(_) => list_signature('t', &ids),
                            DataValue::List(_) => list_signature('l', &ids),
                            DataValue::Record(fields) => record_signature(
                                fields.iter().map(|field| field.name.as_str()).zip(ids),
                            ),
                            _ => dict_signature(ids.chunks(2).filter_map(|pair| {
                                Some((*pair.first()?, *pair.get(1)?))
                            })),
                        };
                    numbers.push(self.number(signature));
                }
            }
        }
        numbers.pop().unwrap_or_default()
    }
}

/// A decoded value and its number in the shaper's [`Interner`].
#[derive(Debug)]
struct Valued {
    node: DataNode,
    number: usize,
}

/// One collected item of a form being shaped.
#[derive(Debug)]
pub(crate) struct Item {
    node: CstNode,
    token: Option<TokenKind>,
    group: Option<GroupKind>,
    value: Option<Valued>,
}

impl Item {
    fn is_trivia(&self) -> bool {
        self.token.is_some_and(TokenKind::is_trivia)
            || self.node.kind() == SyntaxKind::Missing
    }
}

/// How a unit of a form's contents takes part in the data grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// A literal, `true`, `false`, `void`, or a brace or bracket form.
    Value,
    /// `:`.
    Colon,
    /// Something the grammar does not admit, reported at the span unless it
    /// was reported already.
    Invalid(Option<ByteSpan>),
    /// The name of a record field.
    Name,
}

/// A run of items that is one unit of the grammar: a single token or form,
/// or an application chain, which is one invalid unit.
#[derive(Clone, Copy, Debug)]
struct Unit {
    first: usize,
    last: usize,
    class: Class,
    /// Whether the unit is one identifier or reserved word, which can name a
    /// field.
    name: bool,
}

/// A grammar position of a form's contents.
#[derive(Clone, Copy, Debug)]
enum Segment {
    /// One unit on its own.
    Single(usize),
    /// A field or an entry: key, `:`, and value when one follows.
    Pair {
        key: usize,
        colon: usize,
        value: Option<usize>,
    },
}

/// The shaper of the data grammar.
#[derive(Debug)]
pub(crate) struct DataShaper<'a> {
    text: &'a str,
    diagnostics: Vec<Diagnostic>,
    /// How many `(…)` forms are open: everything inside one belongs to an
    /// application, which is reported once, at its first token.
    paren_depth: usize,
    interner: Interner,
}

impl<'a> DataShaper<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            diagnostics: Vec::new(),
            paren_depth: 0,
            interner: Interner::default(),
        }
    }

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    fn report(&mut self, code: DiagnosticCode, span: ByteSpan, message: &str) {
        if self.paren_depth == 0 {
            self.diagnostics.push(Diagnostic::new(code, span, message));
        }
    }

    fn invalid_form(&mut self, span: ByteSpan, message: &str) {
        self.report(DiagnosticCode::SyntaxInvalidForm, span, message);
    }

    fn token_value(&mut self, token: Token) -> Option<Valued> {
        let node = self.token_node(token)?;
        let number = self.interner.number(scalar_key(&node.value));
        Some(Valued { node, number })
    }

    fn token_node(&self, token: Token) -> Option<DataNode> {
        let text = token.span.text(self.text)?;
        let value = match token.kind {
            TokenKind::String => DataValue::String(decode_string(text)?),
            TokenKind::Character => DataValue::Character(decode_character(text)?),
            TokenKind::Integer | TokenKind::Float => {
                let scan = scan_number(text);
                if !scan.valid || scan.len != text.len() {
                    return None;
                }
                let suffix = split_number(text).1;
                if token.kind == TokenKind::Integer {
                    DataValue::Integer(IntegerLiteral {
                        text: text.to_owned(),
                        suffix: IntegerSuffix::parse(suffix),
                    })
                } else {
                    DataValue::Float(FloatLiteral {
                        text: text.to_owned(),
                        suffix: FloatSuffix::parse(suffix),
                    })
                }
            }
            TokenKind::Atom if is_atom(text) => DataValue::Atom(text.to_owned()),
            TokenKind::Identifier if text == "true" => DataValue::Bool(true),
            TokenKind::Identifier if text == "false" => DataValue::Bool(false),
            TokenKind::ReservedWord if text == "void" => DataValue::Void,
            _ => return None,
        };
        Some(DataNode::new(value, token.span))
    }

    /// The class of one item that does not start an application chain.
    fn classify(&self, item: &Item) -> Class {
        let span = item.node.span();
        let text = item.node.text(self.text);
        match (item.token, item.group) {
            (
                Some(
                    TokenKind::String
                    | TokenKind::Character
                    | TokenKind::Integer
                    | TokenKind::Float
                    | TokenKind::Atom,
                ),
                _,
            ) => Class::Value,
            (Some(TokenKind::Identifier), _) if text == "true" || text == "false" => {
                Class::Value
            }
            (Some(TokenKind::ReservedWord), _) if text == "void" => Class::Value,
            (Some(TokenKind::Colon), _) => Class::Colon,
            // An unterminated doc string is already an unmatched delimiter.
            (Some(TokenKind::DocString), _) if !is_terminated_doc_string(text) => {
                Class::Invalid(None)
            }
            (Some(TokenKind::Undecodable), _) => Class::Invalid(None),
            (Some(_), _) | (None, Some(GroupKind::Paren)) => Class::Invalid(Some(span)),
            (None, Some(GroupKind::Brace | GroupKind::Bracket)) => Class::Value,
            // An unmatched closer, already reported.
            (None, None) => Class::Invalid(None),
        }
    }

    /// Splits `items[from..to]` into units, skipping trivia.
    fn units(&self, items: &[Item], from: usize, to: usize) -> Vec<Unit> {
        let next = |start: usize| {
            (start..to)
                .find(|&index| items.get(index).is_some_and(|item| !item.is_trivia()))
        };
        let mut units = Vec::new();
        let mut cursor = next(from);
        while let Some(first) = cursor {
            let Some(item) = items.get(first) else {
                break;
            };
            let mut last = first;
            let mut chained = false;
            if is_primary(item) {
                while let Some(following) = next(last + 1) {
                    let Some(after) = items.get(following) else {
                        break;
                    };
                    if after.group == Some(GroupKind::Paren) {
                        last = following;
                        chained = true;
                    } else if after.token == Some(TokenKind::Dot)
                        && let Some(member) = next(following + 1)
                        && items.get(member).is_some_and(|item| {
                            item.token == Some(TokenKind::Identifier)
                        })
                    {
                        last = member;
                        chained = true;
                    } else {
                        break;
                    }
                }
            }
            let class = if chained {
                Class::Invalid(Some(first_token_span(&item.node)))
            } else {
                self.classify(item)
            };
            let name = first == last
                && matches!(
                    item.token,
                    Some(TokenKind::Identifier | TokenKind::ReservedWord)
                );
            units.push(Unit {
                first,
                last,
                class,
                name,
            });
            cursor = next(last + 1);
        }
        units
    }

    /// Pairs the units of a record or a dict into fields or entries.
    fn pairs(
        &mut self,
        items: &[Item],
        units: &mut [Unit],
        record: bool,
    ) -> (Vec<Segment>, bool) {
        let mut segments = Vec::new();
        let mut valid = true;
        let mut index = 0;
        while let Some(&unit) = units.get(index) {
            let colon = units
                .get(index + 1)
                .filter(|next| next.class == Class::Colon)
                .map(|_| index + 1);
            let keyable = if record {
                unit.name
            } else {
                unit.class != Class::Colon
            };
            if let (true, Some(colon)) = (keyable, colon) {
                let value = units
                    .get(index + 2)
                    .filter(|next| next.class != Class::Colon)
                    .map(|_| index + 2);
                if record {
                    if items
                        .get(unit.first)
                        .is_some_and(|item| item.token == Some(TokenKind::ReservedWord))
                    {
                        valid = false;
                        self.invalid_form(
                            unit_span(items, &unit),
                            "a field name is an identifier, never a reserved word",
                        );
                    }
                    if let Some(key) = units.get_mut(index) {
                        key.class = Class::Name;
                    }
                }
                if value.is_none() {
                    valid = false;
                    let colon_span =
                        units.get(colon).map_or(unit_span(items, &unit), |colon| {
                            unit_span(items, colon)
                        });
                    self.invalid_form(
                        unit_span(items, &unit).join(colon_span),
                        "a field or an entry needs a value after `:`",
                    );
                }
                segments.push(Segment::Pair {
                    key: index,
                    colon,
                    value,
                });
                index = value.unwrap_or(colon) + 1;
            } else {
                valid = false;
                match unit.class {
                    Class::Value | Class::Colon => self.invalid_form(
                        unit_span(items, &unit),
                        if record {
                            "a record holds only `name: value` fields"
                        } else {
                            "a dict holds only `key: value` entries"
                        },
                    ),
                    Class::Invalid(_) | Class::Name => {}
                }
                segments.push(Segment::Single(index));
                index += 1;
            }
        }
        (segments, valid)
    }

    /// Reports the invalid units of `units` at their spans.
    fn report_invalid(&mut self, units: &[Unit]) -> bool {
        let mut valid = true;
        for unit in units {
            if let Class::Invalid(report) = unit.class {
                valid = false;
                if let Some(span) = report {
                    self.invalid_form(
                        span,
                        "the data grammar does not admit this form",
                    );
                }
            }
        }
        valid
    }

    fn shape_group(&mut self, kind: GroupKind, items: Vec<Item>) -> Item {
        let closed = items.last().is_some_and(|item| {
            matches!(
                item.token,
                Some(TokenKind::CloseBrace | TokenKind::CloseBracket)
            )
        });
        let mut units = self.units(&items, 1, items.len().saturating_sub(1));
        let has_colon = units.iter().any(|unit| unit.class == Class::Colon);
        let mut valid = closed;
        let (node_kind, segments) = match kind {
            GroupKind::Brace if has_colon => {
                let (segments, paired) = self.pairs(&items, &mut units, true);
                valid &= paired;
                (SyntaxKind::Record, segments)
            }
            GroupKind::Bracket if units.len() == 1 && has_colon => {
                (SyntaxKind::Dict, vec![Segment::Single(0)])
            }
            GroupKind::Bracket if has_colon => {
                let (segments, paired) = self.pairs(&items, &mut units, false);
                valid &= paired;
                (SyntaxKind::Dict, segments)
            }
            GroupKind::Brace => (SyntaxKind::Tuple, singles(&units)),
            GroupKind::Bracket | GroupKind::Paren => {
                (SyntaxKind::List, singles(&units))
            }
        };
        valid &= self.report_invalid(&units);
        let at = items.first().map_or(0, |item| item.node.span().start());
        let mut slots = items.into_iter().map(Some).collect::<Vec<_>>();
        let mut children = Vec::new();
        let mut cursor = 0;
        let mut values = Vec::new();
        for segment in &segments {
            let first = match *segment {
                Segment::Single(unit) | Segment::Pair { key: unit, .. } => {
                    units.get(unit).map_or(cursor, |unit| unit.first)
                }
            };
            take_range(&mut slots, cursor, first, &mut children);
            match *segment {
                Segment::Single(unit) => {
                    if let Some(unit) = units.get(unit) {
                        let value = take_unit(&mut slots, unit, &mut children);
                        if unit.class == Class::Value {
                            values.push(value);
                        }
                        cursor = unit.last + 1;
                    }
                }
                Segment::Pair { key, colon, value } => {
                    let (Some(key), Some(colon)) = (units.get(key), units.get(colon))
                    else {
                        continue;
                    };
                    let value = value.and_then(|value| units.get(value));
                    let name = slots.get(key.first).and_then(|slot| slot.as_ref()).map(
                        |item| (item.node.text(self.text).to_owned(), item.node.span()),
                    );
                    let mut pair = Vec::new();
                    let key_value = take_unit(&mut slots, key, &mut pair);
                    take_range(&mut slots, key.last + 1, colon.first, &mut pair);
                    take_unit(&mut slots, colon, &mut pair);
                    let mut end = colon.last + 1;
                    let mut pair_value = None;
                    if let Some(value) = value {
                        take_range(&mut slots, end, value.first, &mut pair);
                        pair_value = take_unit(&mut slots, value, &mut pair);
                        if value.class != Class::Value {
                            pair_value = None;
                        }
                        end = value.last + 1;
                    }
                    let pair_kind = if node_kind == SyntaxKind::Record {
                        SyntaxKind::Field
                    } else {
                        SyntaxKind::Entry
                    };
                    let pair_at = pair.first().map_or(0, |node| node.span().start());
                    children.push(CstNode::interior(pair_kind, pair, pair_at));
                    cursor = end;
                    let key_value = key_value.and_then(Shaped::into_node);
                    let pair_value = pair_value.and_then(Shaped::into_node);
                    values.push(match (node_kind, name, key_value, pair_value) {
                        (
                            SyntaxKind::Record,
                            Some((name, name_span)),
                            _,
                            Some(value),
                        ) => Some(Shaped::Field(
                            DataField {
                                name,
                                name_span,
                                value: value.node,
                            },
                            value.number,
                        )),
                        (SyntaxKind::Dict, _, Some(key), Some(value)) => {
                            Some(Shaped::Entry(
                                DataEntry {
                                    key: key.node,
                                    value: value.node,
                                },
                                key.number,
                                value.number,
                            ))
                        }
                        _ => None,
                    });
                }
            }
        }
        take_range(&mut slots, cursor, usize::MAX, &mut children);
        let node = CstNode::interior(node_kind, children, at);
        valid &= self.check_duplicates(node_kind, &values);
        let value = if valid && values.iter().all(Option::is_some) {
            self.assemble(
                node_kind,
                values.into_iter().flatten().collect(),
                node.span(),
            )
        } else {
            None
        };
        Item {
            node,
            token: None,
            group: Some(kind),
            value,
        }
    }

    /// Reports a repeated record field or dict key; returns whether there is
    /// none.
    fn check_duplicates(
        &mut self,
        kind: SyntaxKind,
        values: &[Option<Shaped>],
    ) -> bool {
        let mut names = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut unique = true;
        for shaped in values.iter().flatten() {
            match (kind, shaped) {
                (SyntaxKind::Record, Shaped::Field(field, _))
                    if !names.insert(field.name.as_str()) =>
                {
                    unique = false;
                    self.report(
                        DiagnosticCode::DataDuplicateField,
                        field.name_span,
                        "a record's fields are unique",
                    );
                }
                (SyntaxKind::Dict, Shaped::Entry(entry, key, _))
                    if !keys.insert(*key) =>
                {
                    unique = false;
                    self.report(
                        DiagnosticCode::DataDuplicateKey,
                        entry.key.span,
                        "a dict's keys are unique as decoded values",
                    );
                }
                _ => {}
            }
        }
        unique
    }

    /// The value of a valid form from the values of its parts.
    fn assemble(
        &mut self,
        kind: SyntaxKind,
        parts: Vec<Shaped>,
        span: ByteSpan,
    ) -> Option<Valued> {
        let (value, signature) = match kind {
            SyntaxKind::Tuple | SyntaxKind::List => {
                let elements = parts
                    .into_iter()
                    .map(Shaped::into_node)
                    .collect::<Option<Vec<_>>>()?;
                let numbers = elements
                    .iter()
                    .map(|element| element.number)
                    .collect::<Vec<_>>();
                let nodes = elements.into_iter().map(|element| element.node).collect();
                if kind == SyntaxKind::Tuple {
                    (DataValue::Tuple(nodes), list_signature('t', &numbers))
                } else {
                    (DataValue::List(nodes), list_signature('l', &numbers))
                }
            }
            SyntaxKind::Record => {
                let fields = parts
                    .into_iter()
                    .map(|part| match part {
                        Shaped::Field(field, number) => Some((field, number)),
                        Shaped::Node(_) | Shaped::Entry(..) => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let signature = record_signature(
                    fields
                        .iter()
                        .map(|(field, number)| (field.name.as_str(), *number)),
                );
                let fields = fields.into_iter().map(|(field, _)| field).collect();
                (DataValue::Record(fields), signature)
            }
            SyntaxKind::Dict => {
                let entries = parts
                    .into_iter()
                    .filter_map(|part| match part {
                        Shaped::Entry(entry, key, value) => {
                            Some(Some((entry, key, value)))
                        }
                        // The `:` of `[:]` contributes nothing.
                        Shaped::Node(_) => None,
                        Shaped::Field(..) => Some(None),
                    })
                    .collect::<Option<Vec<_>>>()?;
                let signature = dict_signature(
                    entries.iter().map(|(_, key, value)| (*key, *value)),
                );
                let entries = entries.into_iter().map(|(entry, _, _)| entry).collect();
                (DataValue::Dict(entries), signature)
            }
            _ => return None,
        };
        let number = self.interner.number(signature);
        Some(Valued {
            node: DataNode::new(value, span),
            number,
        })
    }

    /// Shapes the top level of a document: exactly one value.
    pub(crate) fn shape_document(
        &mut self,
        items: Vec<Item>,
    ) -> (CstNode, Option<DataNode>) {
        let units = self.units(&items, 0, items.len());
        let mut valid = self.report_invalid(&units);
        if units.is_empty() {
            valid = false;
            self.invalid_form(
                ByteSpan::new(0, self.text.len()),
                "a VIBON document is exactly one value",
            );
        }
        let mut value_seen = false;
        for unit in &units {
            match unit.class {
                Class::Colon => {
                    valid = false;
                    self.invalid_form(
                        unit_span(&items, unit),
                        "`:` joins a field or an entry",
                    );
                }
                Class::Value if value_seen => {
                    valid = false;
                    self.invalid_form(
                        unit_span(&items, unit),
                        "a VIBON document is exactly one value",
                    );
                }
                // A unit already reported, such as a byte-order mark or an
                // unmatched closer, does not take the value's place.
                Class::Invalid(None) => {}
                Class::Value | Class::Invalid(Some(_)) | Class::Name => {
                    value_seen = true
                }
            }
        }
        let mut slots = items.into_iter().map(Some).collect::<Vec<_>>();
        let mut children = Vec::new();
        let mut cursor = 0;
        let mut value = None;
        for unit in &units {
            take_range(&mut slots, cursor, unit.first, &mut children);
            let taken = take_unit(&mut slots, unit, &mut children);
            if value.is_none() && unit.class == Class::Value {
                value = taken.and_then(Shaped::into_node);
            }
            cursor = unit.last + 1;
        }
        take_range(&mut slots, cursor, usize::MAX, &mut children);
        let root = CstNode::interior(SyntaxKind::Data, children, 0);
        let value = if valid {
            value.map(|value| value.node)
        } else {
            None
        };
        (root, value)
    }
}

/// What a shaped unit contributes to its parent's value, with the numbers of
/// its decoded values.
#[derive(Debug)]
enum Shaped {
    Node(Valued),
    Field(DataField, usize),
    Entry(DataEntry, usize, usize),
}

impl Shaped {
    fn into_node(self) -> Option<Valued> {
        match self {
            Self::Node(node) => Some(node),
            Self::Field(..) | Self::Entry(..) => None,
        }
    }
}

fn singles(units: &[Unit]) -> Vec<Segment> {
    (0..units.len()).map(Segment::Single).collect()
}

const fn is_primary(item: &Item) -> bool {
    match item.token {
        Some(kind) => matches!(
            kind,
            TokenKind::Identifier
                | TokenKind::ReservedWord
                | TokenKind::Integer
                | TokenKind::Float
                | TokenKind::Atom
                | TokenKind::String
                | TokenKind::Character
                | TokenKind::DocString
        ),
        None => matches!(item.group, Some(GroupKind::Brace | GroupKind::Bracket)),
    }
}

fn is_terminated_doc_string(text: &str) -> bool {
    text.len() >= 6 && text.ends_with("\"\"\"")
}

fn first_token_span(node: &CstNode) -> ByteSpan {
    let mut node = node;
    while let Some(first) = node.children().first() {
        node = first;
    }
    node.span()
}

fn unit_span(items: &[Item], unit: &Unit) -> ByteSpan {
    match (items.get(unit.first), items.get(unit.last)) {
        (Some(first), Some(last)) => first.node.span().join(last.node.span()),
        _ => ByteSpan::empty_at(0),
    }
}

/// Moves the items of `from..to` into `out`.
fn take_range(
    slots: &mut [Option<Item>],
    from: usize,
    to: usize,
    out: &mut Vec<CstNode>,
) {
    for slot in slots.iter_mut().take(to).skip(from) {
        if let Some(item) = slot.take() {
            out.push(item.node);
        }
    }
}

/// Moves one unit into `out`: an invalid unit inside an `@error` node, any
/// other unit as its node. Returns the unit's value.
fn take_unit(
    slots: &mut [Option<Item>],
    unit: &Unit,
    out: &mut Vec<CstNode>,
) -> Option<Shaped> {
    let mut items = Vec::new();
    for slot in slots.iter_mut().take(unit.last + 1).skip(unit.first) {
        if let Some(item) = slot.take() {
            items.push(item);
        }
    }
    match unit.class {
        Class::Invalid(_) => {
            let single_error = items.len() == 1
                && items
                    .first()
                    .is_some_and(|item| item.node.kind() == SyntaxKind::Error);
            if single_error {
                out.extend(items.into_iter().map(|item| item.node));
            } else {
                let at = items.first().map_or(0, |item| item.node.span().start());
                let nodes = items.into_iter().map(|item| item.node).collect();
                out.push(CstNode::interior(SyntaxKind::Error, nodes, at));
            }
            None
        }
        Class::Value | Class::Colon | Class::Name => {
            let mut value = None;
            for item in items {
                value = item.value.map(Shaped::Node);
                out.push(item.node);
            }
            value
        }
    }
}

impl Shaper for DataShaper<'_> {
    type Item = Item;

    fn open(&mut self, kind: GroupKind) {
        if kind == GroupKind::Paren {
            self.paren_depth += 1;
        }
    }

    fn token(&mut self, token: Token) -> Item {
        Item {
            node: CstNode::leaf(SyntaxKind::of_token(token.kind), token.span),
            token: Some(token.kind),
            group: None,
            value: self.token_value(token),
        }
    }

    fn missing(&mut self, at: usize) -> Item {
        Item {
            node: CstNode::leaf(SyntaxKind::Missing, ByteSpan::empty_at(at)),
            token: None,
            group: None,
            value: None,
        }
    }

    fn unmatched(&mut self, closer: Token) -> Item {
        Item {
            node: error_around(closer),
            token: None,
            group: None,
            value: None,
        }
    }

    fn group(&mut self, kind: GroupKind, items: Vec<Item>) -> Item {
        if kind == GroupKind::Paren {
            self.paren_depth = self.paren_depth.saturating_sub(1);
            let at = items.first().map_or(0, |item| item.node.span().start());
            let nodes = items.into_iter().map(|item| item.node).collect();
            return Item {
                node: CstNode::interior(SyntaxKind::Paren, nodes, at),
                token: None,
                group: Some(GroupKind::Paren),
                value: None,
            };
        }
        self.shape_group(kind, items)
    }
}
