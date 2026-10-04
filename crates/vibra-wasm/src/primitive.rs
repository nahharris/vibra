//! The integer and `char` primitive rows of the compiler registry, as data.
//!
//! The registry's integer rows are a table of type by operation (`docs/spec/06-runtime.md`,
//! "M3 compiler intrinsic registry"), and a row is a few instructions, so the
//! emitter lowers each inline and no routine of the lowering names a row. A row
//! is read through one table: [`plan`] maps a [`CompilerIntrinsic`] to the
//! operands it takes (each an integer type, or `char`, which is a `u32`), the
//! **outcome** it builds, and the **body** that computes it. The lowering of
//! `lower.rs` builds the outcome from the computed selector the same way for
//! every row of an outcome; this module writes the computation.
//!
//! | Outcome | Rows | The arena value built |
//! | --- | --- | --- |
//! | scalar | a conversion that cannot fail, `char.to-u32` | none: a cell |
//! | `bool` | `equal` | the enum `bool`, discriminant `false` 0 and `true` 1 |
//! | `ordering` | `compare` | an enum of three `void` variants |
//! | `(result t e)` | the checked operations, a conversion that can fail | `ok` of a cell, or `err` of an enum of `void` variants |
//! | `(option char)` | `char.from-u32` | `some` of a cell, or `none` |
//!
//! # The computation
//!
//! Every operand is loaded into one 64-bit local as its exact integer: a type of
//! at most 32 bits is sign- or zero-extended from the 32 bits of its cell, which
//! hold it canonically (a signed type sign-extended to 32 bits, an unsigned one
//! zero-extended), and a 64-bit type is its cell. The value of a row is left in
//! the local `WR`, and one `i32` **selector** is left on the operand stack: the
//! discriminant of a `bool` or an `ordering`, the **kind** of a checked result
//! (`0` for `ok`, otherwise `1` plus the discriminant of the error variant), or
//! whether a scalar is a `char`. No instruction can trap: a divisor is replaced
//! by `1` before it can be zero or `-1`, and a shift count is masked by the
//! engine and checked before its result is used.
//!
//! A type of at most 32 bits is computed exactly in 64 bits, so overflow is the
//! result lying outside the type. A 64-bit type has no wider carrier, so its
//! overflow is read from the operands and the wrapped result (a carry, a sign
//! change, a quotient that does not give the factor back, a shifted-out bit).
//! Division, remainder, negation, and the conversions need no such distinction.

use vibra_ir::external::{CompilerIntrinsic, IntegerOp, NumericType};
use wasm_encoder::Instruction as I;

use crate::lower::{FRAME, WA, WB, WR};
use crate::runtime::{Ins, ValueClass, c32, c64, get, ld32, ld64, set};

/// How a row's result is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// A scalar of this integer type, left in a cell.
    Scalar(NumericType),
    /// `bool`.
    Bool,
    /// `ordering`.
    Ordering,
    /// `(result t e)`, where `t` is this integer type.
    Checked(NumericType),
    /// `(option char)`.
    Option,
}

/// A way a checked row fails, which the checked result's error enum names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    /// `overflow`.
    Overflow,
    /// `division-by-zero`.
    DivisionByZero,
    /// `invalid-shift`.
    InvalidShift,
    /// `out-of-range`.
    OutOfRange,
}

impl Fault {
    /// The variant of the error enum.
    pub(crate) const fn variant(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::DivisionByZero => "division-by-zero",
            Self::InvalidShift => "invalid-shift",
            Self::OutOfRange => "out-of-range",
        }
    }
}

/// The discriminants a computation selects among, which the lowering reads
/// from the shapes of the checked result type.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Names {
    /// For each [`Fault`], `1` plus the discriminant of its variant.
    pub(crate) faults: [i32; 4],
    /// The discriminants of `less`, `equal`, and `greater`.
    pub(crate) order: [i32; 3],
}

impl Names {
    fn fault(&self, fault: Fault) -> i32 {
        self.faults.get(fault as usize).copied().unwrap_or(0)
    }
}

/// The operation a checked row performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    /// A checked arithmetic or shift row of this type.
    Arithmetic(NumericType, IntegerOp),
    /// `equal` of this type.
    Equal,
    /// `compare` of this type.
    Compare(NumericType),
    /// The value of one integer type as another.
    Convert(NumericType, NumericType),
    /// `char.to-u32`: the value as it is.
    CharToU32,
    /// `char.from-u32`: whether the value is a Unicode scalar.
    CharFromU32,
}

/// One row: its operands, its outcome, and the computation.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    operands: Vec<NumericType>,
    outcome: Outcome,
    body: Body,
}

/// The table: what the emitter does for `intrinsic`, or `None` for a row it
/// does not lower inline (the `to-str` and `parse` rows, the float rows, and the
/// rows of the collection and text types).
pub(crate) fn plan(intrinsic: CompilerIntrinsic) -> Option<Plan> {
    use NumericType::U32;
    Some(match intrinsic {
        CompilerIntrinsic::Integer(numeric, op) if numeric.is_integer() => {
            let (operands, outcome, body) = match op {
                IntegerOp::AddChecked
                | IntegerOp::SubChecked
                | IntegerOp::MulChecked
                | IntegerOp::DivChecked
                | IntegerOp::RemChecked => (
                    vec![numeric, numeric],
                    Outcome::Checked(numeric),
                    Body::Arithmetic(numeric, op),
                ),
                IntegerOp::NegChecked => (
                    vec![numeric],
                    Outcome::Checked(numeric),
                    Body::Arithmetic(numeric, op),
                ),
                IntegerOp::ShiftLeftChecked | IntegerOp::ShiftRight => (
                    vec![numeric, U32],
                    Outcome::Checked(numeric),
                    Body::Arithmetic(numeric, op),
                ),
                IntegerOp::Equal => {
                    (vec![numeric, numeric], Outcome::Bool, Body::Equal)
                }
                IntegerOp::Compare => (
                    vec![numeric, numeric],
                    Outcome::Ordering,
                    Body::Compare(numeric),
                ),
                IntegerOp::ToStr | IntegerOp::Parse => return None,
            };
            Plan {
                operands,
                outcome,
                body,
            }
        }
        CompilerIntrinsic::Convert(source, target)
            if source.is_integer() && target.is_integer() =>
        {
            Plan {
                operands: vec![source],
                outcome: if CompilerIntrinsic::conversion_is_total(source, target) {
                    Outcome::Scalar(target)
                } else {
                    Outcome::Checked(target)
                },
                body: Body::Convert(source, target),
            }
        }
        CompilerIntrinsic::CharToU32 => Plan {
            operands: vec![U32],
            outcome: Outcome::Scalar(U32),
            body: Body::CharToU32,
        },
        CompilerIntrinsic::CharFromU32 => Plan {
            operands: vec![U32],
            outcome: Outcome::Option,
            body: Body::CharFromU32,
        },
        _ => return None,
    })
}

impl Plan {
    /// The type of each operand, in order. A `char` is a `u32`.
    pub(crate) fn operands(&self) -> &[NumericType] {
        &self.operands
    }

    /// What the row builds.
    pub(crate) const fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The faults the row can report.
    pub(crate) fn faults(&self) -> Vec<Fault> {
        match self.body {
            Body::Arithmetic(_, op) => match op {
                IntegerOp::AddChecked
                | IntegerOp::SubChecked
                | IntegerOp::MulChecked
                | IntegerOp::NegChecked => vec![Fault::Overflow],
                IntegerOp::DivChecked => vec![Fault::DivisionByZero, Fault::Overflow],
                IntegerOp::RemChecked => vec![Fault::DivisionByZero],
                IntegerOp::ShiftLeftChecked => {
                    vec![Fault::InvalidShift, Fault::Overflow]
                }
                IntegerOp::ShiftRight => vec![Fault::InvalidShift],
                _ => Vec::new(),
            },
            Body::Convert(..) if matches!(self.outcome, Outcome::Checked(_)) => {
                vec![Fault::OutOfRange]
            }
            _ => Vec::new(),
        }
    }

    /// The loads of the operands from the cells at `offsets` into `WA` and
    /// `WB`, then the computation: the value of the row is in `WR`, and the
    /// selector is on the operand stack (nothing is, for a scalar).
    pub(crate) fn compute(&self, offsets: &[u32], names: &Names) -> Vec<Ins> {
        let mut code = Vec::new();
        for ((operand, offset), local) in
            self.operands.iter().zip(offsets).zip([WA, WB])
        {
            load(*operand, *offset, local, &mut code);
        }
        match self.body {
            Body::Arithmetic(numeric, op) => arithmetic(numeric, op, names, &mut code),
            Body::Equal => code.extend([get(WA), get(WB), I::I64Eq]),
            Body::Compare(numeric) => {
                let [less, equal, greater] = names.order;
                code.extend([c32(less), c32(equal), c32(greater)]);
                code.extend([get(WA), get(WB), I::I64Eq, I::Select]);
                code.extend([
                    get(WA),
                    get(WB),
                    if numeric.is_signed() {
                        I::I64LtS
                    } else {
                        I::I64LtU
                    },
                    I::Select,
                ]);
            }
            Body::Convert(source, target) => {
                code.extend([get(WA), set(WR)]);
                if matches!(self.outcome, Outcome::Checked(_)) {
                    // The value is in the type exactly when it lies in the
                    // window where the ranges of the two types meet.
                    let (source_low, source_high) = source.range();
                    let (target_low, target_high) = target.range();
                    let low = source_low.max(target_low);
                    let high = source_high.min(target_high);
                    code.extend([c32(names.fault(Fault::OutOfRange)), c32(0)]);
                    code.extend([
                        get(WA),
                        c64(pattern(low)),
                        I::I64Sub,
                        c64(pattern(high - low)),
                        I::I64GtU,
                        I::Select,
                    ]);
                }
            }
            Body::CharToU32 => code.extend([get(WA), set(WR)]),
            Body::CharFromU32 => {
                // A scalar below U+110000 outside the surrogate range
                // U+D800..U+DFFF.
                code.extend([
                    get(WA),
                    c64(0x11_0000),
                    I::I64LtU,
                    get(WA),
                    c64(0xD800),
                    I::I64Sub,
                    c64(0x800),
                    I::I64GeU,
                    I::I32And,
                ]);
            }
        }
        code
    }

    /// The instructions that leave the value of the row, in the cell format of
    /// its type, on the operand stack: the 64 bits of a 64-bit type, and a type
    /// of at most 32 bits zero-extended from its 32 bits.
    pub(crate) fn value(&self) -> Vec<Ins> {
        let ty = match self.outcome {
            Outcome::Scalar(ty) | Outcome::Checked(ty) => ty,
            Outcome::Bool | Outcome::Ordering | Outcome::Option => NumericType::U32,
        };
        let mut code = vec![get(WR)];
        if !wide(ty) {
            code.extend([I::I32WrapI64, I::I64ExtendI32U]);
        }
        code
    }

    /// The class of the value of the row.
    pub(crate) fn class(&self) -> ValueClass {
        match self.outcome {
            Outcome::Scalar(ty) | Outcome::Checked(ty) if wide(ty) => ValueClass::I64,
            _ => ValueClass::I32,
        }
    }
}

/// Whether a type fills a 64-bit cell.
const fn wide(numeric: NumericType) -> bool {
    numeric.bits() == 64
}

/// The 64 bits of an integer, which is how a constant of the table is written.
fn pattern(value: i128) -> i64 {
    u64::try_from(value & i128::from(u64::MAX))
        .unwrap_or(0)
        .cast_signed()
}

/// Loads the operand in the cell at `offset` into `local`, as its exact integer.
fn load(numeric: NumericType, offset: u32, local: u32, code: &mut Vec<Ins>) {
    code.push(get(FRAME));
    if wide(numeric) {
        code.push(ld64(offset));
    } else {
        code.extend([
            ld32(offset),
            if numeric.is_signed() {
                I::I64ExtendI32S
            } else {
                I::I64ExtendI32U
            },
        ]);
    }
    code.push(set(local));
}

/// `code` when `condition` (an `i32` built by `condition`) holds, else `0`.
fn when(kind: i32, condition: &[Ins], code: &mut Vec<Ins>) {
    code.extend([c32(kind), c32(0)]);
    code.extend_from_slice(condition);
    code.push(I::Select);
}

/// `kind` when `first` holds, and otherwise whatever `rest` selects.
fn first_of(kind: i32, first: &[Ins], rest: &[Ins], code: &mut Vec<Ins>) {
    code.push(c32(kind));
    code.extend_from_slice(rest);
    code.extend_from_slice(first);
    code.push(I::Select);
}

/// `local == value`, as an `i32` condition.
fn equals(local: u32, value: i64) -> [Ins; 3] {
    [get(local), c64(value), I::I64Eq]
}

/// A checked arithmetic or shift row.
fn arithmetic(numeric: NumericType, op: IntegerOp, names: &Names, code: &mut Vec<Ins>) {
    let signed = numeric.is_signed();
    let exact = !wide(numeric);
    let (low, high) = numeric.range();
    let overflow = names.fault(Fault::Overflow);
    let (a, b, r) = (get(WA), get(WB), get(WR));
    let minimum = pattern(low);
    let zero_divisor = [b.clone(), I::I64Eqz];
    let minus_one = equals(WB, -1);
    // A shift is legal below the width, which is checked before its result is
    // used; the engine masks a count it cannot use.
    let bad_shift = [b.clone(), c64(i64::from(numeric.bits())), I::I64GeU];
    let shift_right = if signed { I::I64ShrS } else { I::I64ShrU };
    // A type of at most 32 bits is computed exactly, so it overflows when the
    // result is outside it.
    let outside = [
        r.clone(),
        c64(minimum),
        I::I64Sub,
        c64(pattern(high - low)),
        I::I64GtU,
    ]
    .to_vec();
    match op {
        IntegerOp::AddChecked => {
            code.extend([a.clone(), b.clone(), I::I64Add, set(WR)]);
            // A 64-bit sum overflows on a carry (unsigned), or when the
            // operands have one sign and the sum another (signed).
            let carried = if exact {
                outside
            } else if signed {
                vec![
                    a.clone(),
                    r.clone(),
                    I::I64Xor,
                    b.clone(),
                    r.clone(),
                    I::I64Xor,
                    I::I64And,
                    c64(0),
                    I::I64LtS,
                ]
            } else {
                vec![r, a, I::I64LtU]
            };
            when(overflow, &carried, code);
        }
        IntegerOp::SubChecked => {
            code.extend([a.clone(), b.clone(), I::I64Sub, set(WR)]);
            // A 64-bit difference overflows on a borrow (unsigned), or when the
            // operands differ in sign and the result's sign is not the
            // minuend's (signed).
            let borrowed = if exact {
                outside
            } else if signed {
                vec![
                    a.clone(),
                    b.clone(),
                    I::I64Xor,
                    a.clone(),
                    r.clone(),
                    I::I64Xor,
                    I::I64And,
                    c64(0),
                    I::I64LtS,
                ]
            } else {
                vec![a, b, I::I64LtU]
            };
            when(overflow, &borrowed, code);
        }
        IntegerOp::MulChecked => {
            code.extend([a.clone(), b.clone(), I::I64Mul, set(WR)]);
            let wrapped = if exact {
                outside
            } else {
                // A 64-bit product wrapped when it does not divide back to the
                // other factor. The divisor is `1` where the factor is zero,
                // and for a signed type where it is `-1` too, whose quotient
                // can trap and which has its own test.
                let mut test = vec![r.clone(), c64(1), a.clone(), a.clone(), I::I64Eqz];
                if signed {
                    test.extend(equals(WA, -1));
                    test.push(I::I32Or);
                }
                test.extend([
                    I::Select,
                    if signed { I::I64DivS } else { I::I64DivU },
                    b.clone(),
                    I::I64Ne,
                    a.clone(),
                    c64(0),
                    I::I64Ne,
                    I::I32And,
                ]);
                if signed {
                    test.extend([a.clone(), c64(-1), I::I64Ne, I::I32And]);
                    test.extend(equals(WA, -1));
                    test.extend(equals(WB, minimum));
                    test.extend([I::I32And, I::I32Or]);
                }
                test
            };
            when(overflow, &wrapped, code);
        }
        IntegerOp::NegChecked => {
            code.extend([c64(0), a.clone(), I::I64Sub, set(WR)]);
            when(overflow, &equals(WA, minimum), code);
        }
        IntegerOp::DivChecked | IntegerOp::RemChecked => {
            let divide = op == IntegerOp::DivChecked;
            let operation = match (divide, signed) {
                (true, true) => I::I64DivS,
                (true, false) => I::I64DivU,
                (false, true) => I::I64RemS,
                (false, false) => I::I64RemU,
            };
            // A signed `-1` divides to the negation of the dividend and has the
            // remainder `0`, which is selected over the operation with a
            // divisor it cannot trap on.
            if signed {
                code.push(c64(0));
                if divide {
                    code.extend([a.clone(), I::I64Sub]);
                }
            }
            code.extend([a.clone(), c64(1), b.clone()]);
            code.extend(zero_divisor.iter().cloned());
            if signed {
                code.extend(minus_one.iter().cloned());
                code.push(I::I32Or);
            }
            code.extend([I::Select, operation]);
            if signed {
                code.extend(minus_one.iter().cloned());
                code.push(I::Select);
            }
            code.push(set(WR));
            if divide && signed {
                // The minimum divided by `-1` is outside the type.
                let mut test = minus_one.to_vec();
                test.extend(equals(WA, minimum));
                test.push(I::I32And);
                let mut inner = Vec::new();
                when(overflow, &test, &mut inner);
                first_of(
                    names.fault(Fault::DivisionByZero),
                    &zero_divisor,
                    &inner,
                    code,
                );
            } else {
                when(names.fault(Fault::DivisionByZero), &zero_divisor, code);
            }
        }
        IntegerOp::ShiftLeftChecked => {
            code.extend([a.clone(), b.clone(), I::I64Shl, set(WR)]);
            let leaves = if exact {
                outside
            } else {
                // A bit was shifted out when shifting back does not restore the
                // operand.
                vec![r, b, shift_right, a, I::I64Ne]
            };
            let mut inner = Vec::new();
            when(overflow, &leaves, &mut inner);
            first_of(names.fault(Fault::InvalidShift), &bad_shift, &inner, code);
        }
        IntegerOp::ShiftRight => {
            code.extend([a, b, shift_right, set(WR)]);
            when(names.fault(Fault::InvalidShift), &bad_shift, code);
        }
        // Not a checked row: `plan` never gives these a body.
        IntegerOp::Equal | IntegerOp::Compare | IntegerOp::ToStr | IntegerOp::Parse => {
        }
    }
}
