//! The host-side observation of a result.
//!
//! A run that completes leaves its result in the slot `vibra_v1_result`
//! reports. The host reads the value through the accessors, by the entry's
//! result type, into the neutral [`Value`] of `vibra-ir`, and renders it with
//! the one canonical encoder of `vibra-ir`, which the reference interpreter
//! uses for the same program, so the harness compares the two executors byte
//! for byte. No ID, offset, or index appears in the value: the host holds an
//! ID only while it reads, and releases it.
//!
//! The reader follows the layout of `vibra-wasm`: `Str` and `Atom` are
//! characters, `Bytes` bytes, a list its elements, a dict its entries in key
//! order, each a two-component tuple, a record its fields in the order of its
//! type, a variant value (`Bool` among them) its case index and the case's
//! fields, and a wrapper its representation. A type the reader has no row for
//! is a defect of the toolchain: the emitter lowered a program the host
//! cannot observe.
//!
//! A value may be nested to any depth, since the language bounds depth only
//! by memory, so the reader keeps its own worklist and never recurses.

use vibra_ir::{
    CANONICAL_NAN_F32, CANONICAL_NAN_F64, Library, ObservedValue, Type, TypeBody,
    TypeTable, Value, value_document,
};

use crate::{Instance, Outcome, ResultSlot, Runner, RunnerError, Started, ValueId};

/// The live arena size in bytes at the three moments a run cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveSizes {
    /// Before the entry ran.
    pub start: u64,
    /// After the entry completed, while the host still held its result.
    pub with_result: u64,
    /// After the host read the result and released it.
    pub end: u64,
}

/// A run, observed.
#[derive(Debug, PartialEq, Eq)]
pub enum Observed {
    /// The entry completed and its result was read.
    Completed {
        /// The result at the entry's result type.
        value: Box<ObservedValue>,
        /// The canonical bytes of the result's `@value.v2` document.
        canonical: String,
        /// The live sizes around the run.
        live: LiveSizes,
    },
    /// The run did not complete, or the result could not be read: the trap,
    /// host event, failed assertion, or defect, as [`Outcome`] reports it.
    /// Never [`Outcome::Completed`].
    Stopped(Outcome),
}

impl Runner {
    /// Validates and instantiates `bytes`, runs its entry, and reads the
    /// result as a value of type `result`. `types` are the declared types of
    /// the program and the library bindings, which name the components of a
    /// declared type.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a transition module or lacks
    /// an export.
    pub fn run_observed(
        &self,
        bytes: &[u8],
        result: &Type,
        types: &TypeTable,
    ) -> Result<Observed, RunnerError> {
        let mut instance = match self.start(bytes)? {
            Started::MemoryExhausted => {
                return Ok(Observed::Stopped(Outcome::MemoryExhausted));
            }
            Started::Ready(instance) => instance,
        };
        let start = match instance.live_size() {
            Ok(start) => start,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        let (slot, with_result) = match instance.call_eval()? {
            Outcome::Completed { result, live_size } => (result, live_size),
            stopped => return Ok(Observed::Stopped(stopped)),
        };
        let value = match instance.observe(slot, result, types) {
            Ok(value) => value,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        let end = match instance.live_size() {
            Ok(end) => end,
            Err(stop) => return Ok(Observed::Stopped(stop)),
        };
        let canonical = match value_document(&value, result, types) {
            Ok(canonical) => canonical,
            Err(error) => {
                return Ok(Observed::Stopped(defect(format!(
                    "the result has no encoding: {error}"
                ))));
            }
        };
        Ok(Observed::Completed {
            value: Box::new(ObservedValue::new(result.clone(), value)),
            canonical,
            live: LiveSizes {
                start,
                with_result,
                end,
            },
        })
    }
}

/// What a node's component is once read: a value that needed no more
/// reading, or a reference to a child that is visited next.
enum Component {
    Ready(Value),
    Child(ValueId, Type),
}

/// What a node becomes once its children are built.
enum Build {
    Tuple,
    List,
    Record(Vec<String>),
    Variant { case: String, names: Vec<String> },
    Wrapper,
    Dict,
}

enum Task {
    Visit(ValueId, Type),
    /// A node whose components are `items`, where `None` is a child that is
    /// built, in order, by the visits before this task.
    Build(Build, Vec<Option<Value>>),
}

/// The components of a compound type, in the order the layout stores them.
enum Shape {
    Atom,
    Str,
    Bytes,
    Tuple(Vec<Type>),
    List(Type),
    Record(Vec<(String, Type)>),
    Variant(Vec<(String, Vec<(String, Type)>)>),
    Wrapper(Type),
    Dict(Type, Type),
}

/// The layout of the arena value of type `ty`.
fn shape(ty: &Type, types: &TypeTable) -> Result<Shape, Outcome> {
    let none = || defect(format!("the host has no reader for the type {ty}"));
    Ok(match ty {
        Type::Atom | Type::AtomSingleton(_) => Shape::Atom,
        Type::Tuple(components) => Shape::Tuple(components.clone()),
        Type::List(element) => Shape::List(element.as_ref().clone()),
        Type::Record(fields) => Shape::Record(fields.entries().to_vec()),
        Type::Declared(id, arguments) => {
            match types.library().library_of(id) {
                Some(Library::Str) => return Ok(Shape::Str),
                Some(Library::Bytes) => return Ok(Shape::Bytes),
                Some(Library::Dict) => {
                    return match arguments.as_slice() {
                        [key, value] => Ok(Shape::Dict(key.clone(), value.clone())),
                        _ => Err(none()),
                    };
                }
                _ => {}
            }
            let definition = types.definition(id).ok_or_else(none)?;
            let at = |field: &Type| field.instantiate(arguments);
            let named = |fields: &[(String, Type)]| {
                fields
                    .iter()
                    .map(|(name, field)| (name.clone(), at(field)))
                    .collect::<Vec<_>>()
            };
            match definition.body() {
                TypeBody::Record(fields) => Shape::Record(named(fields)),
                TypeBody::Tuple(components) => {
                    Shape::Tuple(components.iter().map(at).collect())
                }
                TypeBody::Wrapper(representation) => Shape::Wrapper(at(representation)),
                TypeBody::Variant(cases) => Shape::Variant(
                    cases
                        .iter()
                        .map(|case| (case.name.clone(), named(&case.fields)))
                        .collect(),
                ),
            }
        }
        _ => return Err(none()),
    })
}

impl Instance {
    /// Reads the result a completed entry left in `slot` as a value of type
    /// `result`, and releases the ID the slot held when the result is an
    /// arena value.
    ///
    /// # Errors
    ///
    /// The [`Outcome`] of a stop while reading, or a defect when the slot
    /// does not fit the type or the host has no reader for the type.
    pub fn observe(
        &mut self,
        slot: ResultSlot,
        result: &Type,
        types: &TypeTable,
    ) -> Result<Value, Outcome> {
        if let Some(value) = scalar(slot.0, result)? {
            return Ok(value);
        }
        let root = self.result_id(slot);
        self.read_tree(types, root, result.clone())
    }

    /// Reads the arena value `root` of type `ty` and everything below it, and
    /// releases every ID it holds, the root's included.
    fn read_tree(
        &mut self,
        types: &TypeTable,
        root: ValueId,
        ty: Type,
    ) -> Result<Value, Outcome> {
        let mut tasks = vec![Task::Visit(root, ty)];
        let mut done: Vec<Value> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(id, ty) => {
                    self.visit(types, id, &ty, &mut tasks, &mut done)?
                }
                Task::Build(build, items) => {
                    let pending = items.iter().filter(|item| item.is_none()).count();
                    let at = done.len().checked_sub(pending).ok_or_else(|| {
                        defect("a node was built before its children".to_owned())
                    })?;
                    let mut children = done.split_off(at).into_iter();
                    let mut values = Vec::with_capacity(items.len());
                    for item in items {
                        values.push(match item {
                            Some(value) => value,
                            None => children.next().ok_or_else(|| {
                                defect("a node lost a child".to_owned())
                            })?,
                        });
                    }
                    done.push(assemble(build, values)?);
                }
            }
        }
        match (done.pop(), done.is_empty()) {
            (Some(value), true) => Ok(value),
            _ => Err(defect("the reader did not end with one value".to_owned())),
        }
    }

    /// Reads one arena value: a leaf becomes a value at once, and a compound
    /// reads its components, releases its own ID, and schedules its children.
    fn visit(
        &mut self,
        types: &TypeTable,
        id: ValueId,
        ty: &Type,
        tasks: &mut Vec<Task>,
        done: &mut Vec<Value>,
    ) -> Result<(), Outcome> {
        let (build, components) = match shape(ty, types)? {
            Shape::Atom => {
                let text = self.characters(&id)?;
                self.release(id)?;
                done.push(Value::Atom(text));
                return Ok(());
            }
            Shape::Str => {
                let text = self.characters(&id)?;
                self.release(id)?;
                done.push(Value::Str(text));
                return Ok(());
            }
            Shape::Bytes => {
                let length = self.length(&id)?;
                let mut bytes = Vec::new();
                for index in 0..length {
                    let byte = self.read_i32(&id, index)?;
                    bytes.push(u8::try_from(byte).map_err(|_| {
                        defect(format!("a byte of {byte} in a `Bytes` value"))
                    })?);
                }
                self.release(id)?;
                done.push(Value::Bytes(bytes));
                return Ok(());
            }
            Shape::Tuple(components) => (Build::Tuple, components),
            Shape::Record(fields) => {
                let (names, types) = fields.into_iter().unzip();
                (Build::Record(names), types)
            }
            Shape::Wrapper(representation) => (Build::Wrapper, vec![representation]),
            Shape::Variant(cases) => {
                let chosen = self.variant(&id)?;
                let (case, fields) = usize::try_from(chosen)
                    .ok()
                    .and_then(|chosen| cases.into_iter().nth(chosen))
                    .ok_or_else(|| {
                        defect(format!("a variant value of the case {chosen}"))
                    })?;
                let (names, types) = fields.into_iter().unzip();
                (Build::Variant { case, names }, types)
            }
            // A list reads each element by its type, and a dict each entry,
            // which the host reads as a two-component tuple, in the order the
            // module stores them: key order.
            Shape::List(element) => {
                let count = self.length(&id)?;
                (Build::List, repeat(&element, count)?)
            }
            Shape::Dict(key, value) => {
                let count = self.length(&id)?;
                (Build::Dict, repeat(&Type::Tuple(vec![key, value]), count)?)
            }
        };
        let mut items = Vec::with_capacity(components.len());
        let mut children = Vec::new();
        for (index, component) in (0_u64..).zip(&components) {
            match self.component(&id, index, component)? {
                Component::Ready(value) => items.push(Some(value)),
                Component::Child(child, ty) => {
                    items.push(None);
                    children.push((child, ty));
                }
            }
        }
        self.release(id)?;
        tasks.push(Task::Build(build, items));
        // The first child is visited first.
        tasks.extend(
            children
                .into_iter()
                .rev()
                .map(|(child, ty)| Task::Visit(child, ty)),
        );
        Ok(())
    }

    /// Reads the component at `index` of the value `id`, by the component's
    /// type: a scalar is read at once, and an arena value becomes a new ID.
    fn component(
        &mut self,
        id: &ValueId,
        index: u64,
        ty: &Type,
    ) -> Result<Component, Outcome> {
        let value = match ty {
            // A `Void` component holds the scalar `0` and carries no value.
            Type::Void => Value::Void,
            Type::I8
            | Type::I16
            | Type::I32
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::Char => {
                let bits = self.read_i32(id, index)?.cast_unsigned();
                narrow(bits, ty)?
            }
            Type::I64 => Value::I64(self.read_i64(id, index)?),
            Type::U64 => Value::U64(self.read_i64(id, index)?.cast_unsigned()),
            Type::F32 => Value::F32(canonical_f32(self.read_f32(id, index)?.to_bits())),
            Type::F64 => Value::F64(canonical_f64(self.read_f64(id, index)?.to_bits())),
            Type::Never => {
                return Err(defect("a component of type `Never`".to_owned()));
            }
            _ => {
                let child = self.read_id(id, index)?;
                return Ok(Component::Child(child, ty.clone()));
            }
        };
        Ok(Component::Ready(value))
    }

    /// The Unicode scalars of a `Str` or an atom.
    fn characters(&mut self, id: &ValueId) -> Result<String, Outcome> {
        let length = self.length(id)?;
        let mut text = String::new();
        for index in 0..length {
            let scalar = self.read_i32(id, index)?.cast_unsigned();
            text.push(char::from_u32(scalar).ok_or_else(|| {
                defect(format!("{scalar:#x} is not a Unicode scalar value"))
            })?);
        }
        Ok(text)
    }
}

/// A node of the value, built from its components in order.
fn assemble(build: Build, values: Vec<Value>) -> Result<Value, Outcome> {
    Ok(match build {
        Build::Tuple => Value::Tuple(values),
        Build::List => Value::List(values),
        Build::Record(names) => Value::Record(names.into_iter().zip(values).collect()),
        Build::Variant { case, names } => Value::Variant {
            case,
            fields: names.into_iter().zip(values).collect(),
        },
        Build::Wrapper => Value::Wrapper(Box::new(
            values
                .into_iter()
                .next()
                .ok_or_else(|| defect("a wrapper lost its value".to_owned()))?,
        )),
        Build::Dict => Value::Dict(
            values
                .into_iter()
                .map(|mut entry| match &mut entry {
                    Value::Tuple(parts) => {
                        let mut parts = std::mem::take(parts).into_iter();
                        match (parts.next(), parts.next(), parts.next()) {
                            (Some(key), Some(value), None) => Ok((key, value)),
                            _ => Err(defect("a dict entry is not a pair".to_owned())),
                        }
                    }
                    _ => Err(defect("a dict entry is not a tuple".to_owned())),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    })
}

/// `count` copies of `ty`: the component types of a collection of that many
/// elements. A count the host cannot hold is a defect, never an allocation.
fn repeat(ty: &Type, count: u64) -> Result<Vec<Type>, Outcome> {
    let count = usize::try_from(count)
        .map_err(|_| defect("a collection longer than the host can read".to_owned()))?;
    Ok(vec![ty.clone(); count])
}

pub(crate) const fn defect(cause: String) -> Outcome {
    Outcome::Defect { cause }
}

const fn canonical_f32(bits: u32) -> u32 {
    if f32::from_bits(bits).is_nan() {
        CANONICAL_NAN_F32
    } else {
        bits
    }
}

const fn canonical_f64(bits: u64) -> u64 {
    if f64::from_bits(bits).is_nan() {
        CANONICAL_NAN_F64
    } else {
        bits
    }
}

/// The value of a type that crosses in an `i32` slot, from its 32 bits: a
/// narrower integer is the value it sign-extends or zero-extends to.
fn narrow(bits: u32, ty: &Type) -> Result<Value, Outcome> {
    let mismatch = || defect(format!("{bits:#x} does not fit the type {ty}"));
    Ok(match ty {
        Type::I8 => {
            Value::I8(i8::try_from(bits.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I16 => {
            Value::I16(i16::try_from(bits.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I32 => Value::I32(bits.cast_signed()),
        Type::U8 => Value::U8(u8::try_from(bits).map_err(|_| mismatch())?),
        Type::U16 => Value::U16(u16::try_from(bits).map_err(|_| mismatch())?),
        Type::U32 => Value::U32(bits),
        Type::Char => Value::Char(char::from_u32(bits).ok_or_else(mismatch)?),
        _ => return Err(mismatch()),
    })
}

/// The scalar a slot holds for a scalar type, `None` for a type whose result
/// is an arena value. A type that crosses in an `i32` slot is read from the
/// slot's low 32 bits.
fn scalar(bits: u64, ty: &Type) -> Result<Option<Value>, Outcome> {
    let low = u32::try_from(bits & 0xFFFF_FFFF).unwrap_or(0);
    Ok(Some(match ty {
        Type::Void => Value::Void,
        Type::I8
        | Type::I16
        | Type::I32
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::Char => narrow(low, ty)?,
        Type::I64 => Value::I64(bits.cast_signed()),
        Type::U64 => Value::U64(bits),
        Type::F32 => Value::F32(canonical_f32(low)),
        Type::F64 => Value::F64(canonical_f64(bits)),
        Type::Never => return Err(defect("a result of type `Never`".to_owned())),
        _ => return Ok(None),
    }))
}
