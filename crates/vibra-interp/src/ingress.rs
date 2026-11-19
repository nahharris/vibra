//! Neutral values coming into a run (`docs/spec/02-type-system.md`, "Type
//! invariants" and "Dict keys, equality, and order";
//! `docs/spec/06-runtime.md`, "Traps").
//!
//! A constant of the checked IR and an input of an evaluation are neutral
//! values. Converting one gives a runtime value of the right shape. Before
//! the value is used, every invariant its type carries is applied through the
//! machine, as construction applies it, and every dict in it is checked to
//! hold its keys in ascending order, one entry per key, under its key type's
//! `Ordered.compare`, the comparator dict construction uses. The held type of
//! an interface value must widen to the interface value's type, and a
//! function value must have the type it stands at. A failing input is
//! `@runtime.invalidHostValue`; a failing constant is impossible typed IR,
//! `@runtime.invalidCheckedProgram`.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

use vibra_ir::{CheckedModuleSet, Library, Type, TypeBody};

use crate::encode::{Visited, to_visit};
use crate::frame::TypeEnv;
use crate::intrinsics::order_of;
use crate::machine::{Halt, Kont, Machine, Resolved, Step, Target};
use crate::value::{Repr, Val};

/// One check that needs the machine.
#[derive(Debug)]
pub(crate) enum Check<'a> {
    /// An invariant's predicate, which must answer `true`.
    Invariant {
        predicate: usize,
        arguments: Vec<Type>,
        operand: Val<'a>,
    },
    /// Two adjacent dict keys, the left one less than the right one.
    Ascending {
        key: Type,
        left: Val<'a>,
        right: Val<'a>,
    },
}

/// What follows once every check has passed.
#[derive(Debug)]
pub(crate) enum Then<'a> {
    /// The admitted value goes to the top continuation.
    Ret(Val<'a>),
    /// The admitted input fills its slot and goes to the top continuation.
    Bind(usize, Val<'a>),
    /// The run goes on with a step that waited for the checks.
    Continue(Step<'a>),
}

/// The answer a check waits for.
#[derive(Clone, Copy, Debug)]
enum Expect {
    True,
    Less,
}

/// The checks of values coming in, run one after another.
#[derive(Debug)]
pub(crate) struct Ingress<'a> {
    checks: Vec<Check<'a>>,
    waiting: Option<Expect>,
    host: bool,
    then: Then<'a>,
}

impl<'a> Ingress<'a> {
    pub(crate) fn for_each_value(&self, visit: &mut impl FnMut(&Val<'a>)) {
        for check in &self.checks {
            match check {
                Check::Invariant { operand, .. } => visit(operand),
                Check::Ascending { left, right, .. } => {
                    visit(left);
                    visit(right);
                }
            }
        }
        match &self.then {
            Then::Ret(value)
            | Then::Bind(_, value)
            | Then::Continue(Step::Ret(value)) => {
                visit(value);
            }
            Then::Continue(Step::Eval(_) | Step::Root(..)) => {}
        }
    }
}

/// The halt of a value that is not one of its type, by where it came from.
pub(crate) fn refused(host: bool, message: String) -> Halt {
    if host {
        Halt::HostValue(message)
    } else {
        Halt::Invalid(message)
    }
}

/// Whether a value of `ty` can hold anything ingress checks: an invariant,
/// a dict, an interface value, or a function value.
pub(crate) fn needs_checks(set: &CheckedModuleSet, ty: &Type) -> bool {
    let library = set.library();
    let mut seen = HashSet::new();
    let mut pending = vec![ty.clone()];
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty.clone()) {
            continue;
        }
        match &ty {
            Type::Any | Type::Interface(..) | Type::Fn(_) | Type::Proc(_) => {
                return true;
            }
            Type::Tuple(_) | Type::Record(_) | Type::List(_) => {
                pending.extend(ty.components().into_iter().cloned());
            }
            Type::Declared(id, arguments) => match library.library_of(id) {
                Some(Library::Dict) => return true,
                Some(Library::Str | Library::Bytes | Library::Bool) => {}
                _ => {
                    let Some(definition) = set.types().definition(id) else {
                        continue;
                    };
                    if definition.invariant().is_some() {
                        return true;
                    }
                    let components: Vec<&Type> = match definition.body() {
                        TypeBody::Record(fields) => {
                            fields.iter().map(|(_, ty)| ty).collect()
                        }
                        TypeBody::Tuple(values) => values.iter().collect(),
                        TypeBody::Wrapper(value) => vec![value],
                        TypeBody::Variant(cases) => cases
                            .iter()
                            .flat_map(|case| case.fields.iter().map(|(_, ty)| ty))
                            .collect(),
                    };
                    pending.extend(
                        components
                            .into_iter()
                            .map(|component| component.instantiate(arguments)),
                    );
                }
            },
            _ => {}
        }
    }
    false
}

/// The checks `value`, of the closed type `ty`, must pass, after the checks
/// that need no machine have passed.
pub(crate) fn checks_of<'a>(
    set: &CheckedModuleSet,
    value: &Val<'a>,
    ty: &Type,
    host: bool,
) -> Result<Vec<Check<'a>>, Halt> {
    let refuse = |message: String| refused(host, message);
    let mut checks = Vec::new();
    let mut pending: Vec<(Val<'a>, Rc<Type>)> =
        vec![(value.clone(), Rc::new(ty.clone()))];
    while let Some((value, ty)) = pending.pop() {
        if !needs_checks(set, &ty) {
            continue;
        }
        match (&*ty, &value.0) {
            (Type::Any | Type::Interface(..), Repr::Interface(held)) => {
                let widens = set
                    .widens(&held.held_type, &ty)
                    .map_err(|error| refuse(format!("an interface value: {error}")))?;
                if !widens {
                    return Err(refuse(format!(
                        "an interface value of {ty} holding a {}",
                        held.held_type
                    )));
                }
            }
            (Type::Fn(_) | Type::Proc(_), Repr::Function(function)) => {
                let actual = set
                    .function_value_type(function.function, &function.type_arguments)
                    .map_err(|error| refuse(format!("a function value: {error}")))?;
                if actual != *ty {
                    return Err(refuse(format!(
                        "a function value of {actual} where {ty} is expected"
                    )));
                }
            }
            (Type::Declared(id, arguments), _) => {
                if let Repr::Dict(dict) = &value.0 {
                    for pair in dict.entries.windows(2) {
                        if let [(left, _), (right, _)] = pair {
                            checks.push(Check::Ascending {
                                key: dict.key.clone(),
                                left: left.clone(),
                                right: right.clone(),
                            });
                        }
                    }
                }
                if let Some(definition) = set.types().definition(id)
                    && let Some(invariant) = definition.invariant()
                {
                    let operand = match (&value.0, definition.body()) {
                        (Repr::Wrapper(inner), TypeBody::Wrapper(_)) => {
                            Val::clone(inner)
                        }
                        _ => value.clone(),
                    };
                    checks.push(Check::Invariant {
                        predicate: invariant.predicate,
                        arguments: arguments.clone(),
                        operand,
                    });
                }
            }
            _ => {}
        }
        if let Visited::Node(_, children) =
            to_visit(set, &value, &ty).map_err(|halt| match halt {
                Halt::Invalid(message) => refuse(message),
                other => other,
            })?
        {
            pending.extend(children.into_iter().map(|(child, ty)| (child.clone(), ty)));
        }
    }
    Ok(checks)
}

impl<'a> Machine<'a> {
    /// Runs `checks`, then `then`; a failing check halts as `host` says.
    pub(crate) fn admit(
        &mut self,
        checks: Vec<Check<'a>>,
        host: bool,
        then: Then<'a>,
    ) -> Result<Step<'a>, Halt> {
        self.advance_ingress(Ingress {
            checks,
            waiting: None,
            host,
            then,
        })
    }

    fn advance_ingress(&mut self, mut ingress: Ingress<'a>) -> Result<Step<'a>, Halt> {
        let Some(check) = ingress.checks.pop() else {
            return match ingress.then {
                Then::Ret(value) => Ok(Step::Ret(value)),
                Then::Bind(slot, value) => {
                    self.bind_slot(slot, value.clone())?;
                    Ok(Step::Ret(value))
                }
                Then::Continue(step) => Ok(step),
            };
        };
        let origin = self.origin();
        match check {
            Check::Invariant {
                predicate,
                arguments,
                operand,
            } => {
                ingress.waiting = Some(Expect::True);
                self.konts.push(Kont::Ingress(Box::new(ingress)));
                let resolved = Resolved {
                    target: Target::Function(predicate),
                    env: Rc::new(TypeEnv::of(arguments)),
                };
                self.invoke(resolved, vec![operand], false, origin)
            }
            Check::Ascending { key, left, right } => {
                ingress.waiting = Some(Expect::Less);
                self.konts.push(Kont::Ingress(Box::new(ingress)));
                self.compare_keys(&key, left, right)
            }
        }
    }

    pub(crate) fn resume_ingress(
        &mut self,
        ingress: Ingress<'a>,
        answer: &Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        let holds = match ingress.waiting {
            Some(Expect::True) => answer.as_bool().ok_or_else(|| {
                Halt::Invalid("an invariant's predicate returned no Bool".to_owned())
            })?,
            Some(Expect::Less) => {
                order_of(answer).ok_or_else(|| {
                    Halt::Invalid("`compare` returned no Ordering".to_owned())
                })? == Ordering::Less
            }
            None => {
                return Err(Halt::Invalid(
                    "an ingress check with no question".to_owned(),
                ));
            }
        };
        if !holds {
            let message = match ingress.waiting {
                Some(Expect::Less) => {
                    "a dict whose keys are not ascending and distinct"
                }
                _ => "a value whose type's invariant does not hold",
            };
            return Err(refused(ingress.host, message.to_owned()));
        }
        self.advance_ingress(ingress)
    }
}
