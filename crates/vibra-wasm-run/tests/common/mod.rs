//! Hand-built modules for the runner's tests.
//!
//! The emitter lives in another crate that this one does not depend on, so the
//! tests write their own v1 modules with the encoder. A module here exports
//! the six accessors the skeleton's modules export, and the entry's body is
//! the test's choice.

#![allow(dead_code)]

use wasm_encoder::{
    CodeSection, ConstExpr, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};

pub(crate) const GLOBAL_STATUS: u32 = 0;
pub(crate) const GLOBAL_TRAP_CODE: u32 = 1;
pub(crate) const GLOBAL_ORIGIN: u32 = 2;
pub(crate) const GLOBAL_RESULT: u32 = 3;
pub(crate) const GLOBAL_FAILURE: u32 = 4;
pub(crate) const GLOBAL_EXPECTED: u32 = 5;
pub(crate) const GLOBAL_ACTUAL: u32 = 6;

/// The knobs of a test module.
pub(crate) struct Spec<'a> {
    /// The body of `vibra_v1_entry`.
    pub(crate) entry: Vec<Instruction<'a>>,
    /// The initial memory in pages.
    pub(crate) pages: u64,
    /// Whether to export the three failure accessors.
    pub(crate) failure_exports: bool,
    /// Native imports to declare, each as `() -> ()`, before any function.
    pub(crate) imports: Vec<(&'a str, &'a str)>,
}

impl Default for Spec<'_> {
    fn default() -> Self {
        Self {
            entry: Vec::new(),
            pages: 1,
            failure_exports: false,
            imports: Vec::new(),
        }
    }
}

/// Instructions that record `status`, then `trap_code` and `origin`.
pub(crate) fn record(
    status: i32,
    trap_code: i32,
    origin: i32,
) -> Vec<Instruction<'static>> {
    vec![
        Instruction::I32Const(status),
        Instruction::GlobalSet(GLOBAL_STATUS),
        Instruction::I32Const(trap_code),
        Instruction::GlobalSet(GLOBAL_TRAP_CODE),
        Instruction::I32Const(origin),
        Instruction::GlobalSet(GLOBAL_ORIGIN),
    ]
}

pub(crate) fn module(spec: &Spec<'_>) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().function([], [ValType::I32]);
    types.ty().function([], [ValType::I64]);

    let mut imports = ImportSection::new();
    for (module, name) in &spec.imports {
        imports.import(module, name, EntityType::Function(0));
    }
    let base = u32::try_from(spec.imports.len()).expect("a few imports");

    let mut functions = FunctionSection::new();
    functions.function(0); // entry
    for _ in 0..3 {
        functions.function(1); // status, trap code, origin
    }
    for _ in 0..2 {
        functions.function(2); // result, live size
    }
    if spec.failure_exports {
        functions.function(1); // failure
        functions.function(2); // expected
        functions.function(2); // actual
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: spec.pages,
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
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    for _ in 0..2 {
        globals.global(
            GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i64_const(0),
        );
    }

    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    exports.export("vibra_v1_entry", ExportKind::Func, base);
    exports.export("vibra_v1_status", ExportKind::Func, base + 1);
    exports.export("vibra_v1_trap_code", ExportKind::Func, base + 2);
    exports.export("vibra_v1_origin", ExportKind::Func, base + 3);
    exports.export("vibra_v1_result", ExportKind::Func, base + 4);
    exports.export("vibra_v1_live_size", ExportKind::Func, base + 5);
    if spec.failure_exports {
        exports.export("vibra_v1_failure", ExportKind::Func, base + 6);
        exports.export("vibra_v1_failure_expected", ExportKind::Func, base + 7);
        exports.export("vibra_v1_failure_actual", ExportKind::Func, base + 8);
    }

    let mut code = CodeSection::new();
    let mut entry = Function::new([]);
    for instruction in &spec.entry {
        entry.instruction(instruction);
    }
    entry.instruction(&Instruction::End);
    code.function(&entry);
    for global in [
        GLOBAL_STATUS,
        GLOBAL_TRAP_CODE,
        GLOBAL_ORIGIN,
        GLOBAL_RESULT,
    ] {
        let mut accessor = Function::new([]);
        accessor.instruction(&Instruction::GlobalGet(global));
        accessor.instruction(&Instruction::End);
        code.function(&accessor);
    }
    let mut live_size = Function::new([]);
    live_size.instruction(&Instruction::I64Const(0));
    live_size.instruction(&Instruction::End);
    code.function(&live_size);
    if spec.failure_exports {
        for global in [GLOBAL_FAILURE, GLOBAL_EXPECTED, GLOBAL_ACTUAL] {
            let mut accessor = Function::new([]);
            accessor.instruction(&Instruction::GlobalGet(global));
            accessor.instruction(&Instruction::End);
            code.function(&accessor);
        }
    }

    let mut module = Module::new();
    module.section(&types);
    if !spec.imports.is_empty() {
        module.section(&imports);
    }
    module.section(&functions);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&code);
    module.finish()
}
