//! The `vibra_program` descriptor and the read-only data it points to.
//!
//! The layout is the bridge's `#[repr(C)]` types: every size and offset below
//! is `size_of` or `offset_of` of those types, never a number typed here, and
//! every pointer field is a relocation. Strings and byte constants are data
//! objects of their own; empty tables are a null pointer and a zero length.

use std::collections::BTreeMap;
use std::mem::{offset_of, size_of};

use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use vibra_exec::{Constant, Program};
use vibra_nexus_bridge::{
    ABI_VERSION, CONSTANT_ATOM, CONSTANT_BYTES, CONSTANT_STR, MAX_DATA_LEN,
    MAX_TABLE_LEN, PROGRAM_SYMBOL, VibraConstant, VibraFunction, VibraOrigin,
    VibraProgram, VibraTable,
};

use crate::error::EmitError;
use crate::services::backend;

/// What the caller computes from the checked program, because the execution IR
/// alone does not determine it. The build driver (a later package) derives
/// both from the checked program; the generator writes them verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// What the launcher may conclude from the entry's result: `0` when the
    /// entry's result is not a `result`, and `n > 0` when it is one whose `err`
    /// variant has index `n - 1` (`VibraProgram::entry_outcome`).
    pub entry_outcome: u32,
    /// The registry minor the program needs (`Program::registry_minor`, or
    /// more): `VibraProgram::registry_minor`.
    pub registry_minor: u32,
}

/// A relocation target inside a blob.
#[derive(Clone, Copy)]
enum Reloc {
    Function(FuncId),
    Data(DataId),
}

/// The bytes of one data object and the pointers inside them.
struct Blob {
    bytes: Vec<u8>,
    relocs: Vec<(usize, Reloc)>,
    align: u64,
}

impl Blob {
    fn zeroed(len: usize, align: u64) -> Self {
        Self {
            bytes: vec![0; len],
            relocs: Vec::new(),
            align,
        }
    }

    fn put(&mut self, at: usize, value: &[u8]) -> Result<(), EmitError> {
        let end = at.checked_add(value.len());
        let slot = end.and_then(|end| self.bytes.get_mut(at..end));
        slot.ok_or_else(|| {
            EmitError::Contract("a descriptor field lies outside its table".to_owned())
        })?
        .copy_from_slice(value);
        Ok(())
    }

    fn u32_at(&mut self, at: usize, value: u32) -> Result<(), EmitError> {
        self.put(at, &value.to_le_bytes())
    }

    fn u64_at(&mut self, at: usize, value: u64) -> Result<(), EmitError> {
        self.put(at, &value.to_le_bytes())
    }

    /// A `VibraTable` at `at`: the address of `target` and `len`.
    fn table(
        &mut self,
        at: usize,
        target: Option<DataId>,
        len: usize,
    ) -> Result<(), EmitError> {
        let len = u64::try_from(len).map_err(|_| EmitError::TooLarge("a table"))?;
        self.u64_at(at + offset_of!(VibraTable<u8>, len), len)?;
        if let Some(target) = target {
            self.relocs
                .push((at + offset_of!(VibraTable<u8>, ptr), Reloc::Data(target)));
        }
        Ok(())
    }
}

fn define(
    module: &mut ObjectModule,
    name: &str,
    linkage: Linkage,
    blob: Blob,
) -> Result<DataId, EmitError> {
    let id = module
        .declare_data(name, linkage, false, false)
        .map_err(backend)?;
    let mut description = DataDescription::new();
    description.define(blob.bytes.into_boxed_slice());
    description.set_align(blob.align);
    for (offset, target) in blob.relocs {
        let offset =
            u32::try_from(offset).map_err(|_| EmitError::TooLarge("a table"))?;
        match target {
            Reloc::Function(function) => {
                let reference = module.declare_func_in_data(function, &mut description);
                description.write_function_addr(offset, reference);
            }
            Reloc::Data(data) => {
                let global = module.declare_data_in_data(data, &mut description);
                description.write_data_addr(offset, global, 0);
            }
        }
    }
    module.define_data(id, &description).map_err(backend)?;
    Ok(id)
}

/// A UTF-8 or byte string as a data object; `None` for the empty one.
fn bytes(
    module: &mut ObjectModule,
    name: &str,
    content: &[u8],
) -> Result<Option<DataId>, EmitError> {
    if content.is_empty() {
        return Ok(None);
    }
    if u64::try_from(content.len()).map_or(true, |len| len > MAX_DATA_LEN) {
        return Err(EmitError::TooLarge("a string or byte constant"));
    }
    let blob = Blob {
        bytes: content.to_vec(),
        relocs: Vec::new(),
        align: 1,
    };
    define(module, name, Linkage::Local, blob).map(Some)
}

fn limit(len: usize, what: &'static str) -> Result<(), EmitError> {
    if u64::try_from(len).map_or(true, |len| len > MAX_TABLE_LEN) {
        return Err(EmitError::TooLarge(what));
    }
    Ok(())
}

fn index(position: usize) -> Result<u32, EmitError> {
    u32::try_from(position).map_err(|_| EmitError::TooLarge("a table"))
}

/// Defines the descriptor and everything it points to. `steps` is the step
/// function of each execution-IR function, by position, and `symbols` the
/// required host symbols in first-use order.
pub(crate) fn define_program(
    module: &mut ObjectModule,
    program: &Program,
    inputs: Inputs,
    steps: &[FuncId],
    symbols: &[&'static str],
) -> Result<(), EmitError> {
    limit(program.functions.len(), "the function table")?;
    limit(program.globals.len(), "the module value table")?;
    limit(program.constants.len(), "the constant table")?;
    limit(symbols.len(), "the symbol table")?;
    limit(program.origins.len(), "the origin table")?;

    let functions = functions(module, program, steps)?;
    let globals = globals(module, program)?;
    let constants = constants(module, program)?;
    let symbol_table = symbol_table(module, symbols)?;
    let origins = origins(module, program)?;

    let mut descriptor = Blob::zeroed(size_of::<VibraProgram>(), 8);
    descriptor.u32_at(offset_of!(VibraProgram, abi), ABI_VERSION)?;
    descriptor.u32_at(
        offset_of!(VibraProgram, registry_minor),
        inputs.registry_minor,
    )?;
    descriptor.u32_at(offset_of!(VibraProgram, entry), program.entry.0)?;
    descriptor.u32_at(
        offset_of!(VibraProgram, entry_outcome),
        inputs.entry_outcome,
    )?;
    descriptor.table(
        offset_of!(VibraProgram, functions),
        functions,
        program.functions.len(),
    )?;
    descriptor.table(
        offset_of!(VibraProgram, globals),
        globals,
        program.globals.len(),
    )?;
    descriptor.table(
        offset_of!(VibraProgram, constants),
        constants,
        program.constants.len(),
    )?;
    descriptor.table(
        offset_of!(VibraProgram, symbols),
        symbol_table,
        symbols.len(),
    )?;
    descriptor.table(
        offset_of!(VibraProgram, origins),
        origins,
        program.origins.len(),
    )?;
    define(module, PROGRAM_SYMBOL, Linkage::Export, descriptor)?;
    Ok(())
}

fn functions(
    module: &mut ObjectModule,
    program: &Program,
    steps: &[FuncId],
) -> Result<Option<DataId>, EmitError> {
    if program.functions.is_empty() {
        return Ok(None);
    }
    let row = size_of::<VibraFunction>();
    let mut blob = Blob::zeroed(row * program.functions.len(), 8);
    for (position, (function, step)) in program.functions.iter().zip(steps).enumerate()
    {
        let at = row * position;
        blob.relocs
            .push((at + offset_of!(VibraFunction, step), Reloc::Function(*step)));
        blob.u32_at(at + offset_of!(VibraFunction, slots), function.slots)?;
        blob.u32_at(at + offset_of!(VibraFunction, params), function.params)?;
        blob.u32_at(at + offset_of!(VibraFunction, env), u32::from(function.env))?;
        blob.u32_at(
            at + offset_of!(VibraFunction, blocks),
            index(function.blocks.len())?,
        )?;
    }
    define(module, "vibra_functions", Linkage::Local, blob).map(Some)
}

fn globals(
    module: &mut ObjectModule,
    program: &Program,
) -> Result<Option<DataId>, EmitError> {
    if program.globals.is_empty() {
        return Ok(None);
    }
    let mut blob = Blob::zeroed(size_of::<u32>() * program.globals.len(), 4);
    for (position, initializer) in program.globals.iter().enumerate() {
        blob.u32_at(size_of::<u32>() * position, initializer.0)?;
    }
    define(module, "vibra_globals", Linkage::Local, blob).map(Some)
}

fn constants(
    module: &mut ObjectModule,
    program: &Program,
) -> Result<Option<DataId>, EmitError> {
    if program.constants.is_empty() {
        return Ok(None);
    }
    let row = size_of::<VibraConstant>();
    let mut blob = Blob::zeroed(row * program.constants.len(), 8);
    for (position, constant) in program.constants.iter().enumerate() {
        let at = row * position;
        let (kind, content) = match constant {
            Constant::Str(text) => (CONSTANT_STR, text.as_bytes()),
            Constant::Bytes(content) => (CONSTANT_BYTES, content.as_slice()),
            Constant::Atom(text) => (CONSTANT_ATOM, text.as_bytes()),
        };
        let data = bytes(module, &format!("vibra_constant_{position}"), content)?;
        blob.u32_at(at + offset_of!(VibraConstant, kind), kind)?;
        blob.table(at + offset_of!(VibraConstant, data), data, content.len())?;
    }
    define(module, "vibra_constants", Linkage::Local, blob).map(Some)
}

fn symbol_table(
    module: &mut ObjectModule,
    symbols: &[&'static str],
) -> Result<Option<DataId>, EmitError> {
    if symbols.is_empty() {
        return Ok(None);
    }
    let row = size_of::<VibraTable<u8>>();
    let mut blob = Blob::zeroed(row * symbols.len(), 8);
    for (position, symbol) in symbols.iter().enumerate() {
        let data = bytes(
            module,
            &format!("vibra_symbol_{position}"),
            symbol.as_bytes(),
        )?;
        blob.table(row * position, data, symbol.len())?;
    }
    define(module, "vibra_symbols", Linkage::Local, blob).map(Some)
}

fn origins(
    module: &mut ObjectModule,
    program: &Program,
) -> Result<Option<DataId>, EmitError> {
    if program.origins.is_empty() {
        return Ok(None);
    }
    let row = size_of::<VibraOrigin>();
    let mut blob = Blob::zeroed(row * program.origins.len(), 8);
    // One data object for each distinct source, named in order of first use.
    let mut sources: BTreeMap<&str, Option<DataId>> = BTreeMap::new();
    for (position, origin) in program.origins.iter().enumerate() {
        let at = row * position;
        let source = origin.source_id();
        let data = match sources.get(source) {
            Some(known) => *known,
            None => {
                let data = bytes(
                    module,
                    &format!("vibra_source_{}", sources.len()),
                    source.as_bytes(),
                )?;
                sources.insert(source, data);
                data
            }
        };
        let span = origin.span();
        let too_large = || EmitError::TooLarge("a source offset");
        blob.table(at + offset_of!(VibraOrigin, source), data, source.len())?;
        blob.u32_at(
            at + offset_of!(VibraOrigin, start),
            u32::try_from(span.start()).map_err(|_| too_large())?,
        )?;
        blob.u32_at(
            at + offset_of!(VibraOrigin, end),
            u32::try_from(span.end()).map_err(|_| too_large())?,
        )?;
    }
    define(module, "vibra_origins", Linkage::Local, blob).map(Some)
}
