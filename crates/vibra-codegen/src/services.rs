//! What a function translation shares with its neighbours: the object module,
//! the imported routines, and the operand lists in read-only data.

use std::collections::BTreeMap;

use cranelift_codegen::ir::{AbiParam, Signature, Type, types};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use vibra_nexus_bridge::{ROUTINES, Routine};

use crate::error::EmitError;

pub(crate) fn backend(error: impl std::fmt::Display) -> EmitError {
    EmitError::Backend(error.to_string())
}

/// The C type of one prototype entry, as a Cranelift type.
fn c_type(text: &str, pointer: Type) -> Option<Type> {
    if text.contains('*') {
        return Some(pointer);
    }
    match text.split_whitespace().next()? {
        "uint32_t" => Some(types::I32),
        "uint64_t" => Some(types::I64),
        _ => None,
    }
}

/// The Cranelift signature of a routine, read from its C prototype in the
/// bridge's `ROUTINES` table, which is therefore the only description of the
/// boundary.
fn signature(
    module: &ObjectModule,
    routine: &Routine,
) -> Result<(Signature, usize), EmitError> {
    let pointer = module.isa().pointer_type();
    let contract =
        |what: &str| EmitError::Contract(format!("`{}`: {what}", routine.signature));
    let (head, rest) = routine
        .signature
        .split_once('(')
        .ok_or_else(|| contract("no parameter list"))?;
    let parameters = rest
        .strip_suffix(')')
        .ok_or_else(|| contract("no closing parenthesis"))?;
    let returned = head
        .split_whitespace()
        .next()
        .and_then(|text| c_type(text, pointer))
        .ok_or_else(|| contract("an unknown return type"))?;
    let mut signature = module.make_signature();
    let mut arity = 0;
    for parameter in parameters.split(',') {
        let ty = c_type(parameter, pointer)
            .ok_or_else(|| contract("an unknown parameter type"))?;
        signature.params.push(AbiParam::new(ty));
        arity += 1;
    }
    signature.returns.push(AbiParam::new(returned));
    Ok((signature, arity))
}

/// The routines of `ROUTINES`, imported on first use so that the object's
/// undefined symbols are exactly the routines the program needs, in the order
/// the translation first needs them.
#[derive(Debug, Default)]
pub(crate) struct Imports {
    declared: BTreeMap<&'static str, (FuncId, usize)>,
}

impl Imports {
    /// The id and the parameter count of routine `name`, declaring it first
    /// if this is its first use.
    pub(crate) fn get(
        &mut self,
        module: &mut ObjectModule,
        name: &'static str,
    ) -> Result<(FuncId, usize), EmitError> {
        if let Some(known) = self.declared.get(name) {
            return Ok(*known);
        }
        let routine = ROUTINES
            .iter()
            .find(|routine| routine.name == name)
            .ok_or_else(|| {
                EmitError::Contract(format!("the bridge exports no routine `{name}`"))
            })?;
        let (signature, arity) = signature(module, routine)?;
        let id = module
            .declare_function(name, Linkage::Import, &signature)
            .map_err(backend)?;
        self.declared.insert(name, (id, arity));
        Ok((id, arity))
    }
}

/// The operand lists of a program: `u32` slot indices in read-only data, one
/// object for each distinct list, named by the order of first use. A list is
/// never a pointer into the stack, so the bytes of the object do not depend on
/// where the step function runs.
#[derive(Debug, Default)]
pub(crate) struct Lists {
    declared: BTreeMap<Vec<u32>, DataId>,
}

impl Lists {
    /// The data object holding `values`, defining it first if it is new.
    pub(crate) fn get(
        &mut self,
        module: &mut ObjectModule,
        values: &[u32],
    ) -> Result<DataId, EmitError> {
        if let Some(known) = self.declared.get(values) {
            return Ok(*known);
        }
        let name = format!("vibra_list_{}", self.declared.len());
        let id = module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(backend)?;
        let mut description = DataDescription::new();
        let bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<u8>>();
        description.define(bytes.into_boxed_slice());
        description.set_align(4);
        module.define_data(id, &description).map_err(backend)?;
        self.declared.insert(values.to_vec(), id);
        Ok(id)
    }
}

/// The state shared by the translation of every function.
pub(crate) struct Services<'m> {
    pub(crate) module: &'m mut ObjectModule,
    pub(crate) imports: Imports,
    pub(crate) lists: Lists,
}
