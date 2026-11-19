//! Dict order and structural comparison (`docs/spec/02-type-system.md`,
//! "Dict keys, equality, and order").
//!
//! A dict holds one entry per key, sorted by the `Ordered.compare` of its
//! key type. A literal is built by binary insertion in written order: a
//! later duplicate key replaces the earlier entry's value and keeps its
//! place. A lookup is a binary search. Each comparison is an ordinary call
//! of the key type's `compare`, made through the machine, so a user's
//! implementation runs like any other activation.
//!
//! Atoms, atom singletons, and anonymous tuples and records conform to
//! `Equatable` and `Ordered` structurally: atoms by the UTF-8 bytes of their
//! spelling, tuples component-wise, and records component-wise in canonical
//! field order.

use std::cmp::Ordering;
use std::rc::Rc;

use vibra_ir::{Library, Type};

use crate::intrinsics::{order_of, ordering};
use crate::machine::{
    Halt, Kont, Machine, Resolved, Step, Target, invalid, is_structural,
};
use crate::value::{DictVal, Repr, Val};

/// A dict literal being sorted into place.
#[derive(Debug)]
pub(crate) struct DictBuild<'a> {
    key: Type,
    pending: std::vec::IntoIter<(Val<'a>, Val<'a>)>,
    built: Vec<(Val<'a>, Val<'a>)>,
    current: Option<(Val<'a>, Val<'a>)>,
    low: usize,
    high: usize,
}

impl<'a> DictBuild<'a> {
    pub(crate) fn for_each_value(&self, visit: &mut impl FnMut(&Val<'a>)) {
        for (key, value) in self
            .pending
            .as_slice()
            .iter()
            .chain(&self.built)
            .chain(&self.current)
        {
            visit(key);
            visit(value);
        }
    }

    const fn middle(&self) -> usize {
        self.low + (self.high - self.low) / 2
    }
}

/// A binary search for one key.
#[derive(Debug)]
pub(crate) struct DictFind<'a> {
    dict: Rc<DictVal<'a>>,
    key: Val<'a>,
    low: usize,
    high: usize,
}

impl<'a> DictFind<'a> {
    pub(crate) fn for_each_value(&self, visit: &mut impl FnMut(&Val<'a>)) {
        visit(&self.key);
        visit(&Val(Repr::Dict(Rc::clone(&self.dict))));
    }

    const fn middle(&self) -> usize {
        self.low + (self.high - self.low) / 2
    }
}

/// A structural comparison: the component pairs still to compare, the
/// next one last.
#[derive(Debug)]
pub(crate) struct Structural<'a> {
    order: bool,
    pending: Vec<(Type, Val<'a>, Val<'a>)>,
}

impl<'a> Structural<'a> {
    pub(crate) fn for_each_value(&self, visit: &mut impl FnMut(&Val<'a>)) {
        for (_, left, right) in &self.pending {
            visit(left);
            visit(right);
        }
    }

    /// The answer when every component compares equal.
    fn settled<'b>(&self) -> Val<'b> {
        if self.order {
            ordering(Ordering::Equal)
        } else {
            Val::boolean(true)
        }
    }

    /// Whether a component's answer decides the whole comparison.
    fn decides(&self, answer: &Val<'_>) -> Result<bool, Halt> {
        if self.order {
            let order = order_of(answer).ok_or_else(|| {
                Halt::Invalid("`compare` returned no Ordering".to_owned())
            })?;
            Ok(order != Ordering::Equal)
        } else {
            let equal = answer
                .as_bool()
                .ok_or_else(|| Halt::Invalid("`equal` returned no Bool".to_owned()))?;
            Ok(!equal)
        }
    }
}

fn pair<'a, 'v>(
    values: &'v [Val<'a>],
    what: &str,
) -> Result<(&'v Val<'a>, &'v Val<'a>), Halt> {
    match values {
        [left, right] => Ok((left, right)),
        _ => invalid(format!("{what} takes two operands")),
    }
}

impl<'a> Machine<'a> {
    /// The `Ordered.compare` (`order`) or `Equatable.equal` of `ty`.
    fn comparer(&mut self, order: bool, ty: &Type) -> Result<Resolved<'a>, Halt> {
        let (library, member) = if order {
            (Library::Ordered, "compare")
        } else {
            (Library::Equatable, "equal")
        };
        match self.set.library().get(library) {
            Some(interface) => {
                let interface = interface.clone();
                self.dispatch(&interface, member, ty, &[], &[])
            }
            None if is_structural(ty) => Ok(Resolved {
                target: Target::Structural {
                    order,
                    ty: ty.clone(),
                },
                env: Rc::clone(&self.empty_env),
            }),
            None => invalid(format!("no `{}` to compare {ty} with", library.name())),
        }
    }

    /// Compares two keys of `key_type`; the `Ordering` goes to the top
    /// continuation.
    fn compare_keys(
        &mut self,
        key_type: &Type,
        left: Val<'a>,
        right: Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        let resolved = self.comparer(true, key_type)?;
        let origin = self.origin();
        self.invoke(resolved, vec![left, right], false, origin)
    }

    /// Builds a dict of `key` from `entries` in written order.
    pub(crate) fn dict_build(
        &mut self,
        key: Type,
        entries: Vec<(Val<'a>, Val<'a>)>,
    ) -> Result<Step<'a>, Halt> {
        let built = Vec::with_capacity(entries.len());
        self.advance_dict_build(DictBuild {
            key,
            pending: entries.into_iter(),
            built,
            current: None,
            low: 0,
            high: 0,
        })
    }

    fn advance_dict_build(
        &mut self,
        mut build: DictBuild<'a>,
    ) -> Result<Step<'a>, Halt> {
        loop {
            if build.current.is_none() {
                match build.pending.next() {
                    None => {
                        let value = Val::dict(build.key, build.built);
                        self.allocated(&value)?;
                        return Ok(Step::Ret(value));
                    }
                    Some(entry) => {
                        build.current = Some(entry);
                        build.low = 0;
                        build.high = build.built.len();
                    }
                }
            }
            if build.low == build.high {
                if let Some(entry) = build.current.take() {
                    build.built.insert(build.low, entry);
                }
                continue;
            }
            let (Some((left, _)), Some((right, _))) =
                (build.current.as_ref(), build.built.get(build.middle()))
            else {
                return invalid("a dict insertion out of range");
            };
            let (left, right) = (left.clone(), right.clone());
            let key = build.key.clone();
            self.konts.push(Kont::DictBuild(Box::new(build)));
            return self.compare_keys(&key, left, right);
        }
    }

    pub(crate) fn resume_dict_build(
        &mut self,
        mut build: DictBuild<'a>,
        answer: &Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        let middle = build.middle();
        match order_of(answer) {
            Some(Ordering::Less) => build.high = middle,
            Some(Ordering::Greater) => build.low = middle.saturating_add(1),
            Some(Ordering::Equal) => {
                let replaced = build.current.take().map(|(_, value)| value);
                if let (Some(value), Some(entry)) =
                    (replaced, build.built.get_mut(middle))
                {
                    entry.1 = value;
                }
            }
            None => return invalid("`compare` returned no Ordering"),
        }
        self.advance_dict_build(build)
    }

    /// Looks `key` up in `dict`.
    pub(crate) fn dict_find(
        &mut self,
        dict: Rc<DictVal<'a>>,
        key: Val<'a>,
        _key_type: &Type,
    ) -> Result<Step<'a>, Halt> {
        let high = dict.entries.len();
        self.advance_dict_find(DictFind {
            dict,
            key,
            low: 0,
            high,
        })
    }

    fn advance_dict_find(&mut self, find: DictFind<'a>) -> Result<Step<'a>, Halt> {
        if find.low == find.high {
            return Ok(Step::Ret(Val::none()));
        }
        let Some((right, _)) = find.dict.entries.get(find.middle()) else {
            return invalid("a dict search out of range");
        };
        let (left, right) = (find.key.clone(), right.clone());
        let key = find.dict.key.clone();
        self.konts.push(Kont::DictFind(Box::new(find)));
        self.compare_keys(&key, left, right)
    }

    pub(crate) fn resume_dict_find(
        &mut self,
        mut find: DictFind<'a>,
        answer: &Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        let middle = find.middle();
        match order_of(answer) {
            Some(Ordering::Less) => find.high = middle,
            Some(Ordering::Greater) => find.low = middle.saturating_add(1),
            Some(Ordering::Equal) => {
                let value = find
                    .dict
                    .entries
                    .get(middle)
                    .map(|(_, value)| value.clone())
                    .ok_or_else(|| {
                        Halt::Invalid("a dict search out of range".to_owned())
                    })?;
                let found = Val::some(value);
                self.allocated(&found)?;
                return Ok(Step::Ret(found));
            }
            None => return invalid("`compare` returned no Ordering"),
        }
        self.advance_dict_find(find)
    }

    /// The structural `compare` or `equal` of two values of `ty`.
    pub(crate) fn structural(
        &mut self,
        order: bool,
        ty: Type,
        args: Vec<Val<'a>>,
    ) -> Result<Step<'a>, Halt> {
        let (left, right) = pair(&args, "a structural comparison")?;
        let pending = vec![(ty, left.clone(), right.clone())];
        self.advance_structural(Structural { order, pending })
    }

    fn advance_structural(
        &mut self,
        mut state: Structural<'a>,
    ) -> Result<Step<'a>, Halt> {
        while let Some((ty, left, right)) = state.pending.pop() {
            match (&ty, &left.0, &right.0) {
                (Type::Atom | Type::AtomSingleton(_), Repr::Atom(a), Repr::Atom(b)) => {
                    let answer = if state.order {
                        ordering(a.as_bytes().cmp(b.as_bytes()))
                    } else {
                        Val::boolean(a == b)
                    };
                    if state.decides(&answer)? {
                        return Ok(Step::Ret(answer));
                    }
                }
                (Type::Tuple(types), Repr::Tuple(a), Repr::Tuple(b))
                    if types.len() == a.len() && types.len() == b.len() =>
                {
                    for ((ty, a), b) in types.iter().zip(a.iter()).zip(b.iter()).rev() {
                        state.pending.push((ty.clone(), a.clone(), b.clone()));
                    }
                }
                (Type::Record(fields), Repr::Record(a), Repr::Record(b)) => {
                    let field = |values: &[(&'a str, Val<'a>)], name: &str| {
                        values
                            .iter()
                            .find_map(|(field, value)| {
                                (*field == name).then(|| value.clone())
                            })
                            .ok_or_else(|| Halt::Invalid(format!("no field `{name}`")))
                    };
                    for (name, ty) in fields.entries().iter().rev() {
                        state.pending.push((
                            ty.clone(),
                            field(a, name)?,
                            field(b, name)?,
                        ));
                    }
                }
                _ if is_structural(&ty) => {
                    return invalid(format!(
                        "a structural comparison of values that are not {ty}"
                    ));
                }
                _ => {
                    let resolved = self.comparer(state.order, &ty)?;
                    self.konts.push(Kont::Structural(Box::new(state)));
                    let origin = self.origin();
                    return self.invoke(resolved, vec![left, right], false, origin);
                }
            }
        }
        Ok(Step::Ret(state.settled()))
    }

    pub(crate) fn resume_structural(
        &mut self,
        state: Structural<'a>,
        answer: &Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        if state.decides(answer)? {
            return Ok(Step::Ret(answer.clone()));
        }
        self.advance_structural(state)
    }
}
