//! The natives over linear memory (milestone 4 Step 8b), without an engine.
//!
//! These tests lay arena objects out by hand, in the layout of
//! `vibra_ir::arena`, and call each native as the runner does. They hold the
//! protocol the import table states: a row measures with no destination and
//! fills the one it is given, takes one `dup` for each reference it copies,
//! and refuses an operand that is not an object of the row, instead of reading
//! outside the memory.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::arena::{self, class, kind};
use vibra_ir::external::{CompilerIntrinsic, NATIVE_ROWS, NativeRow};
use vibra_native::arena::{Fault, call, memory};

fn row(intrinsic: CompilerIntrinsic) -> NativeRow {
    intrinsic.native_import().expect("a native row")
}

/// A memory of blocks that a test lays out by hand.
struct Heap(Vec<u8>);

const A: usize = 256;
const B: usize = 512;
const OUT: usize = 1024;

impl Heap {
    fn new() -> Self {
        Self(vec![0; 4096])
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn header(&mut self, at: usize, object_kind: u8, stride: u32, len: u32) {
        self.put(at, &1_u32.to_le_bytes());
        self.put(at + 4, &256_u32.to_le_bytes());
        self.put(at + 8, &[object_kind, u8::try_from(stride).unwrap()]);
        self.put(at + 12, &len.to_le_bytes());
    }

    fn text(&mut self, at: usize, text: &str) {
        let scalars = text.chars().map(u32::from).collect::<Vec<_>>();
        self.header(at, kind::STR, 4, u32::try_from(scalars.len()).unwrap());
        for (index, scalar) in scalars.iter().enumerate() {
            self.put(at + 24 + 4 * index, &scalar.to_le_bytes());
        }
    }

    fn bytes(&mut self, at: usize, bytes: &[u8]) {
        self.header(at, kind::BYTES, 1, u32::try_from(bytes.len()).unwrap());
        self.put(at + 24, bytes);
    }

    /// An array of cells `(class, bits)`.
    fn cells(&mut self, at: usize, cells: &[(u8, u64)]) {
        let len = u32::try_from(cells.len()).unwrap();
        self.header(at, kind::ARRAY, 8, len);
        let first = at + usize::try_from(arena::cells_offset(len)).unwrap();
        for (index, (class, bits)) in cells.iter().enumerate() {
            self.put(at + 24 + index, &[*class]);
            self.put(first + 8 * index, &bits.to_le_bytes());
        }
    }

    fn run(
        &mut self,
        intrinsic: CompilerIntrinsic,
        scalars: &[i64],
    ) -> Result<i64, Fault> {
        call(&row(intrinsic), memory(&mut self.0), scalars)
    }

    fn scalars_of(&self, at: usize) -> Vec<u32> {
        let len = u32::from_le_bytes(self.0[at + 12..at + 16].try_into().unwrap());
        (0..len as usize)
            .map(|index| {
                let place = at + 24 + 4 * index;
                u32::from_le_bytes(self.0[place..place + 4].try_into().unwrap())
            })
            .collect()
    }

    fn count(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
    }
}

#[test]
fn a_row_measures_with_no_destination_and_fills_the_one_it_is_given() {
    let mut heap = Heap::new();
    heap.text(A, "hé");
    heap.text(B, "𝄞!");
    let (a, b) = (A as i64, B as i64);
    assert_eq!(heap.run(CompilerIntrinsic::TextConcat, &[a, b, 0]), Ok(4));
    heap.header(OUT, kind::STR, 4, 4);
    assert_eq!(
        heap.run(CompilerIntrinsic::TextConcat, &[a, b, OUT as i64]),
        Ok(4)
    );
    assert_eq!(
        heap.scalars_of(OUT),
        [u32::from('h'), u32::from('é'), 0x1D11E, u32::from('!')]
    );
}

#[test]
fn a_partial_row_measures_minus_one_when_it_has_no_object() {
    let mut heap = Heap::new();
    heap.text(A, "abc");
    let a = A as i64;
    for (start, end, want) in [(1, 3, 2), (2, 1, -1), (0, 4, -1), (3, 3, 0)] {
        assert_eq!(
            heap.run(CompilerIntrinsic::TextSlice, &[a, start, end, 0]),
            Ok(want),
            "{start}..{end}"
        );
    }
    heap.bytes(B, &[0xED, 0xA0, 0x80]);
    assert_eq!(
        heap.run(CompilerIntrinsic::TextFromUtf8, &[B as i64, 0]),
        Ok(-1)
    );
    heap.bytes(B, &[0x61, 0xC3, 0xA9]);
    assert_eq!(
        heap.run(CompilerIntrinsic::TextFromUtf8, &[B as i64, 0]),
        Ok(2)
    );
}

#[test]
fn a_native_that_orders_answers_minus_one_zero_or_one() {
    let mut heap = Heap::new();
    heap.text(A, "ab");
    heap.text(B, "b");
    let (a, b) = (A as i64, B as i64);
    assert_eq!(heap.run(CompilerIntrinsic::TextCompare, &[a, b]), Ok(-1));
    assert_eq!(heap.run(CompilerIntrinsic::TextCompare, &[b, a]), Ok(1));
    assert_eq!(heap.run(CompilerIntrinsic::TextCompare, &[a, a]), Ok(0));
}

#[test]
fn building_an_array_takes_one_dup_for_each_reference_it_copies() {
    let mut heap = Heap::new();
    // Two elements, the first a reference to the text at B.
    heap.text(B, "x");
    heap.cells(A, &[(class::REF, B as u64), (class::I32, 7)]);
    heap.header(OUT, kind::ARRAY, 8, 3);
    let appended = heap.run(
        CompilerIntrinsic::ArrayAppend,
        &[A as i64, B as i64, i64::from(class::REF), OUT as i64],
    );
    assert_eq!(appended, Ok(3));
    // The copy of the first element and the appended one are two more counts of
    // the text, and the source array keeps its own.
    assert_eq!(heap.count(B), 1 + 2);
    assert_eq!(heap.count(A), 1);
    // Measuring takes no count.
    let before = heap.count(B);
    assert_eq!(
        heap.run(CompilerIntrinsic::ArrayConcat, &[A as i64, A as i64, 0]),
        Ok(4)
    );
    assert_eq!(heap.count(B), before);
}

#[test]
fn an_operand_that_is_not_an_object_of_the_row_is_a_fault_and_never_a_read() {
    let mut heap = Heap::new();
    heap.text(A, "abc");
    heap.bytes(B, &[1, 2]);
    let compare = |heap: &mut Heap, left: i64, right: i64| {
        heap.run(CompilerIntrinsic::TextCompare, &[left, right])
    };
    // An offset past the end of the memory, and a header that ends there.
    assert_eq!(
        compare(&mut heap, 1 << 20, A as i64),
        Err(Fault::OutOfBounds)
    );
    assert_eq!(compare(&mut heap, 4090, A as i64), Err(Fault::OutOfBounds));
    // `bytes` where a `str` is taken.
    assert_eq!(
        compare(&mut heap, B as i64, A as i64),
        Err(Fault::WrongObject)
    );
    // A header whose length reaches past its block.
    heap.header(OUT, kind::STR, 4, 1000);
    assert_eq!(
        compare(&mut heap, OUT as i64, A as i64),
        Err(Fault::OutOfBounds)
    );
    // A destination that is not sized to the result.
    heap.header(OUT, kind::STR, 4, 2);
    assert_eq!(
        heap.run(
            CompilerIntrinsic::TextConcat,
            &[A as i64, A as i64, OUT as i64]
        ),
        Err(Fault::Destination)
    );
    // A call that does not match the signature.
    assert_eq!(
        heap.run(CompilerIntrinsic::TextConcat, &[A as i64]),
        Err(Fault::Signature)
    );
}

#[test]
fn every_row_of_the_table_is_implemented() {
    let mut heap = Heap::new();
    heap.text(A, "ab");
    heap.bytes(B, &[1, 2]);
    heap.cells(OUT, &[(class::I32, 1)]);
    for row in &NATIVE_ROWS {
        // Each operand is any object of its kind; the call only has to be
        // recognized, whatever it answers.
        let mut scalars = Vec::new();
        for operand in row.operands {
            use vibra_ir::external::Operand;
            match operand {
                Operand::Text => scalars.push(A as i64),
                Operand::Bytes => scalars.push(B as i64),
                Operand::Cells => scalars.push(OUT as i64),
                Operand::Index | Operand::Number(_) => scalars.push(0),
                Operand::Element => scalars.extend([0, i64::from(class::I32)]),
            }
        }
        if matches!(row.answer, vibra_ir::external::Answer::Object { .. }) {
            scalars.push(0);
        }
        if matches!(row.answer, vibra_ir::external::Answer::Parsed) {
            scalars.push(2048);
        }
        let answer = call(row, memory(&mut heap.0), &scalars);
        assert!(answer.is_ok(), "{}: {answer:?}", row.name());
    }
}
