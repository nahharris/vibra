//! Grant values, the dispatch of host-bound and narrowing members through
//! [`HostContext`], the test-world rows, and the arguments of an entry
//! (`docs/spec/06-runtime.md`, "Host calls", "Host registry", "Cold plans
//! and invocation identity", "Audit trace"; `docs/spec/03-effects.md`,
//! "Capability values", "Narrowing", "Operation interfaces").
//!
//! A grant is [`Repr::Grant`]: a token the context issued, copied freely,
//! never encoded, never compared. The interpreter makes one in exactly two
//! places: [`received`], from what the context hands back (a narrowed grant
//! or an issued one), and [`Machine::entry_arguments`], from what the
//! caller of an entry or a test body hands in. No source expression and no
//! neutral value makes one.
//!
//! An awaited host-bound member makes one request through
//! [`HostContext::host`], with the current invocation, the source origin of
//! the `await`, and the room the memory budget has left. The reply is
//! accepted at the types of the member's registry row and accounted, every
//! part of it against the room, the
//! value goes to the continuation, and only then is the receipt committed
//! ([`Machine::commit`], called from `Machine::execute` once the
//! continuation has taken the value). A reply of the wrong shape and a
//! fault are the trap `@runtime.invalidHostValue`; a memory fault and a
//! reply the budget cannot account are the host event. In each of these
//! the receipt is dropped and never committed.
//!
//! With no context, a host body and a narrowing are
//! [`crate::RuntimeError::Unavailable`].

use std::fmt;
use std::rc::Rc;

use vibra_ir::authority::{
    GrantLimits, GrantToken, HostArgument, HostContext, HostFault, HostFsKind, HostOk,
    HostReply, HostRequest, Issued, NarrowRequest, Narrowed, Receipt, ScopeErrorCase,
};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::compiler::NarrowingType;
use vibra_ir::external::host::{HostOp, HostRow, HostType, TestWorldOp};
use vibra_ir::{CheckedFunction, HostBinding, Library, SourceOrigin, Type, TypeId};

use crate::encode::to_neutral;
use crate::machine::{Fixture, Halt, Machine, Step, invalid, whole_bytes};
use crate::value::{Repr, Val};

/// One argument of an entry or of a test body, as the caller that issued
/// its grants hands it in (`docs/spec/03-effects.md`, "Capability values":
/// a required grant, an optional one, and a bundle).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryArgument {
    /// A grant: a parameter of an operation interface's type, or the
    /// present value of an optional one, `Option.some(value: grant)`.
    Grant(GrantToken),
    /// An optional grant that is absent: `Option.none()`.
    Absent,
    /// A bundle: a record of arguments, by field name.
    Record(Vec<(String, Self)>),
}

/// The host context of one execution, and the receipt of a completed call
/// whose value has not yet reached its continuation.
#[derive(Default)]
pub(crate) struct HostSlot<'a> {
    pub(crate) context: Option<&'a mut dyn HostContext>,
    pub(crate) pending: Option<Receipt>,
    /// Called each time the value of a completed host call has reached its
    /// continuation, before the receipt is committed. It exists only in the
    /// unit tests of this module, which set it to see that order.
    #[cfg(test)]
    pub(crate) delivered: Option<Box<dyn FnMut() + 'a>>,
}

impl HostSlot<'_> {
    /// Reports that the value of the call whose receipt is pending has
    /// reached its continuation: to the unit tests' observer, and to no one
    /// in any other build.
    pub(crate) fn deliver(&mut self) {
        #[cfg(test)]
        if let Some(delivered) = &mut self.delivered {
            delivered();
        }
    }
}

impl fmt::Debug for HostSlot<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut slot = formatter.debug_struct("HostSlot");
        slot.field("context", &self.context.is_some())
            .field("pending", &self.pending);
        #[cfg(test)]
        slot.field("delivered", &self.delivered.is_some());
        slot.finish()
    }
}

/// The grant `token`, which the context handed back: a narrowed grant or
/// an issued one.
const fn received<'a>(token: GrantToken) -> Val<'a> {
    Val(Repr::Grant(token))
}

/// The halt of a fault of the context: a memory fault is the host event,
/// any other the trap `@runtime.invalidHostValue`. Only a test-world member
/// can answer a conflicting fixture, and `Machine::test_world` takes it
/// before this.
fn fault(fault: HostFault) -> Halt {
    match fault {
        HostFault::MemoryExhausted => Halt::Memory,
        HostFault::InvalidHostValue(message) => Halt::HostValue(message),
        HostFault::ConflictingFixture(names) => Halt::HostValue(format!(
            "a host operation answered a conflicting fixture [{}]",
            names.join(" ")
        )),
    }
}

/// The source name of a case of `ScopeError`.
const fn scope_case(case: ScopeErrorCase) -> &'static str {
    match case {
        ScopeErrorCase::InvalidPolicy => "invalidPolicy",
        ScopeErrorCase::UnsupportedVersion => "unsupportedVersion",
        ScopeErrorCase::PolicyTooComplex => "policyTooComplex",
        ScopeErrorCase::ForeignRoot => "foreignRoot",
        ScopeErrorCase::InvalidPath => "invalidPath",
    }
}

/// The source name of a case of `FsKind`.
const fn fs_kind(kind: HostFsKind) -> &'static str {
    match kind {
        HostFsKind::File => "file",
        HostFsKind::Directory => "directory",
        HostFsKind::Other => "other",
    }
}

/// An argument of a host operation, held while the request borrows it.
enum Owned {
    Text(Rc<str>),
    Bytes(Rc<Vec<u8>>),
    Path(Vec<String>),
    Count(u64),
}

impl Owned {
    fn argument(&self) -> HostArgument<'_> {
        match self {
            Self::Text(text) => HostArgument::Text(text),
            Self::Bytes(bytes) => HostArgument::Bytes(bytes),
            Self::Path(names) => HostArgument::Path(names),
            Self::Count(count) => HostArgument::Count(*count),
        }
    }
}

/// The names of a `RelativePath`: a wrapper over `List(Str)`.
fn path_names(value: &Val<'_>) -> Result<Vec<String>, Halt> {
    let not_a_path = || Halt::Invalid("a `RelativePath` that is not one".to_owned());
    let Repr::Wrapper(inner) = &value.0 else {
        return Err(not_a_path());
    };
    inner
        .as_list()
        .ok_or_else(not_a_path)?
        .iter()
        .map(|name| match &name.0 {
            Repr::Str(text) => Ok(text.as_ref().to_owned()),
            _ => Err(not_a_path()),
        })
        .collect()
}

/// The argument of the parameter of type `ty` that `value` is.
fn owned(ty: HostType, value: &Val<'_>) -> Result<Owned, Halt> {
    Ok(match (ty, &value.0) {
        (HostType::Str, Repr::Str(text)) => Owned::Text(Rc::clone(text)),
        (HostType::Bytes, Repr::Bytes(bytes)) => Owned::Bytes(Rc::clone(bytes)),
        (HostType::RelativePath, _) => Owned::Path(path_names(value)?),
        (HostType::U64, Repr::U64(count)) => Owned::Count(*count),
        (ty, _) => {
            return invalid(format!(
                "an argument that is not a value of `{}`",
                ty.spelling()
            ));
        }
    })
}

/// The member's `Result` value of `reply`, at the types of `row`.
fn reply_value<'a>(row: &HostRow, reply: HostReply) -> Result<Val<'a>, Halt> {
    let wrong = |what: &str| {
        Halt::HostValue(format!(
            "`{}` answered {what}, which is no value of `Result({} {})`",
            row.symbol,
            row.ok.spelling(),
            row.error.name()
        ))
    };
    match reply {
        HostReply::Ok(ok) => {
            let value = match (row.ok, ok) {
                (HostType::Void, HostOk::Void) => Val::void(),
                (HostType::Str, HostOk::Text(text)) => Val::string(&text),
                (HostType::Bytes, HostOk::Bytes(bytes)) => Val::bytes(bytes),
                (HostType::List(&HostType::Str), HostOk::Names(names)) => {
                    Val::list(names.iter().map(|name| Val::string(name)).collect())
                }
                (HostType::FsMetadata, HostOk::Metadata { kind, size }) => {
                    Val(Repr::Record(Rc::new(vec![
                        ("kind", Val::tag(fs_kind(kind))),
                        ("size", Val(Repr::U64(size))),
                    ])))
                }
                (HostType::I64, HostOk::I64(value)) => Val(Repr::I64(value)),
                (HostType::U64, HostOk::U64(value)) => Val(Repr::U64(value)),
                (_, other) => return Err(wrong(&format!("{other:?}"))),
            };
            Ok(Val::ok(value))
        }
        HostReply::Err(case) if case.error() == row.error => {
            Ok(Val::err(Val::tag(case.case())))
        }
        HostReply::Err(case) => Err(wrong(&format!(
            "the case `{}` of `{}`",
            case.case(),
            case.error().name()
        ))),
    }
}

/// The member's result value of what the test-world row `op` issued.
fn issued_value<'a>(op: TestWorldOp, issued: Issued) -> Result<Val<'a>, Halt> {
    let row = op.row();
    let wrong = || {
        Halt::HostValue(format!(
            "`{}` issued what is no value of `{}`",
            row.symbol,
            row.result.spelling()
        ))
    };
    Ok(match (row.result, issued) {
        (HostType::Interface(_), Issued::Grant(token)) => received(token),
        (HostType::Record(_), Issued::Output { stdout, stderr })
            if op == TestWorldOp::Output =>
        {
            Val(Repr::Record(Rc::new(vec![
                ("stderr", Val::bytes(stderr)),
                ("stdout", Val::bytes(stdout)),
            ])))
        }
        (HostType::Record(fields), Issued::Bundle(mut grants))
            if op != TestWorldOp::Output =>
        {
            grants.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            let mut expected =
                fields.iter().map(|field| field.name).collect::<Vec<_>>();
            expected.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            if grants.len() != expected.len()
                || grants
                    .iter()
                    .zip(&expected)
                    .any(|((name, _), field)| name != field)
            {
                return Err(wrong());
            }
            Val(Repr::Record(Rc::new(
                grants
                    .into_iter()
                    .map(|(name, token)| (name, received(token)))
                    .collect(),
            )))
        }
        _ => return Err(wrong()),
    })
}

/// The bounds of a `GrantLimits` record.
fn grant_limits(value: &Val<'_>) -> Result<GrantLimits, Halt> {
    let Repr::Record(fields) = &value.0 else {
        return invalid("a `GrantLimits` that is not a record");
    };
    let bound = |name: &str| -> Result<Option<u64>, Halt> {
        let field = fields
            .iter()
            .find_map(|(field, value)| (*field == name).then_some(value))
            .ok_or_else(|| {
                Halt::Invalid(format!("a `GrantLimits` without `{name}`"))
            })?;
        match &field.0 {
            Repr::Opt(None) => Ok(None),
            Repr::Opt(Some(inner)) => inner.as_u64().map(Some).ok_or_else(|| {
                Halt::Invalid(format!("a `GrantLimits` whose `{name}` is no `U64`"))
            }),
            _ => invalid(format!("a `GrantLimits` whose `{name}` is no `Option`")),
        }
    };
    Ok(GrantLimits {
        invocations: bound("invocations")?,
        input_bytes: bound("inputBytes")?,
        output_bytes: bound("outputBytes")?,
    })
}

impl<'a> Machine<'a> {
    /// The function of the member `member` of `interface` that a sealed
    /// grant selects, when the receiver among `args` is one: the
    /// interface's own binding of the member, its host-bound or narrowing
    /// member. `None` when the receiver is a held value, whose type's
    /// implementation the caller selects.
    pub(crate) fn sealed(
        &self,
        interface: &TypeId,
        member: &str,
        args: &[Val<'a>],
    ) -> Result<Option<usize>, Halt> {
        let declared = self
            .set
            .interfaces()
            .get(interface)
            .and_then(|definition| definition.member(member))
            .ok_or_else(|| Halt::Invalid(format!("no member `{member}`")))?;
        let slot = declared
            .signature
            .receiver()
            .and_then(|slot| usize::try_from(slot).ok())
            .ok_or_else(|| Halt::Invalid(format!("`{member}` has no receiver slot")))?;
        if !matches!(args.get(slot), Some(Val(Repr::Grant(_)))) {
            return Ok(None);
        }
        declared.default.map(Some).ok_or_else(|| {
            Halt::Invalid(format!(
                "a sealed grant of `{}` has no member `{member}`",
                interface.path()
            ))
        })
    }
    /// Runs the host body `binding` of `function` on `args`, its parameters
    /// in slot order, for the `await` written at `origin`.
    pub(crate) fn host_body(
        &mut self,
        binding: HostBinding,
        function: &'a CheckedFunction,
        args: &[Val<'a>],
        origin: Option<&'a SourceOrigin>,
    ) -> Result<Step<'a>, Halt> {
        if self.host.context.is_none() {
            return Err(Halt::Unavailable(format!(
                "the host-bound member `{}` runs only with a host context",
                binding.symbol()
            )));
        }
        let Some((Val(Repr::Grant(grant)), operands)) = args.split_first() else {
            return invalid(format!(
                "the host-bound member `{}` reached with a receiver that is not a sealed grant",
                binding.symbol()
            ));
        };
        match binding {
            HostBinding::Operation(op) => self.operation(op, *grant, operands, origin),
            HostBinding::TestWorld(op) => {
                self.test_world(op, *grant, function, operands, origin)
            }
        }
    }

    /// One host call: the request, then the reply accepted and accounted.
    /// The receipt stays pending until the continuation has the value.
    fn operation(
        &mut self,
        op: HostOp,
        grant: GrantToken,
        operands: &[Val<'a>],
        origin: Option<&'a SourceOrigin>,
    ) -> Result<Step<'a>, Halt> {
        let row = op.row();
        if operands.len() != row.parameters.len() {
            return invalid(format!("`{}` with the wrong operands", row.symbol));
        }
        if self.host.pending.is_some() {
            return invalid("a host call while the event of another is pending");
        }
        let held = row
            .parameters
            .iter()
            .zip(operands)
            .map(|(parameter, value)| owned(parameter.ty, value))
            .collect::<Result<Vec<_>, _>>()?;
        let arguments = held.iter().map(Owned::argument).collect::<Vec<_>>();
        let room = self.room();
        let invocation = self.invocation;
        let request = HostRequest {
            op,
            grant,
            arguments: &arguments,
            invocation: invocation.ordinal,
            parent: invocation.parent,
            origin,
            room,
        };
        let context = self
            .host
            .context
            .as_deref_mut()
            .ok_or_else(|| Halt::Unavailable("no host context".to_owned()))?;
        let completion = context.host(request).map_err(fault)?;
        let value = reply_value(&row, completion.reply)?;
        // 06 "Memory": the bytes a call hands the program are compared with
        // what the budget can still account, which is the room measured
        // for the request; nothing has run since. The host built the reply
        // whole, so every part of it counts, not only its top, and enters
        // the accounted footprint at once.
        if whole_bytes(&value) > room {
            return Err(Halt::Memory);
        }
        self.allocated_whole(&value)?;
        self.host.pending = completion.receipt;
        Ok(Step::Ret(value))
    }

    /// A member of `TestWorld`: its labelled arguments as neutral values, in
    /// slot order, and the record, grant, or bytes it issues. A fixture whose
    /// paths conflict ends the test at the `await` written at `origin`.
    fn test_world(
        &mut self,
        op: TestWorldOp,
        world: GrantToken,
        function: &'a CheckedFunction,
        operands: &[Val<'a>],
        origin: Option<&'a SourceOrigin>,
    ) -> Result<Step<'a>, Halt> {
        let Type::Proc(signature) = function.signature.type_info() else {
            return invalid(format!("`{}` is not a procedure", op.symbol()));
        };
        let labels = signature.labels.entries();
        if operands.len() != labels.len() {
            return invalid(format!("`{}` with the wrong operands", op.symbol()));
        }
        let env = Rc::clone(&self.frame()?.env);
        let values = labels
            .iter()
            .zip(operands)
            .map(|((_, ty), value)| to_neutral(self.set, value, &env.apply(ty)))
            .collect::<Result<Vec<_>, _>>()?;
        let invocation = self.invocation.ordinal;
        let context = self
            .host
            .context
            .as_deref_mut()
            .ok_or_else(|| Halt::Unavailable("no host context".to_owned()))?;
        let issued =
            context
                .test_world(op, world, &values, invocation)
                .map_err(|error| match error {
                    HostFault::ConflictingFixture(names) => {
                        Halt::Fixture(Box::new(Fixture {
                            names,
                            origin: origin.cloned(),
                        }))
                    }
                    other => fault(other),
                })?;
        let value = issued_value(op, issued)?;
        self.allocated_whole(&value)?;
        Ok(Step::Ret(value))
    }

    /// `grant.restrict`, `grant.sub`, or `grant.limit` on `args`: the grant,
    /// then the policy, the prefix, or the bounds. A narrowing takes no
    /// invocation ordinal and appends no event.
    pub(crate) fn narrow(
        &mut self,
        intrinsic: CompilerIntrinsic,
        args: Vec<Val<'a>>,
    ) -> Result<Step<'a>, Halt> {
        if self.host.context.is_none() {
            return Err(Halt::Unavailable(format!(
                "`{}` narrows a sealed grant, which needs a host context",
                intrinsic.symbol()
            )));
        }
        let [grant, operand]: [Val<'a>; 2] = args.try_into().map_err(|_| {
            Halt::Invalid(format!("`{}` takes two operands", intrinsic.symbol()))
        })?;
        let Repr::Grant(grant) = grant.0 else {
            return invalid(format!(
                "`{}` reached with a receiver that is not a sealed grant",
                intrinsic.symbol()
            ));
        };
        let policy;
        let prefix;
        let request = match intrinsic {
            CompilerIntrinsic::GrantRestrict => {
                let id = self
                    .set
                    .library()
                    .narrowing(NarrowingType::FsPolicy)
                    .ok_or_else(|| {
                        Halt::Invalid("no `FsPolicy` is bound".to_owned())
                    })?;
                policy = to_neutral(
                    self.set,
                    &operand,
                    &Type::Declared(id.clone(), Vec::new()),
                )?;
                NarrowRequest::Restrict {
                    grant,
                    policy: &policy,
                }
            }
            CompilerIntrinsic::GrantSub => {
                prefix = path_names(&operand)?;
                NarrowRequest::Sub {
                    grant,
                    prefix: &prefix,
                }
            }
            CompilerIntrinsic::GrantLimit => NarrowRequest::Limit {
                grant,
                limits: grant_limits(&operand)?,
            },
            other => return invalid(format!("`{}` narrows nothing", other.symbol())),
        };
        let context = self
            .host
            .context
            .as_deref_mut()
            .ok_or_else(|| Halt::Unavailable("no host context".to_owned()))?;
        let narrowed = context.narrow(request).map_err(fault)?;
        let value = match (intrinsic, narrowed) {
            (CompilerIntrinsic::GrantLimit, Narrowed::Grant(token)) => received(token),
            (_, Narrowed::Grant(token)) => Val::ok(received(token)),
            (
                CompilerIntrinsic::GrantRestrict,
                Narrowed::Refused(
                    case @ (ScopeErrorCase::InvalidPolicy
                    | ScopeErrorCase::PolicyTooComplex),
                ),
            )
            | (
                CompilerIntrinsic::GrantSub,
                Narrowed::Refused(case @ ScopeErrorCase::PolicyTooComplex),
            ) => Val::err(Val::tag(scope_case(case))),
            (_, Narrowed::Refused(case)) => {
                return Err(Halt::HostValue(format!(
                    "`{}` refused with `{}`, which it never gives",
                    intrinsic.symbol(),
                    scope_case(case)
                )));
            }
        };
        self.allocated(&value)?;
        Ok(Step::Ret(value))
    }

    /// Commits `pending`, the receipt of the call whose value the
    /// continuation has just taken. This is the one place the interpreter
    /// calls [`HostContext::commit`].
    pub(crate) fn commit(&mut self, pending: Option<Receipt>) -> Result<(), Halt> {
        let Some(receipt) = pending else {
            return Ok(());
        };
        let context = self.host.context.as_deref_mut().ok_or_else(|| {
            Halt::Invalid("a receipt with no host context".to_owned())
        })?;
        context.commit(receipt).map_err(fault)
    }

    /// The values of `arguments` at `parameters`, in order: a grant at an
    /// operation interface's type or as the present value of an `Option`,
    /// an absent one as `Option.none()`, and a bundle as the record of its
    /// fields. An argument that does not fit its parameter is the trap
    /// `@runtime.invalidHostValue`.
    pub(crate) fn entry_arguments(
        &mut self,
        parameters: &'a [Type],
        arguments: Vec<EntryArgument>,
    ) -> Result<Vec<Val<'a>>, Halt> {
        if parameters.len() != arguments.len() {
            return Err(Halt::HostValue(format!(
                "{} entry arguments for {} parameters",
                arguments.len(),
                parameters.len()
            )));
        }
        // A bundle nests no deeper than its type, which the checked IR
        // bounds, so this work list holds no more than that depth of
        // pending records at once.
        enum Task<'a> {
            Enter(EntryArgument, &'a Type),
            Record(Vec<&'a str>),
        }
        let library = self.set.library();
        let mut values = Vec::with_capacity(parameters.len());
        for (argument, parameter) in arguments.into_iter().zip(parameters) {
            let mut tasks = vec![Task::Enter(argument, parameter)];
            let mut built: Vec<Val<'a>> = Vec::new();
            while let Some(task) = tasks.pop() {
                match task {
                    Task::Enter(argument, ty) => {
                        let optional = library.arguments_of(Library::Option, ty);
                        match (argument, ty, optional) {
                            (EntryArgument::Grant(token), Type::Interface(..), _) => {
                                built.push(Val(Repr::Grant(token)));
                            }
                            (
                                EntryArgument::Grant(token),
                                _,
                                Some([Type::Interface(..)]),
                            ) => {
                                built.push(Val::some(Val(Repr::Grant(token))));
                            }
                            (EntryArgument::Absent, _, Some(_)) => {
                                built.push(Val::none())
                            }
                            (
                                EntryArgument::Record(mut fields),
                                Type::Record(types),
                                _,
                            ) => {
                                fields.sort_by(|left, right| {
                                    left.0.as_bytes().cmp(right.0.as_bytes())
                                });
                                let entries = types.entries();
                                if fields.len() != entries.len()
                                    || fields
                                        .iter()
                                        .zip(entries)
                                        .any(|((name, _), (field, _))| name != field)
                                {
                                    return Err(Halt::HostValue(format!(
                                        "a bundle that is no value of {ty}"
                                    )));
                                }
                                tasks.push(Task::Record(
                                    entries
                                        .iter()
                                        .map(|(name, _)| name.as_str())
                                        .collect(),
                                ));
                                for ((_, field), (_, field_type)) in
                                    fields.into_iter().zip(entries).rev()
                                {
                                    tasks.push(Task::Enter(field, field_type));
                                }
                            }
                            (argument, ty, _) => {
                                return Err(Halt::HostValue(format!(
                                    "the entry argument {argument:?} is no value of {ty}"
                                )));
                            }
                        }
                    }
                    Task::Record(names) => {
                        let fields =
                            built.split_off(built.len().saturating_sub(names.len()));
                        built.push(Val(Repr::Record(Rc::new(
                            names.into_iter().zip(fields).collect(),
                        ))));
                    }
                }
            }
            let value = built.pop().ok_or_else(|| {
                Halt::Invalid("an entry argument with no value".to_owned())
            })?;
            self.allocated_whole(&value)?;
            values.push(value);
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use vibra_diagnostics::ByteSpan;
    use vibra_ir::authority::{
        GrantToken, HostCompletion, HostContext, HostFault, HostOk, HostReply,
        HostRequest, Issued, NarrowRequest, Narrowed, Receipt,
    };
    use vibra_ir::external::CompilerIntrinsic;
    use vibra_ir::external::compiler::NarrowingType;
    use vibra_ir::external::host::{HostErrorType, HostOp, TestWorldOp};
    use vibra_ir::{
        Binder, BodyContext, Case, CheckedFunction, CheckedModuleSet, FnType,
        FunctionBody, HostBinding, InterfaceDefinition, InterfaceMember, IrError,
        Library, LibraryTypes, ModuleSetParts, NamedTypes, ProcType, Row,
        SignatureRecord, SourceOrigin, Trap, Type, TypeBody, TypeDefinition, TypeId,
        Value, Visibility,
    };

    use super::received;
    use crate::evaluate::runtime_error;
    use crate::frame::TypeEnv;
    use crate::machine::{Halt, Machine, Mode, Resolved, Target, whole_bytes};
    use crate::value::{Repr, Val};
    use crate::{MemoryBudget, RuntimeError};

    const SEED: &str =
        "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad";

    /// The host-bound `time.nowMonotonic`.
    const NOW: usize = 0;
    /// `grant.limit`, the default of `test.Narrow.limit`.
    const LIMIT: usize = 1;

    fn id(path: &str) -> Result<TypeId, IrError> {
        TypeId::new(path, SEED)
    }

    fn origin() -> SourceOrigin {
        SourceOrigin::new("test.vibra", ByteSpan::new(0, 0))
    }

    fn declared(path: &str, arguments: Vec<Type>) -> Result<Type, IrError> {
        Ok(Type::Declared(id(path)?, arguments))
    }

    fn definition(
        path: &str,
        parameters: &[&str],
        body: TypeBody,
    ) -> Result<TypeDefinition, IrError> {
        TypeDefinition::new(
            id(path)?,
            parameters
                .iter()
                .map(|name| Binder::new(*name, None))
                .collect::<Result<_, _>>()?,
            body,
            None,
            Visibility::Public,
        )
    }

    /// `MonotonicClock`, whose `now` is the host-bound `time.nowMonotonic`
    /// (function 0), and `test.Narrow`, whose `limit` is `grant.limit`
    /// (function 1).
    fn host_set() -> Result<Arc<CheckedModuleSet>, IrError> {
        let op = HostOp::TimeNowMonotonic;
        let interface = op.interface();
        let owner = interface.owner();
        let clock = id(owner.path().trim_start_matches('@'))?;
        let time_error = HostErrorType::Time;
        let error_path = format!(
            "{}.{}",
            time_error.module().trim_start_matches('@'),
            time_error.name()
        );
        let optional = declared("std.option.Option", vec![Type::U64])?;
        let types = vec![
            definition(
                "std.option.Option",
                &["T"],
                TypeBody::Variant(vec![
                    Case::new("none", Vec::new()),
                    Case::new("some", vec![("value".to_owned(), Type::Binder(0))]),
                ]),
            )?,
            definition(
                "std.result.Result",
                &["T", "E"],
                TypeBody::Variant(vec![
                    Case::new("ok", vec![("value".to_owned(), Type::Binder(0))]),
                    Case::new("err", vec![("error".to_owned(), Type::Binder(1))]),
                ]),
            )?,
            definition(
                &error_path,
                &[],
                TypeBody::Variant(
                    time_error
                        .cases()
                        .into_iter()
                        .map(|case| Case::new(case, Vec::new()))
                        .collect(),
                ),
            )?,
            definition(
                "std.policy.GrantLimits",
                &[],
                TypeBody::Record(vec![
                    ("invocations".to_owned(), optional.clone()),
                    ("inputBytes".to_owned(), optional.clone()),
                    ("outputBytes".to_owned(), optional),
                ]),
            )?,
        ];
        let library = LibraryTypes::new()
            .with(Library::Option, id("std.option.Option")?)
            .with(Library::Result, id("std.result.Result")?)
            .with_narrowing(NarrowingType::GrantLimits, id("std.policy.GrantLimits")?);
        let now = SignatureRecord::interface_member(
            Vec::new(),
            Some(0),
            Type::Proc(Box::new(ProcType {
                parameters: vec![Type::SelfType],
                labels: NamedTypes::empty(),
                result: declared(
                    "std.result.Result",
                    vec![Type::U64, declared(&error_path, Vec::new())?],
                )?,
                effects: Row::new([clock.entity().clone()])?,
            })),
            Vec::new(),
        )?;
        let limit = SignatureRecord::interface_member(
            Vec::new(),
            Some(0),
            Type::function(FnType::new(
                vec![
                    Type::SelfType,
                    declared("std.policy.GrantLimits", Vec::new())?,
                ],
                Type::SelfType,
            )),
            Vec::new(),
        )?;
        let host = CheckedFunction {
            name: op.symbol().to_owned(),
            signature: now.clone(),
            defaults: Vec::new(),
            body: FunctionBody::Host(HostBinding::Operation(op)),
            entry_guard: None,
            exit_guard: None,
            slot_count: 1,
            origin: origin(),
            context: BodyContext::Pure,
        };
        let narrow = CheckedFunction::external(
            "grant.limit",
            limit.clone(),
            CompilerIntrinsic::GrantLimit,
            vec![Type::SelfType],
            origin(),
        );
        CheckedModuleSet::try_new(ModuleSetParts {
            library,
            types,
            interfaces: vec![
                InterfaceDefinition::new(
                    clock,
                    Vec::new(),
                    Vec::new(),
                    vec![InterfaceMember {
                        name: interface.member().to_owned(),
                        signature: now,
                        default: Some(NOW),
                    }],
                )?,
                InterfaceDefinition::new(
                    id("test.Narrow")?,
                    Vec::new(),
                    Vec::new(),
                    vec![InterfaceMember {
                        name: "limit".to_owned(),
                        signature: limit,
                        default: Some(LIMIT),
                    }],
                )?,
            ],
            functions: vec![host, narrow],
            ..ModuleSetParts::default()
        })
    }

    /// A context that logs, into `log`, each request, narrowing, and commit.
    struct Logging {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl HostContext for Logging {
        fn host(
            &mut self,
            _request: HostRequest<'_>,
        ) -> Result<HostCompletion, HostFault> {
            self.log.borrow_mut().push("request");
            Ok(HostCompletion {
                reply: HostReply::Ok(HostOk::U64(9)),
                receipt: Some(Receipt::new(1, 0)),
            })
        }

        fn commit(&mut self, _receipt: Receipt) -> Result<(), HostFault> {
            self.log.borrow_mut().push("commit");
            Ok(())
        }

        fn narrow(
            &mut self,
            _request: NarrowRequest<'_>,
        ) -> Result<Narrowed, HostFault> {
            self.log.borrow_mut().push("narrow");
            Ok(Narrowed::Grant(GrantToken::new(1, 2, 0)))
        }

        fn test_world(
            &mut self,
            _op: TestWorldOp,
            _world: GrantToken,
            _arguments: &[Value],
            _invocation: u64,
        ) -> Result<Issued, HostFault> {
            self.log.borrow_mut().push("world");
            Err(HostFault::InvalidHostValue("no world".to_owned()))
        }
    }

    fn function(index: usize) -> Resolved<'static> {
        Resolved {
            target: Target::Function(index),
            env: Rc::new(TypeEnv::default()),
        }
    }

    /// A record with a field, as a package's model of an interface holds:
    /// not a grant.
    fn model<'a>() -> Val<'a> {
        Val(Repr::Record(Rc::new(vec![("text", Val::string("x"))])))
    }

    /// Runs `index` on `args` in a machine with a `Logging` context, and
    /// gives the outcome and the log.
    fn run(
        index: usize,
        args: Vec<Val<'_>>,
        observe_delivery: bool,
    ) -> Result<(Result<Value, RuntimeError>, Vec<&'static str>), IrError> {
        let set = host_set()?;
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut context = Logging {
            log: Rc::clone(&log),
        };
        let budget = MemoryBudget::DEFAULT;
        let outcome = {
            let mut machine = Machine::new(&set, Mode::Test, budget.bytes, u64::MAX);
            machine.host.context = Some(&mut context);
            if observe_delivery {
                let delivered = Rc::clone(&log);
                machine.host.delivered =
                    Some(Box::new(move || delivered.borrow_mut().push("delivered")));
            }
            let outcome = machine
                .invoke(function(index), args, false, None)
                .and_then(|step| machine.execute(step));
            match outcome {
                Ok(value) => Ok(match &value.0 {
                    Repr::Case(case) => Value::Atom(case.case.to_owned()),
                    _ => Value::Void,
                }),
                Err(halt) => Err(runtime_error(halt, None, budget)),
            }
        };
        let log = log.borrow().clone();
        Ok((outcome, log))
    }

    /// Row "commit follows the continuation": the request, then the value
    /// delivered to the continuation, then the commit of its event.
    #[test]
    fn commit_follows_the_continuation() -> Result<(), IrError> {
        let grant = received(GrantToken::new(1, 1, 0));
        let (outcome, log) = run(NOW, vec![grant], true)?;
        assert_eq!(outcome, Ok(Value::Atom("ok".to_owned())));
        assert_eq!(log, ["request", "delivered", "commit"]);
        Ok(())
    }

    /// Row "host body with a model receiver": no checked set reaches a host
    /// body with a held value (an `impl` supplies every host member, and a
    /// default member is reached only through its interface), so the body
    /// is entered as the dispatch enters it, with a record:
    /// `@runtime.invalidCheckedProgram`, and no request.
    #[test]
    fn host_body_with_a_model_receiver() -> Result<(), IrError> {
        let (outcome, log) = run(NOW, vec![model()], false)?;
        assert!(
            matches!(
                outcome,
                Err(RuntimeError::Trap {
                    trap: Trap::InvalidCheckedProgram,
                    ..
                })
            ),
            "{outcome:?}"
        );
        assert!(log.is_empty(), "{log:?}");
        Ok(())
    }

    /// Row "a narrowing intrinsic with a model receiver": as for a host
    /// body, `grant.limit` entered with a record is
    /// `@runtime.invalidCheckedProgram`, and asks no narrowing.
    #[test]
    fn a_narrowing_intrinsic_with_a_model_receiver() -> Result<(), IrError> {
        let none = || Val::none();
        let limits = Val(Repr::Record(Rc::new(vec![
            ("inputBytes", none()),
            ("invocations", none()),
            ("outputBytes", none()),
        ])));
        let (outcome, log) = run(LIMIT, vec![model(), limits], false)?;
        assert!(
            matches!(
                outcome,
                Err(RuntimeError::Trap {
                    trap: Trap::InvalidCheckedProgram,
                    ..
                })
            ),
            "{outcome:?}"
        );
        assert!(log.is_empty(), "{log:?}");
        Ok(())
    }

    /// A value the host built whole enters the accounted footprint whole:
    /// a list of texts is its storage and every text's, a shared text once,
    /// and a budget that holds only the list's top is exhausted.
    #[test]
    fn a_host_built_value_is_accounted_whole() -> Result<(), IrError> {
        let text = Val::string("a text the host built, long enough to count");
        let shared = Val::list(vec![text.clone(), text.clone()]);
        let distinct = Val::list(vec![text.clone(), Val::string("another text")]);
        let top = |value: &Val<'_>| {
            value
                .storage()
                .map_or(0, |(_, bytes)| u64::try_from(bytes).unwrap_or(u64::MAX))
        };
        assert_eq!(whole_bytes(&shared), top(&shared) + top(&text));
        assert!(whole_bytes(&distinct) > whole_bytes(&shared));
        let set = host_set()?;
        let mut machine = Machine::new(&set, Mode::Test, top(&distinct) + 1, u64::MAX);
        assert!(matches!(
            machine.allocated_whole(&distinct),
            Err(Halt::Memory)
        ));
        let mut machine = Machine::new(&set, Mode::Test, 1 << 20, u64::MAX);
        assert!(machine.allocated_whole(&distinct).is_ok());
        Ok(())
    }

    /// A grant row with no host context is unavailable, and names the row.
    #[test]
    fn a_narrowing_without_a_context_is_unavailable() -> Result<(), IrError> {
        let set = host_set()?;
        let mut machine =
            Machine::new(&set, Mode::Test, MemoryBudget::DEFAULT.bytes, u64::MAX);
        let halt = machine.narrow(
            CompilerIntrinsic::GrantLimit,
            vec![received(GrantToken::new(1, 1, 0)), Val::void()],
        );
        assert!(
            matches!(&halt, Err(Halt::Unavailable(message)) if message.contains("grant.limit")),
            "{halt:?}"
        );
        Ok(())
    }
}
