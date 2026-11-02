//! Validation under exactly the v1 feature baseline
//! (`docs/spec/06-runtime.md`, "WebAssembly boundary").

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod common;

use common::{Spec, module};
use vibra_wasm_run::{ValidationError, validate};
use wasm_encoder::{
    AbstractHeapType, CodeSection, ConstExpr, CustomSection, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, HeapType, Instruction,
    MemorySection, MemoryType, Module, RefType, TypeSection, ValType,
};

/// A conforming module whose entry body is `entry`, with extra sections
/// spliced in by the callers below through [`Module`] directly.
fn conforming() -> Vec<u8> {
    module(&Spec::default())
}

/// A module with one defined memory, optionally exported, and `body` as the
/// only function, exported as `vibra_v1_entry`.
fn small(
    memory: MemoryType,
    memory_export: Option<&str>,
    body: &[Instruction<'_>],
) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut memories = MemorySection::new();
    memories.memory(memory);
    let mut exports = ExportSection::new();
    if let Some(name) = memory_export {
        exports.export(name, ExportKind::Memory, 0);
    }
    exports.export("vibra_v1_entry", ExportKind::Func, 0);
    let mut code = CodeSection::new();
    let mut function = Function::new([]);
    for instruction in body {
        function.instruction(instruction);
    }
    function.instruction(&Instruction::End);
    code.function(&function);
    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&exports);
    module.section(&code);
    module.finish()
}

fn one_page() -> MemoryType {
    MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    }
}

fn rejected(bytes: &[u8]) -> ValidationError {
    validate(bytes).expect_err("the module is not a v1 module")
}

#[test]
fn a_conforming_module_validates() {
    validate(&conforming()).expect("a v1 module");
    validate(&small(one_page(), Some("vibra_v1_memory"), &[])).expect("a v1 module");
}

#[test]
fn the_named_baseline_features_validate() {
    // Sign-extension, non-trapping float-to-integer conversion, and bulk
    // memory operations are baseline.
    let body = [
        Instruction::I32Const(1),
        Instruction::I32Extend8S,
        Instruction::Drop,
        Instruction::F32Const(1.5_f32.into()),
        Instruction::I32TruncSatF32S,
        Instruction::Drop,
        Instruction::I32Const(0),
        Instruction::I32Const(0),
        Instruction::I32Const(0),
        Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        },
    ];
    validate(&small(one_page(), Some("vibra_v1_memory"), &body)).expect("baseline");
}

#[test]
fn multiple_results_and_function_tables_validate() {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    types.ty().function([], [ValType::I32, ValType::I32]);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(1);
    let mut tables = wasm_encoder::TableSection::new();
    tables.table(wasm_encoder::TableType {
        element_type: RefType::FUNCREF,
        minimum: 2,
        maximum: Some(2),
        table64: false,
        shared: false,
    });
    let mut memories = MemorySection::new();
    memories.memory(one_page());
    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    exports.export("vibra_v1_entry", ExportKind::Func, 0);
    let mut code = CodeSection::new();
    let mut entry = Function::new([]);
    entry.instruction(&Instruction::I32Const(0));
    entry.instruction(&Instruction::CallIndirect {
        type_index: 1,
        table_index: 0,
    });
    entry.instruction(&Instruction::Drop);
    entry.instruction(&Instruction::Drop);
    entry.instruction(&Instruction::End);
    code.function(&entry);
    let mut pair = Function::new([]);
    pair.instruction(&Instruction::I32Const(1));
    pair.instruction(&Instruction::I32Const(2));
    pair.instruction(&Instruction::End);
    code.function(&pair);
    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&tables);
    module.section(&memories);
    module.section(&exports);
    module.section(&code);
    validate(&module.finish()).expect("multi-value and a function table");
}

#[test]
fn a_foreign_import_module_is_rejected() {
    let bytes = module(&Spec {
        imports: vec![("env", "print")],
        ..Spec::default()
    });
    assert_eq!(
        rejected(&bytes),
        ValidationError::ForeignImport {
            module: "env".to_owned(),
            name: "print".to_owned()
        }
    );
}

#[test]
fn a_native_import_validates() {
    let bytes = module(&Spec {
        imports: vec![("vibra_native_v1", "anything")],
        ..Spec::default()
    });
    validate(&bytes).expect("the pure native module is the permitted import module");
}

#[test]
fn a_custom_section_is_rejected() {
    let mut bytes = conforming();
    let section = CustomSection {
        name: "name".into(),
        data: [0_u8; 0].as_slice().into(),
    };
    let mut tail = Module::new();
    tail.section(&section);
    // A custom section may appear anywhere: append one after the last section.
    bytes.extend_from_slice(&tail.finish()[8..]);
    assert_eq!(
        rejected(&bytes),
        ValidationError::CustomSection("name".to_owned())
    );
}

#[test]
fn a_memory_exported_under_another_name_is_rejected() {
    let bytes = small(one_page(), Some("memory"), &[]);
    assert_eq!(
        rejected(&bytes),
        ValidationError::ForeignExport {
            name: "memory".to_owned()
        }
    );
}

#[test]
fn a_memory_that_is_not_exported_is_rejected() {
    assert!(matches!(
        rejected(&small(one_page(), None, &[])),
        ValidationError::Memory(_)
    ));
}

#[test]
fn an_export_outside_the_versioned_table_is_rejected() {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut memories = MemorySection::new();
    memories.memory(one_page());
    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    exports.export("vibra_v1_other", ExportKind::Func, 0);
    let mut code = CodeSection::new();
    let mut body = Function::new([]);
    body.instruction(&Instruction::End);
    code.function(&body);
    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&exports);
    module.section(&code);
    assert_eq!(
        rejected(&module.finish()),
        ValidationError::ForeignExport {
            name: "vibra_v1_other".to_owned()
        }
    );
}

#[test]
fn the_full_export_table_validates_and_so_does_a_subset() {
    // Today's modules export a subset of the table; by the end of Stage 4A they
    // export all of it. Validation admits both and nothing outside it.
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let names = vibra_ir::boundary::FUNCTION_EXPORTS;
    let mut functions = FunctionSection::new();
    let mut code = CodeSection::new();
    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    for (index, name) in names.iter().enumerate() {
        functions.function(0);
        let mut body = Function::new([]);
        body.instruction(&Instruction::End);
        code.function(&body);
        exports.export(name, ExportKind::Func, u32::try_from(index).unwrap());
    }
    let mut memories = MemorySection::new();
    memories.memory(one_page());
    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&memories);
    module.section(&exports);
    module.section(&code);
    let summary = validate(&module.finish()).expect("the full table validates");
    assert_eq!(summary.exports.len(), names.len() + 1);
}

#[test]
fn an_exported_global_is_rejected() {
    let mut memories = MemorySection::new();
    memories.memory(one_page());
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: false,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    exports.export("vibra_v1_status", ExportKind::Global, 0);
    let mut module = Module::new();
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    assert_eq!(
        rejected(&module.finish()),
        ValidationError::ForeignExport {
            name: "vibra_v1_status".to_owned()
        }
    );
}

#[test]
fn a_second_memory_is_rejected() {
    let mut memories = MemorySection::new();
    memories.memory(one_page());
    memories.memory(one_page());
    let mut exports = ExportSection::new();
    exports.export("vibra_v1_memory", ExportKind::Memory, 0);
    let mut module = Module::new();
    module.section(&memories);
    module.section(&exports);
    assert!(matches!(
        rejected(&module.finish()),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn a_module_with_no_memory_is_rejected() {
    let mut module = Module::new();
    module.section(&ExportSection::new());
    assert!(matches!(
        rejected(&module.finish()),
        ValidationError::Memory(_)
    ));
}

#[test]
fn tail_calls_are_rejected() {
    let body = [Instruction::ReturnCall(0)];
    assert!(matches!(
        rejected(&small(one_page(), Some("vibra_v1_memory"), &body)),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn simd_is_rejected() {
    let body = [Instruction::V128Const(0), Instruction::Drop];
    assert!(matches!(
        rejected(&small(one_page(), Some("vibra_v1_memory"), &body)),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn threads_are_rejected() {
    let shared = MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: true,
        page_size_log2: None,
    };
    assert!(matches!(
        rejected(&small(shared, Some("vibra_v1_memory"), &[])),
        ValidationError::Invalid(_)
    ));
    let body = [
        Instruction::I32Const(0),
        Instruction::I32AtomicLoad(wasm_encoder::MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }),
        Instruction::Drop,
    ];
    assert!(matches!(
        rejected(&small(one_page(), Some("vibra_v1_memory"), &body)),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn reference_types_are_rejected() {
    let body = [
        Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Extern,
        }),
        Instruction::Drop,
    ];
    assert!(matches!(
        rejected(&small(one_page(), Some("vibra_v1_memory"), &body)),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn memory64_is_rejected() {
    let wide = MemoryType {
        minimum: 1,
        maximum: None,
        memory64: true,
        shared: false,
        page_size_log2: None,
    };
    assert!(matches!(
        rejected(&small(wide, Some("vibra_v1_memory"), &[])),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn garbage_collection_types_are_rejected() {
    let mut types = TypeSection::new();
    types.ty().struct_(vec![wasm_encoder::FieldType {
        element_type: wasm_encoder::StorageType::Val(ValType::I32),
        mutable: false,
    }]);
    let mut module = Module::new();
    module.section(&types);
    assert!(matches!(
        rejected(&module.finish()),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn exceptions_are_rejected() {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut tags = wasm_encoder::TagSection::new();
    tags.tag(wasm_encoder::TagType {
        kind: wasm_encoder::TagKind::Exception,
        func_type_idx: 0,
    });
    let mut module = Module::new();
    module.section(&types);
    module.section(&tags);
    assert!(matches!(
        rejected(&module.finish()),
        ValidationError::Invalid(_)
    ));
}

#[test]
fn a_truncated_module_is_invalid() {
    let bytes = conforming();
    assert!(matches!(
        rejected(&bytes[..bytes.len() - 3]),
        ValidationError::Invalid(_)
    ));
    assert!(matches!(rejected(b"not wasm"), ValidationError::Invalid(_)));
}
