//! Emit wasm32 MVP: import `wasi_snapshot_preview1.fd_write`, exported `memory` and `main`.
//!
//! Linear memory layout for [`emit_fd_write_stdout_wasm`] (little-endian):
//! - **`STRING_BASE` ..+ len**: UTF-8 message bytes (see [`STRING_BASE`]).
//! - **ciovec** at the next 8-byte aligned address: `ptr: i32` (=`STRING_BASE`), `len: i32`.
//! - **`nwritten`**: `i32` at ciovec+8, initialized to 0; `fd_write` writes bytes written there.

/// UTF-8 payload starts at this offset (one wasm page fits the MVP).
pub const STRING_BASE: i32 = 1024;

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
};

/// Single-page hello: write `message` to WASI stdout (fd **1**) via **`fd_write`**.
pub fn emit_fd_write_stdout_wasm(message: &[u8]) -> Vec<u8> {
    let len = message.len();
    let len_i32 = i32::try_from(len).expect("MVP: message length must fit i32");
    let string_end = STRING_BASE.saturating_add(len_i32);
    let iov_base = (string_end + 7) & !7;
    let nwritten_ptr = iov_base + 8;
    let mem_end = nwritten_ptr + 4;

    assert!(
        mem_end as usize <= 65536,
        "MVP: message too large for one wasm page"
    );

    let iov_rel = usize::try_from(iov_base - STRING_BASE).expect("layout");
    let mut payload = Vec::new();
    payload.extend_from_slice(message);
    while payload.len() < iov_rel {
        payload.push(0);
    }
    assert_eq!(payload.len(), iov_rel);
    payload.extend_from_slice(&STRING_BASE.to_le_bytes());
    payload.extend_from_slice(&len_i32.to_le_bytes());
    payload.extend_from_slice(&0i32.to_le_bytes());

    let mut types = TypeSection::new();
    // type 0: (i32, i32, i32, i32) -> i32   [fd_write]
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    // type 1: () -> ()   [main]
    types.ty().function([], []);

    let mut imports = ImportSection::new();
    imports.import(
        "wasi_snapshot_preview1",
        "fd_write",
        EntityType::Function(0),
    );

    let mut functions = FunctionSection::new();
    functions.function(1);

    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("main", ExportKind::Func, 1);

    let mut codes = CodeSection::new();
    let mut func = Function::new(vec![]);
    {
        let mut ins = func.instructions();
        ins.i32_const(crate::runtime::io::STDOUT_FD);
        ins.i32_const(iov_base);
        ins.i32_const(1);
        ins.i32_const(nwritten_ptr);
        ins.call(0);
        ins.drop();
        ins.end();
    }
    codes.function(&func);

    let mut data = DataSection::new();
    data.active(
        0,
        &ConstExpr::i32_const(STRING_BASE),
        payload.iter().copied(),
    );

    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&memory)
        .section(&exports)
        .section(&codes)
        .section(&data);

    module.finish()
}
