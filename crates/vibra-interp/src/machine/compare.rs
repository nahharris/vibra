//! Ordering dict keys without a Rust call across a language call.
//!
//! A dict orders its keys by canonical order, and a key of a declared type by
//! that type's own `compare` (`docs/spec/02-type-system.md`, "Nominal
//! declarations"). That `compare` is Vibra code, and it may itself build or
//! search dicts keyed by the same type, so the comparison cannot hold a Rust
//! frame open while it runs. It is a continuation instead, like `array.fold`:
//! an explicit stack of pending comparisons suspends when a `compare` has to
//! be called and resumes with its `ordering`.
//!
//! Three operations need a comparison: building a `dict` literal (a binary
//! insertion for each entry), a lookup (a binary search), and the closed
//! `ordered.compare` and `equatable.equal` of a structure holding user keys.

use std::cmp::Ordering;

use vibra_ir::{ClosedContract, Type, TypeId};

use super::{Halt, Kont, Machine, Step};
use crate::closed_contract;
use crate::key_order_canonical;
use crate::present_payload;
use crate::value::{Callable, Entries, RuntimeValue};

/// One pending piece of a comparison.
enum Work {
    /// Two values still to compare.
    Pair(RuntimeValue, RuntimeValue),
    /// Components compared in order: the first that differs decides, and when
    /// every one is equal the lengths do.
    Sequence {
        left: Vec<RuntimeValue>,
        right: Vec<RuntimeValue>,
        next: usize,
        tail: Ordering,
    },
}

/// The comparison of two values, suspended whenever a user's `compare` runs.
struct Engine {
    key_order: Option<TypeId>,
    work: Vec<Work>,
}

enum Outcome {
    Done(Ordering),
    Call(Callable, Vec<RuntimeValue>, Type),
}

/// What a comparison is for, and the state of the operation around it.
enum Purpose {
    /// The closed `compare` or `equal` of a structure.
    Closed {
        closed: ClosedContract,
        result: Type,
    },
    /// Building a dict: each pending entry is inserted into `ordered` by a
    /// binary search over `low..high`; a repeated key keeps its first
    /// position and takes the later value.
    Build {
        pairs: std::vec::IntoIter<(RuntimeValue, RuntimeValue)>,
        ordered: Vec<(RuntimeValue, RuntimeValue)>,
        entry: Option<(RuntimeValue, RuntimeValue)>,
        low: usize,
        high: usize,
        value_type: Type,
    },
    /// A binary search for `key`.
    Lookup {
        entries: Entries,
        key: RuntimeValue,
        low: usize,
        high: usize,
        value_type: Type,
    },
}

/// The continuation of a comparison that is waiting for a `compare` to return.
pub(super) struct CompareKont {
    engine: Engine,
    purpose: Purpose,
}

impl CompareKont {
    /// Adds the storage the values held here occupy to a measurement.
    pub(super) fn live_bytes(
        &self,
        seen: &mut std::collections::HashSet<usize>,
        live: &mut usize,
    ) {
        let mut add = |value: &RuntimeValue| value.live_bytes(seen, live);
        for work in &self.engine.work {
            match work {
                Work::Pair(left, right) => {
                    add(left);
                    add(right);
                }
                Work::Sequence { left, right, .. } => {
                    left.iter().for_each(&mut add);
                    right.iter().for_each(&mut add);
                }
            }
        }
        match &self.purpose {
            Purpose::Closed { .. } => {}
            Purpose::Build {
                pairs,
                ordered,
                entry,
                ..
            } => {
                for (key, value) in pairs.as_slice().iter().chain(ordered).chain(entry)
                {
                    add(key);
                    add(value);
                }
            }
            Purpose::Lookup { entries, key, .. } => {
                add(key);
                for (existing, value) in entries.iter() {
                    add(existing);
                    add(value);
                }
            }
        }
    }
}

/// The next thing an operation asks of the comparison.
enum Next<'a> {
    Compare(RuntimeValue, RuntimeValue),
    Finished(Step<'a>),
}

impl<'a> Machine<'a> {
    /// `ordered.compare` or `equatable.equal` of two values, through
    /// `key_order`.
    pub(super) fn start_closed(
        &mut self,
        left: RuntimeValue,
        right: RuntimeValue,
        key_order: Option<TypeId>,
        closed: ClosedContract,
        result: Type,
    ) -> Result<Step<'a>, Halt> {
        let state = Box::new(CompareKont {
            engine: Engine {
                key_order,
                work: Vec::new(),
            },
            purpose: Purpose::Closed { closed, result },
        });
        self.advance(state, Some(Next::Compare(left, right)), None)
    }

    /// A `dict` of `pairs`, inserted in canonical key order.
    pub(super) fn start_build(
        &mut self,
        pairs: Vec<(RuntimeValue, RuntimeValue)>,
        key_order: Option<TypeId>,
        value_type: Type,
    ) -> Result<Step<'a>, Halt> {
        let capacity = pairs.len();
        let mut state = Box::new(CompareKont {
            engine: Engine {
                key_order,
                work: Vec::new(),
            },
            purpose: Purpose::Build {
                pairs: pairs.into_iter(),
                ordered: Vec::with_capacity(capacity),
                entry: None,
                low: 0,
                high: 0,
                value_type,
            },
        });
        let first = self.next_step(&mut state.purpose)?;
        self.advance(state, Some(first), None)
    }

    /// A lookup of `key` in the sorted `entries`.
    pub(super) fn start_lookup(
        &mut self,
        entries: Entries,
        key: RuntimeValue,
        key_order: Option<TypeId>,
        value_type: Type,
    ) -> Result<Step<'a>, Halt> {
        let high = entries.len();
        let mut state = Box::new(CompareKont {
            engine: Engine {
                key_order,
                work: Vec::new(),
            },
            purpose: Purpose::Lookup {
                entries,
                key,
                low: 0,
                high,
                value_type,
            },
        });
        let first = self.next_step(&mut state.purpose)?;
        self.advance(state, Some(first), None)
    }

    /// A user's `compare` returned `answer`.
    pub(super) fn resume_compare(
        &mut self,
        state: Box<CompareKont>,
        answer: RuntimeValue,
    ) -> Result<Step<'a>, Halt> {
        let RuntimeValue::Enum { variant, .. } = &answer else {
            return Err(Halt::Invalid);
        };
        let order = match variant.as_str() {
            "less" => Ordering::Less,
            "equal" => Ordering::Equal,
            "greater" => Ordering::Greater,
            _ => return Err(Halt::Invalid),
        };
        self.advance(state, None, Some(order))
    }

    /// Runs the operation until it finishes or has to call a `compare`, in
    /// which case it suspends as a continuation.
    fn advance(
        &mut self,
        mut state: Box<CompareKont>,
        mut next: Option<Next<'a>>,
        mut incoming: Option<Ordering>,
    ) -> Result<Step<'a>, Halt> {
        loop {
            if let Some(step) = next.take() {
                match step {
                    Next::Finished(step) => return Ok(step),
                    Next::Compare(left, right) => {
                        state.engine.work.push(Work::Pair(left, right));
                    }
                }
            }
            match self.engine_advance(&mut state.engine, incoming.take())? {
                Outcome::Call(callable, values, result) => {
                    self.push(Kont::Compare(state))?;
                    return self.invoke(callable, values, &result);
                }
                Outcome::Done(order) => {
                    next = Some(self.order_known(&mut state.purpose, order)?);
                }
            }
        }
    }

    /// The comparison the operation asked for has an answer.
    fn order_known(
        &mut self,
        purpose: &mut Purpose,
        order: Ordering,
    ) -> Result<Next<'a>, Halt> {
        match &mut *purpose {
            Purpose::Closed { closed, result } => {
                let answer = closed_contract(*closed, order, result);
                Ok(Next::Finished(self.fresh(answer)?))
            }
            Purpose::Build {
                ordered,
                entry,
                low,
                high,
                ..
            } => {
                let middle = *low + (*high - *low) / 2;
                match order {
                    Ordering::Less => *low = middle + 1,
                    Ordering::Greater => *high = middle,
                    // The later pair replaces the earlier one.
                    Ordering::Equal => {
                        if let (Some(slot), Some(pair)) =
                            (ordered.get_mut(middle), entry.take())
                        {
                            *slot = pair;
                        }
                    }
                }
                self.next_step(purpose)
            }
            Purpose::Lookup {
                entries, low, high, ..
            } => {
                let middle = *low + (*high - *low) / 2;
                match order {
                    Ordering::Less => *low = middle + 1,
                    Ordering::Greater => *high = middle,
                    Ordering::Equal => {
                        let found = entries.get(middle).map(|(_, value)| value.clone());
                        return Ok(Next::Finished(self.lookup_result(purpose, found)?));
                    }
                }
                self.next_step(purpose)
            }
        }
    }

    /// The next comparison a dict operation needs, or its finished value.
    fn next_step(&mut self, purpose: &mut Purpose) -> Result<Next<'a>, Halt> {
        match &mut *purpose {
            Purpose::Closed { .. } => Err(Halt::Invalid),
            Purpose::Build {
                pairs,
                ordered,
                entry,
                low,
                high,
                value_type,
            } => loop {
                if entry.is_none() {
                    let Some(pair) = pairs.next() else {
                        let dict = RuntimeValue::Dict {
                            value_type: value_type.clone(),
                            entries: std::mem::take(ordered).into(),
                        };
                        return Ok(Next::Finished(self.fresh(dict)?));
                    };
                    *entry = Some(pair);
                    *low = 0;
                    *high = ordered.len();
                }
                if *low < *high {
                    let middle = *low + (*high - *low) / 2;
                    let (Some((existing, _)), Some((key, _))) =
                        (ordered.get(middle), &*entry)
                    else {
                        return Err(Halt::Invalid);
                    };
                    return Ok(Next::Compare(existing.clone(), key.clone()));
                }
                if let Some(pair) = entry.take() {
                    ordered.insert(*low, pair);
                }
            },
            Purpose::Lookup {
                entries,
                key,
                low,
                high,
                ..
            } => {
                if *low < *high {
                    let middle = *low + (*high - *low) / 2;
                    let (existing, _) = entries.get(middle).ok_or(Halt::Invalid)?;
                    return Ok(Next::Compare(existing.clone(), key.clone()));
                }
                Ok(Next::Finished(self.lookup_result(purpose, None)?))
            }
        }
    }

    /// The `option` a lookup answers with.
    fn lookup_result(
        &mut self,
        purpose: &Purpose,
        found: Option<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Purpose::Lookup { value_type, .. } = purpose else {
            return Err(Halt::Invalid);
        };
        let entry = RuntimeValue::Enum {
            value_type: value_type.clone(),
            variant: if found.is_some() { "some" } else { "none" }.to_owned(),
            payload: found.and_then(present_payload),
        };
        self.fresh(entry)
    }

    /// Advances the pending comparisons until one is decided or a `compare`
    /// has to be called. `incoming` is the answer to the comparison that was
    /// pending when the engine last suspended.
    fn engine_advance(
        &self,
        engine: &mut Engine,
        incoming: Option<Ordering>,
    ) -> Result<Outcome, Halt> {
        let mut pending = incoming;
        loop {
            if let Some(order) = pending.take() {
                // A decided comparison goes to the sequence waiting on it.
                match engine.work.last_mut() {
                    None => return Ok(Outcome::Done(order)),
                    Some(Work::Sequence {
                        left,
                        right,
                        next,
                        tail,
                    }) => {
                        if order.is_ne() {
                            engine.work.pop();
                            pending = Some(order);
                        } else if *next < left.len().min(right.len()) {
                            let at = *next;
                            *next += 1;
                            let (Some(left), Some(right)) =
                                (left.get(at), right.get(at))
                            else {
                                return Err(Halt::Invalid);
                            };
                            let pair = Work::Pair(left.clone(), right.clone());
                            engine.work.push(pair);
                        } else {
                            let decided = *tail;
                            engine.work.pop();
                            pending = Some(decided);
                        }
                    }
                    Some(Work::Pair(..)) => return Err(Halt::Invalid),
                }
                continue;
            }
            let Some(Work::Pair(left, right)) = engine.work.pop() else {
                return Err(Halt::Invalid);
            };
            let Some(interface) = engine.key_order.as_ref() else {
                pending = Some(key_order_canonical(&left, &right));
                continue;
            };
            if let Some(function) = self.key_compare_function(interface, &left) {
                let result = self
                    .program
                    .functions()
                    .get(function)
                    .ok_or(Halt::Invalid)?
                    .signature()
                    .result();
                let callable = self
                    .implementation_callable(function, &left)
                    .ok_or(Halt::Invalid)?;
                return Ok(Outcome::Call(callable, vec![left, right], result));
            }
            match (&left, &right) {
                (
                    RuntimeValue::Tuple { values: left, .. },
                    RuntimeValue::Tuple { values: right, .. },
                ) => {
                    pending = sequence(engine, left.to_vec(), right.to_vec());
                }
                (
                    RuntimeValue::Record { fields: left, .. },
                    RuntimeValue::Record { fields: right, .. },
                ) => {
                    pending = sequence(
                        engine,
                        left.iter().map(|(_, value)| value.clone()).collect(),
                        right.iter().map(|(_, value)| value.clone()).collect(),
                    );
                }
                (
                    RuntimeValue::Enum {
                        variant: left_variant,
                        payload: left_payload,
                        ..
                    },
                    RuntimeValue::Enum {
                        variant: right_variant,
                        payload: right_payload,
                        ..
                    },
                ) => match left_variant.as_bytes().cmp(right_variant.as_bytes()) {
                    Ordering::Equal => match (left_payload, right_payload) {
                        (Some(left), Some(right)) => engine.work.push(Work::Pair(
                            RuntimeValue::clone(left),
                            RuntimeValue::clone(right),
                        )),
                        _ => pending = Some(Ordering::Equal),
                    },
                    order => pending = Some(order),
                },
                (
                    RuntimeValue::Union {
                        member: left_member,
                        value: left,
                        ..
                    },
                    RuntimeValue::Union {
                        member: right_member,
                        value: right,
                        ..
                    },
                ) => match left_member.cmp(right_member) {
                    Ordering::Equal => engine.work.push(Work::Pair(
                        RuntimeValue::clone(left),
                        RuntimeValue::clone(right),
                    )),
                    order => pending = Some(order),
                },
                _ => pending = Some(key_order_canonical(&left, &right)),
            }
        }
    }
}

/// Starts comparing two sequences of components. The lengths decide when
/// every common component is equal, so an empty common prefix is decided
/// at once.
fn sequence(
    engine: &mut Engine,
    left: Vec<RuntimeValue>,
    right: Vec<RuntimeValue>,
) -> Option<Ordering> {
    let tail = left.len().cmp(&right.len());
    let (Some(first_left), Some(first_right)) = (left.first(), right.first()) else {
        return Some(tail);
    };
    let first = (first_left.clone(), first_right.clone());
    engine.work.push(Work::Sequence {
        left,
        right,
        next: 1,
        tail,
    });
    engine.work.push(Work::Pair(first.0, first.1));
    None
}
