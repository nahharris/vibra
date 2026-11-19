//! Runtime values, under the runtime chapter's "Representation latitude".
//!
//! `Str` is a Rust string, `Bytes` a byte vector, `Bool` an unboxed flag,
//! `Option` a compact optional pointer, a case without fields a tag, and a
//! user wrapper a box around the value it wraps. A value carries no type:
//! an observation converts it to the neutral [`vibra_ir::Value`] at its
//! static type (`crate::encode`), so no observation depends on the
//! representation.
//!
//! Compound values are shared and immutable. Releasing one never recurses
//! on the host stack: [`Val`]'s `Drop` moves the parts it owns alone onto a
//! worklist.

use std::rc::Rc;

use vibra_ir::{ClosureBody, Type};

use crate::frame::TypeEnv;

/// A runtime value.
#[derive(Clone, Debug, Default)]
pub(crate) struct Val<'a>(pub(crate) Repr<'a>);

/// The representation of a runtime value.
#[derive(Clone, Debug, Default)]
pub(crate) enum Repr<'a> {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    /// By its exact bits.
    F32(u32),
    /// By its exact bits.
    F64(u64),
    Char(char),
    /// A value of the type playing `@bool`.
    Bool(bool),
    #[default]
    Void,
    Atom(&'a str),
    /// A value of the type playing `@str`.
    Str(Rc<str>),
    /// A value of the type playing `@bytes`.
    Bytes(Rc<Vec<u8>>),
    /// A case without fields of a variant type.
    Tag(&'a str),
    /// A value of the type playing `@option`.
    Opt(Option<Rc<Val<'a>>>),
    /// A case with fields of a variant type.
    Case(Rc<CaseVal<'a>>),
    /// A value of a declared wrapper type other than `Str`, `Bytes`, and
    /// `Dict`.
    Wrapper(Rc<Val<'a>>),
    Tuple(Rc<Vec<Val<'a>>>),
    Record(Rc<Vec<(&'a str, Val<'a>)>>),
    List(Rc<Vec<Val<'a>>>),
    /// A value of the type playing `@dict`.
    Dict(Rc<DictVal<'a>>),
    Interface(Rc<Held<'a>>),
    Function(Rc<FunctionVal>),
    Closure(Rc<ClosureVal<'a>>),
}

/// A case with fields.
#[derive(Clone, Debug)]
pub(crate) struct CaseVal<'a> {
    pub(crate) case: &'a str,
    pub(crate) fields: Vec<(&'a str, Val<'a>)>,
}

/// A dict: its closed key type, which orders it, and its entries in that
/// order, one per key.
#[derive(Clone, Debug)]
pub(crate) struct DictVal<'a> {
    pub(crate) key: Type,
    pub(crate) entries: Vec<(Val<'a>, Val<'a>)>,
}

/// An interface value: the concrete value and its closed type, which
/// selects the implementation at every call.
#[derive(Clone, Debug)]
pub(crate) struct Held<'a> {
    pub(crate) held_type: Type,
    pub(crate) value: Val<'a>,
}

/// A module function as a value, at closed type arguments.
#[derive(Clone, Debug)]
pub(crate) struct FunctionVal {
    pub(crate) function: usize,
    pub(crate) type_arguments: Vec<Type>,
}

/// A closure: its body, its captured values, and the type arguments of the
/// activation that created it.
#[derive(Clone, Debug)]
pub(crate) struct ClosureVal<'a> {
    pub(crate) body: &'a ClosureBody,
    pub(crate) captures: Vec<Val<'a>>,
    pub(crate) env: Rc<TypeEnv>,
}

impl Repr<'_> {
    const fn is_compound(&self) -> bool {
        matches!(
            self,
            Self::Opt(Some(_))
                | Self::Wrapper(_)
                | Self::Case(_)
                | Self::Tuple(_)
                | Self::Record(_)
                | Self::List(_)
                | Self::Dict(_)
                | Self::Interface(_)
                | Self::Closure(_)
        )
    }
}

fn take_repr<'a>(value: &mut Val<'a>) -> Repr<'a> {
    std::mem::take(&mut value.0)
}

impl Drop for Val<'_> {
    fn drop(&mut self) {
        if !self.0.is_compound() {
            return;
        }
        let mut pending = vec![take_repr(self)];
        while let Some(repr) = pending.pop() {
            match repr {
                Repr::Opt(Some(inner)) | Repr::Wrapper(inner) => {
                    if let Ok(mut inner) = Rc::try_unwrap(inner) {
                        pending.push(take_repr(&mut inner));
                    }
                }
                Repr::Case(case) => {
                    if let Ok(case) = Rc::try_unwrap(case) {
                        for (_, mut field) in case.fields {
                            pending.push(take_repr(&mut field));
                        }
                    }
                }
                Repr::Tuple(values) | Repr::List(values) => {
                    if let Ok(values) = Rc::try_unwrap(values) {
                        for mut value in values {
                            pending.push(take_repr(&mut value));
                        }
                    }
                }
                Repr::Record(fields) => {
                    if let Ok(fields) = Rc::try_unwrap(fields) {
                        for (_, mut field) in fields {
                            pending.push(take_repr(&mut field));
                        }
                    }
                }
                Repr::Dict(dict) => {
                    if let Ok(dict) = Rc::try_unwrap(dict) {
                        for (mut key, mut value) in dict.entries {
                            pending.push(take_repr(&mut key));
                            pending.push(take_repr(&mut value));
                        }
                    }
                }
                Repr::Interface(held) => {
                    if let Ok(mut held) = Rc::try_unwrap(held) {
                        pending.push(take_repr(&mut held.value));
                    }
                }
                Repr::Closure(closure) => {
                    if let Ok(closure) = Rc::try_unwrap(closure) {
                        for mut capture in closure.captures {
                            pending.push(take_repr(&mut capture));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'a> Val<'a> {
    pub(crate) const fn void() -> Self {
        Self(Repr::Void)
    }

    pub(crate) const fn boolean(value: bool) -> Self {
        Self(Repr::Bool(value))
    }

    pub(crate) const fn tag(name: &'a str) -> Self {
        Self(Repr::Tag(name))
    }

    pub(crate) const fn none() -> Self {
        Self(Repr::Opt(None))
    }

    pub(crate) fn some(value: Self) -> Self {
        Self(Repr::Opt(Some(Rc::new(value))))
    }

    pub(crate) fn case(case: &'a str, fields: Vec<(&'a str, Self)>) -> Self {
        if fields.is_empty() {
            Self::tag(case)
        } else {
            Self(Repr::Case(Rc::new(CaseVal { case, fields })))
        }
    }

    /// `Result.ok(value: value)`.
    pub(crate) fn ok(value: Self) -> Self {
        Self::case("ok", vec![("value", value)])
    }

    /// `Result.err(error: error)`.
    pub(crate) fn err(error: Self) -> Self {
        Self::case("err", vec![("error", error)])
    }

    pub(crate) fn tuple(values: Vec<Self>) -> Self {
        Self(Repr::Tuple(Rc::new(values)))
    }

    pub(crate) fn list(values: Vec<Self>) -> Self {
        Self(Repr::List(Rc::new(values)))
    }

    pub(crate) fn string(text: &str) -> Self {
        Self(Repr::Str(Rc::from(text)))
    }

    pub(crate) fn bytes(bytes: Vec<u8>) -> Self {
        Self(Repr::Bytes(Rc::new(bytes)))
    }

    pub(crate) fn dict(key: Type, entries: Vec<(Self, Self)>) -> Self {
        Self(Repr::Dict(Rc::new(DictVal { key, entries })))
    }

    /// The case name of a value of a variant type.
    pub(crate) fn case_name(&self) -> Option<&str> {
        Some(match &self.0 {
            Repr::Bool(true) => "true",
            Repr::Bool(false) => "false",
            Repr::Opt(None) => "none",
            Repr::Opt(Some(_)) => "some",
            Repr::Tag(name) => name,
            Repr::Case(case) => case.case,
            _ => return None,
        })
    }

    /// The field `name` of a case with fields.
    pub(crate) fn case_field(&self, name: &str) -> Option<Self> {
        match &self.0 {
            Repr::Opt(Some(inner)) if name == "value" => Some(inner.as_ref().clone()),
            Repr::Case(case) => case
                .fields
                .iter()
                .find_map(|(field, value)| (*field == name).then(|| value.clone())),
            _ => None,
        }
    }

    pub(crate) const fn as_bool(&self) -> Option<bool> {
        match self.0 {
            Repr::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) const fn as_u64(&self) -> Option<u64> {
        match self.0 {
            Repr::U64(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) const fn as_u32(&self) -> Option<u32> {
        match self.0 {
            Repr::U32(value) => Some(value),
            _ => None,
        }
    }

    /// The elements of a list.
    pub(crate) fn as_list(&self) -> Option<&Rc<Vec<Self>>> {
        match &self.0 {
            Repr::List(values) => Some(values),
            _ => None,
        }
    }

    /// The values directly inside this one.
    pub(crate) fn for_each_child(&self, mut visit: impl FnMut(&Self)) {
        match &self.0 {
            Repr::Opt(Some(inner)) | Repr::Wrapper(inner) => visit(inner),
            Repr::Case(case) => case.fields.iter().for_each(|(_, value)| visit(value)),
            Repr::Tuple(values) | Repr::List(values) => values.iter().for_each(visit),
            Repr::Record(fields) => fields.iter().for_each(|(_, value)| visit(value)),
            Repr::Dict(dict) => dict.entries.iter().for_each(|(key, value)| {
                visit(key);
                visit(value);
            }),
            Repr::Interface(held) => visit(&held.value),
            Repr::Closure(closure) => closure.captures.iter().for_each(visit),
            _ => {}
        }
    }

    /// The address of the shared storage this value owns, and its size in
    /// bytes, for the memory accounting; `None` for an unshared scalar.
    pub(crate) fn storage(&self) -> Option<(usize, usize)> {
        use std::mem::size_of;
        let value = size_of::<Self>();
        Some(match &self.0 {
            Repr::Str(text) => (text.as_ptr().addr(), text.len()),
            Repr::Bytes(bytes) => (Rc::as_ptr(bytes).addr(), bytes.len()),
            Repr::Opt(Some(inner)) | Repr::Wrapper(inner) => {
                (Rc::as_ptr(inner).addr(), value)
            }
            Repr::Case(case) => (
                Rc::as_ptr(case).addr(),
                size_of::<CaseVal<'_>>()
                    .saturating_add(case.fields.len().saturating_mul(value + 16)),
            ),
            Repr::Tuple(values) | Repr::List(values) => (
                Rc::as_ptr(values).addr(),
                size_of::<Vec<Self>>()
                    .saturating_add(values.len().saturating_mul(value)),
            ),
            Repr::Record(fields) => (
                Rc::as_ptr(fields).addr(),
                size_of::<Vec<Self>>()
                    .saturating_add(fields.len().saturating_mul(value + 16)),
            ),
            Repr::Dict(dict) => (
                Rc::as_ptr(dict).addr(),
                size_of::<DictVal<'_>>()
                    .saturating_add(dict.entries.len().saturating_mul(2 * value)),
            ),
            Repr::Interface(held) => (Rc::as_ptr(held).addr(), size_of::<Held<'_>>()),
            Repr::Function(function) => (
                Rc::as_ptr(function).addr(),
                size_of::<FunctionVal>()
                    .saturating_add(function.type_arguments.len().saturating_mul(32)),
            ),
            Repr::Closure(closure) => (
                Rc::as_ptr(closure).addr(),
                size_of::<ClosureVal<'_>>()
                    .saturating_add(closure.captures.len().saturating_mul(value)),
            ),
            _ => return None,
        })
    }
}

/// Structural sameness: the equality a constant pattern tests. Floats
/// compare by their bits, which no pattern holds.
pub(crate) fn same(left: &Val<'_>, right: &Val<'_>) -> bool {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        let equal = match (&left.0, &right.0) {
            (Repr::I8(a), Repr::I8(b)) => a == b,
            (Repr::I16(a), Repr::I16(b)) => a == b,
            (Repr::I32(a), Repr::I32(b)) => a == b,
            (Repr::I64(a), Repr::I64(b)) => a == b,
            (Repr::U8(a), Repr::U8(b)) => a == b,
            (Repr::U16(a), Repr::U16(b)) => a == b,
            (Repr::U32(a), Repr::U32(b)) => a == b,
            (Repr::U64(a), Repr::U64(b)) => a == b,
            (Repr::F32(a), Repr::F32(b)) => a == b,
            (Repr::F64(a), Repr::F64(b)) => a == b,
            (Repr::Char(a), Repr::Char(b)) => a == b,
            (Repr::Bool(a), Repr::Bool(b)) => a == b,
            (Repr::Void, Repr::Void) => true,
            (Repr::Atom(a), Repr::Atom(b)) | (Repr::Tag(a), Repr::Tag(b)) => a == b,
            (Repr::Str(a), Repr::Str(b)) => a == b,
            (Repr::Bytes(a), Repr::Bytes(b)) => a == b,
            (Repr::Opt(None), Repr::Opt(None)) => true,
            (Repr::Opt(Some(a)), Repr::Opt(Some(b)))
            | (Repr::Wrapper(a), Repr::Wrapper(b)) => {
                pending.push((a, b));
                true
            }
            (Repr::Case(a), Repr::Case(b)) => {
                a.case == b.case
                    && a.fields.len() == b.fields.len()
                    && a.fields.iter().all(|(name, value)| {
                        b.fields.iter().any(|(other, held)| {
                            other == name && {
                                pending.push((value, held));
                                true
                            }
                        })
                    })
            }
            (Repr::Tuple(a), Repr::Tuple(b)) | (Repr::List(a), Repr::List(b)) => {
                a.len() == b.len() && {
                    pending.extend(a.iter().zip(b.iter()));
                    true
                }
            }
            (Repr::Record(a), Repr::Record(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(name, value)| {
                        b.iter().any(|(other, held)| {
                            other == name && {
                                pending.push((value, held));
                                true
                            }
                        })
                    })
            }
            (Repr::Dict(a), Repr::Dict(b)) => {
                a.entries.len() == b.entries.len() && {
                    for ((ak, av), (bk, bv)) in a.entries.iter().zip(&b.entries) {
                        pending.push((ak, bk));
                        pending.push((av, bv));
                    }
                    true
                }
            }
            (Repr::Interface(a), Repr::Interface(b)) => {
                a.held_type == b.held_type && {
                    pending.push((&a.value, &b.value));
                    true
                }
            }
            (Repr::Function(a), Repr::Function(b)) => {
                a.function == b.function && a.type_arguments == b.type_arguments
            }
            (Repr::Closure(a), Repr::Closure(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
        if !equal {
            return false;
        }
    }
    true
}
