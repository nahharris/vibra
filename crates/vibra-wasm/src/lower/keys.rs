//! Ordering two keys by the descriptor of their type.
//!
//! A key whose type is known only at run time, or that holds a declared type
//! with its own `compare` below its surface, cannot be ordered by a routine
//! the module writes beforehand, because the order calls Vibra: a declared
//! type's `compare` is a language function, and it may build or search dicts
//! keyed by the same type. So the order is a language function itself, the
//! **key unit**, `(a, b, type) -> i32`, one in a module that needs it. It reads
//! the head of the descriptor `type` and compares a scalar, a text, or a
//! `bool` directly, compares the components of a tuple or a record in order by
//! calling itself on each, compares the discriminants of an enum or a union and
//! then the payload they both hold by calling itself, and orders a declared
//! type by the `compare` its program writes for it, selected from the
//! descriptor as every contract call is. It answers `-1`, `0`, or `1`.
//!
//! The unit calls itself and the implementations as any activation calls
//! another, on frames in the arena, so the depth of a comparison is the nesting
//! of the type and the work of a user's `compare`, and never the engine's
//! stack, and a native import is never asked to call back into the module.
//! `ordered.compare` and `equatable.equal` of a closed key type, the conformance
//! the toolchain answers without a written implementation, are calls of the
//! unit, and so is the order of a dict whose key type needs dispatch.

use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{ClosedContract, SourceOrigin, Type};
use wasm_encoder::Instruction as I;

use super::dispatch::{Dispatch, is_interface};
use super::{Builder, ByteSource, Callee, FRAME, KeyAt, Move, SCRATCH, Term, Variant};
use crate::desc::{CLASS_MASK, class};
use crate::form::NotLowered;
use crate::layout::{CellClass, Kind, frame, header};
use crate::runtime::{Ins, ValueClass, c32, get, ld8, ld32, ld64, set, st64};

/// The slot of the first operand of the key unit.
const A: u32 = 0;
/// The slot of its second operand.
const B: u32 = 1;
/// The slot of the descriptor of the type of the keys.
const T: u32 = 3;

impl Builder<'_, '_> {
    /// Stores the 32-bit value the instructions push, sign-extended, in the
    /// scalar slot `slot`.
    fn store_signed(&mut self, slot: u32, value: &[Ins]) {
        let at = self.cell(slot);
        self.push(&[get(FRAME)]);
        self.push(value);
        self.push(&[I::I64ExtendI32S, st64(at)]);
        self.set_byte(slot, CellClass::I32.code());
    }

    /// Answers the 32-bit value the instructions push and leaves the activation.
    fn answer(&mut self, result: u32, value: &[Ins]) {
        self.store_signed(result, value);
        self.end(Term::Return { slot: result });
    }

    /// Branches on the head of the descriptor in `T` having one of `classes`:
    /// the block that follows is the one for the type that has, and the block
    /// returned is the one for any other.
    fn when_class(&mut self, classes: &[u32]) -> u32 {
        let (held, next) = (self.new_block(), self.new_block());
        self.push_head(T);
        self.push(&[c32(CLASS_MASK.cast_signed()), I::I32And, set(SCRATCH)]);
        for (position, class) in classes.iter().enumerate() {
            self.push(&[get(SCRATCH), c32((*class).cast_signed()), I::I32Eq]);
            if position > 0 {
                self.push(&[I::I32Or]);
            }
        }
        self.end(Term::Branch {
            then_block: held,
            else_block: next,
        });
        self.start(held);
        next
    }

    /// Copies the element at the index in the low 32 bits of `index` of the
    /// cells object in `object` into `to`, which then owns it.
    pub(super) fn load_indexed(&mut self, to: u32, object: u32, index: u32) {
        let at = self.cell(to);
        self.push(&[get(FRAME)]);
        self.push_element_address(object, index);
        self.push(&[ld64(header::SIZE), st64(at)]);
        self.claim(to, ValueClass::Dyn, ByteSource::Indexed { object, index });
    }

    /// Calls the key unit on the keys in `a` and `b` and the descriptor in
    /// `descriptor`, all three of which it takes, and leaves its answer in `out`
    /// in the block that follows.
    pub(super) fn call_unit(&mut self, a: u32, b: u32, descriptor: u32, out: u32) {
        let (function, slots) = self.lowering.unit_function();
        let resume = self.new_block();
        self.stage = self.stage.max(3);
        self.end(Term::Call {
            callee: Callee::Named { function, slots },
            moves: vec![
                Move {
                    from: a,
                    to: 0,
                    only_below: None,
                },
                Move {
                    from: b,
                    to: 1,
                    only_below: None,
                },
                Move {
                    from: descriptor,
                    to: T,
                    only_below: None,
                },
            ],
            arity: 2,
            resume,
        });
        self.start(resume);
        self.take_return(out);
    }

    /// The body of the key unit.
    #[allow(clippy::too_many_lines)]
    pub(super) fn key_unit(&mut self) -> Result<(), NotLowered> {
        let program = self.lowering.program;
        let origin = program.entry().origin().clone();
        let result = self.temp();
        let entry = self.new_block();
        self.start(entry);

        // A type nothing fixed orders nothing.
        let (resolved, unresolved) = (self.new_block(), self.new_block());
        self.push(&[
            get(FRAME),
            ld8(frame::class_offset(T)),
            c32(i32::from(CellClass::Ref.code())),
            I::I32Eq,
        ]);
        self.end(Term::Branch {
            then_block: resolved,
            else_block: unresolved,
        });
        self.start(unresolved);
        self.answer(result, &[c32(0)]);
        self.start(resolved);

        // The scalars: the greater minus the less.
        let narrow = [
            (class::NARROW_SIGNED, I::I32GtS, I::I32LtS),
            (class::NARROW_UNSIGNED, I::I32GtU, I::I32LtU),
        ];
        for (kind, greater, less) in narrow {
            let next = self.when_class(&[kind]);
            let (a, b) = (self.cell(A), self.cell(B));
            self.answer(
                result,
                &[
                    get(FRAME),
                    ld32(a),
                    get(FRAME),
                    ld32(b),
                    greater,
                    get(FRAME),
                    ld32(a),
                    get(FRAME),
                    ld32(b),
                    less,
                    I::I32Sub,
                ],
            );
            self.start(next);
        }
        let wide = [
            (class::WIDE_SIGNED, I::I64GtS, I::I64LtS),
            (class::WIDE_UNSIGNED, I::I64GtU, I::I64LtU),
        ];
        for (kind, greater, less) in wide {
            let next = self.when_class(&[kind]);
            let (a, b) = (self.cell(A), self.cell(B));
            self.answer(
                result,
                &[
                    get(FRAME),
                    ld64(a),
                    get(FRAME),
                    ld64(b),
                    greater,
                    get(FRAME),
                    ld64(a),
                    get(FRAME),
                    ld64(b),
                    less,
                    I::I32Sub,
                ],
            );
            self.start(next);
        }

        // The texts, ordered by the code that orders them for the interpreter.
        let next = self.when_class(&[class::TEXT, class::ATOM, class::SINGLETON]);
        let text = self.lowering.import(CompilerIntrinsic::TextCompare);
        let (a, b) = (self.cell(A), self.cell(B));
        self.answer(
            result,
            &[get(FRAME), ld32(a), get(FRAME), ld32(b), I::Call(text)],
        );
        self.start(next);
        let next = self.when_class(&[class::BYTES]);
        let bytes = self.lowering.import(CompilerIntrinsic::BytesCompare);
        self.answer(
            result,
            &[get(FRAME), ld32(a), get(FRAME), ld32(b), I::Call(bytes)],
        );
        self.start(next);

        // `bool`: the discriminant.
        let next = self.when_class(&[class::BOOL]);
        let variant = |slot: u32, this: &Self| -> [Ins; 3] {
            [get(FRAME), ld32(this.cell(slot)), ld32(header::VARIANT)]
        };
        let (va, vb) = (variant(A, self), variant(B, self));
        let value = [
            va[0].clone(),
            va[1].clone(),
            va[2].clone(),
            vb[0].clone(),
            vb[1].clone(),
            vb[2].clone(),
            I::I32GtU,
            va[0].clone(),
            va[1].clone(),
            va[2].clone(),
            vb[0].clone(),
            vb[1].clone(),
            vb[2].clone(),
            I::I32LtU,
            I::I32Sub,
        ];
        self.answer(result, &value);
        self.start(next);

        // `void` has one value.
        let next = self.when_class(&[class::VOID]);
        self.answer(result, &[c32(0)]);
        self.start(next);

        // A tuple or a record: each component in turn, the first that differs
        // deciding.
        let next = self.when_class(&[class::TUPLE, class::RECORD]);
        self.compare_components(result);
        self.start(next);

        // An enum or a union: the discriminant, and then the payload both hold.
        let next = self.when_class(&[class::ENUM, class::UNION]);
        self.compare_variants(result);
        self.start(next);

        // A declared type: the `compare` its program writes for it.
        let next = self.when_class(&[class::DECLARED, class::APPLIED]);
        self.compare_declared(result, &origin)?;
        self.start(next);

        // Nothing else is a key.
        self.answer(result, &[c32(0)]);
        Ok(())
    }

    /// Compares the components of two tuples or records, in order, by calling the
    /// unit on each pair with the descriptor of its type.
    fn compare_components(&mut self, result: u32) {
        let (index, count, child) = (self.temp(), self.temp(), self.temp());
        self.store_word(index, &[c32(0)]);
        let at = self.cell(T);
        self.store_word(
            count,
            &[get(FRAME), ld32(at), ld32(header::LEN), c32(1), I::I32Sub],
        );
        let (head, body, equal, next) = (
            self.new_block(),
            self.new_block(),
            self.new_block(),
            self.new_block(),
        );
        self.end(Term::Goto(head));
        self.start(head);
        let (i, n) = (self.word(index), self.word(count));
        self.push(&[
            i[0].clone(),
            i[1].clone(),
            n[0].clone(),
            n[1].clone(),
            I::I32GeU,
        ]);
        self.end(Term::Branch {
            then_block: equal,
            else_block: body,
        });
        self.start(equal);
        self.answer(result, &[c32(0)]);
        self.start(body);
        let succeeding = [
            self.word(index)[0].clone(),
            self.word(index)[1].clone(),
            c32(1),
            I::I32Add,
        ];
        self.store_word(child, &succeeding);
        let (left, right, kind, verdict) =
            (self.temp(), self.temp(), self.temp(), self.temp());
        self.load_indexed(left, A, index);
        self.load_indexed(right, B, index);
        self.load_indexed(kind, T, child);
        self.call_unit(left, right, kind, verdict);
        let decided = self.word(verdict);
        self.push(&[decided[0].clone(), decided[1].clone()]);
        let advance = self.new_block();
        self.end(Term::Branch {
            then_block: next,
            else_block: advance,
        });
        // A difference decides; the loop goes on past an equal pair.
        self.start(next);
        self.end(Term::Return { slot: verdict });
        self.start(advance);
        let succeeding = [
            self.word(index)[0].clone(),
            self.word(index)[1].clone(),
            c32(1),
            I::I32Add,
        ];
        self.store_word(index, &succeeding);
        self.end(Term::Goto(head));
    }
}

impl Builder<'_, '_> {
    /// Compares two enums or two unions: the discriminants decide when they
    /// differ, and otherwise the payload they both hold, if there is one.
    fn compare_variants(&mut self, result: u32) {
        let (a, b) = (self.cell(A), self.cell(B));
        let (first, second) = (self.temp(), self.temp());
        self.store_word(first, &[get(FRAME), ld32(a), ld32(header::VARIANT)]);
        self.store_word(second, &[get(FRAME), ld32(b), ld32(header::VARIANT)]);
        let (differ, same) = (self.new_block(), self.new_block());
        let (x, y) = (self.word(first), self.word(second));
        self.push(&[
            x[0].clone(),
            x[1].clone(),
            y[0].clone(),
            y[1].clone(),
            I::I32Ne,
        ]);
        self.end(Term::Branch {
            then_block: differ,
            else_block: same,
        });
        self.start(differ);
        let (x, y) = (self.word(first), self.word(second));
        self.answer(
            result,
            &[
                c32(-1),
                c32(1),
                x[0].clone(),
                x[1].clone(),
                y[0].clone(),
                y[1].clone(),
                I::I32LtU,
                I::Select,
            ],
        );
        self.start(same);
        // An enum variant without a payload has no cell.
        let (payload, bare) = (self.new_block(), self.new_block());
        self.push(&[get(FRAME), ld32(a), ld32(header::LEN)]);
        self.end(Term::Branch {
            then_block: payload,
            else_block: bare,
        });
        self.start(bare);
        self.answer(result, &[c32(0)]);
        self.start(payload);
        let (child, zero) = (self.temp(), self.temp());
        let succeeding = [
            self.word(first)[0].clone(),
            self.word(first)[1].clone(),
            c32(1),
            I::I32Add,
        ];
        self.store_word(child, &succeeding);
        self.store_word(zero, &[c32(0)]);
        let (left, right, kind, verdict) =
            (self.temp(), self.temp(), self.temp(), self.temp());
        self.load_indexed(left, A, zero);
        self.load_indexed(right, B, zero);
        self.load_indexed(kind, T, child);
        self.call_unit(left, right, kind, verdict);
        self.end(Term::Return { slot: verdict });
    }

    /// Compares two values of a declared type by the `compare` the program
    /// writes for it, and by nothing for a type it writes none for.
    fn compare_declared(
        &mut self,
        result: u32,
        origin: &SourceOrigin,
    ) -> Result<(), NotLowered> {
        let program = self.lowering.program;
        let Some(interface) = self.lowering.ordered_interface() else {
            self.answer(result, &[c32(0)]);
            return Ok(());
        };
        // Every written implementation returns the one `ordering`.
        let ordering = program.functions().iter().find_map(|function| {
            let implements = function.implements()?;
            (implements.interface == interface
                && implements.member == "compare"
                && !matches!(implements.receiver, Type::Param(_)))
            .then(|| function.signature().result())
        });
        let Some(ordering) = ordering else {
            self.answer(result, &[c32(0)]);
            return Ok(());
        };
        let shape = self.shape(&ordering, origin)?;
        let (Some(less), Some(greater)) =
            (shape.variant("less"), shape.variant("greater"))
        else {
            self.answer(result, &[c32(0)]);
            return Ok(());
        };
        let answered = self.temp();
        let after = self.new_block();
        self.try_candidates(&Dispatch {
            interface: &interface,
            member: "compare",
            interface_arguments: &[],
            member_types: &[],
            operands: &[A, B],
            recv_desc: T,
            argument_descs: &[],
            known: None,
            receiver: None,
            arguments: &[],
            result: &ordering,
            tail: false,
            after,
            origin,
            dest: answered,
            written_only: true,
        })?;
        // No implementation describes the type.
        self.answer(result, &[c32(0)]);
        self.start(after);
        let at = self.cell(answered);
        let (less, greater) = (
            c32(u32::try_from(less).unwrap_or(u32::MAX).cast_signed()),
            c32(u32::try_from(greater).unwrap_or(u32::MAX).cast_signed()),
        );
        self.push(&[get(FRAME), ld32(at), ld32(header::VARIANT), set(SCRATCH)]);
        self.drop_slot(answered);
        self.answer(
            result,
            &[
                get(SCRATCH),
                greater,
                I::I32Eq,
                get(SCRATCH),
                less,
                I::I32Eq,
                I::I32Sub,
            ],
        );
        Ok(())
    }

    /// `ordered.compare` or `equatable.equal` of the closed key types, called
    /// where no written implementation describes the receiver: the key unit
    /// orders the two operands by the descriptor of their type.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn closed_key_call(
        &mut self,
        kind: ClosedContract,
        operands: &[u32],
        recv_desc: u32,
        argument_descs: &[u32],
        result: &Type,
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let (Some(a), Some(b)) = (operands.first().copied(), operands.get(1).copied())
        else {
            return Err(self.closed_error(origin));
        };
        for slot in argument_descs {
            self.discard(*slot, ValueClass::Dyn);
        }
        let descriptor = self.temp();
        self.copy_cell(descriptor, recv_desc);
        self.claim(descriptor, ValueClass::Dyn, ByteSource::Slot(recv_desc));
        self.discard(recv_desc, ValueClass::Dyn);
        let answer = self.temp();
        self.call_unit(a, b, descriptor, answer);
        let at = self.cell(answer);
        let selector = self.temp();
        match kind {
            ClosedContract::KeyEqual => {
                self.store_word(selector, &[get(FRAME), ld32(at), I::I32Eqz]);
            }
            _ => {
                let less = self.discriminant(result, "less", origin)?;
                let equal = self.discriminant(result, "equal", origin)?;
                let greater = self.discriminant(result, "greater", origin)?;
                self.store_word(
                    selector,
                    &[
                        c32(less.cast_signed()),
                        c32(equal.cast_signed()),
                        c32(greater.cast_signed()),
                        get(FRAME),
                        ld32(at),
                        I::I32Eqz,
                        I::Select,
                        get(FRAME),
                        ld32(at),
                        c32(0),
                        I::I32LtS,
                        I::Select,
                    ],
                );
            }
        }
        self.build_with(
            Kind::Enum,
            Variant::Cell {
                slot: selector,
                bias: 0,
            },
            &[],
            dest,
        );
        Ok(())
    }

    fn closed_error(&self, origin: &SourceOrigin) -> NotLowered {
        NotLowered::type_of("shape", Some(origin.clone()))
    }

    /// Orders two keys of a dict by the key unit, where the order of the key
    /// type is not one a routine can write beforehand, and leaves `-1`, `0`, or
    /// `1` in the cell of `out`.
    pub(super) fn compare_dispatched(
        &mut self,
        key: &Type,
        a: KeyAt,
        b: KeyAt,
        out: u32,
    ) -> Result<(), NotLowered> {
        let key_class = if is_interface(key) {
            ValueClass::Ref
        } else {
            super::class_of(key).map_err(|kind| NotLowered::type_of(kind, None))?
        };
        let (left, right) = (self.temp(), self.temp());
        for (at, slot) in [(a, left), (b, right)] {
            self.copy_key(slot, at, key_class);
        }
        let descriptor = self.temp();
        self.build_type(key, descriptor)?;
        self.call_unit(left, right, descriptor, out);
        Ok(())
    }

    /// Copies the key at `at` into `to`, which then owns it.
    fn copy_key(&mut self, to: u32, at: KeyAt, class: ValueClass) {
        let held = self.cell(to);
        match at {
            KeyAt::Slot(slot) => {
                self.copy_cell(to, slot);
                self.claim(to, class, ByteSource::Slot(slot));
            }
            KeyAt::OfTuple(slot) => {
                let from = self.cell(slot);
                self.push(&[
                    get(FRAME),
                    get(FRAME),
                    ld32(from),
                    ld64(crate::layout::cells_offset(2)),
                    st64(held),
                ]);
                self.claim(to, class, ByteSource::Component { slot, position: 0 });
            }
            KeyAt::OfEntry { object, index } => {
                // The entry is a two-component tuple that is the element.
                let entry = self.temp();
                let entry_at = self.cell(entry);
                self.push(&[get(FRAME)]);
                self.push_element_address(object, index);
                self.push(&[ld32(header::SIZE), I::I64ExtendI32U, st64(entry_at)]);
                self.set_byte(entry, CellClass::I32.code());
                self.push(&[
                    get(FRAME),
                    get(FRAME),
                    ld32(entry_at),
                    ld64(crate::layout::cells_offset(2)),
                    st64(held),
                ]);
                self.claim(
                    to,
                    class,
                    ByteSource::Component {
                        slot: entry,
                        position: 0,
                    },
                );
            }
        }
    }
}
