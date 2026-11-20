//! Guards and invariants as expressions.
//!
//! `docs/spec/02-type-system.md`, "Guards" and "Type invariants". A
//! `requires` predicate sees the parameters; an `ensures` predicate also
//! sees the result name, bound to the candidate. A predicate is a `Bool`
//! and a consequence has the callable's result type: a predicate of another
//! type is `@contract.invalidPredicate` and a consequence of another type
//! `@contract.invalidConsequence`. Nothing leaves a callable from a guard.
//!
//! A `where` operand is an expression like any other function value, of
//! type `Fn({R} Bool)` over the representation `R`: an anonymous `fn`, or
//! the name of a `fn`, which resolves in the flat module scope. Its `else`
//! consequence is a constant expression whose static type is the error type
//! of the constructor's `Result`.

use std::ops::Range;

use vibra_syntax::source::{Guard as GuardView, Where};

use super::super::contract::fragment::{operands, reads_any};
use super::super::contract::{CheckedGuard, CheckedInvariant, Guards, Predicate};
use super::*;

/// The guards a callable declaration writes, and its result name.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GuardViews<'a> {
    /// The `requires`.
    pub(crate) requires: Option<GuardView<'a>>,
    /// The `ensures`.
    pub(crate) ensures: Option<GuardView<'a>>,
    /// The result name.
    pub(crate) result: Option<Ident<'a>>,
}

impl<'a> GuardViews<'a> {
    /// The guards of `view`.
    pub(crate) fn of(view: &Callable<'a>) -> Self {
        Self {
            requires: view.requires(),
            ensures: view.ensures(),
            result: view.result().map(|(name, _)| name),
        }
    }
}

/// What checking a `where` found.
#[derive(Debug, Default)]
pub(crate) struct InvariantCheck {
    /// The static type of the `else` consequence, when it has one.
    pub(crate) error: Option<Type>,
    /// The checked invariant, when it has no fault.
    pub(crate) checked: Option<CheckedInvariant>,
}

/// Whether `node` constructs a value of a type whose `where` has an `else`,
/// which makes it no constant expression.
pub(crate) fn fallible_construction(program: &Program<'_>, node: &Node) -> bool {
    let fallible = program
        .types
        .iter()
        .filter(|data| data.fallible)
        .filter_map(|data| data.id.as_ref())
        .collect::<Vec<_>>();
    if fallible.is_empty() {
        return false;
    }
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if let IrExpr::Construct {
            value_type: Type::Declared(id, _),
            ..
        } = &node.expr
            && fallible.contains(&id)
        {
            return true;
        }
        pending.extend(operands(node));
    }
    false
}

impl<'a> BodyChecker<'_, 'a> {
    /// The type of a construction of the type at program index `ty`: the
    /// type itself, or `Result(T E)` when its `where` has an `else` whose
    /// error type `E` is known.
    pub(super) fn constructed(&self, ty: usize) -> Option<Type> {
        let data = self.program.types.get(ty)?;
        let own = data.self_type()?;
        if !data.fallible {
            return Some(own);
        }
        let error = self.layout.errors.get(&ty)?.clone();
        self.program.library_type(Library::Result, vec![own, error])
    }

    /// Records the obligation the lowered call or construction `node`,
    /// written at `span`, states: `site` is the function called (`Ok`) or
    /// the type constructed (`Err`). A call of a function whose `requires`
    /// is bare and a construction of a type whose `where` is bare each
    /// state one.
    pub(super) fn obligation(
        &mut self,
        site: Option<Result<usize, usize>>,
        node: &Node,
        span: ByteSpan,
    ) {
        match site {
            Some(Ok(function)) => {
                let bare = self
                    .program
                    .functions
                    .get(function)
                    .and_then(|data| data.view.requires())
                    .is_some_and(|guard| guard.consequence().is_none());
                if bare {
                    self.sites.push(Site::Requires { function, span });
                }
            }
            Some(Err(ty)) => {
                let Some(data) = self.program.types.get(ty) else {
                    return;
                };
                if data.invariant && !data.fallible {
                    let node = constant_operands(node, &self.layout.constants)
                        .then(|| node.clone());
                    self.sites.push(Site::Where { ty, span, node });
                }
            }
            None => {}
        }
    }

    /// Records the `let` pattern `pattern`, written at `span` without an
    /// `else`, that does not cover every value of its initializer's type,
    /// when the initializer `value` is a constant construction of a type
    /// whose `where` has an `else`: the evaluation of the construction
    /// decides it. Whether it was recorded.
    pub(super) fn construction_pattern(
        &mut self,
        pattern: Option<&IrPattern>,
        span: ByteSpan,
        value: Option<&Node>,
    ) -> bool {
        let (Some(pattern), Some(value)) = (pattern, value) else {
            return false;
        };
        let IrExpr::Construct {
            value_type: Type::Declared(id, _),
            ..
        } = &value.expr
        else {
            return false;
        };
        let Some(ty) = self
            .program
            .types
            .iter()
            .position(|data| data.fallible && data.id.as_ref() == Some(id))
        else {
            return false;
        };
        if !constant_operands(value, &self.layout.constants) {
            return false;
        }
        self.sites.push(Site::Pattern {
            ty,
            span,
            pattern: pattern.clone(),
            value: value.clone(),
        });
        true
    }

    /// Checks the guards `views` of a callable of type `signature` whose
    /// parameters are bound, `destructured` of them taken apart by patterns
    /// that bind the slots `bound`. Whether a guard had a fault, and the
    /// checked guards.
    pub(super) fn guards(
        &mut self,
        views: &GuardViews<'a>,
        signature: &FnType,
        destructured: &[Destructure],
        bound: &Range<usize>,
    ) -> (bool, Guards) {
        // Nothing leaves a callable from a guard.
        let saved = self.result.take();
        let truth = self.program.library_type(Library::Bool, Vec::new());
        let mut failed = false;
        let mut guards = Guards::default();
        if let Some(view) = views.requires {
            match self.guard(
                view,
                truth.as_ref(),
                &signature.result,
                destructured,
                bound,
            ) {
                Some(guard) => guards.entry = Some(guard),
                None => failed = true,
            }
        }
        if let Some(view) = views.ensures {
            let slot = self.new_slot(Some(&signature.result));
            let mark = self.locals.len();
            if let Some(name) = views.result {
                self.locals.push(Local {
                    name: name.text().to_owned(),
                    slot,
                    ty: Some(signature.result.clone()),
                    frame: self.frames.len(),
                    generic: None,
                });
            }
            let checked = self.guard(
                view,
                truth.as_ref(),
                &signature.result,
                destructured,
                bound,
            );
            self.locals.truncate(mark);
            match checked {
                Some(guard) => guards.exit = Some((slot, guard)),
                None => failed = true,
            }
        }
        self.result = saved;
        (failed, guards)
    }

    /// Checks one guard: its predicate as a `Bool` and its consequence at
    /// `result`.
    fn guard(
        &mut self,
        view: GuardView<'a>,
        truth: Option<&Type>,
        result: &Type,
        destructured: &[Destructure],
        bound: &Range<usize>,
    ) -> Option<CheckedGuard> {
        let written = view.predicate()?;
        let span = expr_span(&written);
        let predicate = match truth {
            Some(truth) => {
                self.operand(written, truth, DiagnosticCode::ContractInvalidPredicate)
            }
            None => {
                self.unavailable(span, "a predicate without `Bool`");
                None
            }
        };
        let consequence = view.consequence().map(|written| {
            self.operand(written, result, DiagnosticCode::ContractInvalidConsequence)
        });
        let consequence = match consequence {
            Some(Some(node)) => Some(node),
            Some(None) => return None,
            None => None,
        };
        let predicate = predicate?;
        let truth = truth?.clone();
        Some(CheckedGuard {
            predicate: self.wrap_guard(predicate, destructured, bound, truth),
            consequence: consequence
                .map(|node| self.wrap_guard(node, destructured, bound, result.clone())),
            span,
        })
    }

    /// Checks `written` at `expected`, reporting a value of another type as
    /// `code` at it.
    fn operand(
        &mut self,
        written: Expr<'a>,
        expected: &Type,
        code: DiagnosticCode,
    ) -> Option<Node> {
        let span = expr_span(&written);
        let mark = self.diagnostics.len();
        let out = self.run(Task::Expr(written, Ctx::Value(Some(expected.clone()))));
        let node = match out.ty {
            Some(Type::Never) => {
                self.report(
                    code,
                    span,
                    format!("expected `{expected}`, found an expression that never completes"),
                );
                None
            }
            Some(actual) => self.widen(expected, &actual, out.node, span, code),
            None => None,
        };
        self.retag(mark, span, code);
        node
    }

    /// Reports each `@type.mismatch` found since `mark` at `span` itself as
    /// `code`: the operand there has the wrong type for its guard.
    fn retag(&mut self, mark: usize, span: ByteSpan, code: DiagnosticCode) {
        for diagnostic in self.diagnostics.iter_mut().skip(mark) {
            if diagnostic.code() == DiagnosticCode::TypeMismatch
                && diagnostic.primary_span() == span
            {
                *diagnostic = diagnostic.clone().with_code(code);
            }
        }
    }

    /// `node`, of type `value_type`, inside a `match` of one arm for each
    /// parameter taken apart, when it reads a slot those patterns bind.
    fn wrap_guard(
        &self,
        node: Node,
        destructured: &[Destructure],
        bound: &Range<usize>,
        value_type: Type,
    ) -> Node {
        if destructured.is_empty() || !reads_any(&node, bound) {
            return node;
        }
        destructured
            .iter()
            .rev()
            .fold(node, |body, (slot, ty, pattern, span)| {
                let subject = self.node(
                    IrExpr::Variable {
                        slot: *slot,
                        value_type: ty.clone(),
                    },
                    *span,
                );
                self.node(
                    IrExpr::Match {
                        subject: Box::new(subject),
                        arms: vec![MatchArm {
                            pattern: pattern.clone(),
                            body,
                        }],
                        value_type: value_type.clone(),
                    },
                    *span,
                )
            })
    }

    /// Checks the `where` `section` of a type whose representation is
    /// `representation`, outside any callable.
    pub(crate) fn invariant(
        &mut self,
        section: Where<'a>,
        representation: &Type,
    ) -> InvariantCheck {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.binders.clear();
        self.self_type = None;
        self.reset_scope();
        self.result = None;
        let mut found = InvariantCheck::default();
        let consequence = section
            .consequence()
            .map(|written| self.consequence(written, &mut found));
        let Some(written) = section.predicate() else {
            return found;
        };
        let span = expr_span(&written);
        let Some(truth) = self.program.library_type(Library::Bool, Vec::new()) else {
            self.unavailable(span, "a predicate without `Bool`");
            return found;
        };
        let expected = Type::function(FnType::new(vec![representation.clone()], truth));
        let mark = self.diagnostics.len();
        let out = self.run(Task::Expr(written, Ctx::Value(Some(expected.clone()))));
        let node = match out.ty {
            Some(actual) => self.conform(
                &expected,
                &actual,
                out.node,
                span,
                DiagnosticCode::ContractInvalidPredicate,
            ),
            None => None,
        };
        self.retag(mark, span, DiagnosticCode::ContractInvalidPredicate);
        let predicate = match node.as_ref().map(|node| &node.expr) {
            Some(IrExpr::Closure { closure, captures }) if captures.is_empty() => {
                Some(Predicate::Anonymous(Arc::clone(closure)))
            }
            Some(IrExpr::Function {
                function,
                type_arguments,
            }) if type_arguments.is_empty() => self
                .layout
                .functions
                .iter()
                .position(|at| *at == Some(*function))
                .map(Predicate::Named),
            Some(_) => {
                self.report(
                    DiagnosticCode::ContractUnsupportedPredicate,
                    span,
                    "a `where` operand is an anonymous `fn` or the name of a `fn`",
                );
                None
            }
            None => None,
        };
        found.checked = match (predicate, consequence) {
            (Some(predicate), None) => Some(CheckedInvariant {
                predicate,
                consequence: None,
                span,
            }),
            (Some(predicate), Some(Some(constant))) => Some(CheckedInvariant {
                predicate,
                consequence: Some(constant),
                span,
            }),
            _ => None,
        };
        found
    }

    /// Checks the `else` consequence `written` of a `where`: its static type
    /// is the error type, and it is a constant expression.
    fn consequence(
        &mut self,
        written: Expr<'a>,
        found: &mut InvariantCheck,
    ) -> Option<Constant> {
        let span = expr_span(&written);
        let out = self.run(Task::Expr(written, Ctx::Value(None)));
        let ty = match out.ty {
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::ContractInvalidConsequence,
                    span,
                    "this consequence never completes, so it has no value",
                );
                return None;
            }
            Some(ty) => ty,
            None => return None,
        };
        found.error = Some(ty.clone());
        let node = out.node?;
        let folded = if fallible_construction(self.program, &node) {
            Err(NotConstant::Expression)
        } else {
            fold(&node, &self.layout.constants)
        };
        match folded {
            Ok(value) => {
                let constant = Constant::new(ty, value).ok();
                if constant.is_none() {
                    self.unavailable(span, "a consequence of an open type");
                }
                constant
            }
            Err(NotConstant::Dict) => {
                self.unavailable(span, "a dict as a consequence");
                None
            }
            Err(NotConstant::Pending) if !self.layout.constants_known => None,
            Err(NotConstant::Pending | NotConstant::Expression) => {
                self.report(
                    DiagnosticCode::ContractInvalidConsequence,
                    span,
                    "a `where … else` consequence must be a constant expression: a \
                     literal, an atom, `void`, a construction, tuple, record, or list \
                     of constants, or the name of a constant module value",
                );
                None
            }
        }
    }
}
