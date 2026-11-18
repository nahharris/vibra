//! The integer operations of the unchecked family, written once.
//!
//! `docs/spec/06-runtime.md`, "M3 compiler intrinsic registry", defines the
//! unchecked integer operations: a result past the type saturates at its
//! nearest bound, so overflow never wraps. This module is the only place the
//! runtime computes them. The reference stepper and generated code both reach it
//! through [`crate::Context::scalar_op`], so the two cannot disagree.
//!
//! # Representation
//!
//! A value of an integer type lives in a [`Cell`]. A type of at most 32 bits is
//! an `I32` cell holding the value canonically: a signed type sign-extended to
//! 32 bits, an unsigned one zero-extended. A 64-bit type is an `I64` cell
//! holding all 64 bits. This is the representation the lowering writes for a
//! literal and the one the Wasm backend keeps.
//!
//! Every operation decodes its operands to their exact integer in an `i128`,
//! which holds any value of any integer type and any product of two of them
//! once saturated, computes the exact result of the specification, and clamps
//! it to the bounds of the type. There is no per-width code and no carry trick,
//! so a width cannot be wrong on its own.
//!
//! # What has no answer
//!
//! [`apply`] answers `None` for an input a verified program built by the
//! lowering never gives it: a floating-point type, an operand whose class is
//! not the type's class, an operand outside the type, and a zero divisor. The
//! lowering tests a divisor before it divides and traps with
//! `@runtime.division-by-zero` itself, so a zero divisor here is a defect of the
//! program, and [`crate::Context::scalar_op`] records
//! `@runtime.invalid-checked-program` for every `None`.

use vibra_ir::arena::class;
use vibra_ir::external::NumericType;

use crate::store::Cell;

/// An operation over two integers of one type. A shift takes a `u32` amount as
/// its second operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntOp {
    /// `add`: the sum, saturated at the bounds of the type.
    Add,
    /// `sub`: the difference, saturated at the bounds of the type.
    Sub,
    /// `mul`: the product, saturated at the bounds of the type.
    Mul,
    /// `div`: the quotient truncated toward zero, saturated, so the signed
    /// minimum divided by `-1` is the maximum.
    Div,
    /// `rem`: the remainder with the dividend's sign, so the signed minimum and
    /// `-1` give `0`.
    Rem,
    /// `shift-left`: `left * 2^amount` saturated, for every amount.
    ShiftLeft,
    /// `shift-right`: `floor(left / 2^amount)`, for every amount.
    ShiftRight,
    /// Equality, an `I32` `0` or `1`.
    Equal,
    /// Less-than in the order of the type, an `I32` `0` or `1`.
    Less,
}

impl IntOp {
    /// Every operation, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Rem,
        Self::ShiftLeft,
        Self::ShiftRight,
        Self::Equal,
        Self::Less,
    ];

    /// Whether the second operand is a `u32` shift amount instead of a value of
    /// the operation's type.
    #[must_use]
    pub const fn takes_amount(self) -> bool {
        matches!(self, Self::ShiftLeft | Self::ShiftRight)
    }
}

/// The integer types, in the order of the wire encoding.
pub const INTEGER_TYPES: [NumericType; 8] = [
    NumericType::I8,
    NumericType::I16,
    NumericType::I32,
    NumericType::I64,
    NumericType::U8,
    NumericType::U16,
    NumericType::U32,
    NumericType::U64,
];

/// The exact integer a cell holds as a value of `numeric`, or `None` when the
/// cell is not a canonical value of the type.
fn decode(cell: Cell, numeric: NumericType) -> Option<i128> {
    if !numeric.is_integer() {
        return None;
    }
    let signed = numeric.is_signed();
    let value = if numeric.bits() == 64 {
        if cell.class != class::I64 {
            return None;
        }
        if signed {
            i128::from(i64::from_ne_bytes(cell.bits.to_ne_bytes()))
        } else {
            i128::from(cell.bits)
        }
    } else {
        if cell.class != class::I32 {
            return None;
        }
        let bits = u32::try_from(cell.bits).ok()?;
        if signed {
            i128::from(i32::from_ne_bytes(bits.to_ne_bytes()))
        } else {
            i128::from(bits)
        }
    };
    let (low, high) = numeric.range();
    (low..=high).contains(&value).then_some(value)
}

/// The cell for `value`, which lies in the range of `numeric`.
fn encode(numeric: NumericType, value: i128) -> Option<Cell> {
    if numeric.bits() == 64 {
        let bits = u64::try_from(value & i128::from(u64::MAX)).ok()?;
        Some(Cell::i64(bits))
    } else {
        let bits = u32::try_from(value & i128::from(u32::MAX)).ok()?;
        Some(Cell::i32(bits))
    }
}

/// Applies `op` at `numeric` to the cells `left` and `right`.
///
/// `None` for an input a verified program never gives: see the module
/// documentation. The result of every operation is a cell of the type, except
/// that [`IntOp::Equal`] and [`IntOp::Less`] answer an `I32` `0` or `1`.
#[must_use]
pub fn apply(op: IntOp, numeric: NumericType, left: Cell, right: Cell) -> Option<Cell> {
    let (low, high) = numeric.range();
    let left = decode(left, numeric)?;
    let saturated = |exact: i128| exact.clamp(low, high);
    let result = if op.takes_amount() {
        let amount = decode(right, NumericType::U32)?;
        match op {
            IntOp::ShiftLeft => {
                if left == 0 {
                    0
                } else if amount >= 64 {
                    // Any nonzero value shifted by 64 is past every bound.
                    if left < 0 { low } else { high }
                } else {
                    saturated(left << u32::try_from(amount).ok()?)
                }
            }
            // `floor(left / 2^amount)` is the arithmetic shift, and past the
            // width of the widest carrier it is `0` or `-1` by the sign.
            _ => left >> u32::try_from(amount.min(127)).ok()?,
        }
    } else {
        let right = decode(right, numeric)?;
        match op {
            IntOp::Add => saturated(left + right),
            IntOp::Sub => saturated(left - right),
            // The product of two 64-bit values can leave `i128`; the saturating
            // product keeps its sign, and the clamp does the rest.
            IntOp::Mul => saturated(left.saturating_mul(right)),
            IntOp::Div => saturated(left.checked_div(right)?),
            IntOp::Rem => left.checked_rem(right)?,
            IntOp::Equal => return Some(Cell::i32(u32::from(left == right))),
            _ => return Some(Cell::i32(u32::from(left < right))),
        }
    };
    encode(numeric, result)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use vibra_ir::Value;
    use vibra_ir::external::{CompilerIntrinsic, IntegerOp, Outcome};

    use super::*;

    /// The cell of a sample operand.
    fn cell(value: &Value) -> Cell {
        let narrow = |value: i32| Cell::i32(value.cast_unsigned());
        match value {
            Value::I8(value) => narrow(i32::from(*value)),
            Value::I16(value) => narrow(i32::from(*value)),
            Value::I32(value) => narrow(*value),
            Value::U8(value) => Cell::i32(u32::from(*value)),
            Value::U16(value) => Cell::i32(u32::from(*value)),
            Value::U32(value) => Cell::i32(*value),
            Value::I64(value) => Cell::i64(value.cast_unsigned()),
            Value::U64(value) => Cell::i64(*value),
            other => panic!("{other:?} is not an integer sample"),
        }
    }

    fn operation(op: IntegerOp) -> Option<IntOp> {
        Some(match op {
            IntegerOp::Add => IntOp::Add,
            IntegerOp::Sub => IntOp::Sub,
            IntegerOp::Mul => IntOp::Mul,
            IntegerOp::Div => IntOp::Div,
            IntegerOp::Rem => IntOp::Rem,
            IntegerOp::ShiftLeft => IntOp::ShiftLeft,
            IntegerOp::ShiftRight => IntOp::ShiftRight,
            IntegerOp::Equal => IntOp::Equal,
            _ => return None,
        })
    }

    /// The shared sample vectors of `vibra_ir::external::vectors`, which the
    /// interpreter and the Wasm backend are held to, run through the Nexus
    /// operation. `neg` and `compare` are not operations of their own here: the
    /// lowering writes `neg` as `0 - x` and `compare` as `less` and `equal`, so
    /// they are checked by the same vectors through those operations.
    #[test]
    fn every_vector_of_the_unchecked_integer_family_matches() {
        let mut points = 0_usize;
        for intrinsic in CompilerIntrinsic::all() {
            let CompilerIntrinsic::Integer(numeric, op) = intrinsic else {
                continue;
            };
            if matches!(
                op,
                IntegerOp::AddChecked
                    | IntegerOp::SubChecked
                    | IntegerOp::MulChecked
                    | IntegerOp::DivChecked
                    | IntegerOp::RemChecked
                    | IntegerOp::NegChecked
                    | IntegerOp::ShiftLeftChecked
                    | IntegerOp::ShiftRightChecked
                    | IntegerOp::ToStr
                    | IntegerOp::Parse
            ) {
                continue;
            }
            for vector in intrinsic.vectors() {
                let operands = vector.operands.iter().map(cell).collect::<Vec<_>>();
                let zero = cell(&match numeric {
                    NumericType::I8 => Value::I8(0),
                    NumericType::I16 => Value::I16(0),
                    NumericType::I32 => Value::I32(0),
                    NumericType::I64 => Value::I64(0),
                    NumericType::U8 => Value::U8(0),
                    NumericType::U16 => Value::U16(0),
                    NumericType::U32 => Value::U32(0),
                    _ => Value::U64(0),
                });
                match (op, &vector.outcome) {
                    (IntegerOp::Neg, Outcome::Value(want)) => {
                        let got = apply(IntOp::Sub, numeric, zero, operands[0]);
                        assert_eq!(got, Some(cell(want)), "{vector:?}");
                    }
                    (IntegerOp::Compare, Outcome::Order(order)) => {
                        let less =
                            apply(IntOp::Less, numeric, operands[0], operands[1]);
                        let equal =
                            apply(IntOp::Equal, numeric, operands[0], operands[1]);
                        let got = match (less, equal) {
                            (Some(less), _) if less.bits == 1 => "less",
                            (_, Some(equal)) if equal.bits == 1 => "equal",
                            _ => "greater",
                        };
                        assert_eq!(got, *order, "{vector:?}");
                    }
                    (_, Outcome::Value(want)) => {
                        let Some(op) = operation(op) else {
                            continue;
                        };
                        let got = apply(op, numeric, operands[0], operands[1]);
                        let want = match want {
                            Value::Bool(flag) => Cell::i32(u32::from(*flag)),
                            other => cell(other),
                        };
                        assert_eq!(got, Some(want), "{vector:?}");
                    }
                    _ => continue,
                }
                points += 1;
            }
        }
        assert!(points > 10_000, "{points}");
    }

    #[test]
    fn what_has_no_answer_is_none() {
        let one = Cell::i32(1);
        // A zero divisor, in a division and a remainder.
        let zero = Cell::i32(0);
        assert_eq!(apply(IntOp::Div, NumericType::I32, one, zero), None);
        assert_eq!(apply(IntOp::Rem, NumericType::U8, one, zero), None);
        // The wrong class: a 32-bit cell for a 64-bit type and the reverse.
        assert_eq!(apply(IntOp::Add, NumericType::U64, one, one), None);
        assert_eq!(
            apply(IntOp::Add, NumericType::U32, Cell::i64(1), Cell::i64(1)),
            None
        );
        // A value outside the type: 300 is not a `u8`, and a `u8` is not
        // sign-extended.
        assert_eq!(
            apply(IntOp::Add, NumericType::U8, Cell::i32(300), one),
            None
        );
        assert_eq!(
            apply(IntOp::Add, NumericType::I8, Cell::i32(200), one),
            None
        );
        // A float type.
        assert_eq!(apply(IntOp::Add, NumericType::F32, one, one), None);
        // A shift amount that is not a `u32` cell.
        assert_eq!(
            apply(
                IntOp::ShiftLeft,
                NumericType::U64,
                Cell::i64(1),
                Cell::i64(1)
            ),
            None
        );
    }

    #[test]
    fn the_exceptional_inputs_follow_the_specification() {
        let signed = |value: i64| Cell::i64(value.cast_unsigned());
        // The signed minimum divided by -1 saturates at the maximum, and its
        // remainder is 0.
        let minimum = signed(i64::MIN);
        assert_eq!(
            apply(IntOp::Div, NumericType::I64, minimum, signed(-1)),
            Some(signed(i64::MAX))
        );
        assert_eq!(
            apply(IntOp::Rem, NumericType::I64, minimum, signed(-1)),
            Some(signed(0))
        );
        // A product past 128 bits still saturates by its sign.
        assert_eq!(
            apply(
                IntOp::Mul,
                NumericType::U64,
                Cell::i64(u64::MAX),
                Cell::i64(u64::MAX)
            ),
            Some(Cell::i64(u64::MAX))
        );
        // A shift left by 63 of the largest `u64` stays inside `i128` and
        // saturates; zero stays zero for every amount.
        assert_eq!(
            apply(
                IntOp::ShiftLeft,
                NumericType::U64,
                Cell::i64(u64::MAX),
                Cell::i32(63)
            ),
            Some(Cell::i64(u64::MAX))
        );
        assert_eq!(
            apply(
                IntOp::ShiftLeft,
                NumericType::I8,
                Cell::i32(0),
                Cell::i32(u32::MAX)
            ),
            Some(Cell::i32(0))
        );
    }
}
