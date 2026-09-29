//! Generic instantiation and the one bound-agnostic unifier (M3 Step 3).
//!
//! `docs/spec/02-type-system.md` infers a complete type-argument list from
//! written operand and result types, checks a `types:` list for agreement, and
//! decides every overlap rule by bound-agnostic unification. This module is
//! the single unifier: call-site inference uses it here, and union-member,
//! implementation-target, and conversion-source overlap use it in later steps.
//!
//! An instantiation renames the callee's generic parameters to fresh
//! variables, spelled `?index:name`. Source names cannot start with `?`, so a
//! variable never collides with a rigid parameter of the enclosing declaration
//! that happens to share its spelling.

use std::collections::BTreeMap;

use vibra_ir::{FunctionSignature, LabelledParameter, Type};

/// Whether a parameter name denotes an inference variable.
fn is_variable(name: &str) -> bool {
    name.starts_with('?')
}

/// One instantiation of a generic entity at one call site.
#[derive(Clone, Debug, Default)]
pub(crate) struct Instantiation {
    /// Original parameter names, in `where:` order, with their variables.
    variables: Vec<(String, String)>,
    bindings: BTreeMap<String, Type>,
}

impl Instantiation {
    /// Fresh variables for `parameters`, in their complete `where:` order.
    pub(crate) fn new(parameters: &[String]) -> Self {
        Self {
            variables: parameters
                .iter()
                .enumerate()
                .map(|(index, name)| (name.clone(), format!("?{index}:{name}")))
                .collect(),
            bindings: BTreeMap::new(),
        }
    }

    /// Whether the instantiated entity is generic at all.
    pub(crate) fn is_generic(&self) -> bool {
        !self.variables.is_empty()
    }

    /// The number of generic parameters.
    pub(crate) fn len(&self) -> usize {
        self.variables.len()
    }

    /// `value` with the entity's parameters replaced by their variables.
    pub(crate) fn open(&self, value: &Type) -> Type {
        value.substitute(&self.renaming())
    }

    /// `signature` with the entity's parameters replaced by their variables.
    pub(crate) fn open_signature(
        &self,
        signature: &FunctionSignature,
    ) -> FunctionSignature {
        signature.substitute(&self.renaming())
    }

    fn renaming(&self) -> BTreeMap<String, Type> {
        self.variables
            .iter()
            .map(|(name, variable)| (name.clone(), Type::Param(variable.clone())))
            .collect()
    }

    /// Seeds the variables from a written `types:` list, in `where:` order.
    pub(crate) fn seed(&mut self, arguments: &[Type]) {
        for ((_, variable), argument) in self.variables.iter().zip(arguments) {
            self.bindings.insert(variable.clone(), argument.clone());
        }
    }

    /// `value` with every bound variable replaced.
    pub(crate) fn apply(&self, value: &Type) -> Type {
        value.substitute(&self.bindings)
    }

    /// `value` with every variable bound, or `None` while one is still open.
    pub(crate) fn resolved(&self, value: &Type) -> Option<Type> {
        let applied = self.apply(value);
        (!has_variables(&applied)).then_some(applied)
    }

    /// Unifies an opened `pattern` with a checked `actual` type, binding
    /// variables on either side. Returns `false` and leaves earlier bindings
    /// in place when the two cannot be the same type.
    pub(crate) fn unify(&mut self, pattern: &Type, actual: &Type) -> bool {
        unify(pattern, actual, &mut self.bindings)
    }

    /// Original names of the parameters still unbound.
    pub(crate) fn unbound(&self) -> Vec<String> {
        self.variables
            .iter()
            .filter(|(_, variable)| {
                self.bindings
                    .get(variable)
                    .is_none_or(|value| has_variables(&self.apply(value)))
            })
            .map(|(name, _)| name.clone())
            .collect()
    }
}
/// Whether `value` still mentions an inference variable.
pub(crate) fn has_variables(value: &Type) -> bool {
    match value {
        Type::Param(name) => is_variable(name),
        Type::Applied(_, arguments) => arguments.iter().any(has_variables),
        Type::Record(members) | Type::Enum(members) => {
            members.iter().any(|(_, member)| has_variables(member))
        }
        Type::Function(signature) => {
            signature.parameters().iter().any(has_variables)
                || signature
                    .labelled()
                    .iter()
                    .any(|parameter| has_variables(&parameter.value_type()))
                || has_variables(&signature.result())
        }
        _ => false,
    }
}

/// The bound-agnostic unifier: whether some binding of the variables in
/// `left` and `right` makes them the same fully resolved type. Rigid
/// parameters unify only with themselves, and no bound is consulted.
pub(crate) fn unify(
    left: &Type,
    right: &Type,
    bindings: &mut BTreeMap<String, Type>,
) -> bool {
    let left = left.substitute(bindings);
    let right = right.substitute(bindings);
    match (&left, &right) {
        (Type::Param(name), other) | (other, Type::Param(name))
            if is_variable(name) =>
        {
            if let Type::Param(other_name) = other
                && other_name == name
            {
                return true;
            }
            if occurs(name, other) {
                return false;
            }
            bindings.insert(name.clone(), other.clone());
            true
        }
        (
            Type::Applied(left_id, left_arguments),
            Type::Applied(right_id, right_arguments),
        ) => {
            left_id == right_id
                && left_arguments.len() == right_arguments.len()
                && left_arguments
                    .iter()
                    .zip(right_arguments)
                    .all(|(left, right)| unify(left, right, bindings))
        }
        (Type::Record(left_members), Type::Record(right_members))
        | (Type::Enum(left_members), Type::Enum(right_members)) => {
            left_members.len() == right_members.len()
                && left_members.iter().zip(right_members).all(|(left, right)| {
                    left.0 == right.0 && unify(&left.1, &right.1, bindings)
                })
        }
        (Type::Function(left_signature), Type::Function(right_signature)) => {
            unify_signatures(left_signature, right_signature, bindings)
        }
        _ => left.same_shape(&right),
    }
}

fn unify_signatures(
    left: &FunctionSignature,
    right: &FunctionSignature,
    bindings: &mut BTreeMap<String, Type>,
) -> bool {
    left.parameters().len() == right.parameters().len()
        && left.labelled().len() == right.labelled().len()
        && left
            .parameters()
            .iter()
            .zip(right.parameters())
            .all(|(left, right)| unify(left, right, bindings))
        && left.labelled().iter().zip(right.labelled()).all(
            |(left, right): (&LabelledParameter, &LabelledParameter)| {
                left.name() == right.name()
                    && unify(&left.value_type(), &right.value_type(), bindings)
            },
        )
        && unify(&left.result(), &right.result(), bindings)
}

fn occurs(variable: &str, value: &Type) -> bool {
    match value {
        Type::Param(name) => name == variable,
        Type::Applied(_, arguments) => {
            arguments.iter().any(|value| occurs(variable, value))
        }
        Type::Record(members) | Type::Enum(members) => {
            members.iter().any(|(_, member)| occurs(variable, member))
        }
        Type::Function(signature) => {
            signature
                .parameters()
                .iter()
                .any(|value| occurs(variable, value))
                || signature
                    .labelled()
                    .iter()
                    .any(|parameter| occurs(variable, &parameter.value_type()))
                || occurs(variable, &signature.result())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Instantiation, unify};
    use std::collections::BTreeMap;
    use vibra_ir::{Type, TypeId};

    fn boxed(argument: Type) -> Type {
        Type::Applied(TypeId::new("box", "box"), vec![argument])
    }

    fn var(name: &str) -> Type {
        Type::Param(format!("?0:{name}"))
    }

    #[test]
    fn a_variable_binds_to_a_concrete_type() {
        let mut bindings = BTreeMap::new();
        assert!(unify(&boxed(var("t")), &boxed(Type::I32), &mut bindings));
        assert_eq!(bindings.get("?0:t"), Some(&Type::I32));
    }

    #[test]
    fn distinct_concrete_types_do_not_unify() {
        let mut bindings = BTreeMap::new();
        assert!(!unify(&boxed(Type::I32), &boxed(Type::Str), &mut bindings));
    }

    #[test]
    fn a_rigid_parameter_unifies_only_with_itself() {
        let mut bindings = BTreeMap::new();
        let rigid = Type::Param("t".to_owned());
        assert!(unify(&rigid, &rigid, &mut bindings));
        assert!(!unify(&rigid, &Type::I32, &mut bindings));
        assert!(unify(&var("u"), &rigid, &mut bindings));
    }

    #[test]
    fn unification_is_two_sided_and_bound_agnostic() {
        // `(box t)` and `(box i32)` overlap at `t` = `i32` whatever t's bound.
        let mut bindings = BTreeMap::new();
        assert!(unify(&boxed(Type::I32), &boxed(var("t")), &mut bindings));
    }

    #[test]
    fn the_occurs_check_rejects_an_infinite_type() {
        let mut bindings = BTreeMap::new();
        assert!(!unify(&var("t"), &boxed(var("t")), &mut bindings));
    }

    #[test]
    fn an_instantiation_reports_its_unbound_parameters() {
        let mut instantiation = Instantiation::new(&["a".to_owned(), "b".to_owned()]);
        let opened = instantiation.open(&Type::Param("a".to_owned()));
        assert!(instantiation.unify(&opened, &Type::Bool));
        assert_eq!(instantiation.unbound(), vec!["b".to_owned()]);
        assert_eq!(instantiation.resolved(&opened), Some(Type::Bool));
    }
}
