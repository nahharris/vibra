//! The host-side canonical result observation.
//!
//! A run that completes leaves its result in the slot `vibra_v1_result`
//! reports. The host reads the value through the accessors, by the entry's
//! result type, and builds the [`ObservedValue`] whose canonical encoding the
//! reference interpreter produces for the same program, so the harness compares
//! the two backends byte for byte. No ID, offset, or index appears in the
//! value: the host holds an ID only while it reads, and releases it.
//!
//! The reader is a table over the type of the value, and a later step adds a row
//! for each kind it lowers. A type the reader has no row for is a defect of the
//! toolchain: the emitter lowered a program the host cannot observe.
//!
//! A value may be nested to any depth, since the language bounds depth only by
//! memory, so the reader keeps its own worklist and never recurses: a value
//! is read as a stack of tasks, a node's components are read at once and its
//! children are visited in order, and each node is built when its children
//! have been. The host holds at most the IDs of the nodes whose children it has
//! not visited yet.

use std::collections::BTreeMap;

use vibra_ir::{ObservedValue, Type, TypeBody, TypeDefinition, TypeId, Value};

use crate::{Instance, Outcome, ResultSlot, Runner, RunnerError, Started, ValueId};

/// The live arena size in bytes at the three moments a test cares about.
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
        /// The result, ready for its canonical encoding.
        value: ObservedValue,
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
    /// the program, which name the components of a declared type.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a v1 module or lacks an export.
    pub fn run_observed(
        &self,
        bytes: &[u8],
        result: &Type,
        types: &[TypeDefinition],
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
        let (slot, with_result) = match instance.call_entry()? {
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
        Ok(Observed::Completed {
            value,
            live: LiveSizes {
                start,
                with_result,
                end,
            },
        })
    }
}

/// What a node's component is once read: a value that needed no more reading,
/// or a reference to a child that is visited next.
enum Component {
    Ready(ObservedValue),
    Child(ValueId, Type),
}

/// What a node becomes once its children are built.
enum Build {
    Record {
        type_id: Option<TypeId>,
        names: Vec<String>,
    },
    Enum {
        type_id: Option<TypeId>,
        variant: String,
        payload: bool,
    },
    Wrapper(TypeId),
    Tuple(Option<TypeId>),
    Union {
        type_id: Option<TypeId>,
        member: Type,
    },
}

enum Task {
    Visit(ValueId, Type),
    /// A node whose components are `items`, where `None` is a child that is
    /// built, in order, by the visits before this task.
    Build(Build, Vec<Option<ObservedValue>>),
}

/// The components of a compound type, in the order the layout stores them.
enum Shape {
    Record(Vec<(String, Type)>),
    Enum(Vec<(String, Type)>),
    Wrapper(Type),
    Tuple(Vec<Type>),
    Union(Vec<Type>),
}

/// The declared types of a program by identity.
struct Declared<'a> {
    definitions: BTreeMap<&'a TypeId, &'a TypeDefinition>,
}

impl<'a> Declared<'a> {
    fn shape(&self, ty: &Type) -> Option<Shape> {
        let body = match ty {
            Type::Declared(id) => self.definitions.get(id)?.instantiate(&[])?,
            Type::Applied(id, arguments) => {
                self.definitions.get(id)?.instantiate(arguments)?
            }
            Type::Record(fields) => return Some(Shape::Record(fields.clone())),
            Type::Enum(variants) => return Some(Shape::Enum(variants.clone())),
            Type::Tuple(components) => return Some(Shape::Tuple(components.clone())),
            Type::Union(members) => return Some(Shape::Union(members.clone())),
            _ => return None,
        };
        Some(match body {
            TypeBody::Record(fields) => Shape::Record(fields),
            TypeBody::Enum(variants) => Shape::Enum(variants),
            TypeBody::Wrapper(representation) => Shape::Wrapper(representation),
            TypeBody::Tuple(components) => Shape::Tuple(components),
            TypeBody::Union(members) => Shape::Union(members),
        })
    }
}

/// The identity a declared or applied type carries into an encoding.
fn declared_id(ty: &Type) -> Option<TypeId> {
    match ty {
        Type::Declared(id) | Type::Applied(id, _) => Some(id.clone()),
        _ => None,
    }
}

impl Instance {
    /// Reads the result a completed entry left in `slot` as a value of type
    /// `result`, and releases the ID the slot held when the result is an arena
    /// value.
    ///
    /// # Errors
    ///
    /// The [`Outcome`] of a stop while reading, or a defect when the slot does
    /// not fit the type or the host has no reader for the type.
    pub fn observe(
        &mut self,
        slot: ResultSlot,
        result: &Type,
        types: &[TypeDefinition],
    ) -> Result<ObservedValue, Outcome> {
        if let Some(value) = scalar(slot, result)? {
            return Ok(ObservedValue::Primitive(value));
        }
        let declared = Declared {
            definitions: types
                .iter()
                .map(|definition| (definition.id(), definition))
                .collect(),
        };
        let root = self.result_id(slot);
        self.read_tree(&declared, root, result.clone())
    }

    /// Reads the arena value `root` of type `ty` and everything below it, and
    /// releases every ID it holds, the root's included.
    fn read_tree(
        &mut self,
        declared: &Declared<'_>,
        root: ValueId,
        ty: Type,
    ) -> Result<ObservedValue, Outcome> {
        let mut tasks = vec![Task::Visit(root, ty)];
        let mut done: Vec<ObservedValue> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(id, ty) => self.visit(declared, id, &ty, &mut tasks, &mut done)?,
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
        declared: &Declared<'_>,
        id: ValueId,
        ty: &Type,
        tasks: &mut Vec<Task>,
        done: &mut Vec<ObservedValue>,
    ) -> Result<(), Outcome> {
        let leaf = match ty {
            Type::Bool => Some(match self.variant(&id)? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                other => {
                    return Err(defect(format!("a `bool` with the variant {other}")));
                }
            }),
            Type::Str => Some(Value::Str(self.characters(&id)?)),
            Type::Atom | Type::AtomSingleton(_) => {
                Some(Value::Atom(self.characters(&id)?))
            }
            Type::Bytes => {
                let length = self.length(&id)?;
                let mut bytes = Vec::new();
                for index in 0..length {
                    let byte = self.read_i32(&id, index)?;
                    let byte = u8::try_from(byte).map_err(|_| {
                        defect(format!("a byte of {byte} in a `bytes` value"))
                    })?;
                    bytes.push(byte);
                }
                Some(Value::Bytes(bytes))
            }
            _ => None,
        };
        if let Some(value) = leaf {
            self.release(id)?;
            done.push(ObservedValue::Primitive(value));
            return Ok(());
        }
        let shape = declared
            .shape(ty)
            .ok_or_else(|| defect(format!("the host has no reader for the type {ty}")))?;
        let type_id = declared_id(ty);
        let (build, components) = match shape {
            Shape::Record(fields) => {
                let names = fields.iter().map(|(name, _)| name.clone()).collect();
                let types = fields.into_iter().map(|(_, ty)| ty).collect();
                (Build::Record { type_id, names }, types)
            }
            Shape::Tuple(components) => (Build::Tuple(type_id), components),
            Shape::Wrapper(representation) => (
                Build::Wrapper(type_id.ok_or_else(|| {
                    defect("a wrapper with no declaration".to_owned())
                })?),
                vec![representation],
            ),
            Shape::Enum(variants) => {
                let chosen = self.variant(&id)?;
                let (name, payload) = usize::try_from(chosen)
                    .ok()
                    .and_then(|chosen| variants.get(chosen))
                    .ok_or_else(|| defect(format!("an enum with the variant {chosen}")))?;
                // A `void` payload slot has no payload.
                let types = if *payload == Type::Void {
                    Vec::new()
                } else {
                    vec![payload.clone()]
                };
                (
                    Build::Enum {
                        type_id,
                        variant: name.clone(),
                        payload: !types.is_empty(),
                    },
                    types,
                )
            }
            Shape::Union(members) => {
                let chosen = self.variant(&id)?;
                let member = usize::try_from(chosen)
                    .ok()
                    .and_then(|chosen| members.get(chosen))
                    .ok_or_else(|| defect(format!("a union with the member {chosen}")))?
                    .clone();
                (
                    Build::Union {
                        type_id,
                        member: member.clone(),
                    },
                    vec![member],
                )
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
            // A `void` component holds the scalar `0` and carries no value.
            Type::Void => Value::Void,
            Type::Char => {
                let scalar = self.read_i32(id, index)?.cast_unsigned();
                Value::Char(char::from_u32(scalar).ok_or_else(|| {
                    defect(format!("{scalar:#x} is not a Unicode scalar value"))
                })?)
            }
            Type::I8 => {
                let value = self.read_i32(id, index)?;
                Value::I8(i8::try_from(value).map_err(|_| narrow_defect(ty, value))?)
            }
            Type::I16 => {
                let value = self.read_i32(id, index)?;
                Value::I16(i16::try_from(value).map_err(|_| narrow_defect(ty, value))?)
            }
            Type::I32 => Value::I32(self.read_i32(id, index)?),
            Type::U8 => {
                let value = self.read_i32(id, index)?;
                Value::U8(u8::try_from(value).map_err(|_| narrow_defect(ty, value))?)
            }
            Type::U16 => {
                let value = self.read_i32(id, index)?;
                Value::U16(u16::try_from(value).map_err(|_| narrow_defect(ty, value))?)
            }
            Type::U32 => Value::U32(self.read_i32(id, index)?.cast_unsigned()),
            Type::I64 => Value::I64(self.read_i64(id, index)?),
            Type::U64 => Value::U64(self.read_i64(id, index)?.cast_unsigned()),
            Type::F32 => Value::F32(self.read_f32(id, index)?.to_bits()),
            Type::F64 => Value::F64(self.read_f64(id, index)?.to_bits()),
            _ => {
                let child = self.read_id(id, index)?;
                return Ok(Component::Child(child, ty.clone()));
            }
        };
        Ok(Component::Ready(ObservedValue::Primitive(value)))
    }

    /// The Unicode scalars of a `str` or an atom.
    fn characters(&mut self, id: &ValueId) -> Result<String, Outcome> {
        let length = self.length(id)?;
        let mut text = String::new();
        for index in 0..length {
            let scalar = self.read_i32(id, index)?.cast_unsigned();
            let character = char::from_u32(scalar).ok_or_else(|| {
                defect(format!("{scalar:#x} is not a Unicode scalar value"))
            })?;
            text.push(character);
        }
        Ok(text)
    }
}

/// A node of the value, built from its components in order.
fn assemble(build: Build, values: Vec<ObservedValue>) -> Result<ObservedValue, Outcome> {
    let mut values = values.into_iter();
    Ok(match build {
        Build::Record { type_id, names } => ObservedValue::Record {
            type_id,
            fields: names.into_iter().zip(values).collect(),
        },
        Build::Enum {
            type_id,
            variant,
            payload,
        } => ObservedValue::Enum {
            type_id,
            variant,
            payload: if payload {
                Some(Box::new(values.next().ok_or_else(|| {
                    defect("an enum lost its payload".to_owned())
                })?))
            } else {
                None
            },
        },
        Build::Wrapper(type_id) => ObservedValue::Wrapper {
            type_id,
            value: Box::new(
                values
                    .next()
                    .ok_or_else(|| defect("a wrapper lost its value".to_owned()))?,
            ),
        },
        Build::Tuple(type_id) => ObservedValue::Tuple {
            type_id,
            values: values.collect(),
        },
        Build::Union { type_id, member } => ObservedValue::Union {
            type_id,
            member: Box::new(member),
            value: Box::new(
                values
                    .next()
                    .ok_or_else(|| defect("a union lost its member".to_owned()))?,
            ),
        },
    })
}

fn defect(cause: String) -> Outcome {
    Outcome::Defect { cause }
}

fn narrow_defect(ty: &Type, value: i32) -> Outcome {
    defect(format!("{value} does not fit a component of the type {ty}"))
}

/// The scalar a slot holds for a scalar type, `None` for a type whose result is
/// an arena value.
fn scalar(slot: ResultSlot, ty: &Type) -> Result<Option<Value>, Outcome> {
    let bits = slot.0;
    let mismatch = || defect(format!("the result slot does not fit the type {ty}"));
    // A type that crosses in an `i32` slot has no bit above the 32nd.
    let narrow = || u32::try_from(bits).map_err(|_| mismatch());
    Ok(Some(match ty {
        Type::Void => {
            if bits != 0 {
                return Err(mismatch());
            }
            Value::Void
        }
        Type::Char => Value::Char(char::from_u32(narrow()?).ok_or_else(mismatch)?),
        Type::I8 => {
            Value::I8(i8::try_from(narrow()?.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I16 => {
            Value::I16(i16::try_from(narrow()?.cast_signed()).map_err(|_| mismatch())?)
        }
        Type::I32 => Value::I32(narrow()?.cast_signed()),
        Type::U8 => Value::U8(u8::try_from(narrow()?).map_err(|_| mismatch())?),
        Type::U16 => Value::U16(u16::try_from(narrow()?).map_err(|_| mismatch())?),
        Type::U32 => Value::U32(narrow()?),
        Type::I64 => Value::I64(bits.cast_signed()),
        Type::U64 => Value::U64(bits),
        Type::F32 => Value::F32(narrow()?),
        Type::F64 => Value::F64(bits),
        _ => return Ok(None),
    }))
}
