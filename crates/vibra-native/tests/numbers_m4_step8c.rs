//! The number-text natives over linear memory (milestone 4 Step 8c), without an
//! engine.
//!
//! Like `arena_m4_step8b`, these tests lay arena objects out by hand and call
//! each native as the runner does. A `to-str` row builds a `str`, so it measures
//! with no destination and fills the one it is given; a `parse` row reads a `str`
//! and writes the number into the 8-byte cell it is given, answering a status. No
//! native allocates, and a destination outside the memory is a fault, never a
//! write outside it.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::arena::kind;
use vibra_ir::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType};
use vibra_native::arena::{Fault, call, memory};

const TEXT: usize = 256;
const OUT: usize = 1024;
const CELL: usize = 2048;

struct Heap(Vec<u8>);

impl Heap {
    fn new() -> Self {
        Self(vec![0; 4096])
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn text(&mut self, at: usize, text: &str) {
        let scalars = text.chars().map(u32::from).collect::<Vec<_>>();
        self.header(at, kind::STR, 4, u32::try_from(scalars.len()).unwrap());
        for (index, scalar) in scalars.iter().enumerate() {
            self.put(at + 24 + 4 * index, &scalar.to_le_bytes());
        }
    }

    fn header(&mut self, at: usize, object_kind: u8, stride: u32, len: u32) {
        self.put(at, &1_u32.to_le_bytes());
        self.put(at + 4, &256_u32.to_le_bytes());
        self.put(at + 8, &[object_kind, u8::try_from(stride).unwrap()]);
        self.put(at + 12, &len.to_le_bytes());
    }

    fn run(
        &mut self,
        intrinsic: CompilerIntrinsic,
        scalars: &[i64],
    ) -> Result<i64, Fault> {
        let row = intrinsic.native_import().expect("a native row");
        call(&row, memory(&mut self.0), scalars)
    }

    fn string(&self, at: usize) -> String {
        let len = u32::from_le_bytes(self.0[at + 12..at + 16].try_into().unwrap());
        (0..len as usize)
            .map(|index| {
                let place = at + 24 + 4 * index;
                char::from_u32(u32::from_le_bytes(
                    self.0[place..place + 4].try_into().unwrap(),
                ))
                .unwrap()
            })
            .collect()
    }

    fn cell(&self) -> u64 {
        u64::from_le_bytes(self.0[CELL..CELL + 8].try_into().unwrap())
    }
}

/// `to-str` of the number whose cell holds `bits`: measured, then filled.
fn to_str(intrinsic: CompilerIntrinsic, bits: u64) -> String {
    let mut heap = Heap::new();
    let bits = bits.cast_signed();
    let length = heap.run(intrinsic, &[bits, 0]).unwrap();
    heap.header(OUT, kind::STR, 4, u32::try_from(length).unwrap());
    assert_eq!(heap.run(intrinsic, &[bits, OUT as i64]), Ok(length));
    heap.string(OUT)
}

/// `parse` of `text`: the status, and the cell when the status is `0`.
fn parse(intrinsic: CompilerIntrinsic, text: &str) -> (i64, Option<u64>) {
    let mut heap = Heap::new();
    heap.text(TEXT, text);
    let status = heap.run(intrinsic, &[TEXT as i64, CELL as i64]).unwrap();
    (status, (status == 0).then(|| heap.cell()))
}

fn integer(numeric: NumericType, op: IntegerOp) -> CompilerIntrinsic {
    CompilerIntrinsic::Integer(numeric, op)
}

fn float(numeric: NumericType, op: FloatOp) -> CompilerIntrinsic {
    CompilerIntrinsic::Float(numeric, op)
}

#[test]
fn an_integer_is_read_from_its_cell_by_its_type() {
    use IntegerOp::ToStr;
    use NumericType::{I8, I32, I64, U8, U32, U64};
    // A type of at most 32 bits is its 32-bit pattern zero-extended in the cell.
    assert_eq!(to_str(integer(I8, ToStr), 0xFFFF_FF80), "-128");
    assert_eq!(to_str(integer(U8, ToStr), 0x80), "128");
    assert_eq!(to_str(integer(I32, ToStr), 0x8000_0000), "-2147483648");
    assert_eq!(to_str(integer(U32, ToStr), 0x8000_0000), "2147483648");
    assert_eq!(
        to_str(integer(I64, ToStr), i64::MIN.cast_unsigned()),
        "-9223372036854775808"
    );
    assert_eq!(
        to_str(integer(U64, ToStr), u64::MAX),
        "18446744073709551615"
    );
    assert_eq!(to_str(integer(I8, ToStr), 0), "0");
}

#[test]
fn a_float_is_read_from_its_cell_and_a_nan_of_any_sign_and_payload_is_nan() {
    use FloatOp::ToStr;
    use NumericType::{F32, F64};
    assert_eq!(to_str(float(F64, ToStr), 1.5e300_f64.to_bits()), "1.5e300");
    assert_eq!(
        to_str(float(F32, ToStr), u64::from(0.1_f32.to_bits())),
        "0.1"
    );
    assert_eq!(to_str(float(F64, ToStr), (-0.0_f64).to_bits()), "-0.0");
    for bits in [
        0x7FF8_0000_0000_0000_u64,
        0xFFF8_0000_0000_0000,
        0x7FF0_0000_0000_0001,
        0xFFFF_FFFF_FFFF_FFFF,
        0xFFF4_0000_DEAD_BEEF,
    ] {
        assert_eq!(to_str(float(F64, ToStr), bits), "nan", "{bits:#x}");
    }
    for bits in [0x7FC0_0000_u64, 0xFFC0_0000, 0x7F80_0001, 0xFFA0_BEEF] {
        assert_eq!(to_str(float(F32, ToStr), bits), "nan", "{bits:#x}");
    }
}

#[test]
fn parse_writes_the_cell_of_the_type_and_answers_a_status() {
    use IntegerOp::Parse;
    use NumericType::{F32, F64, I8, I64, U8, U64};
    // `0`: the cell holds the number, in the cell format of the type.
    assert_eq!(parse(integer(I8, Parse), "-128"), (0, Some(0xFFFF_FF80)));
    assert_eq!(parse(integer(U8, Parse), "255"), (0, Some(255)));
    assert_eq!(
        parse(integer(I64, Parse), "-9223372036854775808"),
        (0, Some(i64::MIN.cast_unsigned()))
    );
    assert_eq!(
        parse(integer(U64, Parse), "18446744073709551615"),
        (0, Some(u64::MAX))
    );
    // `1`: `invalid-format`; `2`: `out-of-range`; the cell is left alone.
    assert_eq!(parse(integer(I8, Parse), "128"), (2, None));
    assert_eq!(parse(integer(U8, Parse), "-1"), (1, None));
    assert_eq!(parse(integer(U8, Parse), ""), (1, None));
    assert_eq!(parse(integer(U8, Parse), "\u{661}"), (1, None));
    assert_eq!(
        parse(float(F64, FloatOp::Parse), "2.5e-3"),
        (0, Some(2.5e-3_f64.to_bits()))
    );
    assert_eq!(
        parse(float(F32, FloatOp::Parse), "1.5"),
        (0, Some(u64::from(1.5_f32.to_bits())))
    );
    assert_eq!(parse(float(F32, FloatOp::Parse), "1e39"), (2, None));
    assert_eq!(parse(float(F64, FloatOp::Parse), "12"), (1, None));
    let mut heap = Heap::new();
    heap.text(TEXT, "129");
    heap.put(CELL, &[0xAB; 8]);
    assert_eq!(
        heap.run(integer(I8, Parse), &[TEXT as i64, CELL as i64]),
        Ok(2)
    );
    assert_eq!(
        heap.cell(),
        0xABAB_ABAB_ABAB_ABAB,
        "an error leaves the cell"
    );
}

#[test]
fn a_destination_outside_the_memory_or_missing_is_a_fault_and_never_a_write() {
    use IntegerOp::{Parse, ToStr};
    let mut heap = Heap::new();
    heap.text(TEXT, "5");
    let parse = integer(NumericType::U8, Parse);
    assert_eq!(heap.run(parse, &[TEXT as i64, 0]), Err(Fault::Destination));
    assert_eq!(
        heap.run(parse, &[TEXT as i64, 4093]),
        Err(Fault::OutOfBounds)
    );
    assert_eq!(
        heap.run(parse, &[TEXT as i64, 1 << 30]),
        Err(Fault::OutOfBounds)
    );
    // `to-str` into a destination that is not sized to its text.
    heap.header(OUT, kind::STR, 4, 3);
    assert_eq!(
        heap.run(integer(NumericType::U8, ToStr), &[5, OUT as i64]),
        Err(Fault::Destination)
    );
    // A parse of something that is not a `str`.
    heap.header(OUT, kind::BYTES, 1, 0);
    assert_eq!(
        heap.run(parse, &[OUT as i64, CELL as i64]),
        Err(Fault::WrongObject)
    );
}
