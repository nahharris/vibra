//! The closed test assertions: `assert.true`, `assert.false`, and `assert.equal`.
//!
//! An assertion evaluates through the ordinary typed call path and is neither a
//! language function nor a host operation (`docs/spec/06-runtime.md`, "Test
//! assertions"), so a call of one is lowered in line, where the call is. Its
//! operands are evaluated first, in order, as every call's are.
//!
//! - `assert.true` and `assert.false` read the discriminant of their `bool`.
//! - `assert.equal` compares canonical encodings, with the one walk of
//!   [`crate::canon_typed`] for every type: a NaN is replaced by the quiet NaN
//!   of its width (decision D11.1), so a NaN equals a NaN and a negative zero
//!   does not equal a positive one. The walk takes arena values, so an operand
//!   that is a scalar or of a generic type is compared as the one component of a
//!   tuple. Each operand is first scanned for a function ([`crate::canon`]),
//!   because a function has no encoding: finding one in either is the trap
//!   `@runtime.unobservable-function`, at the assertion call.
//!
//! A false assertion records the failure and stops the test: the status `3`,
//! which assertion it was, the origin ordinal of the call, and, for
//! `assert.equal`, the two operands as the bits of a scalar or as value IDs.
//! The origin table says by what type the host reads them. An operand whose
//! type names a generic parameter has no type the host could read, so it
//! crosses as an interface value, which carries the descriptor of the type it
//! holds, and the table says `any`.
//!
//! A module that runs a program, and not tests, stops at an assertion, as the
//! reference interpreter does when it is not running a test: the trap
//! `@runtime.invalid-checked-program`.

use vibra_ir::boundary::{Failure, TrapCode};
use vibra_ir::{Expr, SourceOrigin, TestAssertion, Type};
use wasm_encoder::Instruction as I;

use super::{Builder, ByteSource, FRAME, Kind, Part, Term};
use crate::form::NotLowered;
use crate::layout::{header, state};
use crate::runtime::{ValueClass, c32, get, ld32, ld64, st32, st64};

impl Builder<'_, '_> {
    /// Lowers a call of the assertion `assertion`, whose operands are
    /// `arguments`, into `dest`.
    pub(super) fn assertion(
        &mut self,
        assertion: TestAssertion,
        arguments: &[Expr],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), NotLowered> {
        let mark = self.next_temp;
        let mut operands = Vec::with_capacity(arguments.len());
        for argument in arguments {
            self.class(&argument.result_type(), argument.origin())?;
            let slot = self.temp();
            self.expr(argument, slot)?;
            operands.push(slot);
        }
        if !self.lowering.tests {
            let stop = self.lowering.fns.stop_trap;
            self.push(&[c32(TrapCode::InvalidCheckedProgram.code()), I::Call(stop)]);
            self.store_bits(dest, 0, ValueClass::Void);
            self.release_to(mark);
            return Ok(());
        }
        match (assertion, operands.as_slice()) {
            (TestAssertion::True, [actual]) => {
                self.assert_bool(*actual, 1, Failure::True, origin);
            }
            (TestAssertion::False, [actual]) => {
                self.assert_bool(*actual, 0, Failure::False, origin);
            }
            (TestAssertion::Equal, [expected, actual]) => {
                let ty = arguments.first().map(Expr::result_type).ok_or_else(|| {
                    NotLowered::type_of("shape", Some(origin.clone()))
                })?;
                self.assert_equal(*expected, *actual, &ty, origin)?;
            }
            _ => return Err(NotLowered::type_of("shape", Some(origin.clone()))),
        }
        self.store_bits(dest, 0, ValueClass::Void);
        self.release_to(mark);
        Ok(())
    }

    /// Records a stop by a registered trap at the origin ordinal and stops.
    fn stop_at(&mut self, code: TrapCode, ordinal: u32) {
        self.push(&[
            c32(0),
            c32(1),
            st32(state::STATUS),
            c32(0),
            c32(code.code()),
            st32(state::TRAP_CODE),
            c32(0),
            c32(ordinal.cast_signed()),
            st32(state::ORIGIN),
            I::Unreachable,
        ]);
    }

    /// Records a failed assertion at the origin ordinal and stops. The operands
    /// of an `assert.equal` are in their state fields already.
    fn stop_failed(&mut self, failure: Failure, ordinal: u32) {
        self.push(&[
            c32(0),
            c32(3),
            st32(state::STATUS),
            c32(0),
            c32(failure.code()),
            st32(state::FAILURE),
            c32(0),
            c32(ordinal.cast_signed()),
            st32(state::ORIGIN),
            I::Unreachable,
        ]);
    }

    /// `assert.true` and `assert.false`: the `bool` in `actual` must be the
    /// variant `required`.
    fn assert_bool(
        &mut self,
        actual: u32,
        required: i32,
        failure: Failure,
        origin: &SourceOrigin,
    ) {
        let ordinal = self.lowering.origin_ordinal(origin, None);
        let (pass, fail) = (self.new_block(), self.new_block());
        let at = self.cell(actual);
        self.push(&[
            get(FRAME),
            ld32(at),
            ld32(header::VARIANT),
            c32(required),
            I::I32Eq,
        ]);
        self.end(Term::Branch {
            then_block: pass,
            else_block: fail,
        });
        self.start(fail);
        self.stop_failed(failure, ordinal);
        self.goto(pass, pass);
        self.drop_slot(actual);
    }

    /// Stores into the state field `field` what an operand of a failed
    /// `assert.equal` crosses the boundary as: the bits of a scalar, or the ID
    /// of a value. An operand whose type names a generic parameter is first
    /// widened to an interface value, which holds the descriptor of its type.
    fn cross_operand(
        &mut self,
        slot: u32,
        ty: &Type,
        class: ValueClass,
        field: u32,
    ) -> Result<(), NotLowered> {
        let new_id = self.lowering.fns.new_id;
        if ty.has_params() {
            let pair = self.temp();
            self.widen_to_interface(slot, ty, pair)?;
            let at = self.cell(pair);
            self.push(&[c32(0), get(FRAME), ld32(at), I::Call(new_id), st64(field)]);
            return Ok(());
        }
        let at = self.cell(slot);
        match class {
            ValueClass::Ref | ValueClass::Dyn => {
                self.push(&[
                    c32(0),
                    get(FRAME),
                    ld32(at),
                    I::Call(new_id),
                    st64(field),
                ]);
            }
            ValueClass::I32 | ValueClass::F32 => {
                self.push(&[
                    c32(0),
                    get(FRAME),
                    ld32(at),
                    I::I64ExtendI32U,
                    st64(field),
                ]);
            }
            ValueClass::I64 | ValueClass::F64 => {
                self.push(&[c32(0), get(FRAME), ld64(at), st64(field)]);
            }
            ValueClass::Void => {
                self.push(&[c32(0), crate::runtime::c64(0), st64(field)]);
            }
        }
        Ok(())
    }

    /// `assert.equal`: the two operands, of the type `ty`, must have the same
    /// canonical encoding. There is one comparison for every type
    /// ([`crate::canon_typed`]); it takes arena values, so an operand that is a
    /// scalar, or of a generic type, is compared as the one component of a
    /// tuple. Each operand is first scanned for a function, which has no
    /// encoding.
    fn assert_equal(
        &mut self,
        expected: u32,
        actual: u32,
        ty: &Type,
        origin: &SourceOrigin,
    ) -> Result<(), NotLowered> {
        let class = self.class(ty, origin)?;
        let recorded = if ty.has_params() {
            Type::Any
        } else {
            ty.clone()
        };
        let ordinal = self.lowering.origin_ordinal(origin, Some(&recorded));
        let (pass, fail) = (self.new_block(), self.new_block());
        let mut owned = Vec::new();

        let objects = if class == ValueClass::Ref {
            [expected, actual]
        } else {
            let mut wrapped = [expected, actual];
            for slot in &mut wrapped {
                let source = *slot;
                let copy = self.temp();
                self.copy_cell(copy, source);
                self.claim(copy, class, ByteSource::Slot(source));
                let tuple = self.temp();
                self.build(Kind::Tuple, 0, &[Part::Slot { slot: copy, class }], tuple);
                owned.push(tuple);
                *slot = tuple;
            }
            wrapped
        };
        let scan = self.lowering.scan();
        for object in objects {
            let (visible, hidden) = (self.new_block(), self.new_block());
            let at = self.cell(object);
            self.push(&[get(FRAME), ld32(at), I::Call(scan)]);
            self.end(Term::Branch {
                then_block: visible,
                else_block: hidden,
            });
            self.start(hidden);
            self.stop_at(TrapCode::UnobservableFunction, ordinal);
            self.goto(visible, visible);
        }
        let typed = self.lowering.typed_walk();
        let desc = self.temp();
        let described = if class == ValueClass::Ref {
            ty.clone()
        } else {
            Type::Tuple(vec![ty.clone()])
        };
        self.build_type(&described, desc)?;
        let (left, right, held) = (
            self.cell(objects[0]),
            self.cell(objects[1]),
            self.cell(desc),
        );
        self.push(&[
            get(FRAME),
            ld32(left),
            get(FRAME),
            ld32(right),
            get(FRAME),
            ld32(held),
            I::Call(typed),
        ]);
        self.drop_slot(desc);
        self.end(Term::Branch {
            then_block: pass,
            else_block: fail,
        });

        self.start(fail);
        self.cross_operand(expected, ty, class, state::EXPECTED)?;
        self.cross_operand(actual, ty, class, state::ACTUAL)?;
        self.stop_failed(Failure::Equal, ordinal);
        self.goto(pass, pass);

        for object in owned {
            self.drop_slot(object);
        }
        self.discard(expected, class);
        self.discard(actual, class);
        Ok(())
    }
}
