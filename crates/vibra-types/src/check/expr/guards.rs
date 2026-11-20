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
use super::super::declare::TypeData;
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

/// One guard operand of an anonymous callable: of its `ensures` when
/// `exit`, and its consequence when `consequence`.
#[derive(Clone)]
struct GuardJob<'a> {
    exit: bool,
    consequence: bool,
    written: Expr<'a>,
}

/// The guards of an anonymous callable being checked, one operand at a
/// time as a task of the checker's work list, so that a guard holding
/// another anonymous callable grows no host stack.
#[derive(Default)]
pub(super) struct ClosureGuards<'a> {
    /// The operands still to check, last first.
    jobs: Vec<GuardJob<'a>>,
    /// The operand being checked, and the diagnostic count when it began.
    current: Option<(GuardJob<'a>, usize)>,
    /// The checked predicate and consequence of the `requires`, then of
    /// the `ensures`.
    parts: [[Option<Node>; 2]; 2],
    /// The span of the predicate of the `requires`, then of the `ensures`.
    spans: [Option<ByteSpan>; 2],
    /// The result name.
    result: Option<Ident<'a>>,
    /// The slot the candidate is bound to, and the length of the locals
    /// before the result name.
    candidate: Option<(usize, usize)>,
    failed: bool,
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
    /// state one, and so does a call of a contract member whose `requires`
    /// is bare: the guard copied into the implementation member a closed
    /// receiver selects, and otherwise the contract's own guard, decided
    /// once at the call.
    pub(super) fn obligation(
        &mut self,
        site: Option<Result<usize, usize>>,
        node: &Node,
        span: ByteSpan,
    ) {
        if let IrExpr::Call {
            target: CallTarget::Contract(call),
            ..
        } = &node.expr
            && self.contract_requires_bare(call)
        {
            let site = match self.implementation_member(call) {
                Some(function) => Some(Site::Requires { function, span }),
                None => self.contract_site(call, span),
            };
            self.sites.extend(site);
        }
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
                    let program = self.program;
                    let node = constant_operands(node, &self.layout.constants, &|id| {
                        program.library_of(id)
                    })
                    .then(|| node.clone());
                    self.sites.push(Site::Where { ty, span, node });
                }
            }
            None => {}
        }
    }

    /// The function supplying the member `call` calls, when a program's own
    /// implementation without parameters supplies it at a closed receiver.
    fn implementation_member(&self, call: &vibra_ir::ContractCall) -> Option<usize> {
        if call.receiver.is_open() || !call.member_arguments.is_empty() {
            return None;
        }
        self.program.impls.iter().find_map(|data| {
            let interface = self.program.interfaces.get(data.interface)?;
            (data.usable
                && data.binders.is_empty()
                && interface.id.as_ref() == Some(&call.interface)
                && data.receiver == call.receiver
                && data.arguments == call.arguments)
                .then(|| {
                    data.members
                        .iter()
                        .find(|(name, _)| *name == call.member)
                        .map(|(_, function)| *function)
                })
                .flatten()
        })
    }

    /// The site of a call at `span` of the contract member `call` calls at
    /// a receiver whose implementation member is not known: the default
    /// body's own guard, or the abstract member's.
    fn contract_site(
        &self,
        call: &vibra_ir::ContractCall,
        span: ByteSpan,
    ) -> Option<Site> {
        let (interface, data) = self
            .program
            .interfaces
            .iter()
            .enumerate()
            .find(|(_, data)| data.id.as_ref() == Some(&call.interface))?;
        let member = data.member(&call.member)?;
        Some(match member.default {
            Some(function) => Site::Requires { function, span },
            None => Site::Contract {
                interface,
                member: member.name.clone(),
                span,
            },
        })
    }

    /// Whether the contract member `call` calls has a bare `requires`.
    fn contract_requires_bare(&self, call: &vibra_ir::ContractCall) -> bool {
        self.program
            .interfaces
            .iter()
            .find(|interface| interface.id.as_ref() == Some(&call.interface))
            .and_then(|interface| {
                interface.view.members().into_iter().find(|member| {
                    member.name().is_some_and(|name| name.text() == call.member)
                })
            })
            .and_then(|member| member.requires())
            .is_some_and(|guard| guard.consequence().is_none())
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
        let program = self.program;
        if !constant_operands(value, &self.layout.constants, &|id| {
            program.library_of(id)
        }) {
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

    /// The guards `views` of an anonymous callable, to check one operand at
    /// a time with [`Self::next_closure_guard`].
    pub(super) fn closure_guards(
        &mut self,
        views: &GuardViews<'a>,
    ) -> ClosureGuards<'a> {
        let mut guards = ClosureGuards {
            result: views.result,
            ..ClosureGuards::default()
        };
        let truth = self
            .program
            .library_type(Library::Bool, Vec::new())
            .is_some();
        for (exit, view) in [(false, views.requires), (true, views.ensures)] {
            let Some(view) = view else { continue };
            let Some(written) = view.predicate() else {
                guards.failed = true;
                continue;
            };
            let span = expr_span(&written);
            if let Some(at) = guards.spans.get_mut(usize::from(exit)) {
                *at = Some(span);
            }
            if !truth {
                self.unavailable(span, "a predicate without `Bool`");
                guards.failed = true;
                continue;
            }
            guards.jobs.push(GuardJob {
                exit,
                consequence: false,
                written,
            });
            if let Some(written) = view.consequence() {
                guards.jobs.push(GuardJob {
                    exit,
                    consequence: true,
                    written,
                });
            }
        }
        guards.jobs.reverse();
        guards
    }

    /// Takes the operand of `guards` that arrived as `input`, and gives the
    /// task of the next one, of a callable of type `signature`: the
    /// `ensures` sees the result name, bound to the candidate's slot.
    /// `None` once every operand is checked.
    pub(super) fn next_closure_guard(
        &mut self,
        guards: &mut ClosureGuards<'a>,
        signature: &FnType,
        input: Option<Out>,
    ) -> Option<Task<'a>> {
        let truth = self.program.library_type(Library::Bool, Vec::new());
        if let (Some((job, mark)), Some(out)) = (guards.current.take(), input) {
            let (expected, code) = if job.consequence {
                (
                    Some(signature.result.clone()),
                    DiagnosticCode::ContractInvalidConsequence,
                )
            } else {
                (truth.clone(), DiagnosticCode::ContractInvalidPredicate)
            };
            let span = expr_span(&job.written);
            let node = expected.and_then(|expected| {
                self.operand_done(out, &expected, span, code, mark)
            });
            let part = guards
                .parts
                .get_mut(usize::from(job.exit))
                .and_then(|parts| parts.get_mut(usize::from(job.consequence)));
            match (node, part) {
                (Some(node), Some(part)) => *part = Some(node),
                _ => guards.failed = true,
            }
        }
        let Some(job) = guards.jobs.pop() else {
            if let Some((_, locals)) = guards.candidate {
                self.locals.truncate(locals);
            }
            return None;
        };
        if job.exit && guards.candidate.is_none() {
            let slot = self.new_slot(Some(&signature.result));
            let locals = self.locals.len();
            if let Some(name) = guards.result {
                self.locals.push(Local {
                    name: name.text().to_owned(),
                    slot,
                    ty: Some(signature.result.clone()),
                    frame: self.frames.len(),
                    generic: None,
                });
            }
            guards.candidate = Some((slot, locals));
        }
        let expected = if job.consequence {
            Some(signature.result.clone())
        } else {
            truth
        };
        let task = Task::Expr(job.written.clone(), Ctx::Value(expected));
        guards.current = Some((job, self.diagnostics.len()));
        Some(task)
    }

    /// The checked guards of `guards`, of a callable of type `signature`
    /// whose parameters `destructured` are taken apart by patterns that
    /// bind the slots `bound`, and whether one had a fault.
    pub(super) fn finish_closure_guards(
        &self,
        guards: &mut ClosureGuards<'a>,
        signature: &FnType,
        destructured: &[Destructure],
        bound: &Range<usize>,
    ) -> (bool, Guards) {
        let mut out = Guards::default();
        let Some(truth) = self.program.library_type(Library::Bool, Vec::new()) else {
            return (guards.failed, out);
        };
        for exit in [false, true] {
            let at = usize::from(exit);
            let Some([predicate, consequence]) =
                guards.parts.get_mut(at).map(std::mem::take)
            else {
                continue;
            };
            let (Some(predicate), Some(span)) =
                (predicate, guards.spans.get(at).copied().flatten())
            else {
                continue;
            };
            let guard = CheckedGuard {
                predicate: self.wrap_guard(
                    predicate,
                    destructured,
                    bound,
                    truth.clone(),
                ),
                consequence: consequence.map(|node| {
                    self.wrap_guard(node, destructured, bound, signature.result.clone())
                }),
                span,
            };
            if !exit {
                out.entry = Some(guard);
            } else if let Some((slot, _)) = guards.candidate {
                out.exit = Some((slot, guard));
            }
        }
        (guards.failed, out)
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
        self.operand_done(out, expected, span, code, mark)
    }

    /// The node of a guard operand written at `span` and checked at
    /// `expected` as `out`, the diagnostics found since `mark` of a wrong
    /// type reported as `code`.
    fn operand_done(
        &mut self,
        out: Out,
        expected: &Type,
        span: ByteSpan,
        code: DiagnosticCode,
        mark: usize,
    ) -> Option<Node> {
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

    /// Checks the `where` `section` of the type `data`, whose representation
    /// is `representation`, outside any callable, with the type's parameters
    /// and their bounds in scope.
    pub(crate) fn invariant(
        &mut self,
        section: Where<'a>,
        representation: &Type,
        data: &TypeData<'_>,
    ) -> InvariantCheck {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.self_type = None;
        self.reset_scope();
        self.binders.clone_from(&data.params);
        self.bounds.clone_from(&data.bounds);
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
            // A generic predicate is applied at the type's own parameters.
            Some(IrExpr::Function {
                function,
                type_arguments,
            }) if type_arguments
                .iter()
                .enumerate()
                .all(|(at, ty)| *ty == Type::Binder(at))
                && type_arguments.len() == data.params.len() =>
            {
                self.layout
                    .functions
                    .iter()
                    .position(|at| *at == Some(*function))
                    .map(Predicate::Named)
            }
            Some(_) => {
                self.report(
                    DiagnosticCode::ContractUnsupportedPredicate,
                    span,
                    "a `where` operand is an anonymous `fn`, or the name of a `fn` \
                     applied at the type's own parameters in order",
                );
                None
            }
            None => None,
        };
        let closures = std::mem::take(&mut self.closures);
        // A `where … else` whose consequence is not a constant has no
        // checked invariant.
        found.checked = match (predicate, consequence, node) {
            (Some(predicate), None, Some(operand)) => Some(CheckedInvariant {
                predicate,
                consequence: None,
                span,
                operand,
                closures,
            }),
            (Some(predicate), Some(Some(constant)), Some(operand)) => {
                Some(CheckedInvariant {
                    predicate,
                    consequence: Some(constant),
                    span,
                    operand,
                    closures,
                })
            }
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
            let program = self.program;
            fold(&node, &self.layout.constants, &|id| program.library_of(id))
        };
        match folded {
            Ok(value) => {
                let constant = Constant::new(ty, value).ok();
                if constant.is_none() {
                    self.unavailable(span, "a consequence of an open type");
                }
                constant
            }
            Err(NotConstant::Pending) if !self.layout.constants_known => None,
            Err(NotConstant::Pending | NotConstant::Expression | NotConstant::Dict) => {
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
