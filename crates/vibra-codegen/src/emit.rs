//! Emission: the whole object, or an [`EmitError`].

use std::collections::{BTreeMap, BTreeSet};

use cranelift_codegen::ir::{AbiParam, types};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use vibra_exec::{Inst, Program, Terminator, Verified};

use crate::descriptor::{Inputs, define_program};
use crate::error::EmitError;
use crate::function::{Callees, translate};
use crate::services::{Imports, Lists, Services, backend};
use crate::target::Target;

/// The object file a program was emitted to: PE/COFF for Windows, ELF for
/// Linux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    bytes: Vec<u8>,
}

impl Object {
    /// The bytes of the file.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The bytes of the file, by value.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// The Cranelift IR of each function as text, and the routine each imported
/// declaration is. For tests of the shape of the generated code only.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// `Function::display()` of each step function, by position.
    pub functions: Vec<String>,
    /// The routine each external declaration (`u0:<id>`) calls.
    pub routines: BTreeMap<u32, &'static str>,
}

/// Emits `program` for `target`.
///
/// The output is a function of `(program, target, inputs)` alone. The object
/// defines one global symbol, `vibra_program`, and one local function
/// `vibra_fn_<n>` for each function of the program; it imports only routines
/// of the bridge's `ROUTINES` table, and no generated function calls another.
///
/// # Errors
///
/// [`EmitError`] for a program using something the generator does not
/// implement, a target it does not know, or a size the descriptor cannot
/// state. No object is produced in that case.
pub fn emit(
    program: Verified<'_>,
    target: &Target,
    inputs: &Inputs,
) -> Result<Object, EmitError> {
    build(program, target, inputs, false).map(|(object, _)| object)
}

/// As [`emit`], and the Cranelift IR of every function as well.
///
/// # Errors
///
/// As [`emit`].
#[doc(hidden)]
pub fn emit_with_listing(
    program: Verified<'_>,
    target: &Target,
    inputs: &Inputs,
) -> Result<(Object, Listing), EmitError> {
    build(program, target, inputs, true).and_then(|(object, listing)| {
        listing
            .map(|listing| (object, listing))
            .ok_or_else(|| EmitError::Contract("no listing was kept".to_owned()))
    })
}

/// The registry symbols of the program's host calls, each once, in the order
/// of their first use over the functions and blocks.
fn host_symbols(program: &Program) -> Vec<&'static str> {
    let mut seen = BTreeSet::new();
    let mut symbols = Vec::new();
    for function in &program.functions {
        for block in &function.blocks {
            if let Terminator::Host { row, .. } = &block.term
                && seen.insert(row.symbol)
            {
                symbols.push(row.symbol);
            }
        }
    }
    symbols
}

/// Refuses, before anything is generated, a program that has an instruction
/// the generator does not implement: an object is never a part of a program.
fn refuse_unsupported(program: &Program) -> Result<(), EmitError> {
    let intrinsic = program
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .any(|inst| matches!(inst, Inst::Intrinsic { .. }));
    if intrinsic {
        return Err(EmitError::Unsupported("an intrinsic"));
    }
    Ok(())
}

fn build(
    verified: Verified<'_>,
    target: &Target,
    inputs: &Inputs,
    keep_listing: bool,
) -> Result<(Object, Option<Listing>), EmitError> {
    let program = verified.program();
    refuse_unsupported(program)?;
    let symbols = host_symbols(program);

    let isa = target.isa()?;
    let builder =
        ObjectBuilder::new(isa, "vibra", cranelift_module::default_libcall_names())
            .map_err(backend)?;
    let mut module = ObjectModule::new(builder);

    let pointer = module.isa().pointer_type();
    let mut step = module.make_signature();
    step.params.push(AbiParam::new(pointer));
    step.params.push(AbiParam::new(types::I32));
    step.returns.push(AbiParam::new(types::I32));

    let mut steps: Vec<FuncId> = Vec::with_capacity(program.functions.len());
    for position in 0..program.functions.len() {
        let id = module
            .declare_function(&format!("vibra_fn_{position}"), Linkage::Local, &step)
            .map_err(backend)?;
        steps.push(id);
    }

    let mut services = Services {
        module: &mut module,
        imports: Imports::default(),
        lists: Lists::default(),
    };
    let mut builder_context = FunctionBuilderContext::new();
    let mut callees = Callees::new();
    let mut texts = Vec::new();
    for (function, id) in program.functions.iter().zip(&steps) {
        let mut context = services.module.make_context();
        context.func.signature = step.clone();
        translate(
            &mut services,
            &symbols,
            function,
            &mut context.func,
            &mut builder_context,
            &mut callees,
        )?;
        if keep_listing {
            texts.push(context.func.display().to_string());
        }
        services
            .module
            .define_function(*id, &mut context)
            .map_err(backend)?;
        services.module.clear_context(&mut context);
    }
    drop(services);

    define_program(&mut module, program, *inputs, &steps, &symbols)?;

    let bytes = module.finish().emit().map_err(backend)?;
    let listing = keep_listing.then_some(Listing {
        functions: texts,
        routines: callees,
    });
    Ok((Object { bytes }, listing))
}
