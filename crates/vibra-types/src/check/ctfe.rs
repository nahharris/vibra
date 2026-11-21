//! Compile-time evaluation.
//!
//! `docs/spec/02-type-system.md`, "Guards with `else`" and "Bare guards": the
//! checker evaluates a certified expression over constant operands through
//! the reference interpreter's [`vibra_interp::evaluate`], under the limits
//! of the published limit profile. An evaluation that stops at a limit has
//! no verdict; `docs/spec/07-diagnostics-and-conformance.md`, "Runtime and
//! tool failures", reports it as `@tool.limitExceeded`.
//!
//! Only a certified expression is ever handed to the interpreter: one the
//! fragment admits, whose every call is of a certified function, and that
//! reads no slot. Anything else is not evaluated.

use std::sync::Arc;

use vibra_interp::{Evaluation, LimitProfile, Reads};
use vibra_ir::{CheckedModuleSet, Node, Type, Value};

use super::contract::fragment::{Facts, Standing, reads_slots, walk};

/// The limit profile compile-time evaluations run under.
pub(crate) const PROFILE: LimitProfile = LimitProfile::M1_DEFAULT;

/// What one compile-time evaluation established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The value.
    Value(Value),
    /// A limit stopped it before it finished.
    LimitReached,
    /// It was not run: the expression is not certified, reads a slot, or
    /// there is no module set to run it over.
    Unevaluated,
}

/// The module set an evaluation runs over, built on first use.
type Build<'e> = Box<dyn FnMut() -> Option<Arc<CheckedModuleSet>> + 'e>;

/// Evaluates certified expressions over one module set.
pub(crate) struct Evaluator<'e> {
    facts: &'e Facts,
    standings: &'e [Standing],
    build: Build<'e>,
    set: Option<Option<Arc<CheckedModuleSet>>>,
    /// The number of expressions handed to the interpreter.
    pub(crate) evaluated: usize,
}

impl std::fmt::Debug for Evaluator<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Evaluator")
            .field("evaluated", &self.evaluated)
            .finish_non_exhaustive()
    }
}

impl<'e> Evaluator<'e> {
    /// An evaluator deciding certification by `facts` and `standings`, whose
    /// entries are the function indices of the module set `build` makes.
    pub(crate) fn new(
        facts: &'e Facts,
        standings: &'e [Standing],
        build: impl FnMut() -> Option<Arc<CheckedModuleSet>> + 'e,
    ) -> Self {
        Self {
            facts,
            standings,
            build: Box::new(build),
            set: None,
            evaluated: 0,
        }
    }

    /// Whether `node` is certified: in the fragment, with every call of a
    /// certified function.
    pub(crate) fn certified(&self, node: &Node) -> bool {
        let found = walk(self.facts, [node], Some);
        found.outside.is_empty()
            && found.calls.iter().all(|(_, entry)| {
                self.standings.get(*entry) == Some(&Standing::Certified)
            })
    }

    /// The module set evaluations run over, built on first use.
    fn set(&mut self) -> Option<Arc<CheckedModuleSet>> {
        match &self.set {
            Some(set) => set.clone(),
            None => {
                let set = (self.build)();
                self.set = Some(set.clone());
                set
            }
        }
    }

    /// The canonical VIBON text of `value`, of type `ty`, over the types of
    /// the module set evaluations run over.
    pub(crate) fn encode(&mut self, value: &Value, ty: &Type) -> Option<String> {
        let set = self.set()?;
        vibra_ir::encode_value(value, ty, set.types()).ok()
    }

    /// Evaluates `node`, a certified expression that reads no slot.
    pub(crate) fn evaluate(&mut self, node: &Node) -> Outcome {
        self.evaluate_reading(node).0
    }

    /// [`Self::evaluate`], with the functions, by entry, and the module
    /// values, by global index, whose checked body or value the evaluation
    /// read.
    pub(crate) fn evaluate_reading(&mut self, node: &Node) -> (Outcome, Reads) {
        if reads_slots(node) || !self.certified(node) {
            return (Outcome::Unevaluated, Reads::default());
        }
        let Some(set) = self.set() else {
            return (Outcome::Unevaluated, Reads::default());
        };
        self.evaluated += 1;
        let (evaluation, reads) =
            vibra_interp::evaluate_reading(&set, &node.expr, &[], PROFILE.compile_time);
        let outcome = match evaluation {
            Evaluation::Value(value) => Outcome::Value(value),
            Evaluation::LimitReached(_) => Outcome::LimitReached,
            Evaluation::Defect(_) => Outcome::Unevaluated,
        };
        (outcome, reads)
    }
}

#[cfg(test)]
mod tests {
    use vibra_diagnostics::ByteSpan;
    use vibra_ir::{Constant, Construction, Expr, SourceOrigin, Type};

    use super::*;
    use crate::check::check_source;

    fn node(expr: Expr) -> Node {
        Node::new(expr, SourceOrigin::new("main.vib", ByteSpan::new(0, 1)))
    }

    fn literal(value: i32) -> Result<Node, String> {
        Constant::new(Type::I32, Value::I32(value))
            .map(|constant| node(Expr::Literal(constant)))
            .map_err(|error| error.to_string())
    }

    #[test]
    fn evaluate_is_never_called_with_a_non_certified_expression() -> Result<(), String>
    {
        let checked =
            check_source("main.vib", "fn main returns r: I32 do return 1i32 endfn\n");
        let set = checked
            .module_set()
            .cloned()
            .ok_or("the program is checked")?;
        let facts = Facts::default();
        let mut built = 0;
        let mut evaluator = Evaluator::new(&facts, &[], || {
            built += 1;
            Some(Arc::clone(&set))
        });
        let outside = [
            node(Expr::Lookup {
                collection: Box::new(literal(1)?),
                key: Box::new(literal(0)?),
            }),
            node(Expr::Try(Box::new(literal(1)?))),
            node(Expr::Variable {
                slot: 0,
                value_type: Type::I32,
            }),
            node(Expr::Call {
                target: vibra_ir::CallTarget::Direct {
                    function: 0,
                    type_arguments: Vec::new(),
                },
                arguments: vibra_ir::Arguments::default(),
            }),
        ];
        for expr in &outside {
            assert_eq!(evaluator.evaluate(expr), Outcome::Unevaluated);
        }
        assert_eq!(evaluator.evaluated, 0);
        let tuple = node(Expr::Construct {
            value_type: Type::Tuple(vec![Type::I32, Type::I32]),
            construction: Construction::Tuple(vec![literal(1)?, literal(2)?]),
        });
        assert_eq!(
            evaluator.evaluate(&tuple),
            Outcome::Value(Value::Tuple(vec![Value::I32(1), Value::I32(2)]))
        );
        assert_eq!(evaluator.evaluated, 1);
        drop(evaluator);
        assert_eq!(built, 1);
        Ok(())
    }
}
