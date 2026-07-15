//! Deterministic WebAssembly execution envelope for fully lowered Vibra programs.
//!
//! The module owns the stable guest boundary: it imports only
//! `vibra_v1.run_program`, exports `main`, and carries a deterministic program
//! fingerprint as a custom section. The v1 host function executes the already
//! lowered and statically linked program under the host-owned capability state.

use crate::lower::LoweredProgram;
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::cell::RefCell;
use wasm_encoder::{
    CodeSection, CustomSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, Module, TypeSection, ValType,
};
use wasmer::{imports, Function as HostFunction, Instance, Store};

pub const ABI_MODULE: &str = "vibra_v1";
pub const RUN_PROGRAM_IMPORT: &str = "run_program";

struct HostExecution {
    program: LoweredProgram,
    config: RunConfig,
    error: Option<String>,
}

thread_local! {
    static HOST_EXECUTION: RefCell<Option<HostExecution>> = const { RefCell::new(None) };
}

pub fn emit_program_wasm(program: &LoweredProgram) -> Vec<u8> {
    let fingerprint = deterministic_program_fingerprint(program);
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);

    let mut imports = ImportSection::new();
    imports.import(ABI_MODULE, RUN_PROGRAM_IMPORT, EntityType::Function(0));

    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, 1);
    let mut code = CodeSection::new();
    let mut main = Function::new([]);
    main.instruction(&Instruction::Call(0));
    main.instruction(&Instruction::End);
    code.function(&main);

    let metadata = CustomSection {
        name: Cow::Borrowed("vibra.program.v1"),
        data: Cow::Owned(fingerprint.into_bytes()),
    };
    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports)
        .section(&code)
        .section(&metadata);
    module.finish()
}

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let wasm = emit_program_wasm(program);
    let mut store = Store::default();
    let module = wasmer::Module::new(&store, &wasm).context("compile Vibra Wasm module")?;
    validate_imports(&module)?;
    HOST_EXECUTION.with(|slot| {
        slot.replace(Some(HostExecution {
            program: program.clone(),
            config: config.clone(),
            error: None,
        }))
    });
    let run_program = HostFunction::new_typed(&mut store, || -> i32 {
        HOST_EXECUTION.with(|slot| {
            let mut execution = slot.borrow_mut();
            let execution = execution
                .as_mut()
                .expect("vibra_v1 host execution must be installed");
            match crate::execute::run_lowered_interpreted(&execution.program, &execution.config) {
                Ok(()) => 0,
                Err(error) => {
                    execution.error = Some(format!("{error:#}"));
                    1
                }
            }
        })
    });
    let imports = imports! {
        ABI_MODULE => {
            RUN_PROGRAM_IMPORT => run_program,
        }
    };
    let instance =
        Instance::new(&mut store, &module, &imports).context("instantiate Vibra Wasm")?;
    let main = instance
        .exports
        .get_typed_function::<(), i32>(&store, "main")
        .context("Vibra Wasm must export main")?;
    let status = main.call(&mut store).context("execute Vibra Wasm main")?;
    if status != 0 {
        let message = HOST_EXECUTION.with(|slot| {
            slot.borrow_mut()
                .take()
                .and_then(|execution| execution.error)
                .unwrap_or_else(|| format!("vibra_v1.run_program failed with status {status}"))
        });
        bail!(message);
    }
    HOST_EXECUTION.with(|slot| slot.replace(None));
    Ok(())
}

fn validate_imports(module: &wasmer::Module) -> Result<()> {
    for import in module.imports() {
        if import.module() != ABI_MODULE || import.name() != RUN_PROGRAM_IMPORT {
            bail!(
                "unsupported Wasm import `{}.{}`; expected `{}.{}`",
                import.module(),
                import.name(),
                ABI_MODULE,
                RUN_PROGRAM_IMPORT
            );
        }
    }
    Ok(())
}

fn deterministic_program_fingerprint(program: &LoweredProgram) -> String {
    let mut canonical = format!(
        "abi=vibra-v1\nstatements={:?}\nmain-args={:?}\ngrants={:?}\nwarnings={:?}\n",
        program.statements, program.main_arg_bindings, program.main_grant_decls, program.warnings
    );
    let mut constant_names: Vec<_> = program.constants.keys().collect();
    constant_names.sort();
    for name in constant_names {
        canonical.push_str(&format!("constant:{name}={:?}\n", program.constants[name]));
    }
    let mut function_names: Vec<_> = program.functions.keys().collect();
    function_names.sort();
    for name in function_names {
        canonical.push_str(&format!("function:{name}={:?}\n", program.functions[name]));
    }
    let mut implementation_keys: Vec<_> = program.impls.keys().collect();
    implementation_keys.sort_by(|a, b| {
        (&a.implementing_type, &a.interface).cmp(&(&b.implementing_type, &b.interface))
    });
    for key in implementation_keys {
        let implementation = &program.impls[key];
        canonical.push_str(&format!(
            "impl:{}::{}:args={:?}:params={:?}\n",
            key.implementing_type,
            key.interface,
            implementation.interface_args,
            implementation.impl_type_params
        ));
        let mut method_names: Vec<_> = implementation.methods.keys().collect();
        method_names.sort();
        for method in method_names {
            canonical.push_str(&format!(
                "method:{method}={:?}\n",
                implementation.methods[method]
            ));
        }
    }
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_program() -> LoweredProgram {
        LoweredProgram {
            statements: vec![],
            main_arg_bindings: vec![],
            main_grant_decls: vec![],
            constants: HashMap::new(),
            functions: HashMap::new(),
            impls: HashMap::new(),
            warnings: vec![],
        }
    }

    #[test]
    fn emitted_program_is_byte_deterministic() {
        let program = empty_program();
        assert_eq!(emit_program_wasm(&program), emit_program_wasm(&program));
    }

    #[test]
    fn emitted_program_fingerprint_covers_lowered_contents() {
        let first = empty_program();
        let mut second = empty_program();
        second
            .statements
            .push(crate::lower::Statement::Eval(crate::lower::Expr::Value(
                crate::lower::RuntimeValue::Int(1),
            )));
        assert_ne!(emit_program_wasm(&first), emit_program_wasm(&second));
    }

    #[test]
    fn emitted_program_executes_through_vibra_v1() {
        run_lowered(&empty_program(), &RunConfig::default()).unwrap();
    }

    #[test]
    fn emitted_program_has_only_the_versioned_host_import() {
        let store = Store::default();
        let module = wasmer::Module::new(&store, emit_program_wasm(&empty_program())).unwrap();
        let imports: Vec<_> = module
            .imports()
            .map(|import| (import.module().to_string(), import.name().to_string()))
            .collect();
        assert_eq!(
            imports,
            vec![(ABI_MODULE.to_string(), RUN_PROGRAM_IMPORT.to_string())]
        );
        validate_imports(&module).unwrap();
    }
}
