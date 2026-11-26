//! Plans, `await`, and invocation identity (`docs/spec/06-runtime.md`,
//! "Cold plans and invocation identity", "Tail calls", "Audit trace").
//!
//! Applying a callee of a `Proc` type builds a [`PlanVal`]: the resolved
//! callee, its evaluated operands, and the closed substitution it runs
//! under. Building runs nothing of the callee. `await` takes the next
//! invocation ordinal and enters the plan's callee through
//! [`Machine::invoke`], exactly as a call enters a function, so its
//! `requires … else`, its body, and its `ensures … else` run on each one.
//!
//! A non-tail `await` pushes one restoration boundary, `Kont::Await`, that
//! holds the caller's [`Invocation`]. An `await` that is the operand of a
//! `return` in tail position replaces the activation and pushes nothing:
//! the parent of the new invocation is the one it replaces, and the
//! boundary of the nearest non-tail `await` below it restores the right
//! caller when the chain returns. Nothing here recurses on the host stack.

use std::rc::Rc;

use vibra_ir::{BodyContext, Node, SourceOrigin, Type};

use crate::machine::{Halt, Kont, Machine, Resolved, Step, Target, Work, invalid};
use crate::value::{Repr, Val};

/// The identity of one invocation: its ordinal, numbered from 1 in the
/// order invocations start within one execution, and the ordinal of the
/// invocation whose body performed the `await` that started it
/// (`docs/spec/06-runtime.md`, "Audit trace", "Invocations").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Invocation {
    /// The invocation's ordinal.
    pub ordinal: u64,
    /// The ordinal of its parent; `None` for the run of an entry or of a
    /// test body.
    pub parent: Option<u64>,
}

impl Invocation {
    /// The run of an entry or of a test body: invocation 1, with no parent.
    pub(crate) const ROOT: Self = Self {
        ordinal: 1,
        parent: None,
    };
}

/// A plan: an immutable, reusable value that holds the resolved callee,
/// its evaluated operands in slot order, and the substitution the callee
/// runs under. It holds no run state: no "consumed" bit, no result.
#[derive(Clone, Debug)]
pub(crate) struct PlanVal<'a> {
    /// The callee and the closed substitution it runs under.
    pub(crate) callee: Resolved<'a>,
    /// The operands, in the callee's slot order.
    pub(crate) arguments: Vec<Val<'a>>,
    /// For a member dispatched through an interface value whose result
    /// holds `Self`: the result type and the held type, to rebox the
    /// awaited result with.
    pub(crate) rebox: Option<(Type, Type)>,
}

impl<'a> PlanVal<'a> {
    /// The closure the plan's callee runs, when it is one.
    pub(crate) const fn closure(&self) -> Option<&Rc<crate::value::ClosureVal<'a>>> {
        match &self.callee.target {
            Target::Closure(closure) => Some(closure),
            _ => None,
        }
    }
}

impl<'a> Machine<'a> {
    /// Whether applying `target` builds a plan: the callee has a `Proc`
    /// type.
    pub(crate) fn builds_plan(&self, target: &Target<'a>) -> Result<bool, Halt> {
        Ok(match target {
            Target::Function(index) => {
                matches!(self.function(*index)?.signature.type_info(), Type::Proc(_))
            }
            Target::Closure(closure) => {
                matches!(closure.body.value_type, Type::Proc(_))
            }
            Target::Structural { .. } => false,
        })
    }

    /// Whether an activation of `target` runs in a procedure context: a
    /// `proc`, an anonymous `proc`, or a function whose context states one,
    /// such as a test body.
    pub(crate) fn runs_in_procedure(&self, target: &Target<'a>) -> Result<bool, Halt> {
        Ok(match target {
            Target::Function(index) => {
                let function = self.function(*index)?;
                matches!(function.signature.type_info(), Type::Proc(_))
                    || matches!(function.context, BodyContext::Procedure(_))
            }
            Target::Closure(closure) => {
                matches!(closure.body.value_type, Type::Proc(_))
            }
            Target::Structural { .. } => false,
        })
    }

    /// Builds the plan of applying `callee` to `arguments`. No guard, body,
    /// or consequence of the callee is evaluated, and no frame is pushed.
    pub(crate) fn build_plan(
        &mut self,
        callee: Resolved<'a>,
        arguments: Vec<Val<'a>>,
        rebox: Option<(Type, Type)>,
    ) -> Result<Step<'a>, Halt> {
        let value = Val(Repr::Plan(Rc::new(PlanVal {
            callee,
            arguments,
            rebox,
        })));
        self.allocated(&value)?;
        Ok(Step::Ret(value))
    }

    /// Starts an `await` of `plan`, written at `origin`: only an
    /// activation that runs in a procedure context may perform one.
    pub(crate) fn start_await(
        &mut self,
        plan: &'a Node,
        origin: &'a SourceOrigin,
    ) -> Result<Step<'a>, Halt> {
        if !self.frame()?.procedure {
            return invalid("an `await` in an activation with no procedure context");
        }
        self.operands(Work::Await { plan, origin }, Vec::new())
    }

    /// Awaits the evaluated plan `plan`: takes the next invocation ordinal
    /// and enters the plan's callee. An `await` that is the operand of a
    /// `return` in tail position replaces the activation and pushes no
    /// continuation, unless a pending `ensures … else` or a reboxing
    /// forbids it.
    pub(crate) fn await_plan(
        &mut self,
        mut plan: Val<'a>,
        origin: &'a SourceOrigin,
    ) -> Result<Step<'a>, Halt> {
        let Repr::Plan(plan) = std::mem::take(&mut plan.0) else {
            return invalid("an `await` of a value that is not a plan");
        };
        let PlanVal {
            callee,
            arguments,
            rebox,
        } = Rc::try_unwrap(plan).unwrap_or_else(|shared| PlanVal::clone(&shared));
        // A fault of the call is at the `await`, not at its last operand.
        self.origin = Some(origin);
        let ordinal = self.stats.invocation_count.saturating_add(1);
        self.stats.invocation_count = ordinal;
        let tail = rebox.is_none()
            && matches!(self.konts.last(), Some(Kont::Return))
            && self.prepare_tail();
        let caller = self.invocation;
        if !tail {
            if let Some((result, held_type)) = rebox {
                self.konts.push(Kont::Rebox { result, held_type });
            }
            self.konts.push(Kont::Await { caller });
        }
        self.invocation = Invocation {
            ordinal,
            parent: Some(caller.ordinal),
        };
        self.invoke(callee, arguments, tail, Some(origin))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use vibra_diagnostics::DiagnosticCode;
    use vibra_ir::external::host::{
        HostInterface, Owner, TEST_WORLD_OWNER, TestWorldOp,
    };
    use vibra_ir::{
        BodyContext, CheckedFunction, CheckedModuleSet, FunctionBody, HostBinding,
        InterfaceDefinition, InterfaceMember, IrError, ModuleSetParts, NamedTypes,
        ProcType, Row, SignatureRecord, SourceOrigin, Type, TypeId,
    };

    use crate::evaluate::runtime_error;
    use crate::frame::TypeEnv;
    use crate::machine::{Halt, Machine, Mode, Resolved, Target};
    use crate::value::{Repr, Val};
    use crate::{MemoryBudget, RuntimeError};

    const SEED: &str =
        "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

    fn id(owner: Owner) -> Result<TypeId, IrError> {
        let module = owner.module.strip_prefix('@').unwrap_or(owner.module);
        TypeId::new(format!("{module}.{}", owner.name), SEED)
    }

    /// `TestWorld`, whose member `random` is bound to `testWorld.random`
    /// (function 0), and `RandomBytes`, the interface it returns.
    fn world_set() -> Result<std::sync::Arc<CheckedModuleSet>, IrError> {
        let world = id(TEST_WORLD_OWNER)?;
        let random = id(HostInterface::RandomBytes.owner())?;
        let member = SignatureRecord::interface_member(
            Vec::new(),
            Some(0),
            Type::Proc(Box::new(ProcType {
                parameters: vec![Type::SelfType],
                labels: NamedTypes::empty(),
                result: Type::Interface(
                    random.clone(),
                    Vec::new(),
                    NamedTypes::empty(),
                ),
                effects: Row::new([world.entity().clone()])?,
            })),
            Vec::new(),
        )?;
        let host = CheckedFunction {
            name: "std.test.TestWorld.random".to_owned(),
            signature: member.clone(),
            defaults: Vec::new(),
            body: FunctionBody::Host(HostBinding::TestWorld(TestWorldOp::Random)),
            entry_guard: None,
            exit_guard: None,
            slot_count: 1,
            origin: SourceOrigin::new(
                "test.vibra",
                vibra_diagnostics::ByteSpan::new(0, 0),
            ),
            context: BodyContext::Pure,
        };
        CheckedModuleSet::try_new(ModuleSetParts {
            interfaces: vec![
                InterfaceDefinition::new(
                    world,
                    Vec::new(),
                    Vec::new(),
                    vec![InterfaceMember {
                        name: TestWorldOp::Random.member().to_owned(),
                        signature: member,
                        default: Some(0),
                    }],
                )?,
                InterfaceDefinition::new(random, Vec::new(), Vec::new(), Vec::new())?,
            ],
            functions: vec![host],
            ..ModuleSetParts::default()
        })
    }

    /// Row "a host body without a context": the checked IR reaches a
    /// host-bound member only through the dispatch on a sealed grant, which
    /// is step 20's, so the member is entered here as that dispatch enters
    /// it, in the machine `run_test` builds. With no host context it is
    /// `@tool.unavailable`.
    #[test]
    fn a_host_body_without_a_context_is_unavailable() -> Result<(), IrError> {
        let set = world_set()?;
        let budget = MemoryBudget::DEFAULT;
        let mut machine = Machine::new(&set, Mode::Test, budget.bytes, u64::MAX);
        let callee = Resolved {
            target: Target::Function(0),
            env: Rc::new(TypeEnv::default()),
        };
        let halt = machine.invoke(callee, vec![Val(Repr::I32(0))], false, None);
        let Err(halt @ Halt::Unavailable(_)) = halt else {
            return Err(IrError::InvalidExpression(format!(
                "expected the host body to be unavailable, got {halt:?}"
            )));
        };
        let error = runtime_error(halt, None, budget);
        assert!(
            matches!(&error, RuntimeError::Unavailable { message } if message.contains("testWorld.random")),
            "{error:?}"
        );
        assert_eq!(error.code(), DiagnosticCode::ToolUnavailable);
        Ok(())
    }
}
