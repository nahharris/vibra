//! The type table a module carries when it compares canonical encodings of
//! values whose types are known only at run time.
//!
//! The canonical encoding of an interface value is the encoding of the value it
//! holds (`docs/spec/06-runtime.md`, "Canonical value encoding"), and an
//! encoding names a type only in a few places: a primitive's suffix, the
//! `type: P` of a declared value, a variant or field name, and the `member: T`
//! of a union. Two interface values that hold different types are equal exactly
//! when their encodings are, so the comparison (`crate::canon::typed_routine`)
//! follows both values with the descriptors of their types. A descriptor names
//! a constructor and its arguments; this table says, for each constructor
//! (indexed by the ordinal of its head), what the encoding of a value of it is
//! made of. All words are little-endian `u32`, and an offset counts bytes from
//! the start of the table:
//!
//! ```text
//! count                          constructors in the table
//! per constructor, 5 words:
//!   kind                         one of [`kind`]
//!   identity                     the declared type, numbered from 1, 0 for none
//!   names                        offset of the names, 0 for none
//!   components                   offset of the component templates, 0 for none
//!   reserved
//! names:      count, then one word per record field or enum variant, the same
//!             number for the same spelling
//! components: count, then the offset of one template per component of a
//!             declared type, in layout order
//! template:   tag (0 parameter, 1 constructor), the parameter index or the
//!             head, the argument count, then one template offset per argument
//! ```
//!
//! An anonymous type needs no components, since its descriptor carries them as
//! arguments. A declared type's components are instantiated at run time from the
//! templates and the arguments of its own descriptor.

use std::collections::BTreeMap;

use vibra_ir::{Type, TypeBody, TypeId};

use super::Lowering;

/// What the encoding of a value of a constructor is made of.
pub(crate) mod kind {
    /// No value is compared: `never`, a generic parameter nothing fixed.
    pub(crate) const NONE: u32 = 0;
    /// A primitive scalar: the same type and the same bits.
    pub(crate) const SCALAR: u32 = 1;
    /// `void`, which has one value.
    pub(crate) const VOID: u32 = 2;
    /// `bool`, whose encoding is a literal.
    pub(crate) const BOOL: u32 = 3;
    /// `str`.
    pub(crate) const TEXT: u32 = 4;
    /// `atom` and an atom singleton, which encode as the atom.
    pub(crate) const ATOM: u32 = 5;
    /// `bytes`.
    pub(crate) const BYTES: u32 = 6;
    /// A tuple.
    pub(crate) const TUPLE: u32 = 7;
    /// A record.
    pub(crate) const RECORD: u32 = 8;
    /// An enum.
    pub(crate) const ENUM: u32 = 9;
    /// A union.
    pub(crate) const UNION: u32 = 10;
    /// An array.
    pub(crate) const ARRAY: u32 = 11;
    /// A dict.
    pub(crate) const DICT: u32 = 12;
    /// A wrapper.
    pub(crate) const WRAPPER: u32 = 13;
    /// An interface value, which is the value it holds.
    pub(crate) const INTERFACE: u32 = 14;
    /// A function, which has no encoding.
    pub(crate) const FUNCTION: u32 = 15;
}

/// The bytes of one constructor's row.
pub(crate) const ROW: u32 = 20;

/// A word of the part of the table after the rows.
#[derive(Clone, Copy)]
enum Word {
    Plain(u32),
    /// The position, counted from `1` in the part after the rows, of a word.
    At(u32),
}

#[derive(Default)]
struct Table {
    rows: Vec<[Word; 5]>,
    tail: Vec<Word>,
    names: BTreeMap<String, u32>,
    identities: BTreeMap<String, u32>,
}

impl Table {
    fn name(&mut self, name: &str) -> u32 {
        let next = u32::try_from(self.names.len() + 1).unwrap_or(u32::MAX);
        *self.names.entry(name.to_owned()).or_insert(next)
    }

    fn identity(&mut self, id: &TypeId) -> u32 {
        let next = u32::try_from(self.identities.len() + 1).unwrap_or(u32::MAX);
        *self.identities.entry(format!("{id:?}")).or_insert(next)
    }

    /// The position of the next word of the tail, counted from `1`.
    fn next(&self) -> u32 {
        u32::try_from(self.tail.len()).unwrap_or(0) + 1
    }
}

impl Lowering<'_> {
    /// Writes the type table for every constructor this module has a head for.
    /// The templates of a declared type mention constructors that need heads of
    /// their own, so the table is written again until no head is new.
    pub(crate) fn type_table(&mut self) -> Vec<u8> {
        loop {
            let before = self.heads.len();
            let table = self.write_table();
            if self.heads.len() == before {
                return Self::bytes(&table);
            }
        }
    }

    fn bytes(table: &Table) -> Vec<u8> {
        let rows = u32::try_from(table.rows.len()).unwrap_or(0);
        let tail_start = 4 + ROW * rows;
        let resolve = |word: &Word| match word {
            Word::Plain(value) => *value,
            Word::At(0) => 0,
            Word::At(position) => tail_start + 4 * (position - 1),
        };
        let mut words = vec![rows];
        for row in &table.rows {
            words.extend(row.iter().map(resolve));
        }
        words.extend(table.tail.iter().map(resolve));
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn write_table(&mut self) -> Table {
        let mut table = Table::default();
        let mut known = self
            .head_types
            .iter()
            .map(|(head, ty)| (*head, ty.clone()))
            .collect::<Vec<_>>();
        known.sort_by_key(|(head, _)| head >> crate::desc::ORDINAL_SHIFT);
        for (_, ty) in known {
            let row = self.constructor(&mut table, &ty);
            table.rows.push(row);
        }
        table
    }

    /// The generic parameters and body of a declared type.
    fn declared(&self, id: &TypeId) -> Option<(Vec<String>, TypeBody)> {
        let definition = self
            .program
            .types()
            .iter()
            .find(|definition| definition.id() == id)?;
        let parameters = definition.parameters().to_vec();
        let named = parameters
            .iter()
            .cloned()
            .map(Type::Param)
            .collect::<Vec<_>>();
        Some((parameters, definition.instantiate(&named)?))
    }

    /// The row of the constructor of `ty`.
    fn constructor(&mut self, table: &mut Table, ty: &Type) -> [Word; 5] {
        use kind as k;
        let mut identity = 0;
        let mut names: Vec<String> = Vec::new();
        let mut components: Option<(Vec<String>, Vec<Type>)> = None;
        let kind = match ty {
            Type::Void => k::VOID,
            Type::Never | Type::Param(_) => k::NONE,
            Type::Bool => k::BOOL,
            Type::Str => k::TEXT,
            Type::Atom | Type::AtomSingleton(_) => k::ATOM,
            Type::Bytes => k::BYTES,
            Type::Tuple(_) => k::TUPLE,
            Type::Record(fields) => {
                names = fields.iter().map(|(name, _)| name.clone()).collect();
                k::RECORD
            }
            Type::Enum(variants) => {
                names = variants.iter().map(|(name, _)| name.clone()).collect();
                k::ENUM
            }
            Type::Union(_) => k::UNION,
            Type::Array(_) => k::ARRAY,
            Type::Dict(..) => k::DICT,
            Type::Function(_) => k::FUNCTION,
            Type::Interface(..) | Type::Any => k::INTERFACE,
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::F32
            | Type::F64
            | Type::Char => k::SCALAR,
            Type::Declared(id) | Type::Applied(id, _) => {
                identity = table.identity(id);
                match self.declared(id) {
                    None => k::NONE,
                    Some((parameters, body)) => {
                        let (kind, parts) = match body {
                            TypeBody::Record(fields) => {
                                names = fields.iter().map(|(n, _)| n.clone()).collect();
                                (
                                    k::RECORD,
                                    fields.into_iter().map(|(_, t)| t).collect(),
                                )
                            }
                            TypeBody::Enum(variants) => {
                                names =
                                    variants.iter().map(|(n, _)| n.clone()).collect();
                                (
                                    k::ENUM,
                                    variants.into_iter().map(|(_, t)| t).collect(),
                                )
                            }
                            TypeBody::Wrapper(representation) => {
                                (k::WRAPPER, vec![representation])
                            }
                            TypeBody::Tuple(parts) => (k::TUPLE, parts),
                            TypeBody::Union(members) => (k::UNION, members),
                        };
                        components = Some((parameters, parts));
                        kind
                    }
                }
            }
        };
        let names_at = if names.is_empty() {
            Word::At(0)
        } else {
            let at = table.next();
            table
                .tail
                .push(Word::Plain(u32::try_from(names.len()).unwrap_or(0)));
            for name in &names {
                let id = table.name(name);
                table.tail.push(Word::Plain(id));
            }
            Word::At(at)
        };
        let components_at = match components {
            None => Word::At(0),
            Some((parameters, parts)) => {
                let templates = parts
                    .iter()
                    .map(|part| self.template(table, part, &parameters))
                    .collect::<Vec<_>>();
                let at = table.next();
                table
                    .tail
                    .push(Word::Plain(u32::try_from(templates.len()).unwrap_or(0)));
                table.tail.extend(templates.into_iter().map(Word::At));
                Word::At(at)
            }
        };
        [
            Word::Plain(kind),
            Word::Plain(identity),
            names_at,
            components_at,
            Word::Plain(0),
        ]
    }

    /// Writes the template of `ty`, whose generic parameters are `parameters`,
    /// and returns its position. It mirrors how a descriptor is built: a head and
    /// the descriptors of the components.
    fn template(&mut self, table: &mut Table, ty: &Type, parameters: &[String]) -> u32 {
        if let Type::Param(name) = ty {
            let index = parameters
                .iter()
                .position(|known| known == name)
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(u32::MAX);
            let at = table.next();
            table
                .tail
                .extend([Word::Plain(0), Word::Plain(index), Word::Plain(0)]);
            return at;
        }
        let children = ty
            .components()
            .iter()
            .map(|component| self.template(table, component, parameters))
            .collect::<Vec<_>>();
        let head = self.head(ty);
        let at = table.next();
        table.tail.extend([
            Word::Plain(1),
            Word::Plain(head),
            Word::Plain(u32::try_from(children.len()).unwrap_or(0)),
        ]);
        table.tail.extend(children.into_iter().map(Word::At));
        at
    }
}
