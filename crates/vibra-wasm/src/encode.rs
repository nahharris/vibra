//! Writes the module bytes.
//!
//! Every section is written in the fixed order of the binary format, from
//! tables that are built in program order, so the bytes depend on nothing but
//! the checked program: no clock, path, address, or hash-table order reaches
//! them. No custom section is written, not even a `name` section, because a v1
//! module the toolchain emits before `vibra build` exists carries none.
//!
//! The function index space is the lowered functions in source order, then the
//! runtime routines of [`crate::runtime`], the entry export, and last the object
//! constructor when the program builds an object.

use vibra_ir::boundary::{ENTRY_EXPORT, MEMORY_EXPORT};
use wasm_encoder::{
    CodeSection, DataCountSection, DataSection, ExportKind, ExportSection, Function,
    FunctionSection, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use crate::lower::FunctionCode;
use crate::runtime::{
    Routine, Routines, accessor_exports, core_routines, entry_export, new_object,
};

/// The pages of linear memory the module defines. The arena grows from here by
/// `memory.grow`.
const INITIAL_PAGES: u64 = 1;

/// The function signatures of a module, numbered in the order they are first
/// used, so the type section depends only on the program.
#[derive(Debug, Default)]
struct Types {
    list: Vec<(Vec<ValType>, Vec<ValType>)>,
}

impl Types {
    fn intern(&mut self, params: &[ValType], results: &[ValType]) -> u32 {
        let position = self
            .list
            .iter()
            .position(|(known_params, known_results)| {
                known_params == params && known_results == results
            })
            .unwrap_or_else(|| {
                self.list.push((params.to_vec(), results.to_vec()));
                self.list.len() - 1
            });
        u32::try_from(position).unwrap_or(u32::MAX)
    }
}

/// What a module is made of.
#[derive(Debug)]
pub(crate) struct Parts {
    pub(crate) functions: Vec<FunctionCode>,
    pub(crate) entry: u32,
    pub(crate) segments: Vec<Vec<u8>>,
    pub(crate) include_new: bool,
}

/// The routine indices of a module of `function_count` lowered functions.
pub(crate) const fn routines_for(function_count: u32) -> Routines {
    Routines::plan(function_count)
}

/// Encodes the module, or `None` when the entry is not one of its functions.
pub(crate) fn module(parts: &Parts) -> Option<Vec<u8>> {
    let function_count = u32::try_from(parts.functions.len()).ok()?;
    let fns = routines_for(function_count);
    let entry_class = parts
        .functions
        .get(usize::try_from(parts.entry).ok()?)?
        .class;

    let mut types = Types::default();
    let mut functions = FunctionSection::new();
    let mut code = CodeSection::new();
    for lowered in &parts.functions {
        let results = lowered.class.val_type();
        let results = results.as_slice();
        functions.function(types.intern(&[], results));
        let mut function =
            Function::new_with_locals_types(lowered.locals.iter().copied());
        for instruction in &lowered.body {
            function.instruction(instruction);
        }
        code.function(&function);
    }
    let mut routines = core_routines(&fns);
    routines.push(entry_export(&fns, parts.entry, entry_class));
    if parts.include_new {
        routines.push(new_object(&fns));
    }
    let mut exports = ExportSection::new();
    exports.export(MEMORY_EXPORT, ExportKind::Memory, 0);
    exports.export(ENTRY_EXPORT, ExportKind::Func, fns.entry_export);
    for (name, index) in accessor_exports(&fns) {
        exports.export(name, ExportKind::Func, index);
    }
    for routine in &routines {
        add_routine(&mut types, &mut functions, &mut code, routine);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: INITIAL_PAGES,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut type_section = TypeSection::new();
    for (params, results) in &types.list {
        type_section
            .ty()
            .function(params.iter().copied(), results.iter().copied());
    }

    let mut module = Module::new();
    module.section(&type_section);
    module.section(&functions);
    module.section(&memories);
    module.section(&exports);
    if !parts.segments.is_empty() {
        let count = u32::try_from(parts.segments.len()).ok()?;
        module.section(&DataCountSection { count });
    }
    module.section(&code);
    if !parts.segments.is_empty() {
        let mut data = DataSection::new();
        for segment in &parts.segments {
            data.passive(segment.iter().copied());
        }
        module.section(&data);
    }
    Some(module.finish())
}

fn add_routine(
    types: &mut Types,
    functions: &mut FunctionSection,
    code: &mut CodeSection,
    routine: &Routine,
) {
    functions.function(types.intern(&routine.params, &routine.results));
    // A routine's locals follow its parameters.
    let mut function = Function::new_with_locals_types(routine.locals.iter().copied());
    for instruction in &routine.body {
        function.instruction(instruction);
    }
    code.function(&function);
}
