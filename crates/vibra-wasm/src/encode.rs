//! Writes the module bytes.
//!
//! Every section is written in the fixed order of the binary format, from
//! tables that are built in program order, so the bytes depend on nothing but
//! the checked program: no clock, path, address, or hash-table order reaches
//! them. No custom section is written, not even a `name` section, because a v1
//! module the toolchain emits before `vibra build` exists carries none.

use vibra_ir::boundary::{
    ENTRY_EXPORT, LIVE_SIZE_EXPORT, MEMORY_EXPORT, ORIGIN_EXPORT, RESULT_EXPORT,
    STATUS_EXPORT, TRAP_CODE_EXPORT,
};
use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, GlobalType, Instruction, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

/// The pages of linear memory the module defines. The arena of a later step
/// grows from here; a module with none still defines one memory.
const INITIAL_PAGES: u64 = 1;

/// `() -> ()`, the type of every lowered function and of the entry export.
const TYPE_UNIT: u32 = 0;
/// `() -> i32`, the type of the status accessors.
const TYPE_I32: u32 = 1;
/// `() -> i64`, the type of the result and size accessors.
const TYPE_I64: u32 = 2;

/// The recorded-state globals, in the order they are defined.
const GLOBAL_STATUS: u32 = 0;
const GLOBAL_TRAP_CODE: u32 = 1;
const GLOBAL_ORIGIN: u32 = 2;
const GLOBAL_RESULT: u32 = 3;

/// Encodes a module with one `() -> ()` function per source function, in
/// source order, and the exports of a program whose entry is `entry`.
///
/// The caller has established that every function is lowered, which for the
/// skeleton means each one is the empty `void` function.
pub(crate) fn module(function_count: u32, entry: u32) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().function([], [ValType::I32]);
    types.ty().function([], [ValType::I64]);

    // Lowered functions first, then the six accessors the exports name.
    let entry_wrapper = function_count;
    let status = function_count + 1;
    let trap_code = function_count + 2;
    let origin = function_count + 3;
    let result = function_count + 4;
    let live_size = function_count + 5;

    let mut functions = FunctionSection::new();
    for _ in 0..function_count {
        functions.function(TYPE_UNIT);
    }
    functions.function(TYPE_UNIT);
    for _ in 0..3 {
        functions.function(TYPE_I32);
    }
    for _ in 0..2 {
        functions.function(TYPE_I64);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: INITIAL_PAGES,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut globals = GlobalSection::new();
    for _ in 0..3 {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
    }
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(0),
    );

    let mut exports = ExportSection::new();
    exports.export(MEMORY_EXPORT, ExportKind::Memory, 0);
    exports.export(ENTRY_EXPORT, ExportKind::Func, entry_wrapper);
    exports.export(STATUS_EXPORT, ExportKind::Func, status);
    exports.export(TRAP_CODE_EXPORT, ExportKind::Func, trap_code);
    exports.export(ORIGIN_EXPORT, ExportKind::Func, origin);
    exports.export(RESULT_EXPORT, ExportKind::Func, result);
    exports.export(LIVE_SIZE_EXPORT, ExportKind::Func, live_size);

    let mut code = CodeSection::new();
    for _ in 0..function_count {
        code.function(&body(&[]));
    }
    // The entry export runs the entry. Its result is `void`, whose ID is `0`,
    // and the result global already holds `0`.
    code.function(&body(&[Instruction::Call(entry)]));
    for global in [
        GLOBAL_STATUS,
        GLOBAL_TRAP_CODE,
        GLOBAL_ORIGIN,
        GLOBAL_RESULT,
    ] {
        code.function(&body(&[Instruction::GlobalGet(global)]));
    }
    // No arena exists yet, so no byte of it is live.
    code.function(&body(&[Instruction::I64Const(0)]));

    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&code);
    module.finish()
}

fn body(instructions: &[Instruction<'_>]) -> Function {
    let mut function = Function::new([]);
    for instruction in instructions {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    function
}
