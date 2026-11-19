//! The assertion rows and canonical equality.
//!
//! An assertion that holds is `void`. One that fails records status `3`, the
//! failed row, the origin, and for `assert.equal` the two operands (the bits
//! of a scalar, or an ID of an arena value, which the runner reads at the type
//! the origin table gives), and stops the instance, so no later effect runs
//! (`docs/spec/06-runtime.md`, "Wasm transition"). An assertion outside a test
//! is the trap `@runtime.invalidCheckedProgram`, as it is for the reference
//! interpreter.
//!
//! `assert.equal` compares canonical encodings, which two values of one type
//! have exactly when they have the same shape and the same scalars, where
//! every NaN of a float type encodes as the canonical NaN. The routine
//! [`deep_equal`] decides it for two arena values, iteratively over a
//! worklist it allocates in the arena, so the engine stack stays bounded
//! however deeply a value nests.

use vibra_ir::boundary::Failure;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{SourceOrigin, Type};
use wasm_encoder::{BlockType, Instruction as I};

use super::{Builder, Term};
use crate::error::EmitError;
use crate::layout::{CellClass, header, state};
use crate::runtime::{
    I32, I64, Ins, Routine, Routines, ValueClass, c32, c64, get, ld8, ld32, ld64, make,
    set, st32, st64, tee,
};

impl<'a> Builder<'_, 'a> {
    /// `assert.true`, `assert.false`, or `assert.equal` on the operands in
    /// `operands`, whose types are `parameters`.
    pub(super) fn assertion(
        &mut self,
        intrinsic: CompilerIntrinsic,
        parameters: &[Type],
        operands: &[u32],
        origin: &SourceOrigin,
        dest: u32,
    ) -> Result<(), EmitError> {
        if !self.lowering.tests {
            self.stop_invalid(origin);
            self.store_bits(dest, 0, ValueClass::Void);
            return Ok(());
        }
        let (holds, fails) = (self.new_block(), self.new_block());
        let failure = match (intrinsic, operands, parameters) {
            (
                CompilerIntrinsic::AssertTrue | CompilerIntrinsic::AssertFalse,
                [operand],
                _,
            ) => {
                self.truth(*operand, origin)?;
                let failure = if intrinsic == CompilerIntrinsic::AssertTrue {
                    Failure::AssertTrue
                } else {
                    self.push(&[I::I32Eqz]);
                    Failure::AssertFalse
                };
                self.end(Term::Branch {
                    then_block: holds,
                    else_block: fails,
                });
                self.start(fails);
                self.record_failure(failure, origin, None);
                failure
            }
            (CompilerIntrinsic::AssertEqual, [expected, actual], [ty, _]) => {
                if !self.observable(ty, origin)? {
                    return Err(EmitError::unsupported("assertEqual", origin));
                }
                let class = self.class(ty, origin)?;
                if class == ValueClass::Ref {
                    let deep_equal = self.lowering.deep_equal();
                    self.load_ref(*expected);
                    self.load_ref(*actual);
                    self.push(&[I::Call(deep_equal)]);
                } else {
                    self.scalar_equal(*expected, *actual, class);
                }
                self.end(Term::Branch {
                    then_block: holds,
                    else_block: fails,
                });
                self.start(fails);
                let new_id = self.lowering.fns.new_id;
                for (slot, address) in
                    [(*expected, state::EXPECTED), (*actual, state::ACTUAL)]
                {
                    let at = self.cell(slot);
                    self.push(&[c32(0)]);
                    if class == ValueClass::Ref {
                        self.load_ref(slot);
                        self.push(&[I::Call(new_id)]);
                    } else {
                        self.push(&[get(super::FRAME), ld64(at)]);
                    }
                    self.push(&[st64(address)]);
                }
                self.record_failure(Failure::AssertEqual, origin, Some(ty.clone()));
                Failure::AssertEqual
            }
            _ => {
                return Err(EmitError::malformed(
                    format!(
                        "`{}` with {} operands",
                        intrinsic.symbol(),
                        operands.len()
                    ),
                    origin,
                ));
            }
        };
        self.goto(holds, holds);
        if failure == Failure::AssertEqual {
            let class = parameters
                .first()
                .map(|ty| self.class(ty, origin))
                .transpose()?
                .unwrap_or(ValueClass::Void);
            for slot in operands {
                self.discard(*slot, class);
            }
        }
        self.store_bits(dest, 0, ValueClass::Void);
        Ok(())
    }

    /// Records a failed assertion and stops.
    fn record_failure(
        &mut self,
        failure: Failure,
        origin: &SourceOrigin,
        operand: Option<Type>,
    ) {
        let ordinal = self.lowering.origin_ordinal(origin, operand);
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
}

/// `(a, b) -> i32`: `1` when two arena values of one type have the same
/// canonical encoding, and `0` otherwise.
///
/// It walks pairs of objects from a worklist: two objects are equal when they
/// are the same object, or have the same kind, case, and length and equal
/// payloads. A payload of characters or bytes is compared byte by byte; a
/// payload of cells compares the class bytes, then each cell: a reference
/// pushes its pair, a float compares by bits or as two NaNs, and any other
/// scalar compares by bits. The worklist is a block of the arena that doubles
/// when it fills, and it is freed before the routine returns.
pub(crate) fn deep_equal(fns: &Routines) -> Routine {
    // Params: 0 a, 1 b. Locals: 2 stack, 3 capacity, 4 top, 5 x, 6 y, 7 len,
    // 8 i, 9 x cells, 10 y cells, 11 class byte, 12 x bits (i64), 13 y bits
    // (i64), 14 scratch, 15 payload bytes.
    let (stack, capacity, top, x, y, len, i, x_cells, y_cells, class) =
        (2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
    let (x_bits, y_bits, scratch, bytes) = (12, 13, 14, 15);
    let first = 16_i32;
    let empty = BlockType::Empty;
    let mut code: Vec<Ins> = vec![
        get(0),
        get(1),
        I::I32Eq,
        I::If(empty),
        c32(1),
        I::Return,
        I::End,
        c32(first),
        set(capacity),
        c64(8 + 8 * i64::from(first)),
        I::Call(fns.alloc),
        set(stack),
        get(stack),
        get(0),
        st32(8),
        get(stack),
        get(1),
        st32(12),
        c32(1),
        set(top),
        // $fail
        I::Block(empty),
        // $next
        I::Loop(empty),
        get(top),
        I::I32Eqz,
        I::If(empty),
        get(stack),
        I::Call(fns.free),
        c32(1),
        I::Return,
        I::End,
        get(top),
        c32(1),
        I::I32Sub,
        set(top),
        get(stack),
        get(top),
        c32(8),
        I::I32Mul,
        I::I32Add,
        tee(scratch),
        ld32(8),
        set(x),
        get(scratch),
        ld32(12),
        set(y),
        get(x),
        get(y),
        I::I32Eq,
        I::BrIf(0),
        get(x),
        ld8(header::KIND),
        get(y),
        ld8(header::KIND),
        I::I32Ne,
        I::BrIf(1),
        get(x),
        ld32(header::VARIANT),
        get(y),
        ld32(header::VARIANT),
        I::I32Ne,
        I::BrIf(1),
        get(x),
        ld32(header::LEN),
        tee(len),
        get(y),
        ld32(header::LEN),
        I::I32Ne,
        I::BrIf(1),
        // Characters or bytes: compare the payload bytes.
        get(x),
        ld8(header::STRIDE),
        c32(8),
        I::I32Ne,
        I::If(empty),
        get(len),
        get(x),
        ld8(header::STRIDE),
        I::I32Mul,
        set(bytes),
        c32(0),
        set(i),
        I::Block(empty),
        I::Loop(empty),
        get(i),
        get(bytes),
        I::I32GeU,
        I::BrIf(1),
        get(x),
        get(i),
        I::I32Add,
        ld8(header::SIZE),
        get(y),
        get(i),
        I::I32Add,
        ld8(header::SIZE),
        I::I32Ne,
        I::BrIf(4),
        get(i),
        c32(1),
        I::I32Add,
        set(i),
        I::Br(0),
        I::End,
        I::End,
        I::Br(1),
        I::End,
        // Cells: the class bytes, padded to 8, precede the cells.
        get(x),
        get(len),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        set(x_cells),
        get(y),
        get(len),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        set(y_cells),
        c32(0),
        set(i),
        I::Block(empty),
        I::Loop(empty),
        get(i),
        get(len),
        I::I32GeU,
        I::BrIf(1),
        get(x),
        get(i),
        I::I32Add,
        ld8(header::SIZE),
        tee(class),
        get(y),
        get(i),
        I::I32Add,
        ld8(header::SIZE),
        I::I32Ne,
        I::BrIf(3),
        get(x_cells),
        get(i),
        c32(8),
        I::I32Mul,
        I::I32Add,
        ld64(header::SIZE),
        set(x_bits),
        get(y_cells),
        get(i),
        c32(8),
        I::I32Mul,
        I::I32Add,
        ld64(header::SIZE),
        set(y_bits),
        get(class),
        c32(i32::from(CellClass::Ref.code())),
        I::I32Eq,
        I::If(empty),
        // Push the pair, doubling the worklist when it is full.
        get(top),
        get(capacity),
        I::I32Eq,
        I::If(empty),
        get(capacity),
        I::I64ExtendI32U,
        c64(16),
        I::I64Mul,
        c64(8),
        I::I64Add,
        I::Call(fns.alloc),
        set(scratch),
        get(scratch),
        c32(8),
        I::I32Add,
        get(stack),
        c32(8),
        I::I32Add,
        get(capacity),
        c32(8),
        I::I32Mul,
        I::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        },
        get(stack),
        I::Call(fns.free),
        get(scratch),
        set(stack),
        get(capacity),
        c32(1),
        I::I32Shl,
        set(capacity),
        I::End,
        get(stack),
        get(top),
        c32(8),
        I::I32Mul,
        I::I32Add,
        tee(scratch),
        get(x_bits),
        I::I32WrapI64,
        st32(8),
        get(scratch),
        get(y_bits),
        I::I32WrapI64,
        st32(12),
        get(top),
        c32(1),
        I::I32Add,
        set(top),
        I::Else,
        get(x_bits),
        get(y_bits),
        I::I64Ne,
        I::If(empty),
        get(class),
        c32(i32::from(CellClass::F32.code())),
        I::I32Eq,
        I::If(empty),
    ];
    code.extend(both_nan(x_bits, y_bits, 0x7FFF_FFFF, 0x7F80_0000));
    code.extend([
        I::I32Eqz,
        I::BrIf(6),
        I::Else,
        get(class),
        c32(i32::from(CellClass::F64.code())),
        I::I32Eq,
        I::If(empty),
    ]);
    code.extend(both_nan(
        x_bits,
        y_bits,
        0x7FFF_FFFF_FFFF_FFFF,
        0x7FF0_0000_0000_0000,
    ));
    code.extend([
        I::I32Eqz,
        I::BrIf(7),
        I::Else,
        I::Br(7),
        I::End,
        I::End,
        I::End,
        I::End,
        get(i),
        c32(1),
        I::I32Add,
        set(i),
        I::Br(0),
        I::End,
        I::End,
        I::Br(0),
        I::End,
        I::End,
        // $fail
        get(stack),
        I::Call(fns.free),
        c32(0),
        I::End,
    ]);
    make(
        &[I32, I32],
        &[I32],
        &[
            I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I64, I64, I32, I32,
        ],
        code,
    )
}

/// Whether the two cells in the locals `x` and `y` are both NaNs, by their
/// magnitude bits under `magnitude` exceeding those of infinity.
fn both_nan(x: u32, y: u32, magnitude: i64, infinity: i64) -> [Ins; 11] {
    [
        get(x),
        c64(magnitude),
        I::I64And,
        c64(infinity),
        I::I64GtU,
        get(y),
        c64(magnitude),
        I::I64And,
        c64(infinity),
        I::I64GtU,
        I::I32And,
    ]
}
