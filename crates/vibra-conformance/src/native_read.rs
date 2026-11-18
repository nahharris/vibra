//! The reader of the native executors: it turns the value an entry returned
//! into an `ObservedValue` by the entry's result type, from either of two
//! sources.
//!
//! The typing logic (which type expects which shape, what is a fault, how an
//! `ObservedValue` is built, where the depth stops) exists once, in [`read`].
//! It asks a [`ValueSource`] for the handful of things it needs to know about a
//! value and does not know where they come from:
//!
//! - [`Live`] is the value store of the reference stepper's context;
//! - [`DumpSource`] is a [`Tree`], the decoded structural dump that the `harness`
//!   launcher writes into its observation document (the grammar is the module
//!   documentation of `vibra_nexus_bridge::harness`). The dump names a scalar's
//!   class and bits and never a type, so this is where the type is applied.
//!
//! A dump that was cut off by one of the launcher's bounds, or that holds a
//! node this reader has no row for, is not observable: [`decode_dump`] says so
//! and the case stays pending at the package that owns the observation of
//! values.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use vibra_ir::{ObservedValue, Type, TypeBody, TypeDefinition, TypeId, Value};
use vibra_nexus::{Cell, Ref, Store};
use vibra_syntax::{DataNode, DataValue, Literal};

// The test writer of the dump: the integration tests share it.
#[cfg(test)]
#[allow(dead_code, unreachable_pub)]
#[path = "../tests/it/dump_text.rs"]
mod dump_text;

/// Why a result was not read.
#[derive(Debug)]
pub(crate) enum Stop {
    /// The reader has no row for the value yet.
    Unobservable(&'static str),
    /// The source does not hold what the type says.
    Fault(String),
}

/// The deepest value the reader follows. A value is read by explicit recursion
/// over its type, and a deeper one waits for the observer of package X3-18. It
/// is also the depth at which the launcher's dump is cut.
pub(crate) const MAX_DEPTH: usize = 64;

pub(crate) type Definitions<'a> = BTreeMap<&'a TypeId, &'a TypeDefinition>;

fn fault(what: &str) -> Stop {
    Stop::Fault(what.to_owned())
}

/// What the reader asks of a value, wherever it is kept.
///
/// A `Node` is what a slot or a component holds: a scalar or a reference to an
/// `Object`. An `Object` is a value with a layout: text, bytes, or cells.
pub(crate) trait ValueSource {
    /// A slot or a component.
    type Node;
    /// A value with a layout.
    type Object: Copy;

    /// The bits of a scalar node.
    fn bits(&self, node: &Self::Node) -> Result<u64, Stop>;
    /// The value a node refers to.
    fn object(&self, node: &Self::Node) -> Result<Self::Object, Stop>;
    /// The variant index of an enum, a `bool`, or the member index of a union.
    fn variant(&self, object: Self::Object) -> Result<u32, Stop>;
    /// The component count of an object of cells.
    fn len(&self, object: Self::Object) -> Result<u32, Stop>;
    /// One component of an object of cells.
    fn cell(&self, object: Self::Object, index: u32) -> Result<Self::Node, Stop>;
    /// The scalars of a text value (a `str` or an atom).
    fn text(&self, object: Self::Object) -> Result<String, Stop>;
    /// The bytes of a byte string.
    fn bytes(&self, object: Self::Object) -> Result<Vec<u8>, Stop>;
}

/// The value store of a context.
pub(crate) struct Live<'a>(pub(crate) &'a Store);

fn store_fault<E>(_: E) -> Stop {
    fault("a handle the store does not admit")
}

impl ValueSource for Live<'_> {
    type Node = Cell;
    type Object = Ref;

    fn bits(&self, node: &Cell) -> Result<u64, Stop> {
        Ok(node.bits)
    }

    fn object(&self, node: &Cell) -> Result<Ref, Stop> {
        node.as_ref()
            .ok_or_else(|| fault("a scalar where a value is stored"))
    }

    fn variant(&self, object: Ref) -> Result<u32, Stop> {
        self.0.variant(object).map_err(store_fault)
    }

    fn len(&self, object: Ref) -> Result<u32, Stop> {
        self.0.len(object).map_err(store_fault)
    }

    fn cell(&self, object: Ref, index: u32) -> Result<Cell, Stop> {
        self.0.cell(object, index).map_err(store_fault)
    }

    fn text(&self, object: Ref) -> Result<String, Stop> {
        self.0.text(object).map_err(store_fault)
    }

    fn bytes(&self, object: Ref) -> Result<Vec<u8>, Stop> {
        self.0.byte_values(object).map_err(store_fault)
    }
}

/// One node of a decoded dump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tree {
    /// A scalar: its bits, whatever its class.
    Scalar(u64),
    /// A `str`.
    Str(String),
    /// An atom.
    Atom(String),
    /// A byte string.
    Bytes(Vec<u8>),
    /// A function value, which is opaque.
    Function,
    /// An object of cells.
    Object {
        /// The variant index, or the member index of a union.
        variant: u32,
        /// The components in order.
        cells: Vec<Tree>,
    },
}

/// The source over a decoded dump.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DumpSource<'a>(PhantomData<&'a Tree>);

impl<'a> ValueSource for DumpSource<'a> {
    type Node = &'a Tree;
    type Object = &'a Tree;

    fn bits(&self, node: &&'a Tree) -> Result<u64, Stop> {
        match node {
            Tree::Scalar(bits) => Ok(*bits),
            _ => Err(fault("a value where a scalar is stored")),
        }
    }

    fn object(&self, node: &&'a Tree) -> Result<&'a Tree, Stop> {
        match node {
            Tree::Scalar(_) => Err(fault("a scalar where a value is stored")),
            other => Ok(other),
        }
    }

    fn variant(&self, object: &'a Tree) -> Result<u32, Stop> {
        match object {
            Tree::Object { variant, .. } => Ok(*variant),
            _ => Err(fault("a value with no variant")),
        }
    }

    fn len(&self, object: &'a Tree) -> Result<u32, Stop> {
        match object {
            Tree::Object { cells, .. } => {
                u32::try_from(cells.len()).map_err(|_| fault("too many components"))
            }
            _ => Err(fault("a value with no cells")),
        }
    }

    fn cell(&self, object: &'a Tree, index: u32) -> Result<&'a Tree, Stop> {
        match object {
            Tree::Object { cells, .. } => usize::try_from(index)
                .ok()
                .and_then(|index| cells.get(index))
                .ok_or_else(|| fault("a component the value does not have")),
            _ => Err(fault("a value with no cells")),
        }
    }

    fn text(&self, object: &'a Tree) -> Result<String, Stop> {
        match object {
            Tree::Str(text) | Tree::Atom(text) => Ok(text.clone()),
            _ => Err(fault("a value that is not text")),
        }
    }

    fn bytes(&self, object: &'a Tree) -> Result<Vec<u8>, Stop> {
        match object {
            Tree::Bytes(bytes) => Ok(bytes.clone()),
            _ => Err(fault("a value that is not a byte string")),
        }
    }
}

/// The value `node` holds, read by `ty`. The node stays owned by the caller.
pub(crate) fn read<S: ValueSource>(
    source: &S,
    definitions: &Definitions<'_>,
    node: &S::Node,
    ty: &Type,
    depth: usize,
) -> Result<ObservedValue, Stop> {
    if depth > MAX_DEPTH {
        return Err(Stop::Unobservable("a value nested deeper than the reader"));
    }
    let bits = || source.bits(node);
    let word =
        || u32::try_from(bits()? & 0xFFFF_FFFF).map_err(|_| fault("a scalar cell"));
    let reference = || source.object(node);
    let primitive = |value| Ok(ObservedValue::Primitive(value));
    let narrow = |what: &str| fault(&format!("a cell that does not fit {what}"));
    match ty {
        Type::Void => primitive(Value::Void),
        Type::Bool => {
            let value = reference()?;
            match source.variant(value)? {
                0 => primitive(Value::Bool(false)),
                1 => primitive(Value::Bool(true)),
                _ => Err(fault("a bool with another variant")),
            }
        }
        Type::Char => primitive(Value::Char(
            char::from_u32(word()?).ok_or_else(|| narrow("a char"))?,
        )),
        Type::I8 => primitive(Value::I8(
            i8::try_from(word()?.cast_signed()).map_err(|_| narrow("i8"))?,
        )),
        Type::I16 => primitive(Value::I16(
            i16::try_from(word()?.cast_signed()).map_err(|_| narrow("i16"))?,
        )),
        Type::I32 => primitive(Value::I32(word()?.cast_signed())),
        Type::U8 => {
            primitive(Value::U8(u8::try_from(word()?).map_err(|_| narrow("u8"))?))
        }
        Type::U16 => primitive(Value::U16(
            u16::try_from(word()?).map_err(|_| narrow("u16"))?,
        )),
        Type::U32 => primitive(Value::U32(word()?)),
        Type::I64 => primitive(Value::I64(bits()?.cast_signed())),
        Type::U64 => primitive(Value::U64(bits()?)),
        Type::F32 => primitive(Value::F32(vibra_ir::canonical_f32_bits(word()?))),
        Type::F64 => primitive(Value::F64(vibra_ir::canonical_f64_bits(bits()?))),
        Type::Str => primitive(Value::Str(source.text(reference()?)?)),
        Type::Atom | Type::AtomSingleton(_) => {
            primitive(Value::Atom(source.text(reference()?)?))
        }
        Type::Bytes => primitive(Value::Bytes(source.bytes(reference()?)?)),
        Type::Never
        | Type::Function(_)
        | Type::Param(_)
        | Type::Dict(..)
        | Type::Interface(..)
        | Type::Any => Err(Stop::Unobservable("a result of a kind the reader lacks")),
        Type::Array(element) => {
            let value = reference()?;
            let length = source.len(value)?;
            let mut elements = Vec::new();
            for index in 0..length {
                let item = source.cell(value, index)?;
                elements.push(read(source, definitions, &item, element, depth + 1)?);
            }
            Ok(ObservedValue::Array(elements))
        }
        Type::Declared(_)
        | Type::Applied(..)
        | Type::Record(_)
        | Type::Enum(_)
        | Type::Tuple(_)
        | Type::Union(_) => {
            let body = match ty {
                Type::Declared(id) => definitions
                    .get(id)
                    .and_then(|definition| definition.instantiate(&[])),
                Type::Applied(id, arguments) => definitions
                    .get(id)
                    .and_then(|definition| definition.instantiate(arguments)),
                Type::Record(fields) => Some(TypeBody::Record(fields.clone())),
                Type::Enum(variants) => Some(TypeBody::Enum(variants.clone())),
                Type::Tuple(components) => Some(TypeBody::Tuple(components.clone())),
                _ => match ty {
                    Type::Union(members) => Some(TypeBody::Union(members.clone())),
                    _ => None,
                },
            }
            .ok_or_else(|| fault("a type with no declaration"))?;
            let type_id = match ty {
                Type::Declared(id) | Type::Applied(id, _) => Some(id.clone()),
                _ => None,
            };
            read_body(source, definitions, reference()?, body, type_id, depth)
        }
    }
}

fn read_body<S: ValueSource>(
    source: &S,
    definitions: &Definitions<'_>,
    value: S::Object,
    body: TypeBody,
    type_id: Option<TypeId>,
    depth: usize,
) -> Result<ObservedValue, Stop> {
    let component = |index: usize, ty: &Type| {
        let index = u32::try_from(index).map_err(|_| fault("too many components"))?;
        let item = source.cell(value, index)?;
        read(source, definitions, &item, ty, depth + 1)
    };
    Ok(match body {
        TypeBody::Record(fields) => {
            let mut read_fields = Vec::new();
            for (index, (name, ty)) in fields.iter().enumerate() {
                read_fields.push((name.clone(), component(index, ty)?));
            }
            ObservedValue::Record {
                type_id,
                fields: read_fields,
            }
        }
        TypeBody::Tuple(components) => {
            let mut values = Vec::new();
            for (index, ty) in components.iter().enumerate() {
                values.push(component(index, ty)?);
            }
            ObservedValue::Tuple { type_id, values }
        }
        TypeBody::Wrapper(representation) => ObservedValue::Wrapper {
            type_id: type_id.ok_or_else(|| fault("a wrapper with no declaration"))?,
            value: Box::new(component(0, &representation)?),
        },
        TypeBody::Enum(variants) => {
            let chosen = usize::try_from(source.variant(value)?)
                .map_err(|_| fault("a variant index"))?;
            let (name, payload) = variants
                .get(chosen)
                .ok_or_else(|| fault("an enum with a variant it does not declare"))?;
            let payload = if *payload == Type::Void {
                None
            } else {
                Some(Box::new(component(0, payload)?))
            };
            ObservedValue::Enum {
                type_id,
                variant: name.clone(),
                payload,
            }
        }
        TypeBody::Union(members) => {
            let chosen = usize::try_from(source.variant(value)?)
                .map_err(|_| fault("a member index"))?;
            let member = members
                .get(chosen)
                .ok_or_else(|| fault("a union with a member it does not declare"))?;
            ObservedValue::Union {
                type_id,
                member: Box::new(member.clone()),
                value: Box::new(component(0, member)?),
            }
        }
    })
}

// -- decoding a dump ---------------------------------------------------------------

/// The tree of a dump node, or why the result cannot be observed.
///
/// A node the launcher cut off (`@truncated`) or has no name for (`@unknown`)
/// is not a result the reader can type: [`Stop::Unobservable`], whichever
/// position it is in. A node the launcher's store refused (`@fault`) and a
/// document that is not the launcher's grammar are a [`Stop::Fault`].
pub(crate) fn decode_dump(node: &DataNode) -> Result<Tree, Stop> {
    decode_node(node, 0)
}

fn malformed(what: &str) -> Stop {
    Stop::Fault(format!("the dump is malformed: {what}"))
}

fn decode_node(node: &DataNode, depth: usize) -> Result<Tree, Stop> {
    // The launcher stops at `MAX_DEPTH`, so a document that goes further than
    // it, even by the one level of a truncated node, is not the launcher's.
    if depth > MAX_DEPTH + 1 {
        return Err(malformed("it is nested deeper than the launcher writes"));
    }
    let DataValue::Record(fields) = node.value() else {
        return Err(malformed("a node is a record"));
    };
    let mut fields = fields.iter();
    let kind = match fields.next() {
        Some(field) if field.label().value() == "node" => match field.value().value() {
            DataValue::Atom(atom) => atom.value().to_owned(),
            _ => return Err(malformed("`node` is an atom")),
        },
        _ => return Err(malformed("a node starts with `node:`")),
    };
    let rest = fields.collect::<Vec<_>>();
    let field = |label: &str| {
        rest.iter()
            .find(|field| field.label().value() == label)
            .map(|field| field.value())
            .ok_or_else(|| malformed(&format!("`{kind}` has no `{label}`")))
    };
    let string = |label: &str| match field(label)?.value() {
        DataValue::Literal(Literal::String(literal)) => Ok(literal.value().to_owned()),
        _ => Err(malformed(&format!("`{label}` is a string"))),
    };
    let count = |label: &str| match field(label)?.value() {
        DataValue::Literal(Literal::Integer(literal)) if !literal.is_negative() => {
            literal
                .digits()
                .parse::<u32>()
                .map_err(|_| malformed(&format!("`{label}` does not fit 32 bits")))
        }
        _ => Err(malformed(&format!("`{label}` is a count"))),
    };
    match kind.as_str() {
        "scalar" => {
            let bits = string("bits")?
                .parse::<u64>()
                .map_err(|_| malformed("`bits` is an unsigned decimal"))?;
            Ok(Tree::Scalar(bits))
        }
        "str" => Ok(Tree::Str(string("value")?)),
        "atom" => Ok(Tree::Atom(string("value")?)),
        "bytes" => Ok(Tree::Bytes(decode_hex(&string("hex")?)?)),
        "function" => Ok(Tree::Function),
        "object" => {
            let variant = count("variant")?;
            let DataValue::Array(items) = field("cells")?.value() else {
                return Err(malformed("`cells` is an array"));
            };
            let cells = items
                .iter()
                .map(|item| decode_node(item, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Tree::Object { variant, cells })
        }
        "truncated" => Err(Stop::Unobservable("a result the launcher cut off")),
        "unknown" => Err(Stop::Unobservable("a value kind the launcher cannot name")),
        "fault" => Err(fault("a value the launcher's store refused")),
        other => Err(malformed(&format!("`{other}` is not a node"))),
    }
}

fn decode_hex(text: &str) -> Result<Vec<u8>, Stop> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return Err(malformed("`hex` is pairs of hexadecimal digits"));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| {
            text.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| malformed("`hex` is pairs of hexadecimal digits"))
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use std::path::Path;

    use vibra_host::{HostWorld, WorldProvider};
    use vibra_ir::arena::{STRIDE_CELLS, kind};
    use vibra_nexus::Context;
    use vibra_syntax::parse_data;

    use super::dump_text::dump;
    use super::*;

    fn object(store: &mut Store, code: u8, variant: u32, cells: &[Cell]) -> Cell {
        let len = u32::try_from(cells.len()).unwrap();
        let value = store.alloc(code, STRIDE_CELLS, len, variant).unwrap();
        for (index, cell) in cells.iter().enumerate() {
            store
                .set_cell(value, u32::try_from(index).unwrap(), *cell)
                .unwrap();
        }
        Cell::reference(value)
    }

    fn text(store: &mut Store, code: u8, text: &str) -> Cell {
        Cell::reference(store.new_text(code, text).unwrap())
    }

    fn scalar(class: u8, bits: u64) -> Cell {
        Cell { class, bits }
    }

    /// The dump of `cell` as the reader's tree, through the text of the dump
    /// and the VIBON reader, as the runner gets it from a process.
    fn tree_of(store: &Store, cell: Cell) -> Result<Tree, Stop> {
        let text = dump(store, cell);
        let parsed = parse_data(Path::new("dump.vibon"), &text).expect("a VIBON value");
        assert!(parsed.accepted() && !parsed.recovered(), "{text}");
        decode_dump(parsed.data().expect("a value"))
    }

    #[test]
    fn the_two_sources_read_the_same_value() {
        let world = HostWorld::default();
        let mut host = WorldProvider::new(&world);
        let mut ctx = Context::new(1 << 28, 0, &mut host);
        let store = ctx.store();

        let wrapper_id = TypeId::new("app.age", "app.age");
        let record_id = TypeId::new("app.point", "app.point");
        let enum_id = TypeId::new("app.shape", "app.shape");
        let definitions = [
            TypeDefinition::new(wrapper_id.clone(), TypeBody::Wrapper(Type::U8)),
            TypeDefinition::new(
                record_id.clone(),
                TypeBody::Record(vec![
                    ("x".to_owned(), Type::I64),
                    ("label".to_owned(), Type::Str),
                ]),
            ),
            TypeDefinition::new(
                enum_id.clone(),
                TypeBody::Enum(vec![
                    ("empty".to_owned(), Type::Void),
                    ("point".to_owned(), Type::Declared(record_id.clone())),
                ]),
            ),
        ];
        let definitions: Definitions<'_> =
            definitions.iter().map(|d| (d.id(), d)).collect();

        let true_value = object(store, kind::ENUM, 1, &[]);
        let false_value = object(store, kind::ENUM, 0, &[]);
        let label = text(store, kind::STR, "caf\u{e9} \"quoted\"\n");
        let point = object(store, kind::RECORD, 0, &[Cell::i64(u64::MAX - 4), label]);
        let shape = object(store, kind::ENUM, 1, &[point]);
        let nothing = object(store, kind::ENUM, 0, &[]);
        let atom = text(store, kind::ATOM, "ready");
        let bytes =
            Cell::reference(store.new_bytes(kind::BYTES, &[0, 1, 254, 255]).unwrap());
        let empty_bytes = Cell::reference(store.new_bytes(kind::BYTES, &[]).unwrap());
        let age = object(store, kind::WRAPPER, 0, &[Cell::i32(41)]);
        let member_text = text(store, kind::STR, "member");
        let union = object(store, kind::UNION, 1, &[member_text]);
        let items = object(
            store,
            kind::ARRAY,
            0,
            &[Cell::i32(3), Cell::i32(0), Cell::i32(9)],
        );
        let no_items = object(store, kind::ARRAY, 0, &[]);
        let inner_tuple = object(store, kind::TUPLE, 0, &[Cell::i64(7), true_value]);
        let second_text = text(store, kind::STR, "two");
        let first = object(store, kind::TUPLE, 0, &[Cell::i32(1), second_text]);
        let blank = text(store, kind::STR, "");
        let second = object(store, kind::TUPLE, 0, &[Cell::i32(2), blank]);
        let nested = object(store, kind::ARRAY, 0, &[first, second]);
        let some = object(store, kind::ENUM, 1, &[Cell::i32(5)]);
        let values: Vec<(&str, Cell, Type)> = vec![
            ("void", Cell::EMPTY, Type::Void),
            ("true", true_value, Type::Bool),
            ("false", false_value, Type::Bool),
            ("char", Cell::i32(u32::from('\u{1f600}')), Type::Char),
            (
                "i8",
                Cell::i32(u32::from_ne_bytes((-5_i32).to_ne_bytes())),
                Type::I8,
            ),
            (
                "i16",
                Cell::i32(u32::from_ne_bytes((-300_i32).to_ne_bytes())),
                Type::I16,
            ),
            (
                "i32",
                Cell::i32(u32::from_ne_bytes(i32::MIN.to_ne_bytes())),
                Type::I32,
            ),
            ("u8", Cell::i32(200), Type::U8),
            ("u16", Cell::i32(65_535), Type::U16),
            ("u32", Cell::i32(u32::MAX), Type::U32),
            (
                "i64",
                Cell::i64(u64::from_ne_bytes((-9_i64).to_ne_bytes())),
                Type::I64,
            ),
            ("u64", Cell::i64(u64::MAX), Type::U64),
            ("f32", scalar(2, 0x3fc0_0000), Type::F32),
            ("f32 nan", scalar(2, 0x7fa0_0001), Type::F32),
            ("f64", scalar(3, 0x3ff8_0000_0000_0000), Type::F64),
            ("f64 nan", scalar(3, 0x7ff0_0000_0000_0001), Type::F64),
            ("str", label, Type::Str),
            ("atom", atom, Type::Atom),
            ("singleton", atom, Type::AtomSingleton("ready".to_owned())),
            ("bytes", bytes, Type::Bytes),
            ("empty bytes", empty_bytes, Type::Bytes),
            (
                "anonymous record",
                point,
                Type::Record(vec![
                    ("x".to_owned(), Type::I64),
                    ("label".to_owned(), Type::Str),
                ]),
            ),
            ("declared record", point, Type::Declared(record_id)),
            (
                "tuple",
                inner_tuple,
                Type::Tuple(vec![Type::I64, Type::Bool]),
            ),
            (
                "anonymous enum",
                some,
                Type::Enum(vec![
                    ("none".to_owned(), Type::Void),
                    ("some".to_owned(), Type::I32),
                ]),
            ),
            (
                "declared enum, payload",
                shape,
                Type::Declared(enum_id.clone()),
            ),
            ("declared enum, nullary", nothing, Type::Declared(enum_id)),
            ("wrapper", age, Type::Declared(wrapper_id)),
            ("union", union, Type::Union(vec![Type::I32, Type::Str])),
            ("array", items, Type::Array(Box::new(Type::I32))),
            ("empty array", no_items, Type::Array(Box::new(Type::Str))),
            (
                "nested",
                nested,
                Type::Array(Box::new(Type::Tuple(vec![Type::I32, Type::Str]))),
            ),
        ];
        let store = &*store;
        let mut agreed = 0;
        for (name, cell, ty) in &values {
            let live = read(&Live(store), &definitions, cell, ty, 0);
            let tree =
                tree_of(store, *cell).unwrap_or_else(|_| panic!("{name}: no tree"));
            let dumped = read(&DumpSource::default(), &definitions, &&tree, ty, 0);
            let (live, dumped) = (
                live.unwrap_or_else(|stop| panic!("{name}: live {stop:?}")),
                dumped.unwrap_or_else(|stop| panic!("{name}: dump {stop:?}")),
            );
            assert_eq!(live, dumped, "{name}");
            assert_eq!(
                live.canonical_observation(ty),
                dumped.canonical_observation(ty),
                "{name}"
            );
            agreed += 1;
        }
        assert_eq!(agreed, values.len());
    }

    #[test]
    fn a_dump_that_was_cut_off_is_not_observable() {
        let world = HostWorld::default();
        let mut host = WorldProvider::new(&world);
        let mut ctx = Context::new(1 << 28, 0, &mut host);
        let store = ctx.store();
        // Nested past the depth of the dump.
        let mut cell = Cell::i32(1);
        for _ in 0..70 {
            cell = object(store, kind::TUPLE, 0, &[cell]);
        }
        let tree = tree_of(store, cell);
        assert!(matches!(tree, Err(Stop::Unobservable(_))), "{tree:?}");
        // The store would read it, to the depth the reader follows.
        let ty = (0..70).fold(Type::I32, |inner, _| Type::Tuple(vec![inner]));
        let definitions = Definitions::new();
        let live = read(&Live(store), &definitions, &cell, &ty, 0);
        assert!(matches!(live, Err(Stop::Unobservable(_))), "{live:?}");

        // A wide array past the node bound.
        let wide = object(store, kind::ARRAY, 0, &vec![Cell::i32(0); 70_000]);
        assert!(matches!(tree_of(store, wide), Err(Stop::Unobservable(_))));
    }
}
