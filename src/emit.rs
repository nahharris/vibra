//! Emit wasm32 MVP: `env.println(i32 ptr, i32 len)`, exported `memory` and `main`.

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
};

/// UTF-8 string placed at this offset in linear memory (one page is enough for MVP).
const DATA_OFFSET: i32 = 1024;

pub fn emit_println_wasm(message: &[u8]) -> Vec<u8> {
    let len = message.len();
    assert!(
        DATA_OFFSET as usize + len <= 65536,
        "MVP: message too large for one wasm page"
    );

    let mut types = TypeSection::new();
    types.ty().function([ValType::I32, ValType::I32], []);
    types.ty().function([], []);

    let mut imports = ImportSection::new();
    imports.import("env", "println", EntityType::Function(0));

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
        ins.i32_const(DATA_OFFSET);
        ins.i32_const(len as i32);
        ins.call(0);
        ins.end();
    }
    codes.function(&func);

    let mut data = DataSection::new();
    data.active(
        0,
        &ConstExpr::i32_const(DATA_OFFSET),
        message.iter().copied(),
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
