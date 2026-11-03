//! Helpers shared by the `vibra-native` integration tests: a block of linear
//! memory the tests lay arena objects out in by hand, and the native each
//! intrinsic names.

use vibra_ir::arena::kind;
use vibra_ir::external::{CompilerIntrinsic, NativeRow};
use vibra_native::arena::{Fault, call, memory};

/// Where a test puts the destination it hands a native.
pub(crate) const OUT: usize = 1024;

/// The native import row of `intrinsic`.
pub(crate) fn row(intrinsic: CompilerIntrinsic) -> NativeRow {
    intrinsic.native_import().expect("a native row")
}

/// A memory of blocks that a test lays out by hand.
pub(crate) struct Heap(pub(crate) Vec<u8>);

impl Heap {
    pub(crate) fn new() -> Self {
        Self(vec![0; 4096])
    }

    pub(crate) fn put(&mut self, at: usize, bytes: &[u8]) {
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
    }

    pub(crate) fn header(&mut self, at: usize, object_kind: u8, stride: u32, len: u32) {
        self.put(at, &1_u32.to_le_bytes());
        self.put(at + 4, &256_u32.to_le_bytes());
        self.put(at + 8, &[object_kind, u8::try_from(stride).unwrap()]);
        self.put(at + 12, &len.to_le_bytes());
    }

    pub(crate) fn text(&mut self, at: usize, text: &str) {
        let scalars = text.chars().map(u32::from).collect::<Vec<_>>();
        self.header(at, kind::STR, 4, u32::try_from(scalars.len()).unwrap());
        for (index, scalar) in scalars.iter().enumerate() {
            self.put(at + 24 + 4 * index, &scalar.to_le_bytes());
        }
    }

    /// Calls the native `intrinsic` names over this memory, as the runner does.
    pub(crate) fn run(
        &mut self,
        intrinsic: CompilerIntrinsic,
        scalars: &[i64],
    ) -> Result<i64, Fault> {
        call(&row(intrinsic), memory(&mut self.0), scalars)
    }
}
