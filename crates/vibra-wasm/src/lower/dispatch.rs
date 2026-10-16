//! Interface values, contract dispatch, and the conversions between them.
//!
//! # Interface values
//!
//! A value of an interface type, `any` included, is an arena `tuple` of two
//! cells: the **descriptor** of the type the value holds ([`crate::desc`]) and
//! the value itself. Widening writes the pair, and nothing else adds run-time
//! information to a value: a union has no method table, an `as` is erased, and
//! the pair is an ordinary arena value that is counted, moved, and stored like
//! every other. A value of a generic parameter is whatever its type argument
//! says, so an argument instantiated at an interface type holds a pair, and an
//! argument instantiated at the type the pair holds does not.
//!
//! # Dispatch
//!
//! A contract call selects its implementation from instantiated types, as the
//! reference interpreter does: the type the receiver holds, the interface
//! arguments of the call, and the member's own type arguments. The receiver's
//! type is the descriptor in an interface value, the type argument of a generic
//! parameter, the destination of a member selected by its destination, or the
//! descriptor the call builds of the receiver's static type. The candidates are
//! the functions the checker recorded as implementing the member, a written
//! implementation before a default, in program order. Each candidate is a run
//! of blocks that test the descriptors against the candidate's receiver and
//! interface arguments by one recursive scheme and bind the candidate's
//! generic parameters to the parts of the descriptors they meet; the first
//! candidate that holds is called, as any language function is, with the
//! operands and the type arguments its parameters name, and a call in tail
//! position replaces the frame. A candidate that fails leaves nothing behind.

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir::{CallTarget, Expr, SourceOrigin, Type, TypeId};
use wasm_encoder::{BlockType, Instruction as I};

use super::{
    Builder, ByteSource, Callee, FRAME, Move, Part, Place, SCRATCH, Term, UNRESOLVED,
};
use crate::desc::{CLASS_MASK, class};
use crate::form::NotLowered;
use crate::layout::{CellClass, Kind, frame, header};
use crate::runtime::{ValueClass, c32, get, ld8, ld32, set, st8};
use crate::types::{bind, function_params, head_key, params_of};

/// Where the argument of one of a callee's generic parameters comes from.
#[derive(Clone, Debug)]
pub(super) enum TypeSource {
    /// A type over the running activation's own parameters.
    Type(Type),
    /// A slot of the running frame that holds a descriptor.
    Slot(u32),
}

/// The slots the generic parameters of one candidate's patterns are bound in.
#[derive(Debug, Default)]
struct Binds {
    slots: BTreeMap<String, u32>,
    seen: BTreeSet<String>,
}

/// Whether a type is an interface type, `any` included.
pub(super) const fn is_interface(ty: &Type) -> bool {
    matches!(ty, Type::Interface(..) | Type::Any)
}

/// Whether a value that a callee returns at the type `from` is not yet a value
/// of the type `to` that the call expects: `to` holds an interface type where
/// `from` holds a type that is not one. A member that names `self` in its
/// result is called through an interface value as a member whose result is the
/// interface value, but the implementation returns the type it is for, so the
/// result is widened where the two differ, at any depth (`docs/spec/02-type-system.md`,
/// "Interfaces and methods"). A generic parameter of the callee that is the
/// whole result is an interface value or not as its type argument says, which
/// only the run time knows, so it is converted by a test of the descriptor; one
/// nested in the result is not, because the call fixed it from the types it
/// was given, and `from` has had the arguments the call fixed put in its place.
pub(super) fn conversion_needed(from: &Type, to: &Type) -> bool {
    needs_conversion(from, to, true)
}

fn needs_conversion(from: &Type, to: &Type, top: bool) -> bool {
    if is_interface(to) {
        return !is_interface(from) && (top || !matches!(from, Type::Param(_)));
    }
    let all = |from: &[Type], to: &[Type]| {
        from.len() == to.len()
            && from
                .iter()
                .zip(to)
                .any(|(from, to)| needs_conversion(from, to, false))
    };
    match (from, to) {
        (Type::Tuple(from), Type::Tuple(to)) | (Type::Union(from), Type::Union(to)) => {
            all(from, to)
        }
        (Type::Applied(left, from), Type::Applied(right, to)) => {
            left == right && all(from, to)
        }
        (Type::Array(from), Type::Array(to)) => needs_conversion(from, to, false),
        (Type::Dict(from_key, from_value), Type::Dict(to_key, to_value)) => {
            needs_conversion(from_key, to_key, false)
                || needs_conversion(from_value, to_value, false)
        }
        (Type::Record(from), Type::Record(to)) | (Type::Enum(from), Type::Enum(to)) => {
            from.len() == to.len()
                && from
                    .iter()
                    .zip(to)
                    .any(|((_, from), (_, to))| needs_conversion(from, to, false))
        }
        _ => false,
    }
}

/// The result type `from` of a callee with what the call fixed of its generic
/// parameters put in their place: the types over the running activation's own
/// parameters, and not the descriptors the call only holds at run time.
pub(super) fn with_fixed(from: &Type, bound: &BTreeMap<String, TypeSource>) -> Type {
    let fixed = bound
        .iter()
        .filter_map(|(name, source)| match source {
            TypeSource::Type(ty) => Some((name.clone(), ty.clone())),
            TypeSource::Slot(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    from.substitute(&fixed)
}

/// Whether `pattern` can never describe a value of the type `actual`, by the
/// constructor alone. Only constructors that both sides know are compared.
fn constructors_differ(pattern: &Type, actual: &Type) -> bool {
    let open =
        |ty: &Type| matches!(ty, Type::Param(_) | Type::Never) || is_interface(ty);
    if open(pattern) || open(actual) {
        return false;
    }
    if matches!((pattern, actual), (Type::Atom, Type::AtomSingleton(_))) {
        return false;
    }
    head_key(pattern) != head_key(actual)
}

impl Builder<'_, '_> {
    // -- slots --------------------------------------------------------------------

    /// Moves the cell and class byte of `from` into `to`, which owned nothing,
    /// and leaves `from` owning nothing.
    pub(super) fn move_slot(&mut self, to: u32, from: u32) {
        self.copy_cell(to, from);
        self.push(&[
            get(FRAME),
            get(FRAME),
            ld8(frame::class_offset(from)),
            st8(frame::class_offset(to)),
        ]);
        self.disown(from);
    }

    /// Pushes the head of the descriptor that `slot` holds, a reference.
    pub(super) fn push_head(&mut self, slot: u32) {
        let at = self.cell(slot);
        self.push(&[
            get(FRAME),
            ld32(at),
            set(SCRATCH),
            get(SCRATCH),
            get(SCRATCH),
            ld32(header::LEN),
            c32(7),
            I::I32Add,
            c32(-8),
            I::I32And,
            I::I32Add,
            ld32(header::SIZE),
        ]);
    }

    /// Pushes `1` when `slot` holds a descriptor of the given class, and `0`
    /// when it holds a descriptor of another class or none at all.
    pub(super) fn push_class_is(&mut self, slot: u32, class: u32) {
        self.push(&[
            get(FRAME),
            ld8(frame::class_offset(slot)),
            c32(i32::from(CellClass::Ref.code())),
            I::I32Eq,
            I::If(BlockType::Result(wasm_encoder::ValType::I32)),
        ]);
        self.push_head(slot);
        self.push(&[
            c32(CLASS_MASK.cast_signed()),
            I::I32And,
            c32(class.cast_signed()),
            I::I32Eq,
            I::Else,
            c32(0),
            I::End,
        ]);
    }

    // -- interface values ---------------------------------------------------------

    /// Widens `value` to an interface type: the pair of the descriptor of the
    /// value's own type and the value.
    pub(super) fn widen_interface(
        &mut self,
        value: &Expr,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let from = value.result_type();
        let mark = self.next_temp;
        let slot = self.temp();
        self.expr(value, slot)?;
        self.to_interface(slot, &from, dest)?;
        self.release_to(mark);
        Ok(())
    }

    /// Hands the value in `slot`, of the type `from`, to `dest` as an interface
    /// value. A value that is one already is moved, whatever interface it was
    /// widened to, because the pair names the type it holds and not the
    /// interface it was reached by.
    pub(super) fn to_interface(
        &mut self,
        slot: u32,
        from: &Type,
        dest: u32,
    ) -> Result<(), NotLowered> {
        match from {
            Type::Interface(..) | Type::Any => self.move_slot(dest, slot),
            Type::Param(name) => {
                // A parameter instantiated at an interface type holds a pair
                // already, and one instantiated at any other type does not.
                let desc = self.temp();
                self.type_argument(name, desc);
                self.push_class_is(desc, class::INTERFACE);
                let (held, wrap, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.end(Term::Branch {
                    then_block: held,
                    else_block: wrap,
                });
                self.start(held);
                self.discard(desc, ValueClass::Dyn);
                self.move_slot(dest, slot);
                self.goto(join, wrap);
                self.wrap_pair(desc, slot, dest);
                self.goto(join, join);
            }
            other => {
                let desc = self.temp();
                self.build_type(other, desc)?;
                self.wrap_pair(desc, slot, dest);
            }
        }
        Ok(())
    }

    /// Builds the interface value of the descriptor in `desc` and the value in
    /// `slot`, moving both.
    pub(super) fn wrap_pair(&mut self, desc: u32, slot: u32, dest: u32) {
        self.build(
            Kind::Tuple,
            0,
            &[
                Part::Slot {
                    slot: desc,
                    class: ValueClass::Dyn,
                },
                Part::Slot {
                    slot,
                    class: ValueClass::Dyn,
                },
            ],
            dest,
        );
    }

    /// Splits the interface value in `pair` into an owned descriptor in `desc`
    /// and an owned value in `payload`, and drops the pair.
    pub(super) fn unwrap_pair(&mut self, pair: u32, desc: u32, payload: u32) {
        self.copy_part(desc, pair, 0, 2, ValueClass::Dyn);
        self.copy_part(payload, pair, 1, 2, ValueClass::Dyn);
        self.drop_slot(pair);
    }

    // -- matching descriptors -----------------------------------------------------

    /// Pushes `1` when the descriptor at `place` is present.
    fn place_resolved(&mut self, place: Place) {
        let reference = i32::from(CellClass::Ref.code());
        match place {
            Place::Slot(slot) => self.push(&[
                get(FRAME),
                ld8(frame::class_offset(slot)),
                c32(reference),
                I::I32Eq,
            ]),
            Place::Component {
                object, position, ..
            } => {
                let held = self.cell(object);
                self.push(&[
                    get(FRAME),
                    ld32(held),
                    ld8(header::SIZE + position),
                    c32(reference),
                    I::I32Eq,
                ]);
            }
        }
    }

    /// Pushes the offset of the descriptor at `place`, or `0` when it is absent.
    fn push_descriptor(&mut self, place: Place) {
        self.push_cell_of(place);
        self.push(&[c32(0)]);
        self.place_resolved(place);
        self.push(&[I::Select]);
    }

    fn push_cell_of(&mut self, place: Place) {
        match place {
            Place::Slot(slot) => {
                let at = self.cell(slot);
                self.push(&[get(FRAME), ld32(at)]);
            }
            Place::Component {
                object,
                position,
                len,
            } => {
                let held = self.cell(object);
                self.push(&[
                    get(FRAME),
                    ld32(held),
                    ld32(crate::layout::cells_offset(len) + 8 * position),
                ]);
            }
        }
    }

    /// The slot that holds the descriptor at `place` as a reference this code
    /// does not own.
    fn place_object(&mut self, place: Place) -> u32 {
        match place {
            Place::Slot(slot) => slot,
            Place::Component { .. } => {
                let slot = self.temp();
                self.copy_place(slot, place);
                self.disown(slot);
                slot
            }
        }
    }

    /// Tests the descriptor at `place` against `pattern`, a type over generic
    /// parameters, and binds each parameter the pattern names to the part of
    /// the descriptor it meets. A test that fails jumps to `fail`. A part the
    /// run time did not fix agrees with any pattern and binds nothing below it,
    /// as the reference interpreter's binding does, and a pattern that is an
    /// interface type admits any type.
    fn match_descriptor(
        &mut self,
        pattern: &Type,
        place: Place,
        fail: u32,
        binds: &mut Binds,
    ) -> Result<(), NotLowered> {
        match pattern {
            Type::Param(name) => self.bind_descriptor(name, place, fail, binds),
            Type::Interface(..) | Type::Any => Ok(()),
            _ => {
                let (go, skip, after, check) = (
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                );
                self.place_resolved(place);
                self.end(Term::Branch {
                    then_block: go,
                    else_block: skip,
                });
                self.start(go);
                let object = self.place_object(place);
                // A descriptor of `never` agrees with any pattern.
                self.push_head(object);
                self.push(&[
                    c32(CLASS_MASK.cast_signed()),
                    I::I32And,
                    c32(class::NEVER.cast_signed()),
                    I::I32Eq,
                ]);
                self.end(Term::Branch {
                    then_block: after,
                    else_block: check,
                });
                self.start(check);
                let components = pattern.components();
                let len = u32::try_from(components.len() + 1).unwrap_or(u32::MAX);
                if matches!(pattern, Type::Atom) {
                    // An atom admits the singleton of any written atom.
                    self.push_head(object);
                    self.push(&[
                        c32(CLASS_MASK.cast_signed()),
                        I::I32And,
                        set(SCRATCH),
                        get(SCRATCH),
                        c32(class::ATOM.cast_signed()),
                        I::I32Eq,
                        get(SCRATCH),
                        c32(class::SINGLETON.cast_signed()),
                        I::I32Eq,
                        I::I32Or,
                    ]);
                } else {
                    let head = self.lowering.head(pattern);
                    self.push_head(object);
                    self.push(&[c32(head.cast_signed()), I::I32Eq]);
                    let held = self.cell(object);
                    self.push(&[
                        get(FRAME),
                        ld32(held),
                        ld32(header::LEN),
                        c32(len.cast_signed()),
                        I::I32Eq,
                        I::I32And,
                    ]);
                }
                self.continue_unless(fail);
                for (position, component) in (1_u32..).zip(&components) {
                    self.match_descriptor(
                        component,
                        Place::Component {
                            object,
                            position,
                            len,
                        },
                        fail,
                        binds,
                    )?;
                }
                self.goto(after, skip);
                self.goto(after, after);
                Ok(())
            }
        }
    }

    /// Binds the generic parameter `name` to the descriptor at `place`, or, when
    /// it is already bound, requires the two to be the same type.
    fn bind_descriptor(
        &mut self,
        name: &str,
        place: Place,
        fail: u32,
        binds: &mut Binds,
    ) -> Result<(), NotLowered> {
        let Some(slot) = binds.slots.get(name).copied() else {
            return Err(NotLowered::single(crate::Form::Call));
        };
        if !binds.seen.insert(name.to_owned()) {
            let equal = self.lowering.desc_equal();
            self.push_descriptor(Place::Slot(slot));
            self.push_descriptor(place);
            self.push(&[I::Call(equal)]);
            self.continue_unless(fail);
            return Ok(());
        }
        self.copy_place(slot, place);
        let source = match place {
            Place::Slot(from) => ByteSource::Slot(from),
            Place::Component {
                object, position, ..
            } => ByteSource::Component {
                slot: object,
                position,
            },
        };
        self.claim(slot, ValueClass::Dyn, source);
        Ok(())
    }

    // -- the call -----------------------------------------------------------------

    /// The functions that implement `member` of `interface`, in the order a
    /// call tries them: a written implementation before a default, and each in
    /// program order.
    fn candidates(&self, interface: &TypeId, member: &str) -> Vec<usize> {
        let program = self.lowering.program;
        let matching = program
            .functions()
            .iter()
            .enumerate()
            .filter_map(|(position, function)| {
                let implements = function.implements()?;
                (implements.interface == *interface && implements.member == member)
                    .then_some((
                        position,
                        matches!(implements.receiver, Type::Param(_)),
                    ))
            })
            .collect::<Vec<_>>();
        matching
            .iter()
            .filter(|(_, default)| !default)
            .chain(matching.iter().filter(|(_, default)| *default))
            .map(|(position, _)| *position)
            .collect()
    }

    /// A call of a contract member: the operands are evaluated, the candidates
    /// are tried in order, and the first whose receiver and interface arguments
    /// describe the types the call dispatches on is called.
    #[allow(clippy::too_many_lines)]
    pub(super) fn contract_call(
        &mut self,
        target: &CallTarget,
        arguments: &[Expr],
        result: &Type,
        tail: bool,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let CallTarget::Contract {
            interface,
            member,
            receiver,
            arguments: interface_arguments,
            member_types,
            destination,
            closed,
            ..
        } = target
        else {
            return Err(self.forms_of_call(origin));
        };
        let mark = self.next_temp;

        // The operands, in the order of the member's parameters.
        let mut operands = Vec::with_capacity(arguments.len());
        for argument in arguments {
            self.class(&argument.result_type(), argument.origin())?;
            let slot = self.temp();
            self.expr(argument, slot)?;
            operands.push(slot);
        }

        // The type the call dispatches on.
        let receiver_type = match destination {
            Some(_) => None,
            None => Some(
                arguments
                    .get(*receiver)
                    .ok_or_else(|| self.forms_of_call(origin))?
                    .result_type(),
            ),
        };
        let recv_desc = self.temp();
        match (destination, &receiver_type) {
            (Some(destination), _) => self.build_type(destination, recv_desc)?,
            (None, Some(Type::Interface(..) | Type::Any)) => {
                let payload = self.temp();
                let held = *operands
                    .get(*receiver)
                    .ok_or_else(|| self.forms_of_call(origin))?;
                self.unwrap_pair(held, recv_desc, payload);
                if let Some(entry) = operands.get_mut(*receiver) {
                    *entry = payload;
                }
            }
            (None, Some(Type::Param(name))) => {
                // A parameter instantiated at an interface type holds a pair,
                // whose descriptor names the type the receiver is.
                self.type_argument(name, recv_desc);
                self.push_class_is(recv_desc, class::INTERFACE);
                let (unwrap, join) = (self.new_block(), self.new_block());
                self.end(Term::Branch {
                    then_block: unwrap,
                    else_block: join,
                });
                self.start(unwrap);
                let held = *operands
                    .get(*receiver)
                    .ok_or_else(|| self.forms_of_call(origin))?;
                let (inner, payload) = (self.temp(), self.temp());
                self.unwrap_pair(held, inner, payload);
                self.discard(recv_desc, ValueClass::Dyn);
                self.move_slot(recv_desc, inner);
                self.move_slot(held, payload);
                self.goto(join, join);
            }
            (None, Some(other)) => self.build_type(other, recv_desc)?,
            (None, None) => return Err(self.forms_of_call(origin)),
        }
        let mut argument_descs = Vec::with_capacity(interface_arguments.len());
        for argument in interface_arguments {
            let slot = self.temp();
            self.build_type(argument, slot)?;
            argument_descs.push(slot);
        }
        let known = destination.as_ref().or(receiver_type.as_ref());
        let known = known.filter(|ty| !is_interface(ty));

        let after = self.new_block();
        self.try_candidates(&Dispatch {
            interface,
            member,
            interface_arguments,
            member_types,
            operands: &operands,
            recv_desc,
            argument_descs: &argument_descs,
            known,
            receiver: if destination.is_none() {
                Some(*receiver)
            } else {
                None
            },
            arguments,
            result,
            tail,
            after,
            origin,
            dest,
            written_only: false,
        })?;

        // No written implementation or default holds: the conformance the
        // toolchain answers for the closed types, or the toolchain's defect.
        match closed {
            Some(
                kind @ (vibra_ir::ClosedContract::KeyCompare
                | vibra_ir::ClosedContract::KeyEqual),
            ) => {
                self.closed_key_call(
                    *kind,
                    &operands,
                    recv_desc,
                    &argument_descs,
                    result,
                    origin,
                    dest,
                )?;
            }
            Some(vibra_ir::ClosedContract::IterNext) => {
                self.closed_next(
                    &operands,
                    recv_desc,
                    &argument_descs,
                    result,
                    origin,
                    dest,
                )?;
            }
            None => {
                let stop = self.lowering.fns.stop_trap;
                self.push(&[
                    c32(vibra_ir::boundary::TrapCode::InvalidCheckedProgram.code()),
                    I::Call(stop),
                ]);
            }
        }
        self.goto(after, after);
        self.release_to(mark);
        Ok(())
    }

    /// Tries the candidates in order. The block that is current when it
    /// returns is the one no candidate held in; every candidate that holds ends
    /// its own block with a call.
    pub(super) fn try_candidates(
        &mut self,
        d: &Dispatch<'_>,
    ) -> Result<(), NotLowered> {
        let program = self.lowering.program;
        let module_size = || NotLowered::single(crate::Form::ModuleSize);
        let first = self.new_block();
        self.goto(first, first);
        for candidate in self.candidates(d.interface, d.member) {
            let function =
                program.functions().get(candidate).ok_or_else(module_size)?;
            let Some(implements) = function.implements() else {
                continue;
            };
            // A candidate the types decide before the program runs is left out.
            if d.written_only && matches!(implements.receiver, Type::Param(_)) {
                continue;
            }
            if !d.interface_arguments.is_empty()
                && implements.arguments.len() != d.interface_arguments.len()
            {
                continue;
            }
            if d.known
                .is_some_and(|ty| constructors_differ(&implements.receiver, ty))
            {
                continue;
            }
            let candidate_mark = self.next_temp;
            let fail = self.new_block();
            let next = self.new_block();

            // The parameters the patterns bind start out empty.
            let mut names = BTreeSet::new();
            params_of(&implements.receiver, &mut names);
            if !d.interface_arguments.is_empty() {
                for argument in &implements.arguments {
                    params_of(argument, &mut names);
                }
            }
            let mut binds = Binds::default();
            for name in &names {
                let slot = self.temp();
                self.store_bits(slot, 0, ValueClass::Void);
                binds.slots.insert(name.clone(), slot);
            }
            self.match_descriptor(
                &implements.receiver,
                Place::Slot(d.recv_desc),
                fail,
                &mut binds,
            )?;
            if !d.interface_arguments.is_empty() {
                for (pattern, desc) in implements.arguments.iter().zip(d.argument_descs)
                {
                    self.match_descriptor(
                        pattern,
                        Place::Slot(*desc),
                        fail,
                        &mut binds,
                    )?;
                }
            }

            // The candidate holds: call it.
            self.commit_candidate(
                candidate,
                &binds,
                d.member_types,
                &mut d.operands.to_vec(),
                &Commit {
                    arguments: d.arguments,
                    result: d.result,
                    tail: d.tail,
                    receiver: d.receiver,
                    recv_desc: d.recv_desc,
                    argument_descs: d.argument_descs,
                    after: d.after,
                    origin: d.origin,
                    dest: d.dest,
                },
            )?;

            // The candidate failed: it bound what it met before the test that
            // failed, and gives that back.
            self.start(fail);
            for slot in binds.slots.values() {
                self.discard(*slot, ValueClass::Dyn);
            }
            self.release_to(candidate_mark);
            self.goto(next, next);
        }
        Ok(())
    }

    /// The form that reports a call the lowering cannot make.
    fn forms_of_call(&self, origin: &SourceOrigin) -> NotLowered {
        NotLowered::from_uses(vec![
            crate::form::UnloweredForm::new(crate::Form::Call, Some(origin.clone()))
                .with_detail("contract"),
        ])
        .unwrap_or_else(|| NotLowered::single(crate::Form::Call))
    }

    /// Calls candidate `candidate`: the type arguments of its parameters, the
    /// operands moved into its frame, and either a call that continues at a block
    /// which takes the result or a tail call.
    #[allow(clippy::too_many_lines)]
    fn commit_candidate(
        &mut self,
        candidate: usize,
        binds: &Binds,
        member_types: &[Type],
        operands: &mut [u32],
        commit: &Commit<'_>,
    ) -> Result<(), NotLowered> {
        let program = self.lowering.program;
        let module_size = || NotLowered::single(crate::Form::ModuleSize);
        let function = program.functions().get(candidate).ok_or_else(module_size)?;
        let Some(implements) = function.implements() else {
            return Err(module_size());
        };
        let signature = function.signature();
        let params = function_params(function);
        let slot_types = signature.slot_types();

        // What each generic parameter of the candidate is: the parts of the
        // descriptors it was matched against, then the member's own written
        // types, and then, for any that remain, what the operands and the
        // result fix, as the reference interpreter binds a call.
        let mut bound: BTreeMap<String, TypeSource> = BTreeMap::new();
        for (name, slot) in &binds.slots {
            bound.insert(name.clone(), TypeSource::Slot(*slot));
        }
        for (name, written) in implements.member_generics.iter().zip(member_types) {
            if let Some(name) = name {
                bound.insert(name.clone(), TypeSource::Type(written.clone()));
            }
        }
        let mut unwrapped = Vec::new();
        for (position, (operand, argument)) in
            operands.iter_mut().zip(commit.arguments).enumerate()
        {
            let argument_type = argument.result_type();
            let slot_type = slot_types.get(position);
            let is_receiver = commit.receiver == Some(position);
            if is_interface(&argument_type) && !slot_type.is_some_and(is_interface) {
                // The callee takes the value itself, so a pair is opened. The
                // receiver's pair was opened before the candidates.
                let desc = if is_receiver {
                    commit.recv_desc
                } else {
                    let (desc, payload) = (self.temp(), self.temp());
                    self.unwrap_pair(*operand, desc, payload);
                    *operand = payload;
                    unwrapped.push(desc);
                    desc
                };
                if let Some(Type::Param(name)) = slot_type {
                    bound
                        .entry(name.clone())
                        .or_insert_with(|| TypeSource::Slot(desc));
                }
                continue;
            }
            if let Some(slot_type) = slot_type {
                let mut inferred = BTreeMap::new();
                bind(slot_type, &argument_type, &mut inferred);
                for (name, ty) in inferred {
                    bound.entry(name).or_insert(TypeSource::Type(ty));
                }
            }
        }
        let mut inferred = BTreeMap::new();
        bind(&signature.result(), commit.result, &mut inferred);
        for (name, ty) in inferred {
            bound.entry(name).or_insert(TypeSource::Type(ty));
        }

        // The type arguments of the callee's parameters, built before the
        // descriptors they came from are given back.
        let arity = u32::try_from(operands.len()).map_err(|_| module_size())?;
        let mut moves = Vec::with_capacity(operands.len() + params.len());
        for (position, operand) in (0_u32..).zip(operands.iter()) {
            moves.push(Move {
                from: *operand,
                to: position,
                only_below: None,
            });
        }
        for (position, name) in (0_u32..).zip(&params) {
            let slot = self.temp();
            match bound.get(name) {
                Some(TypeSource::Type(ty)) => self.build_type(ty, slot)?,
                Some(TypeSource::Slot(from)) => {
                    self.copy_cell(slot, *from);
                    self.claim(slot, ValueClass::Dyn, ByteSource::Slot(*from));
                }
                None => {
                    self.build_type(&Type::Param(UNRESOLVED.to_owned()), slot)?;
                }
            }
            moves.push(Move {
                from: slot,
                to: arity + 1 + position,
                only_below: None,
            });
        }
        // The descriptors the dispatch used are given back, unless the result
        // is converted afterwards by what they say; a callee holds its own
        // counts.
        let result_type = with_fixed(&signature.result(), &bound);
        let converts = conversion_needed(&result_type, commit.result);
        let mut spent: Vec<u32> = binds.slots.values().copied().collect();
        spent.extend(unwrapped);
        spent.extend(commit.argument_descs.iter().copied());
        spent.push(commit.recv_desc);
        if !converts {
            for slot in &spent {
                self.discard(*slot, ValueClass::Dyn);
            }
        }

        let callee = Callee::Named {
            function: u32::try_from(candidate).map_err(|_| module_size())?,
            slots: self.lowering.sizes.get(candidate).copied().unwrap_or(0),
        };
        self.class(&result_type, commit.origin)?;
        // A call whose result is converted is not the last thing the
        // activation does, whatever position the checker found it in.
        if commit.tail && !converts {
            self.stage = self.stage.max(moves.len());
            self.end(Term::Tail {
                callee,
                moves,
                arity,
            });
            return Ok(());
        }
        let resume = self.new_block();
        self.end(Term::Call {
            callee,
            moves,
            arity,
            resume,
        });
        self.start(resume);
        self.take_return(commit.dest);
        if converts {
            self.convert(
                commit.dest,
                &result_type,
                commit.result,
                &Convert {
                    receiver: Some(&implements.receiver),
                    recv_desc: Some(commit.recv_desc),
                    bound: &bound,
                    origin: commit.origin,
                },
            )?;
            for slot in &spent {
                self.discard(*slot, ValueClass::Dyn);
            }
        }
        self.end(Term::Goto(commit.after));
        Ok(())
    }
}

impl Builder<'_, '_> {
    /// Converts the result of a call of a module function or of a function
    /// value, which has no receiver, from the callee's result type to the
    /// type the call expects.
    pub(super) fn convert_result(
        &mut self,
        slot: u32,
        from: &Type,
        to: &Type,
        bound: &BTreeMap<String, TypeSource>,
        origin: &SourceOrigin,
    ) -> Result<(), NotLowered> {
        self.convert(
            slot,
            from,
            to,
            &Convert {
                receiver: None,
                recv_desc: None,
                bound,
                origin,
            },
        )
    }
}

/// What the conversion of a returned value knows of the call that returned it.
struct Convert<'a> {
    /// The receiver type the implementation is for. Where its result names
    /// that type, the value is the receiver's type.
    receiver: Option<&'a Type>,
    /// The descriptor of the receiver's type.
    recv_desc: Option<u32>,
    /// What the implementation's generic parameters were at the call.
    bound: &'a BTreeMap<String, TypeSource>,
    origin: &'a SourceOrigin,
}

impl Builder<'_, '_> {
    /// Converts the value in `slot`, which the callee returned at the type
    /// `from`, to the type `to` that the call expects: every place `to` has an
    /// interface type where `from` does not is widened.
    fn convert(
        &mut self,
        slot: u32,
        from: &Type,
        to: &Type,
        ctx: &Convert<'_>,
    ) -> Result<(), NotLowered> {
        if !conversion_needed(from, to) {
            return Ok(());
        }
        if is_interface(to) {
            return self.convert_leaf(slot, from, ctx);
        }
        let unsupported = || {
            NotLowered::from_uses(vec![
                crate::form::UnloweredForm::new(
                    crate::Form::Call,
                    Some(ctx.origin.clone()),
                )
                .with_detail("interface-result"),
            ])
            .unwrap_or_else(|| NotLowered::single(crate::Form::Call))
        };
        match (from, to) {
            (Type::Tuple(from_parts), Type::Tuple(to_parts)) => {
                let mark = self.next_temp;
                let mut parts = Vec::with_capacity(from_parts.len());
                let len = u32::try_from(from_parts.len()).unwrap_or(u32::MAX);
                for (position, (from_part, to_part)) in
                    (0_u32..).zip(from_parts.iter().zip(to_parts))
                {
                    let from_class = self.class(from_part, ctx.origin)?;
                    let to_class = self.class(to_part, ctx.origin)?;
                    let part = self.temp();
                    self.copy_part(part, slot, position, len, from_class);
                    self.convert(part, from_part, to_part, ctx)?;
                    parts.push(Part::Slot {
                        slot: part,
                        class: to_class,
                    });
                }
                self.drop_slot(slot);
                self.build(Kind::Tuple, 0, &parts, slot);
                self.release_to(mark);
                Ok(())
            }
            (Type::Applied(..) | Type::Enum(_), Type::Applied(..) | Type::Enum(_)) => {
                let (
                    crate::types::Shape::Enum(from_variants),
                    crate::types::Shape::Enum(to_variants),
                ) = (self.shape(from, ctx.origin)?, self.shape(to, ctx.origin)?)
                else {
                    return Err(unsupported());
                };
                let join = self.new_block();
                for (discriminant, ((_, from_payload), (_, to_payload))) in
                    (0_u32..).zip(from_variants.iter().zip(&to_variants))
                {
                    if *from_payload == Type::Void
                        || !conversion_needed(from_payload, to_payload)
                    {
                        continue;
                    }
                    let (convert, next) = (self.new_block(), self.new_block());
                    let at = self.cell(slot);
                    self.push(&[
                        get(FRAME),
                        ld32(at),
                        ld32(header::VARIANT),
                        c32(discriminant.cast_signed()),
                        I::I32Eq,
                    ]);
                    self.end(Term::Branch {
                        then_block: convert,
                        else_block: next,
                    });
                    self.start(convert);
                    let mark = self.next_temp;
                    let from_class = self.class(from_payload, ctx.origin)?;
                    let to_class = self.class(to_payload, ctx.origin)?;
                    let part = self.temp();
                    self.copy_part(part, slot, 0, 1, from_class);
                    self.convert(part, from_payload, to_payload, ctx)?;
                    let rebuilt = self.temp();
                    self.build(
                        Kind::Enum,
                        discriminant,
                        &[Part::Slot {
                            slot: part,
                            class: to_class,
                        }],
                        rebuilt,
                    );
                    self.drop_slot(slot);
                    self.move_slot(slot, rebuilt);
                    self.release_to(mark);
                    self.goto(join, next);
                }
                self.goto(join, join);
                Ok(())
            }
            _ => Err(unsupported()),
        }
    }

    /// Widens the value in `slot`, of the type `from`, which the call expects
    /// as an interface value.
    fn convert_leaf(
        &mut self,
        slot: u32,
        from: &Type,
        ctx: &Convert<'_>,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let desc = self.temp();
        let described = |this: &mut Self| -> Result<(), NotLowered> {
            if let (Some(receiver), Some(recv_desc)) = (ctx.receiver, ctx.recv_desc)
                && from == receiver
            {
                // The result names the type the implementation is for.
                this.copy_cell(desc, recv_desc);
                this.claim(desc, ValueClass::Dyn, ByteSource::Slot(recv_desc));
                return Ok(());
            }
            if let Type::Param(name) = from
                && let Some(source) = ctx.bound.get(name)
            {
                match source {
                    TypeSource::Type(ty) => this.build_type(ty, desc)?,
                    TypeSource::Slot(from) => {
                        this.copy_cell(desc, *from);
                        this.claim(desc, ValueClass::Dyn, ByteSource::Slot(*from));
                    }
                }
                return Ok(());
            }
            let mut arguments = BTreeMap::new();
            for (name, source) in ctx.bound {
                if let TypeSource::Type(ty) = source {
                    arguments.insert(name.clone(), ty.clone());
                }
            }
            this.build_type(&from.substitute(&arguments), desc)
        };
        described(self)?;
        let held = self.temp();
        self.move_slot(held, slot);
        if matches!(from, Type::Param(_)) {
            // A parameter instantiated at an interface type holds a pair
            // already.
            self.push_class_is(desc, class::INTERFACE);
            let (kept, wrap, join) =
                (self.new_block(), self.new_block(), self.new_block());
            self.end(Term::Branch {
                then_block: kept,
                else_block: wrap,
            });
            self.start(kept);
            self.discard(desc, ValueClass::Dyn);
            self.move_slot(slot, held);
            self.goto(join, wrap);
            self.wrap_pair(desc, held, slot);
            self.goto(join, join);
        } else {
            self.wrap_pair(desc, held, slot);
        }
        self.release_to(mark);
        Ok(())
    }
}

/// Everything the dispatch of one contract call is made of.
pub(super) struct Dispatch<'a> {
    pub(super) interface: &'a TypeId,
    pub(super) member: &'a str,
    pub(super) interface_arguments: &'a [Type],
    pub(super) member_types: &'a [Type],
    /// The operand slots, with the receiver's pair already opened.
    pub(super) operands: &'a [u32],
    /// The descriptor of the type the call dispatches on.
    pub(super) recv_desc: u32,
    pub(super) argument_descs: &'a [u32],
    /// The static type the call dispatches on, when it has one that is not an
    /// interface.
    pub(super) known: Option<&'a Type>,
    pub(super) receiver: Option<usize>,
    pub(super) arguments: &'a [Expr],
    pub(super) result: &'a Type,
    pub(super) tail: bool,
    pub(super) after: u32,
    pub(super) origin: &'a SourceOrigin,
    pub(super) dest: u32,
    /// Whether a default member is left out.
    pub(super) written_only: bool,
}

/// What a candidate's call needs of the call that tries it.
struct Commit<'a> {
    arguments: &'a [Expr],
    result: &'a Type,
    tail: bool,
    /// The operand that selected the implementation, when one did.
    receiver: Option<usize>,
    recv_desc: u32,
    argument_descs: &'a [u32],
    after: u32,
    origin: &'a SourceOrigin,
    dest: u32,
}
