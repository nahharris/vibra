//! The wire encoding of `nexus_op`: one mapping from the runtime's operations to
//! codes and back, held to every operation and to every code.

use std::collections::BTreeMap;

use vibra_nexus::ScalarOp;
use vibra_nexus_bridge::{
    OP_I64_ADD, OP_I64_EQ, OP_I64_LT_S, OP_I64_SUB, OP_INT_ADD, OP_INT_DIV,
    OP_INT_EQUAL, OP_INT_LESS, OP_INT_MUL, OP_INT_REM, OP_INT_SHIFT_LEFT,
    OP_INT_SHIFT_RIGHT, OP_INT_SUB, decode_scalar_op, encode_scalar_op, int_op_code,
};

#[test]
fn the_four_original_codes_keep_their_values() {
    // The codes are append-only: a program built for them still means what it
    // meant.
    assert_eq!(
        [OP_I64_ADD, OP_I64_SUB, OP_I64_EQ, OP_I64_LT_S],
        [0, 1, 2, 3]
    );
    assert_eq!(
        [
            OP_INT_ADD,
            OP_INT_SUB,
            OP_INT_MUL,
            OP_INT_DIV,
            OP_INT_REM,
            OP_INT_SHIFT_LEFT,
            OP_INT_SHIFT_RIGHT,
            OP_INT_EQUAL,
            OP_INT_LESS
        ],
        [4, 5, 6, 7, 8, 9, 10, 11, 12]
    );
}

#[test]
fn every_operation_has_one_code_and_decodes_back() {
    let all = ScalarOp::all();
    assert_eq!(all.len(), 4 + 9 * 8);
    let mut by_code = BTreeMap::new();
    for op in &all {
        let code = encode_scalar_op(*op);
        assert_eq!(decode_scalar_op(code), Some(*op), "{op:?} as {code:#x}");
        assert_eq!(by_code.insert(code, *op), None, "{op:?} shares {code:#x}");
    }
}

#[test]
fn every_code_that_decodes_is_the_code_of_its_operation() {
    // Every code up to the first three bytes: the operation field, the type
    // field, and a good deal of what lies beyond them.
    let mut decoded = 0_usize;
    for code in 0..(1_u32 << 18) {
        if let Some(op) = decode_scalar_op(code) {
            assert_eq!(encode_scalar_op(op), code, "{op:?}");
            decoded += 1;
        }
    }
    assert_eq!(decoded, ScalarOp::all().len());
    // And no code with any high bit set is an operation.
    for shift in 18..32 {
        for low in [0, OP_INT_ADD, int_op_code(OP_INT_ADD, 2)] {
            assert_eq!(decode_scalar_op(low | (1 << shift)), None);
        }
    }
    assert_eq!(decode_scalar_op(u32::MAX), None);
}
