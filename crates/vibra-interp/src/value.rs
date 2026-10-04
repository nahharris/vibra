//! The interpreter's runtime values.
//!
//! A value may be nested arbitrarily deep: the language bounds its depth only
//! by memory (`docs/spec/06-runtime.md`, "The value arena" and
//! "Reclamation"). Two properties follow from that and shape this module:
//!
//! - Values are immutable, so every compound value shares its parts. Reading
//!   a binding copies a handle and not the structure beneath it, which keeps
//!   a read as cheap for a value a hundred thousand levels deep as for a
//!   scalar.
//! - Releasing a value walks an explicit worklist and never recurses on the
//!   host stack, whatever the nesting.

use std::collections::{BTreeMap, HashSet};
use std::mem::size_of;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use vibra_ir::{FunctionSignature, Type, Value};

/// The type arguments of one activation: each generic parameter the running
/// function names, at the type this call instantiated it to.
///
/// Generics are not erased at run time. A value built in generic code carries
/// its instantiated type, and a contract call selects its implementation from
/// instantiated types (`docs/spec/06-runtime.md`, "Generic instantiation").
pub(crate) type TypeMap = BTreeMap<String, Type>;

/// A `lambda` body registered with the machine that runs it.
///
/// A closure value is not observed and never leaves the machine that made it,
/// so it names its body by an index into that machine's table. The body stays
/// in the checked program, which outlives every activation that runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LambdaId(pub(crate) usize);

/// What a [`Shared`] edge points at: a value or the elements of one.
pub(crate) trait Nested {
    /// Moves the compound values held directly in this one into `out`,
    /// leaving nothing nested beneath it.
    fn detach(&mut self, out: &mut Vec<RuntimeValue>);

    /// The storage this part holds directly, in bytes: its elements, not the
    /// storage of the values in them. The count follows lengths rather than
    /// capacities so it does not depend on how a vector grew.
    fn size(&self) -> usize;

    /// Pushes the values held directly in this part.
    fn push_children<'v>(&'v self, out: &mut Vec<&'v RuntimeValue>);
}

/// A shared, immutable part of a value.
///
/// Clones share the part. When the last owner drops, the compound values it
/// held are released through a worklist, so a chain of them is released with
/// bounded stack.
pub(crate) struct Shared<T: Nested>(Rc<T>);

impl<T: Nested> Shared<T> {
    pub(crate) fn new(part: T) -> Self {
        Self(Rc::new(part))
    }

    /// The address that identifies this part among the others of a run.
    fn address(&self) -> usize {
        Rc::as_ptr(&self.0).addr()
    }

    /// Hands the compound values this part alone holds to `out`. A part
    /// another value also holds is left to that value.
    fn detach_into(&mut self, out: &mut Vec<RuntimeValue>) {
        if let Some(part) = Rc::get_mut(&mut self.0) {
            part.detach(out);
        }
    }
}

impl<T: Nested> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: Nested> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Nested + PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl<T: Nested + Eq> Eq for Shared<T> {}

impl<T: Nested + std::fmt::Debug> std::fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<T: Nested> Drop for Shared<T> {
    /// Releases the part with bounded host stack. The last owner hands the
    /// compound values it holds to a worklist and then releases each of them
    /// after handing over its own, so no release is more than one level deep.
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.detach_into(&mut pending);
        while let Some(mut value) = pending.pop() {
            value.detach_children(&mut pending);
        }
    }
}

fn compound(value: RuntimeValue) -> Option<RuntimeValue> {
    (!matches!(value, RuntimeValue::Primitive(_))).then_some(value)
}

impl Nested for RuntimeValue {
    fn detach(&mut self, out: &mut Vec<RuntimeValue>) {
        out.extend(compound(self.take()));
    }

    fn size(&self) -> usize {
        size_of::<RuntimeValue>()
    }

    fn push_children<'v>(&'v self, out: &mut Vec<&'v RuntimeValue>) {
        out.push(self);
    }
}

impl Nested for Vec<RuntimeValue> {
    fn detach(&mut self, out: &mut Vec<RuntimeValue>) {
        out.extend(std::mem::take(self).into_iter().filter_map(compound));
    }

    fn size(&self) -> usize {
        self.len().saturating_mul(size_of::<RuntimeValue>())
    }

    fn push_children<'v>(&'v self, out: &mut Vec<&'v RuntimeValue>) {
        out.extend(self);
    }
}

impl Nested for Vec<(String, RuntimeValue)> {
    fn detach(&mut self, out: &mut Vec<RuntimeValue>) {
        out.extend(
            std::mem::take(self)
                .into_iter()
                .filter_map(|(_, value)| compound(value)),
        );
    }

    fn size(&self) -> usize {
        let names: usize = self.iter().map(|(name, _)| name.len()).sum();
        self.len()
            .saturating_mul(size_of::<(String, RuntimeValue)>())
            .saturating_add(names)
    }

    fn push_children<'v>(&'v self, out: &mut Vec<&'v RuntimeValue>) {
        out.extend(self.iter().map(|(_, value)| value));
    }
}

impl Nested for Vec<(RuntimeValue, RuntimeValue)> {
    fn detach(&mut self, out: &mut Vec<RuntimeValue>) {
        for (key, value) in std::mem::take(self) {
            out.extend(compound(key));
            out.extend(compound(value));
        }
    }

    fn size(&self) -> usize {
        self.len()
            .saturating_mul(size_of::<(RuntimeValue, RuntimeValue)>())
    }

    fn push_children<'v>(&'v self, out: &mut Vec<&'v RuntimeValue>) {
        for (key, value) in self {
            out.push(key);
            out.push(value);
        }
    }
}

/// A value's elements.
pub(crate) type Elements = Shared<Vec<RuntimeValue>>;
/// A record's fields, in the order of the record type.
pub(crate) type Fields = Shared<Vec<(String, RuntimeValue)>>;
/// A dict's entries, in canonical key order.
pub(crate) type Entries = Shared<Vec<(RuntimeValue, RuntimeValue)>>;
/// The one value inside another.
pub(crate) type Inner = Shared<RuntimeValue>;

impl From<Vec<RuntimeValue>> for Elements {
    fn from(values: Vec<RuntimeValue>) -> Self {
        Self::new(values)
    }
}

impl From<Vec<(String, RuntimeValue)>> for Fields {
    fn from(fields: Vec<(String, RuntimeValue)>) -> Self {
        Self::new(fields)
    }
}

impl From<Vec<(RuntimeValue, RuntimeValue)>> for Entries {
    fn from(entries: Vec<(RuntimeValue, RuntimeValue)>) -> Self {
        Self::new(entries)
    }
}

impl From<RuntimeValue> for Inner {
    fn from(value: RuntimeValue) -> Self {
        Self::new(value)
    }
}

impl Inner {
    /// The value inside, taken when this is its only owner and copied
    /// otherwise.
    pub(crate) fn into_value(mut self) -> RuntimeValue {
        match Rc::get_mut(&mut self.0) {
            Some(value) => value.take(),
            None => RuntimeValue::clone(&self.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeValue {
    Primitive(Value),
    Function(Callable),
    /// A record; fields are in type order (declaration order when declared,
    /// canonical order when anonymous), whatever the evaluation order.
    Record {
        value_type: Type,
        fields: Fields,
    },
    Enum {
        value_type: Type,
        variant: String,
        payload: Option<Inner>,
    },
    Wrapper {
        value_type: Type,
        value: Inner,
    },
    Tuple {
        value_type: Type,
        values: Elements,
    },
    Array {
        value_type: Type,
        values: Elements,
    },
    /// Entries in canonical key order, so no host hash order is reachable.
    Dict {
        value_type: Type,
        entries: Entries,
    },
    /// A union value: its discriminant, the member type, and the member
    /// value.
    Union {
        value_type: Type,
        member: usize,
        member_type: Rc<Type>,
        value: Inner,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Callable {
    Named {
        index: usize,
        /// Shared with every other value of the same function, so making a
        /// callable never copies the signature.
        signature: Arc<FunctionSignature>,
        captures: Elements,
        /// The type arguments fixed so far: where the value was made, and
        /// then by the call that invokes it.
        types: Arc<TypeMap>,
    },
    Lambda {
        signature: Arc<FunctionSignature>,
        /// Activation slots, validated by checked IR to cover the body.
        slot_count: usize,
        /// The body, in the machine's table of closure bodies.
        body: LambdaId,
        captures: Elements,
        /// The type arguments of the activation that made the closure, and
        /// then its own from the call that invokes it.
        types: Arc<TypeMap>,
    },
}

impl Callable {
    pub(crate) fn signature(&self) -> &FunctionSignature {
        match self {
            Self::Named { signature, .. } | Self::Lambda { signature, .. } => signature,
        }
    }

    pub(crate) fn types_mut(&mut self) -> &mut Arc<TypeMap> {
        match self {
            Self::Named { types, .. } | Self::Lambda { types, .. } => types,
        }
    }

    pub(crate) fn captures(&self) -> &Elements {
        match self {
            Self::Named { captures, .. } | Self::Lambda { captures, .. } => captures,
        }
    }

    fn captures_mut(&mut self) -> &mut Elements {
        match self {
            Self::Named { captures, .. } | Self::Lambda { captures, .. } => captures,
        }
    }
}

impl RuntimeValue {
    /// Moves this value out, leaving the `void` value in its place.
    pub(crate) fn take(&mut self) -> Self {
        std::mem::replace(self, Self::Primitive(Value::Void))
    }

    /// The callable this value holds, when it holds one.
    pub(crate) fn into_callable(self) -> Option<Callable> {
        match self {
            Self::Function(callable) => Some(callable),
            _ => None,
        }
    }

    /// Hands the compound values this one alone holds to `out`, so that it
    /// can be released without recursion.
    fn detach_children(&mut self, out: &mut Vec<Self>) {
        match self {
            Self::Primitive(_) => {}
            Self::Function(callable) => callable.captures_mut().detach_into(out),
            Self::Record { fields, .. } => fields.detach_into(out),
            Self::Enum { payload, .. } => {
                if let Some(payload) = payload {
                    payload.detach_into(out);
                }
            }
            Self::Wrapper { value, .. } | Self::Union { value, .. } => {
                value.detach_into(out);
            }
            Self::Tuple { values, .. } | Self::Array { values, .. } => {
                values.detach_into(out);
            }
            Self::Dict { entries, .. } => entries.detach_into(out),
        }
    }

    /// The storage this value owns directly, in bytes, as a value just built:
    /// the elements, boxes, and text that hang off this node and not those of
    /// the values nested in it. A scalar owns none.
    pub(crate) fn built_bytes(&self) -> usize {
        match self {
            Self::Primitive(Value::Str(text)) => text.len(),
            Self::Primitive(Value::Bytes(data)) => data.len(),
            Self::Primitive(_) => 0,
            Self::Function(callable) => callable.captures().size(),
            Self::Record { fields, .. } => fields.size(),
            Self::Enum {
                variant, payload, ..
            } => variant
                .len()
                .saturating_add(payload.as_ref().map_or(0, |payload| payload.size())),
            Self::Wrapper { value, .. } | Self::Union { value, .. } => value.size(),
            Self::Tuple { values, .. } | Self::Array { values, .. } => values.size(),
            Self::Dict { entries, .. } => entries.size(),
        }
    }

    /// Adds the storage this value and everything nested in it holds to
    /// `bytes`, counting each shared part once across all the values `seen`
    /// has visited. The walk uses an explicit worklist.
    pub(crate) fn live_bytes(&self, seen: &mut HashSet<usize>, bytes: &mut usize) {
        fn part<'v, T: Nested>(
            shared: &'v Shared<T>,
            seen: &mut HashSet<usize>,
            bytes: &mut usize,
            out: &mut Vec<&'v RuntimeValue>,
        ) {
            if seen.insert(shared.address()) {
                *bytes = bytes.saturating_add(shared.size());
                shared.push_children(out);
            }
        }
        let mut pending = vec![self];
        while let Some(value) = pending.pop() {
            match value {
                Self::Primitive(Value::Str(text)) => {
                    *bytes = bytes.saturating_add(text.len());
                }
                Self::Primitive(Value::Bytes(data)) => {
                    *bytes = bytes.saturating_add(data.len());
                }
                Self::Primitive(_) => {}
                Self::Function(callable) => {
                    part(callable.captures(), seen, bytes, &mut pending);
                }
                Self::Record { fields, .. } => part(fields, seen, bytes, &mut pending),
                Self::Enum {
                    variant, payload, ..
                } => {
                    *bytes = bytes.saturating_add(variant.len());
                    if let Some(payload) = payload {
                        part(payload, seen, bytes, &mut pending);
                    }
                }
                Self::Wrapper { value, .. } | Self::Union { value, .. } => {
                    part(value, seen, bytes, &mut pending);
                }
                Self::Tuple { values, .. } | Self::Array { values, .. } => {
                    part(values, seen, bytes, &mut pending);
                }
                Self::Dict { entries, .. } => part(entries, seen, bytes, &mut pending),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thread whose stack is far too small for a recursive drop of the
    /// nesting below, so a recursion would end the process.
    fn on_small_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(body)
            .expect("thread")
            .join()
            .expect("the drop must not overflow the stack");
    }

    fn nested(
        depth: usize,
        wrap: impl Fn(RuntimeValue) -> RuntimeValue,
    ) -> RuntimeValue {
        let mut value = RuntimeValue::Primitive(Value::U8(1));
        for _ in 0..depth {
            value = wrap(value);
        }
        value
    }

    #[test]
    fn every_nesting_shape_releases_with_bounded_stack() {
        const DEPTH: usize = 300_000;
        on_small_stack(|| {
            let ty = || Type::Void;
            drop(nested(DEPTH, |inner| RuntimeValue::Wrapper {
                value_type: ty(),
                value: inner.into(),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Enum {
                value_type: ty(),
                variant: "some".to_owned(),
                payload: Some(inner.into()),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Union {
                value_type: ty(),
                member: 0,
                member_type: Rc::new(ty()),
                value: inner.into(),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Array {
                value_type: ty(),
                values: vec![inner].into(),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Tuple {
                value_type: ty(),
                values: vec![inner, RuntimeValue::Primitive(Value::Void)].into(),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Record {
                value_type: ty(),
                fields: vec![("prev".to_owned(), inner)].into(),
            }));
            drop(nested(DEPTH, |inner| RuntimeValue::Dict {
                value_type: ty(),
                entries: vec![(RuntimeValue::Primitive(Value::U8(0)), inner)].into(),
            }));
            drop(nested(DEPTH, |inner| {
                RuntimeValue::Function(Callable::Lambda {
                    signature: Arc::new(FunctionSignature::new(Vec::new(), Type::Void)),
                    slot_count: 0,
                    body: LambdaId(0),
                    captures: vec![inner].into(),
                    types: Arc::default(),
                })
            }));
        });
    }

    #[test]
    fn a_mixed_deep_value_releases_with_bounded_stack() {
        on_small_stack(|| {
            let mut value = RuntimeValue::Primitive(Value::Str("leaf".to_owned()));
            for level in 0..300_000 {
                value = match level % 3 {
                    0 => RuntimeValue::Record {
                        value_type: Type::Void,
                        fields: vec![("a".to_owned(), value)].into(),
                    },
                    1 => RuntimeValue::Array {
                        value_type: Type::Void,
                        values: vec![value].into(),
                    },
                    _ => RuntimeValue::Wrapper {
                        value_type: Type::Void,
                        value: value.into(),
                    },
                };
            }
            drop(value);
        });
    }

    #[test]
    fn a_shared_part_outlives_the_value_that_dropped_first() {
        on_small_stack(|| {
            let deep = nested(200_000, |inner| RuntimeValue::Array {
                value_type: Type::Void,
                values: vec![inner].into(),
            });
            let second = deep.clone();
            drop(deep);
            // The second owner still holds the whole chain.
            let mut seen = HashSet::new();
            let mut bytes = 0;
            second.live_bytes(&mut seen, &mut bytes);
            assert_eq!(bytes, 200_000 * size_of::<RuntimeValue>());
            drop(second);
        });
    }

    #[test]
    fn built_bytes_count_each_node_once_and_live_bytes_count_shared_parts_once() {
        let leaf = RuntimeValue::Primitive(Value::Str("abcd".to_owned()));
        assert_eq!(leaf.built_bytes(), 4);
        assert_eq!(RuntimeValue::Primitive(Value::U64(9)).built_bytes(), 0);
        let array = RuntimeValue::Array {
            value_type: Type::Void,
            values: vec![leaf.clone(), leaf.clone(), leaf].into(),
        };
        assert_eq!(array.built_bytes(), 3 * size_of::<RuntimeValue>());
        let mut seen = HashSet::new();
        let mut bytes = 0;
        array.live_bytes(&mut seen, &mut bytes);
        assert_eq!(bytes, 3 * size_of::<RuntimeValue>() + 12);

        // A second handle to the same array adds nothing.
        let copy = array.clone();
        copy.live_bytes(&mut seen, &mut bytes);
        assert_eq!(bytes, 3 * size_of::<RuntimeValue>() + 12);
    }

    #[test]
    fn cloning_a_compound_value_shares_it() {
        let array = RuntimeValue::Array {
            value_type: Type::Void,
            values: vec![RuntimeValue::Primitive(Value::U8(0)); 10].into(),
        };
        let copy = array.clone();
        let (
            RuntimeValue::Array { values: left, .. },
            RuntimeValue::Array { values: right, .. },
        ) = (&array, &copy)
        else {
            panic!("arrays");
        };
        assert!(Rc::ptr_eq(&left.0, &right.0));
        assert_eq!(copy, array);
    }
}
