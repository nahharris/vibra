//! Deterministic guest-side WebAssembly code generation for lowered Vibra.
//!
//! Wasm owns control flow, locals, user functions, evaluation order, mutation,
//! and pattern selection. Values use opaque i32 arena addresses; the versioned
//! host ABI constructs dynamic values and performs privileged stdlib calls.

use crate::lower::{
    Call, Expr, FunctionBody, LetValue, LoweredProgram, Pattern, RuntimeValue, Statement, TypeRef,
};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::rc::Rc;
use wasm_encoder::{
    BlockType, CodeSection, CustomSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, Instruction, Module, TypeSection, ValType,
};
use wasmer::{imports, Function as HostFunction, Instance, Store};

pub const ABI_MODULE: &str = "vibra_v1";
pub const RUN_PROGRAM_IMPORT: &str = "run_program";

const HOST_SEED: u32 = 0;
const HOST_CONST: u32 = 1;
const HOST_READ: u32 = 2;
const HOST_FRAME_BEGIN: u32 = 3;
const HOST_FRAME_PUSH: u32 = 4;
const HOST_CONSTRUCT: u32 = 5;
const HOST_CALL: u32 = 6;
const HOST_SET: u32 = 7;
const HOST_BOOL: u32 = 8;
const HOST_MATCH: u32 = 9;
const HOST_BINDING: u32 = 10;
const HOST_STATUS: u32 = 11;
const HOST_NO_MATCH: u32 = 12;
const HOST_ITER_LEN: u32 = 13;
const HOST_ITER_GET: u32 = 14;
const HOST_FUNCTIONS: u32 = 15;

const ABI_IMPORTS: &[&str] = &[
    "seed",
    "value_const",
    "value_read",
    "frame_begin",
    "frame_push",
    "value_construct",
    "host_call",
    "value_set",
    "value_bool",
    "pattern_match",
    "pattern_binding",
    "status",
    "no_match",
    "iter_len",
    "iter_get",
];
const WASI_IMPORTS: &[&str] = &[
    "args_get",
    "args_sizes_get",
    "environ_get",
    "environ_sizes_get",
    "clock_res_get",
    "clock_time_get",
    "fd_advise",
    "fd_allocate",
    "fd_close",
    "fd_datasync",
    "fd_fdstat_get",
    "fd_fdstat_set_flags",
    "fd_fdstat_set_rights",
    "fd_filestat_get",
    "fd_filestat_set_size",
    "fd_filestat_set_times",
    "fd_pread",
    "fd_prestat_get",
    "fd_prestat_dir_name",
    "fd_pwrite",
    "fd_read",
    "fd_readdir",
    "fd_renumber",
    "fd_seek",
    "fd_sync",
    "fd_tell",
    "fd_write",
    "path_create_directory",
    "path_filestat_get",
    "path_filestat_set_times",
    "path_link",
    "path_open",
    "path_readlink",
    "path_remove_directory",
    "path_rename",
    "path_symlink",
    "path_unlink_file",
    "poll_oneoff",
    "proc_exit",
    "proc_raise",
    "random_get",
    "sched_yield",
    "sock_accept",
    "sock_recv",
    "sock_send",
    "sock_shutdown",
];

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct WasmPlan {
    seeds: Vec<String>,
    expressions: Vec<Expr>,
    calls: Vec<Call>,
    patterns: Vec<Pattern>,
    main_arg_bindings: Vec<(String, TypeRef)>,
    host_functions: BTreeMap<String, crate::lower::FunctionSig>,
    impl_keys: Vec<crate::lower::ImplKey>,
    foreign_modules: BTreeMap<String, Vec<u8>>,
}

struct CompiledProgram {
    wasm: Vec<u8>,
}

pub fn emit_program_wasm(program: &LoweredProgram) -> Vec<u8> {
    compile_program(program).wasm
}

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    run_lowered_inner(program, config, None)
}

pub fn run_lowered_with_io(
    program: &LoweredProgram,
    config: &RunConfig,
    stdout: Box<dyn Write>,
    stderr: Box<dyn Write>,
) -> Result<()> {
    run_lowered_inner(program, config, Some((stdout, stderr)))
}

fn run_lowered_inner(
    program: &LoweredProgram,
    config: &RunConfig,
    io: Option<(Box<dyn Write>, Box<dyn Write>)>,
) -> Result<()> {
    let compiled = compile_program(program);
    run_wasm_inner(&compiled.wasm, config, io)
}

pub fn run_wasm(wasm: &[u8], config: &RunConfig) -> Result<()> {
    run_wasm_inner(wasm, config, None)
}

fn run_wasm_inner(
    wasm: &[u8],
    config: &RunConfig,
    io: Option<(Box<dyn Write>, Box<dyn Write>)>,
) -> Result<()> {
    let mut store = Store::default();
    let module = wasmer::Module::new(&store, wasm).context("compile Vibra Wasm module")?;
    validate_imports(&module)?;
    let plan_bytes = module
        .custom_sections("vibra.plan.v1")
        .next()
        .context("Vibra Wasm is missing `vibra.plan.v1`")?;
    let plan: WasmPlan = serde_json::from_slice(&plan_bytes).context("decode `vibra.plan.v1`")?;
    let state = HostExecution::new(config.clone(), plan, io)?;
    HOST_EXECUTION.with(|slot| slot.replace(Some(state)));

    let seed = HostFunction::new_typed(&mut store, |id: i32| host_value(|host| host.seed(id)));
    let value_const =
        HostFunction::new_typed(&mut store, |id: i32| host_value(|host| host.constant(id)));
    let value_read = HostFunction::new_typed(&mut store, |handle: i32| {
        host_value(|host| host.read(handle))
    });
    let frame_begin = HostFunction::new_typed(&mut store, || {
        host_unit(|host| {
            host.frames.push(Vec::new());
            Ok(())
        })
    });
    let frame_push = HostFunction::new_typed(&mut store, |handle: i32| {
        host_unit(|host| host.frame_push(handle))
    });
    let construct =
        HostFunction::new_typed(&mut store, |id: i32| host_value(|host| host.construct(id)));
    let host_call = HostFunction::new_typed(&mut store, |id: i32| host_value(|host| host.call(id)));
    let value_set = HostFunction::new_typed(&mut store, |target: i32, value: i32| {
        host_unit(|host| host.set(target, value))
    });
    let value_bool = HostFunction::new_typed(&mut store, |handle: i32| {
        host_value(|host| host.as_bool(handle))
    });
    let pattern_match = HostFunction::new_typed(&mut store, |id: i32, handle: i32| {
        host_value(|host| host.matches(id, handle))
    });
    let pattern_binding = HostFunction::new_typed(&mut store, |index: i32| {
        host_value(|host| host.binding(index))
    });
    let status = HostFunction::new_typed(&mut store, || -> i32 {
        HOST_EXECUTION.with(|slot| slot.borrow().as_ref().is_some_and(|h| h.error.is_some()) as i32)
    });
    let no_match = HostFunction::new_typed(&mut store, || {
        host_unit(|_| bail!("non-exhaustive $match reached runtime"))
    });
    let iter_len = HostFunction::new_typed(&mut store, |handle: i32| {
        host_value(|host| host.iter_len(handle))
    });
    let iter_get = HostFunction::new_typed(&mut store, |handle: i32, index: i32| {
        host_value(|host| host.iter_get(handle, index))
    });
    let imports = imports! { ABI_MODULE => {
        "seed" => seed, "value_const" => value_const, "value_read" => value_read,
        "frame_begin" => frame_begin, "frame_push" => frame_push, "value_construct" => construct,
        "host_call" => host_call, "value_set" => value_set, "value_bool" => value_bool,
        "pattern_match" => pattern_match, "pattern_binding" => pattern_binding,
        "status" => status, "no_match" => no_match,
        "iter_len" => iter_len, "iter_get" => iter_get,
    }};
    let instance =
        Instance::new(&mut store, &module, &imports).context("instantiate Vibra Wasm")?;
    let main = instance
        .exports
        .get_typed_function::<(), i32>(&store, "main")
        .context("Vibra Wasm must export main")?;
    let status = main.call(&mut store).context("execute Vibra Wasm main")?;
    let execution = HOST_EXECUTION
        .with(|slot| slot.borrow_mut().take())
        .context("missing Vibra host execution")?;
    if let Some(error) = execution.error {
        bail!(error);
    }
    if status != 0 {
        bail!("Vibra guest failed with status {status}");
    }
    Ok(())
}

fn host_value(operation: impl FnOnce(&mut HostExecution) -> Result<i32>) -> i32 {
    HOST_EXECUTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let host = slot.as_mut().expect("Vibra host execution installed");
        if host.error.is_some() {
            return 0;
        }
        match operation(host) {
            Ok(value) => value,
            Err(error) => {
                host.error = Some(format!("{error:#}"));
                0
            }
        }
    })
}

fn host_unit(operation: impl FnOnce(&mut HostExecution) -> Result<()>) {
    let _ = host_value(|host| operation(host).map(|()| 0));
}

struct HostExecution {
    program: LoweredProgram,
    config: RunConfig,
    plan: WasmPlan,
    files: crate::execute::FileTable,
    seed_env: HashMap<String, RuntimeValue>,
    arena: Vec<RuntimeValue>,
    frames: Vec<Vec<i32>>,
    bindings: Vec<RuntimeValue>,
    error: Option<String>,
}

thread_local! { static HOST_EXECUTION: RefCell<Option<HostExecution>> = const { RefCell::new(None) }; }

impl HostExecution {
    fn new(
        config: RunConfig,
        plan: WasmPlan,
        io: Option<(Box<dyn Write>, Box<dyn Write>)>,
    ) -> Result<Self> {
        let functions = plan.host_functions.clone().into_iter().collect();
        let impls = plan
            .impl_keys
            .iter()
            .cloned()
            .map(|key| {
                (
                    key,
                    crate::lower::ImplBody {
                        methods: HashMap::new(),
                        interface_args: vec![],
                        impl_type_params: vec![],
                    },
                )
            })
            .collect();
        let program = LoweredProgram {
            statements: vec![],
            main_arg_bindings: plan.main_arg_bindings.clone(),
            constants: HashMap::new(),
            functions,
            impls,
            warnings: vec![],
            foreign_modules: plan.foreign_modules.clone(),
        };
        let seed_env = HashMap::new();
        let files = match io {
            Some((stdout, stderr)) => {
                crate::execute::FileTable::with_io(config.max_open_files, stdout, stderr)
            }
            None => crate::execute::FileTable::new(config.max_open_files),
        };
        Ok(Self {
            files,
            program,
            config,
            plan,
            seed_env,
            arena: Vec::new(),
            frames: Vec::new(),
            bindings: Vec::new(),
            error: None,
        })
    }

    fn alloc(&mut self, value: RuntimeValue) -> i32 {
        self.arena.push(value);
        self.arena.len() as i32
    }
    fn get(&self, handle: i32) -> Result<&RuntimeValue> {
        handle
            .checked_sub(1)
            .and_then(|v| self.arena.get(v as usize))
            .context("invalid guest value address")
    }
    fn seed(&mut self, id: i32) -> Result<i32> {
        let name = self
            .plan
            .seeds
            .get(id as usize)
            .context("invalid seed id")?;
        let value = self
            .seed_env
            .get(name)
            .with_context(|| format!("missing seeded main value `{name}`"))?
            .clone();
        Ok(self.alloc(value))
    }
    fn constant(&mut self, id: i32) -> Result<i32> {
        let expr = self
            .plan
            .expressions
            .get(id as usize)
            .context("invalid constant id")?;
        let Expr::Value(value) = expr else {
            bail!("expression is not a constant")
        };
        Ok(self.alloc(value.clone()))
    }
    fn read(&mut self, handle: i32) -> Result<i32> {
        let value = match self.get(handle)? {
            RuntimeValue::Mutable(cell) | RuntimeValue::Reference { cell, .. } => {
                cell.borrow().clone()
            }
            value => value.clone(),
        };
        Ok(self.alloc(value))
    }
    fn frame_push(&mut self, handle: i32) -> Result<()> {
        self.get(handle)?;
        self.frames
            .last_mut()
            .context("missing guest argument frame")?
            .push(handle);
        Ok(())
    }
    fn take_frame_values(&mut self) -> Result<Vec<RuntimeValue>> {
        let handles = self.frames.pop().context("missing guest argument frame")?;
        handles
            .into_iter()
            .map(|handle| self.get(handle).cloned())
            .collect()
    }
    fn construct(&mut self, id: i32) -> Result<i32> {
        let expr = self
            .plan
            .expressions
            .get(id as usize)
            .context("invalid expression id")?
            .clone();
        let values = self.take_frame_values()?;
        let value = match expr {
            Expr::Mutable(_) => match &values[0] {
                RuntimeValue::Mutable(cell)
                | RuntimeValue::Reference {
                    cell,
                    mutable: true,
                } => RuntimeValue::Mutable(cell.clone()),
                value => RuntimeValue::Mutable(Rc::new(RefCell::new(value.clone()))),
            },
            Expr::Reference { mutable, .. } => {
                let cell = match &values[0] {
                    RuntimeValue::Mutable(cell) | RuntimeValue::Reference { cell, .. } => {
                        cell.clone()
                    }
                    value => Rc::new(RefCell::new(value.clone())),
                };
                RuntimeValue::Reference { cell, mutable }
            }
            Expr::Cast { target, .. } => RuntimeValue::Typed {
                type_ref: target,
                value: Box::new(values[0].clone()),
            },
            Expr::Primitive {
                op,
                operand_type,
                return_type,
                ..
            } => crate::execute::eval_primitive(op, &operand_type, &return_type, &values)?,
            Expr::HostCall {
                import,
                return_type,
                ..
            } => crate::execute::eval_expr(
                &Expr::HostCall {
                    import,
                    args: values.into_iter().map(Expr::Value).collect(),
                    return_type,
                },
                &HashMap::new(),
                &self.program,
                &mut self.files,
                &self.config,
            )?,
            Expr::EnumConstructor {
                enum_key,
                tag,
                payload,
            } => RuntimeValue::Enum {
                enum_key,
                tag,
                payload: payload.map(|_| Box::new(values[0].clone())),
            },
            Expr::Record(fields) => {
                RuntimeValue::Record(fields.keys().cloned().zip(values).collect())
            }
            Expr::Tuple(_) => RuntimeValue::Tuple(values),
            Expr::Array(_) => RuntimeValue::Array(values),
            Expr::Map(_) => RuntimeValue::Map(
                values
                    .chunks_exact(2)
                    .map(|pair| (pair[0].clone(), pair[1].clone()))
                    .collect(),
            ),
            Expr::Range { .. } => {
                let values: Vec<i64> = values
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Int(value) => Ok(*value),
                        other => bail!("range component must be integer, got {other:?}"),
                    })
                    .collect::<Result<_>>()?;
                if values[2] == 0 {
                    bail!("E-ITER-002: `$range.step` must not be zero");
                }
                RuntimeValue::Range {
                    start: values[0],
                    end: values[1],
                    step: values[2],
                }
            }
            other => bail!("unsupported host value construction {other:?}"),
        };
        Ok(self.alloc(value))
    }
    fn call(&mut self, id: i32) -> Result<i32> {
        let mut call = self
            .plan
            .calls
            .get(id as usize)
            .context("invalid call id")?
            .clone();
        let values = self.take_frame_values()?;
        call.args = values.iter().cloned().map(Expr::Value).collect();
        let sig = self
            .program
            .functions
            .get(&call.callee_key)
            .context("missing host function")?;
        if !matches!(sig.body, FunctionBody::Wasm { .. }) {
            bail!(
                "user function `{}` attempted host interpretation",
                call.callee_key
            )
        }
        if let FunctionBody::Wasm { import, .. } = &sig.body {
            if import.module.starts_with('@') {
                let value =
                    exec_static_wasm_scalar(&self.plan.foreign_modules, import, sig, &values)?;
                return Ok(self.alloc(value));
            }
        }
        let value = crate::execute::exec_call(
            &call,
            &self.program,
            &self.seed_env,
            &mut self.files,
            &self.config,
        )?;
        Ok(self.alloc(value))
    }
    fn set(&mut self, target: i32, value: i32) -> Result<()> {
        let next = self.get(value)?.clone();
        match self.get(target)? {
            RuntimeValue::Mutable(cell)
            | RuntimeValue::Reference {
                cell,
                mutable: true,
            } => {
                *cell.borrow_mut() = next;
                Ok(())
            }
            _ => bail!("E-SET-002: guest target is not writable"),
        }
    }
    fn as_bool(&mut self, handle: i32) -> Result<i32> {
        match self.get(handle)? {
            RuntimeValue::Bool(value) => Ok(*value as i32),
            RuntimeValue::Typed { value, .. }
                if matches!(value.as_ref(), RuntimeValue::Bool(_)) =>
            {
                match value.as_ref() {
                    RuntimeValue::Bool(v) => Ok(*v as i32),
                    _ => unreachable!(),
                }
            }
            other => bail!("condition must be `$bool`, got {other:?}"),
        }
    }
    fn matches(&mut self, id: i32, handle: i32) -> Result<i32> {
        let pattern = self
            .plan
            .patterns
            .get(id as usize)
            .context("invalid pattern id")?
            .clone();
        let value = self.get(handle)?.clone();
        let mut env = HashMap::new();
        let matched = crate::execute::pattern_matches(&pattern, &value, &self.program, &mut env)?;
        let mut bindings: Vec<_> = env.into_iter().collect();
        bindings.sort_by(|a, b| a.0.cmp(&b.0));
        self.bindings = bindings.into_iter().map(|(_, value)| value).collect();
        Ok(matched as i32)
    }
    fn binding(&mut self, index: i32) -> Result<i32> {
        let value = self
            .bindings
            .get(index as usize)
            .context("invalid pattern binding")?
            .clone();
        Ok(self.alloc(value))
    }
    fn iter_len(&mut self, handle: i32) -> Result<i32> {
        let len = match self.get(handle)? {
            RuntimeValue::Array(items) => items.len(),
            RuntimeValue::Map(items) => items.len(),
            RuntimeValue::Str(text) => text.chars().count(),
            RuntimeValue::Range { start, end, step } => range_len(*start, *end, *step)?,
            other => bail!("E-ITER-001: value is not traversable: {other:?}"),
        };
        if len > self.config.max_alloc_len {
            bail!(
                "E-ITER-003: traversal exceeds configured limit `{}`",
                self.config.max_alloc_len
            );
        }
        i32::try_from(len).context("E-ITER-003: traversal is too large for wasm32")
    }
    fn iter_get(&mut self, handle: i32, index: i32) -> Result<i32> {
        let index = usize::try_from(index).context("E-ITER-005: negative traversal index")?;
        let item = match self.get(handle)? {
            RuntimeValue::Array(items) => items.get(index).cloned(),
            RuntimeValue::Map(items) => items
                .get(index)
                .map(|(key, value)| RuntimeValue::Tuple(vec![key.clone(), value.clone()])),
            RuntimeValue::Str(text) => text
                .chars()
                .nth(index)
                .map(|scalar| RuntimeValue::Str(scalar.to_string())),
            RuntimeValue::Range { start, end, step } => {
                if index >= range_len(*start, *end, *step)? {
                    None
                } else {
                    let offset = step
                        .checked_mul(
                            i64::try_from(index).context("E-ITER-004: range index overflow")?,
                        )
                        .context("E-ITER-004: integer range overflow")?;
                    Some(RuntimeValue::Int(
                        start
                            .checked_add(offset)
                            .context("E-ITER-004: integer range overflow")?,
                    ))
                }
            }
            other => bail!("E-ITER-001: value is not traversable: {other:?}"),
        }
        .context("E-ITER-005: traversal index out of bounds")?;
        Ok(self.alloc(item))
    }
}

fn exec_static_wasm_scalar(
    modules: &BTreeMap<String, Vec<u8>>,
    import: &crate::lower::ImportTarget,
    signature: &crate::lower::FunctionSig,
    args: &[RuntimeValue],
) -> Result<RuntimeValue> {
    let bytes = modules.get(&import.module).with_context(|| {
        format!(
            "E-WASM-005: static wasm module `{}` was not embedded",
            import.module
        )
    })?;
    let mut store = Store::default();
    let module = wasmer::Module::new(&store, bytes)
        .with_context(|| format!("E-WASM-005: compile static wasm module `{}`", import.module))?;
    let mut memory_type = None;
    for required in module.imports() {
        match required.ty() {
            wasmer::ExternType::Memory(ty)
                if required.module() == "vibra_ffi" && required.name() == "memory" =>
            {
                if memory_type.replace(ty.clone()).is_some() {
                    bail!(
                        "E-WASM-005: static wasm module imports `vibra_ffi.memory` more than once"
                    );
                }
            }
            _ => bail!(
                "E-WASM-005: static wasm module has forbidden import `{}.{}`",
                required.module(),
                required.name()
            ),
        }
    }
    let has_buffer = signature
        .parameters
        .iter()
        .any(|parameter| is_foreign_buffer_type(&parameter.ty));
    if has_buffer && memory_type.is_none() {
        bail!("E-WASM-005: buffer wrapper requires the artifact to import `vibra_ffi.memory`");
    }
    let memory = memory_type
        .map(|ty| wasmer::Memory::new(&mut store, ty))
        .transpose()
        .context("E-WASM-005: create caller-owned FFI memory")?;
    let imports = match &memory {
        Some(memory) => wasmer::imports! {
            "vibra_ffi" => { "memory" => memory.clone() }
        },
        None => wasmer::Imports::new(),
    };
    let mut wasm_args = Vec::new();
    let mut buffers = Vec::new();
    let mut next_pointer = 8_u64;
    for (value, parameter) in args.iter().zip(&signature.parameters) {
        let ty = &parameter.ty;
        if is_foreign_buffer_type(ty) {
            let bytes = runtime_buffer_bytes(value, ty)?;
            if bytes.is_empty() {
                wasm_args.extend([wasmer::Value::I32(0), wasmer::Value::I32(0)]);
                continue;
            }
            let length = i32::try_from(bytes.len())
                .context("E-WASM-007: caller-owned buffer exceeds wasm32 length")?;
            let end = next_pointer
                .checked_add(bytes.len() as u64)
                .context("E-WASM-007: caller-owned buffer range overflow")?;
            if end > i32::MAX as u64 {
                bail!("E-WASM-007: caller-owned buffer exceeds wasm32 address space");
            }
            wasm_args.extend([
                wasmer::Value::I32(next_pointer as i32),
                wasmer::Value::I32(length),
            ]);
            buffers.push((next_pointer, bytes));
            next_pointer = end
                .checked_add(7)
                .context("E-WASM-007: buffer alignment overflow")?
                & !7;
        } else {
            wasm_args.push(runtime_to_wasm_scalar(value, ty)?);
        }
    }
    if let Some(memory) = &memory {
        memory
            .grow_at_least(&mut store, next_pointer)
            .context("E-WASM-007: imported memory cannot hold caller-owned buffers")?;
        let view = memory.view(&store);
        for (pointer, bytes) in &buffers {
            view.write(*pointer, bytes)
                .context("E-WASM-007: write caller-owned buffer")?;
        }
    }
    let instance = Instance::new(&mut store, &module, &imports)
        .with_context(|| format!("instantiate static wasm module `{}`", import.module))?;
    let function = instance
        .exports
        .get_function(&import.name)
        .with_context(|| {
            format!(
                "E-WASM-006: static wasm module `{}` has no function `{}`",
                import.module, import.name
            )
        })?;
    let results = function.call(&mut store, &wasm_args).with_context(|| {
        format!(
            "call static wasm export `{}.{}`",
            import.module, import.name
        )
    })?;
    match signature.return_type {
        TypeRef::Void => {
            if !results.is_empty() {
                bail!("E-WASM-007: void wrapper received a foreign result");
            }
            Ok(RuntimeValue::Void)
        }
        _ => {
            if results.len() != 1 {
                bail!(
                    "E-WASM-007: scalar wrapper expected one result, got {}",
                    results.len()
                );
            }
            wasm_to_runtime_scalar(&results[0], &signature.return_type)
        }
    }
}

fn is_foreign_buffer_type(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Str)
        || matches!(ty, TypeRef::Array(inner) if matches!(inner.as_ref(), TypeRef::UInt8))
}

fn runtime_buffer_bytes(value: &RuntimeValue, ty: &TypeRef) -> Result<Vec<u8>> {
    let value = match value {
        RuntimeValue::Typed { value, .. } => value.as_ref(),
        value => value,
    };
    match (ty, value) {
        (TypeRef::Str, RuntimeValue::Str(text)) => {
            // Rust strings are valid UTF-8, which establishes the v1 string
            // precondition before any foreign instruction executes.
            Ok(text.as_bytes().to_vec())
        }
        (TypeRef::Array(inner), RuntimeValue::Array(items))
            if matches!(inner.as_ref(), TypeRef::UInt8) =>
        {
            items
                .iter()
                .map(|item| match item {
                    RuntimeValue::Int(value) => u8::try_from(*value)
                        .context("E-WASM-007: byte buffer item is outside 0..=255"),
                    RuntimeValue::Typed { value, .. } => match value.as_ref() {
                        RuntimeValue::Int(value) => u8::try_from(*value)
                            .context("E-WASM-007: byte buffer item is outside 0..=255"),
                        other => bail!("E-WASM-007: byte buffer contains `{other:?}`"),
                    },
                    other => bail!("E-WASM-007: byte buffer contains `{other:?}`"),
                })
                .collect()
        }
        _ => bail!("E-WASM-007: value `{value:?}` is not caller-owned buffer `{ty:?}`"),
    }
}

fn runtime_to_wasm_scalar(value: &RuntimeValue, ty: &TypeRef) -> Result<wasmer::Value> {
    let value = match value {
        RuntimeValue::Typed { value, .. } => value.as_ref(),
        value => value,
    };
    match (ty, value) {
        (TypeRef::Bool, RuntimeValue::Bool(value)) => Ok(wasmer::Value::I32(i32::from(*value))),
        (
            TypeRef::Int8
            | TypeRef::Int16
            | TypeRef::Int32
            | TypeRef::UInt8
            | TypeRef::UInt16
            | TypeRef::UInt32,
            RuntimeValue::Int(value),
        ) => Ok(wasmer::Value::I32(*value as i32)),
        (TypeRef::Int64 | TypeRef::UInt64, RuntimeValue::Int(value)) => {
            Ok(wasmer::Value::I64(*value))
        }
        (TypeRef::Float32, RuntimeValue::Float(value)) => Ok(wasmer::Value::F32(*value as f32)),
        (TypeRef::Float64, RuntimeValue::Float(value)) => Ok(wasmer::Value::F64(*value)),
        _ => bail!("E-WASM-007: value `{value:?}` is not scalar type `{ty:?}`"),
    }
}

fn wasm_to_runtime_scalar(value: &wasmer::Value, ty: &TypeRef) -> Result<RuntimeValue> {
    match (ty, value) {
        (TypeRef::Bool, wasmer::Value::I32(value)) => Ok(RuntimeValue::Bool(*value != 0)),
        (
            TypeRef::Int8
            | TypeRef::Int16
            | TypeRef::Int32
            | TypeRef::UInt8
            | TypeRef::UInt16
            | TypeRef::UInt32,
            wasmer::Value::I32(value),
        ) => Ok(RuntimeValue::Int(i64::from(*value))),
        (TypeRef::Int64 | TypeRef::UInt64, wasmer::Value::I64(value)) => {
            Ok(RuntimeValue::Int(*value))
        }
        (TypeRef::Float32, wasmer::Value::F32(value)) => Ok(RuntimeValue::Float(*value as f64)),
        (TypeRef::Float64, wasmer::Value::F64(value)) => Ok(RuntimeValue::Float(*value)),
        _ => bail!("E-WASM-007: foreign result `{value:?}` does not match `{ty:?}`"),
    }
}

fn range_len(start: i64, end: i64, step: i64) -> Result<usize> {
    if step == 0 {
        bail!("E-ITER-002: `$range.step` must not be zero");
    }
    if (step > 0 && start >= end) || (step < 0 && start <= end) {
        return Ok(0);
    }
    let distance = if step > 0 {
        (end as i128) - (start as i128)
    } else {
        (start as i128) - (end as i128)
    };
    let magnitude = (step as i128).abs();
    usize::try_from((distance + magnitude - 1) / magnitude)
        .context("E-ITER-003: range length exceeds platform limits")
}

fn compile_program(program: &LoweredProgram) -> CompiledProgram {
    let mut compiler = Compiler::new(program);
    compiler.compile()
}

struct Compiler<'a> {
    program: &'a LoweredProgram,
    plan: WasmPlan,
    user_functions: Vec<UserFunction>,
    function_indexes: HashMap<String, u32>,
}

#[derive(Clone)]
struct UserFunction {
    name: String,
    key: String,
}

fn call_instance_key(call: &Call) -> String {
    format!("{}<{:?}>", call.callee_key, call.type_args)
}

fn reachable_user_functions(program: &LoweredProgram) -> Vec<UserFunction> {
    let mut pending = Vec::new();
    collect_calls_in_statements(&program.statements, &mut pending);
    let mut found = BTreeMap::new();
    while let Some(call) = pending.pop() {
        for arg in &call.args {
            collect_calls_in_expr(arg, &mut pending);
        }
        let Some(sig) = program.functions.get(&call.callee_key) else {
            continue;
        };
        let FunctionBody::User { statements } = &sig.body else {
            continue;
        };
        let key = call_instance_key(&call);
        if found.contains_key(&key) {
            continue;
        }
        found.insert(
            key.clone(),
            UserFunction {
                name: call.callee_key.clone(),
                key,
            },
        );
        collect_calls_in_statements(statements, &mut pending);
    }
    found.into_values().collect()
}

fn reachable_host_function_names(program: &LoweredProgram) -> BTreeSet<String> {
    let mut pending = Vec::new();
    collect_calls_in_statements(&program.statements, &mut pending);
    let mut seen_users = BTreeSet::new();
    let mut hosts = BTreeSet::new();
    while let Some(call) = pending.pop() {
        for arg in &call.args {
            collect_calls_in_expr(arg, &mut pending);
        }
        let Some(sig) = program.functions.get(&call.callee_key) else {
            continue;
        };
        match &sig.body {
            FunctionBody::Wasm { .. } => {
                hosts.insert(call.callee_key);
            }
            FunctionBody::User { statements } => {
                if seen_users.insert(call_instance_key(&call)) {
                    collect_calls_in_statements(statements, &mut pending);
                }
            }
        }
    }
    hosts
}

fn collect_calls_in_statements(statements: &[Statement], calls: &mut Vec<Call>) {
    for statement in statements {
        match statement {
            Statement::Call(call) => {
                calls.push(call.clone());
                for arg in &call.args {
                    collect_calls_in_expr(arg, calls);
                }
            }
            Statement::Let { value, .. } => match value {
                LetValue::Call(call) => {
                    calls.push(call.clone());
                    for arg in &call.args {
                        collect_calls_in_expr(arg, calls);
                    }
                }
                LetValue::Expr(expr) => collect_calls_in_expr(expr, calls),
            },
            Statement::Set { value, .. } | Statement::Return(value) | Statement::Eval(value) => {
                collect_calls_in_expr(value, calls)
            }
            Statement::Match { target, arms } => {
                collect_calls_in_expr(target, calls);
                for arm in arms {
                    collect_calls_in_statements(&arm.body, calls);
                }
            }
            Statement::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_calls_in_expr(cond, calls);
                collect_calls_in_statements(then_body, calls);
                collect_calls_in_statements(else_body, calls);
            }
            Statement::While { cond, body } => {
                collect_calls_in_expr(cond, calls);
                collect_calls_in_statements(body, calls);
            }
            Statement::For { source, body, .. } => {
                collect_calls_in_expr(source, calls);
                collect_calls_in_statements(body, calls);
            }
            Statement::Task { body, .. } => collect_calls_in_statements(body, calls),
            Statement::Spawn { value, .. } => collect_calls_in_expr(value, calls),
            Statement::Join { .. } => {}
            Statement::Break | Statement::Continue => {}
        }
    }
}

fn collect_calls_in_expr(expr: &Expr, calls: &mut Vec<Call>) {
    match expr {
        Expr::Call { call, .. } => {
            calls.push((**call).clone());
            for arg in &call.args {
                collect_calls_in_expr(arg, calls);
            }
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            collect_calls_in_expr(cond, calls);
            collect_calls_in_expr(then_e, calls);
            collect_calls_in_expr(else_e, calls);
        }
        _ => {
            for child in expr_children(expr) {
                collect_calls_in_expr(child, calls);
            }
        }
    }
}

impl<'a> Compiler<'a> {
    fn new(program: &'a LoweredProgram) -> Self {
        let user_functions = reachable_user_functions(program);
        let function_indexes = user_functions
            .iter()
            .enumerate()
            .map(|(i, function)| (function.key.clone(), HOST_FUNCTIONS + i as u32))
            .collect();
        let reachable_hosts = reachable_host_function_names(program);
        let host_functions = program
            .functions
            .iter()
            .filter(|(name, sig)| {
                reachable_hosts.contains(*name) && matches!(sig.body, FunctionBody::Wasm { .. })
            })
            .map(|(name, sig)| (name.clone(), sig.clone()))
            .collect();
        let mut impl_keys: Vec<_> = program.impls.keys().cloned().collect();
        impl_keys.sort_by(|a, b| {
            (&a.implementing_type, &a.interface).cmp(&(&b.implementing_type, &b.interface))
        });
        Self {
            program,
            plan: WasmPlan {
                main_arg_bindings: program.main_arg_bindings.clone(),
                host_functions,
                impl_keys,
                foreign_modules: program.foreign_modules.clone(),
                ..WasmPlan::default()
            },
            user_functions,
            function_indexes,
        }
    }

    fn compile(&mut self) -> CompiledProgram {
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32], [ValType::I32]); // 0
        types.ty().function([], []); // 1
        types.ty().function([ValType::I32], []); // 2
        types.ty().function([ValType::I32, ValType::I32], []); // 3
        types
            .ty()
            .function([ValType::I32, ValType::I32], [ValType::I32]); // 4
        types.ty().function([], [ValType::I32]); // 5
        let mut arity_types = BTreeMap::new();
        for function in &self.user_functions {
            let arity = self.program.functions[&function.name].parameters.len();
            if !arity_types.contains_key(&arity) {
                let id = 6 + arity_types.len() as u32;
                types
                    .ty()
                    .function(vec![ValType::I32; arity], [ValType::I32]);
                arity_types.insert(arity, id);
            }
        }

        let mut imports = ImportSection::new();
        for (name, ty) in [
            ("seed", 0),
            ("value_const", 0),
            ("value_read", 0),
            ("frame_begin", 1),
            ("frame_push", 2),
            ("value_construct", 0),
            ("host_call", 0),
            ("value_set", 3),
            ("value_bool", 0),
            ("pattern_match", 4),
            ("pattern_binding", 0),
            ("status", 5),
            ("no_match", 1),
            ("iter_len", 0),
            ("iter_get", 4),
        ] {
            imports.import(ABI_MODULE, name, EntityType::Function(ty));
        }
        let mut functions = FunctionSection::new();
        for function in &self.user_functions {
            functions
                .function(arity_types[&self.program.functions[&function.name].parameters.len()]);
        }
        functions.function(5);

        let mut code = CodeSection::new();
        for function in self.user_functions.clone() {
            code.function(&self.compile_user_function(&function.name));
        }
        code.function(&self.compile_main());
        let mut exports = ExportSection::new();
        exports.export(
            "main",
            ExportKind::Func,
            HOST_FUNCTIONS + self.user_functions.len() as u32,
        );
        let metadata = CustomSection {
            name: Cow::Borrowed("vibra.program.v1"),
            data: Cow::Owned(deterministic_program_fingerprint(self.program).into_bytes()),
        };
        let plan = CustomSection {
            name: Cow::Borrowed("vibra.plan.v1"),
            data: Cow::Owned(
                serde_json::to_vec(&self.plan).expect("serialize deterministic Wasm plan"),
            ),
        };
        let mut module = Module::new();
        module
            .section(&types)
            .section(&imports)
            .section(&functions)
            .section(&exports)
            .section(&code)
            .section(&metadata)
            .section(&plan);
        CompiledProgram {
            wasm: module.finish(),
        }
    }

    fn compile_user_function(&mut self, name: &str) -> Function {
        let sig = &self.program.functions[name];
        let FunctionBody::User { statements } = &sig.body else {
            unreachable!()
        };
        let args: Vec<_> = sig
            .parameters
            .iter()
            .map(|parameter| format!("args.{}", parameter.name))
            .collect();
        let mut context = FunctionCompiler::new(self, args, vec![], statements, false);
        context.emit_statements(statements);
        context.emit_const_value(RuntimeValue::Void);
        context.function.instruction(&Instruction::End);
        context.function
    }

    fn compile_main(&mut self) -> Function {
        let statements = self.program.statements.clone();
        let seeds: Vec<String> = self
            .program
            .main_arg_bindings
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let mut context = FunctionCompiler::new(self, vec![], seeds.clone(), &statements, true);
        for name in seeds {
            let seed = context.compiler.seed_id(name.clone());
            let local = context.locals[&name];
            context
                .function
                .instruction(&Instruction::I32Const(seed as i32));
            context.function.instruction(&Instruction::Call(HOST_SEED));
            context.function.instruction(&Instruction::LocalSet(local));
        }
        context.emit_statements(&statements);
        context
            .function
            .instruction(&Instruction::Call(HOST_STATUS));
        context.function.instruction(&Instruction::End);
        context.function
    }

    fn seed_id(&mut self, name: String) -> u32 {
        if let Some(i) = self.plan.seeds.iter().position(|v| v == &name) {
            i as u32
        } else {
            self.plan.seeds.push(name);
            (self.plan.seeds.len() - 1) as u32
        }
    }
    fn expr_id(&mut self, expr: Expr) -> u32 {
        self.plan.expressions.push(expr);
        (self.plan.expressions.len() - 1) as u32
    }
    fn call_id(&mut self, call: Call) -> u32 {
        self.plan.calls.push(call);
        (self.plan.calls.len() - 1) as u32
    }
    fn pattern_id(&mut self, pattern: Pattern) -> u32 {
        self.plan.patterns.push(pattern);
        (self.plan.patterns.len() - 1) as u32
    }
}

struct FunctionCompiler<'a, 'b> {
    compiler: &'a mut Compiler<'b>,
    function: Function,
    locals: HashMap<String, u32>,
    match_temp: u32,
    next_shadow: u32,
    is_main: bool,
    for_temps: Vec<(u32, u32)>,
    for_depth: usize,
    control_depth: u32,
    loop_stack: Vec<(u32, u32)>,
    call_temps: Vec<u32>,
    call_temp_depth: usize,
}

impl<'a, 'b> FunctionCompiler<'a, 'b> {
    fn new(
        compiler: &'a mut Compiler<'b>,
        args: Vec<String>,
        declared: Vec<String>,
        statements: &[Statement],
        is_main: bool,
    ) -> Self {
        let mut names = Vec::new();
        collect_locals(statements, &mut names);
        names.extend(declared);
        names.sort();
        names.dedup();
        for arg in &args {
            if !names.contains(arg) {
                names.push(arg.clone());
            }
        }
        names.sort();
        names.dedup();
        let mut locals = HashMap::new();
        for (i, arg) in args.iter().enumerate() {
            locals.insert(arg.clone(), i as u32);
        }
        let mut next = args.len() as u32;
        for name in names {
            if !locals.contains_key(&name) {
                locals.insert(name, next);
                next += 1;
            }
        }
        let match_temp = next;
        next += 1;
        let next_shadow = next;
        next += count_pattern_bindings(statements) as u32;
        let mut for_temps = Vec::new();
        for _ in 0..max_for_depth(statements) {
            for_temps.push((next, next + 1));
            next += 2;
        }
        let call_temp_count = max_call_temp_depth_in_statements(statements);
        let call_temps = (next..next + call_temp_count as u32).collect::<Vec<_>>();
        next += call_temp_count as u32;
        let extra = next - args.len() as u32;
        Self {
            compiler,
            function: Function::new(if extra == 0 {
                vec![]
            } else {
                vec![(extra, ValType::I32)]
            }),
            locals,
            match_temp,
            next_shadow,
            is_main,
            for_temps,
            for_depth: 0,
            control_depth: 0,
            loop_stack: Vec::new(),
            call_temps,
            call_temp_depth: 0,
        }
    }

    fn emit_statements(&mut self, statements: &[Statement]) {
        for statement in statements {
            self.emit_statement(statement);
        }
    }
    fn emit_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Call(call) => {
                self.emit_call(call);
                self.function.instruction(&Instruction::Drop);
            }
            Statement::Let { var, value } => {
                match value {
                    LetValue::Call(call) => self.emit_call(call),
                    LetValue::Expr(expr) => self.emit_expr(expr),
                };
                self.function
                    .instruction(&Instruction::LocalSet(self.locals[var]));
            }
            Statement::Set { var, value } => {
                self.function
                    .instruction(&Instruction::LocalGet(self.locals[var]));
                self.emit_expr(value);
                self.function.instruction(&Instruction::Call(HOST_SET));
            }
            Statement::Return(expr) => {
                self.emit_expr(expr);
                if self.is_main {
                    self.function.instruction(&Instruction::Drop);
                    self.function.instruction(&Instruction::Call(HOST_STATUS));
                }
                self.function.instruction(&Instruction::Return);
            }
            Statement::Eval(expr) => {
                self.emit_expr(expr);
                self.function.instruction(&Instruction::Drop);
            }
            Statement::If {
                cond,
                then_body,
                else_body,
            } => {
                self.emit_bool(cond);
                self.function
                    .instruction(&Instruction::If(BlockType::Empty));
                self.control_depth += 1;
                self.emit_statements(then_body);
                self.function.instruction(&Instruction::Else);
                self.emit_statements(else_body);
                self.function.instruction(&Instruction::End);
                self.control_depth -= 1;
            }
            Statement::While { cond, body } => {
                self.function
                    .instruction(&Instruction::Block(BlockType::Empty));
                self.control_depth += 1;
                let break_depth = self.control_depth;
                self.function
                    .instruction(&Instruction::Loop(BlockType::Empty));
                self.control_depth += 1;
                let loop_depth = self.control_depth;
                self.loop_stack.push((break_depth, loop_depth));
                self.emit_bool(cond);
                self.function.instruction(&Instruction::I32Eqz);
                self.function.instruction(&Instruction::BrIf(1));
                self.emit_statements(body);
                self.function.instruction(&Instruction::Br(0));
                self.function.instruction(&Instruction::End);
                self.function.instruction(&Instruction::End);
                self.loop_stack.pop();
                self.control_depth -= 2;
            }
            Statement::For { var, source, body } => {
                let (source_local, index_local) = self.for_temps[self.for_depth];
                self.for_depth += 1;
                self.emit_expr(source);
                self.function
                    .instruction(&Instruction::LocalSet(source_local));
                self.function.instruction(&Instruction::I32Const(0));
                self.function
                    .instruction(&Instruction::LocalSet(index_local));
                self.function
                    .instruction(&Instruction::Block(BlockType::Empty));
                self.control_depth += 1;
                let break_depth = self.control_depth;
                self.function
                    .instruction(&Instruction::Loop(BlockType::Empty));
                self.control_depth += 1;
                let continue_depth = self.control_depth;
                self.loop_stack.push((break_depth, continue_depth));
                self.function
                    .instruction(&Instruction::LocalGet(index_local));
                self.function
                    .instruction(&Instruction::LocalGet(source_local));
                self.function.instruction(&Instruction::Call(HOST_ITER_LEN));
                self.function.instruction(&Instruction::I32GeU);
                self.function.instruction(&Instruction::BrIf(1));
                self.function
                    .instruction(&Instruction::LocalGet(source_local));
                self.function
                    .instruction(&Instruction::LocalGet(index_local));
                self.function.instruction(&Instruction::Call(HOST_ITER_GET));
                self.function
                    .instruction(&Instruction::LocalSet(self.locals[var]));
                self.function
                    .instruction(&Instruction::Block(BlockType::Empty));
                self.control_depth += 1;
                self.loop_stack.last_mut().expect("loop pushed").1 = self.control_depth;
                self.emit_statements(body);
                self.function.instruction(&Instruction::End);
                self.control_depth -= 1;
                self.function
                    .instruction(&Instruction::LocalGet(index_local));
                self.function.instruction(&Instruction::I32Const(1));
                self.function.instruction(&Instruction::I32Add);
                self.function
                    .instruction(&Instruction::LocalSet(index_local));
                self.function.instruction(&Instruction::Br(0));
                self.function.instruction(&Instruction::End);
                self.function.instruction(&Instruction::End);
                self.loop_stack.pop();
                self.control_depth -= 2;
                self.for_depth -= 1;
            }
            Statement::Task { body, .. } => self.emit_statements(body),
            Statement::Spawn { handle, value, .. } => {
                // The deterministic Wasm executor runs the child computation
                // to its first (currently terminal) result, then retains that
                // value in the opaque handle local until `$join`.
                self.emit_expr(value);
                self.function
                    .instruction(&Instruction::LocalSet(self.locals[handle]));
            }
            Statement::Join { handle, var } => {
                self.function
                    .instruction(&Instruction::LocalGet(self.locals[handle]));
                self.function
                    .instruction(&Instruction::LocalSet(self.locals[var]));
            }
            Statement::Break => {
                let (target, _) = self.loop_stack.last().expect("validated loop control");
                self.function
                    .instruction(&Instruction::Br(self.control_depth - target));
            }
            Statement::Continue => {
                let (_, target) = self.loop_stack.last().expect("validated loop control");
                self.function
                    .instruction(&Instruction::Br(self.control_depth - target));
            }
            Statement::Match { target, arms } => {
                self.emit_expr(target);
                self.function
                    .instruction(&Instruction::LocalSet(self.match_temp));
                self.emit_match_arms(arms, 0);
            }
        }
    }

    fn emit_match_arms(&mut self, arms: &[crate::lower::MatchArm], index: usize) {
        if index == arms.len() {
            self.function.instruction(&Instruction::Call(HOST_NO_MATCH));
            return;
        }
        let arm = &arms[index];
        let id = self.compiler.pattern_id(arm.pattern.clone());
        self.function.instruction(&Instruction::I32Const(id as i32));
        self.function
            .instruction(&Instruction::LocalGet(self.match_temp));
        self.function.instruction(&Instruction::Call(HOST_MATCH));
        self.function
            .instruction(&Instruction::If(BlockType::Empty));
        self.control_depth += 1;
        let mut bindings = Vec::new();
        collect_pattern_bindings(&arm.pattern, &mut bindings);
        bindings.sort();
        bindings.dedup();
        let mut scoped_names = bindings.clone();
        collect_locals(&arm.body, &mut scoped_names);
        scoped_names.sort();
        scoped_names.dedup();
        let mut previous = Vec::new();
        for name in &scoped_names {
            previous.push((
                name.clone(),
                self.locals.insert(name.clone(), self.next_shadow),
            ));
            self.next_shadow += 1;
        }
        for (binding_index, name) in bindings.iter().enumerate() {
            self.function
                .instruction(&Instruction::I32Const(binding_index as i32));
            self.function.instruction(&Instruction::Call(HOST_BINDING));
            self.function
                .instruction(&Instruction::LocalSet(self.locals[name]));
        }
        self.emit_statements(&arm.body);
        for (name, prior) in previous {
            if let Some(local) = prior {
                self.locals.insert(name, local);
            } else {
                self.locals.remove(&name);
            }
        }
        self.function.instruction(&Instruction::Else);
        self.emit_match_arms(arms, index + 1);
        self.function.instruction(&Instruction::End);
        self.control_depth -= 1;
    }

    fn emit_bool(&mut self, expr: &Expr) {
        self.emit_expr(expr);
        self.function.instruction(&Instruction::Call(HOST_BOOL));
    }
    fn emit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::VarRef(name) => {
                self.function
                    .instruction(&Instruction::LocalGet(self.locals[name]));
                self.function.instruction(&Instruction::Call(HOST_READ));
            }
            Expr::Call { call, .. } => self.emit_call(call),
            Expr::If {
                cond,
                then_e,
                else_e,
            } => {
                self.emit_bool(cond);
                self.function
                    .instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                self.emit_expr(then_e);
                self.function.instruction(&Instruction::Else);
                self.emit_expr(else_e);
                self.function.instruction(&Instruction::End);
            }
            Expr::Value(_) => {
                let id = self.compiler.expr_id(expr.clone());
                self.function.instruction(&Instruction::I32Const(id as i32));
                self.function.instruction(&Instruction::Call(HOST_CONST));
            }
            _ => {
                self.function
                    .instruction(&Instruction::Call(HOST_FRAME_BEGIN));
                let preserves_place = matches!(expr, Expr::Mutable(_) | Expr::Reference { .. });
                for child in expr_children(expr) {
                    if preserves_place {
                        self.emit_raw_if_place(child);
                    } else {
                        self.emit_expr(child);
                    }
                    self.function
                        .instruction(&Instruction::Call(HOST_FRAME_PUSH));
                }
                let id = self.compiler.expr_id(expr.clone());
                self.function.instruction(&Instruction::I32Const(id as i32));
                self.function
                    .instruction(&Instruction::Call(HOST_CONSTRUCT));
            }
        }
    }
    fn emit_raw_if_place(&mut self, expr: &Expr) {
        if let Expr::VarRef(name) = expr {
            self.function
                .instruction(&Instruction::LocalGet(self.locals[name]));
        } else {
            self.emit_expr(expr);
        }
    }
    fn emit_call(&mut self, call: &Call) {
        if !call.source_args.is_empty() {
            self.emit_source_ordered_call(call);
            return;
        }
        if let Some(index) = self
            .compiler
            .function_indexes
            .get(&call_instance_key(call))
            .copied()
        {
            for arg in &call.args {
                self.emit_expr(arg);
            }
            self.function.instruction(&Instruction::Call(index));
        } else {
            self.function
                .instruction(&Instruction::Call(HOST_FRAME_BEGIN));
            for arg in &call.args {
                self.emit_expr(arg);
                self.function
                    .instruction(&Instruction::Call(HOST_FRAME_PUSH));
            }
            let id = self.compiler.call_id(call.clone());
            self.function.instruction(&Instruction::I32Const(id as i32));
            self.function.instruction(&Instruction::Call(HOST_CALL));
        }
    }

    fn emit_source_ordered_call(&mut self, call: &Call) {
        let start = self.call_temp_depth;
        self.call_temp_depth += call.source_args.len();
        for (offset, argument) in call.source_args.iter().enumerate() {
            self.emit_expr(argument);
            self.function
                .instruction(&Instruction::LocalSet(self.call_temps[start + offset]));
        }
        self.call_temp_depth = start;

        let fixed_count = call
            .argument_targets
            .iter()
            .filter_map(|target| match target {
                crate::lower::CallArgumentTarget::Fixed(index) => Some(index + 1),
                crate::lower::CallArgumentTarget::Variadic => None,
            })
            .max()
            .unwrap_or(0);
        let push_arguments = |this: &mut Self, push_to_frame: bool| {
            for fixed_index in 0..fixed_count {
                let source_index = call
                    .argument_targets
                    .iter()
                    .position(|target| matches!(target, crate::lower::CallArgumentTarget::Fixed(index) if *index == fixed_index))
                    .expect("fixed call argument was bound");
                this.function.instruction(&Instruction::LocalGet(
                    this.call_temps[start + source_index],
                ));
                if push_to_frame {
                    this.function
                        .instruction(&Instruction::Call(HOST_FRAME_PUSH));
                }
            }
            if call
                .argument_targets
                .iter()
                .any(|target| matches!(target, crate::lower::CallArgumentTarget::Variadic))
            {
                this.function
                    .instruction(&Instruction::Call(HOST_FRAME_BEGIN));
                for (source_index, target) in call.argument_targets.iter().enumerate() {
                    if matches!(target, crate::lower::CallArgumentTarget::Variadic) {
                        this.function.instruction(&Instruction::LocalGet(
                            this.call_temps[start + source_index],
                        ));
                        this.function
                            .instruction(&Instruction::Call(HOST_FRAME_PUSH));
                    }
                }
                let id = this.compiler.expr_id(Expr::Array(Vec::new()));
                this.function.instruction(&Instruction::I32Const(id as i32));
                this.function
                    .instruction(&Instruction::Call(HOST_CONSTRUCT));
                if push_to_frame {
                    this.function
                        .instruction(&Instruction::Call(HOST_FRAME_PUSH));
                }
            }
        };

        if let Some(index) = self
            .compiler
            .function_indexes
            .get(&call_instance_key(call))
            .copied()
        {
            push_arguments(self, false);
            self.function.instruction(&Instruction::Call(index));
        } else {
            self.function
                .instruction(&Instruction::Call(HOST_FRAME_BEGIN));
            push_arguments(self, true);
            let id = self.compiler.call_id(call.clone());
            self.function.instruction(&Instruction::I32Const(id as i32));
            self.function.instruction(&Instruction::Call(HOST_CALL));
        }
    }
    fn emit_const_value(&mut self, value: RuntimeValue) {
        self.emit_expr(&Expr::Value(value));
    }
}

fn expr_children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Mutable(v) | Expr::Cast { from: v, .. } => {
            vec![v]
        }
        Expr::Reference { target, .. } => vec![target],
        Expr::EnumConstructor { payload, .. } => payload.iter().map(|v| v.as_ref()).collect(),
        Expr::Record(fields) => fields.values().collect(),
        Expr::Tuple(v)
        | Expr::Array(v)
        | Expr::Primitive { args: v, .. }
        | Expr::HostCall { args: v, .. } => v.iter().collect(),
        Expr::Map(v) => v.iter().flat_map(|(k, v)| [k, v]).collect(),
        Expr::Range { start, end, step } => vec![start, end, step],
        _ => vec![],
    }
}

fn collect_locals(statements: &[Statement], names: &mut Vec<String>) {
    for statement in statements {
        match statement {
            Statement::Let { var, .. } | Statement::Set { var, .. } => names.push(var.clone()),
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_locals(&arm.body, names);
                }
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                collect_locals(then_body, names);
                collect_locals(else_body, names);
            }
            Statement::While { body, .. } => collect_locals(body, names),
            Statement::For { var, body, .. } => {
                names.push(var.clone());
                collect_locals(body, names);
            }
            Statement::Task { body, .. } => collect_locals(body, names),
            Statement::Spawn { handle, .. } => names.push(handle.clone()),
            Statement::Join { var, .. } => names.push(var.clone()),
            _ => {}
        }
    }
}

fn count_pattern_bindings(statements: &[Statement]) -> usize {
    let mut count = 0;
    for statement in statements {
        match statement {
            Statement::Match { arms, .. } => {
                for arm in arms {
                    let mut names = Vec::new();
                    collect_pattern_bindings(&arm.pattern, &mut names);
                    collect_locals(&arm.body, &mut names);
                    names.sort();
                    names.dedup();
                    count += names.len();
                    count += count_pattern_bindings(&arm.body);
                }
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_pattern_bindings(then_body);
                count += count_pattern_bindings(else_body);
            }
            Statement::While { body, .. } => count += count_pattern_bindings(body),
            Statement::For { body, .. } => count += count_pattern_bindings(body),
            Statement::Task { body, .. } => count += count_pattern_bindings(body),
            Statement::Spawn { .. } | Statement::Join { .. } => {}
            _ => {}
        }
    }
    count
}

fn max_for_depth(statements: &[Statement]) -> usize {
    statements
        .iter()
        .map(|statement| match statement {
            Statement::For { body, .. } => 1 + max_for_depth(body),
            Statement::Task { body, .. } => max_for_depth(body),
            Statement::Spawn { .. } | Statement::Join { .. } => 0,
            Statement::While { body, .. } => max_for_depth(body),
            Statement::If {
                then_body,
                else_body,
                ..
            } => max_for_depth(then_body).max(max_for_depth(else_body)),
            Statement::Match { arms, .. } => arms
                .iter()
                .map(|arm| max_for_depth(&arm.body))
                .max()
                .unwrap_or(0),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

fn max_call_temp_depth_in_statements(statements: &[Statement]) -> usize {
    let mut calls = Vec::new();
    collect_calls_in_statements(statements, &mut calls);
    calls.iter().map(|call| call.source_args.len()).sum()
}

fn collect_pattern_bindings(pattern: &Pattern, names: &mut Vec<String>) {
    match pattern {
        Pattern::Bind(name) => names.push(name.clone()),
        Pattern::Enum { payload, .. } => {
            if let Some(p) = payload {
                collect_pattern_bindings(p, names);
            }
        }
        Pattern::Record(fields) => {
            for p in fields.values() {
                collect_pattern_bindings(p, names);
            }
        }
        Pattern::Tuple(v) | Pattern::Array(v) => {
            for p in v {
                collect_pattern_bindings(p, names);
            }
        }
        Pattern::Map(v) => {
            for (k, v) in v {
                collect_pattern_bindings(k, names);
                collect_pattern_bindings(v, names);
            }
        }
        Pattern::Newtype { inner, .. } => collect_pattern_bindings(inner, names),
        _ => {}
    }
}

fn validate_imports(module: &wasmer::Module) -> Result<()> {
    for import in module.imports() {
        let valid_vibra = import.module() == ABI_MODULE && ABI_IMPORTS.contains(&import.name());
        let valid_wasi =
            import.module() == "wasi_snapshot_preview1" && WASI_IMPORTS.contains(&import.name());
        if !valid_vibra && !valid_wasi {
            bail!(
                "unsupported Wasm import `{}.{}`",
                import.module(),
                import.name()
            );
        }
    }
    Ok(())
}

fn deterministic_program_fingerprint(program: &LoweredProgram) -> String {
    let mut canonical = format!(
        "abi=vibra-v1\nstatements={:?}\nmain-args={:?}\nwarnings={:?}\n",
        program.statements, program.main_arg_bindings, program.warnings
    );
    let mut names: Vec<_> = program.constants.keys().collect();
    names.sort();
    for name in names {
        canonical.push_str(&format!("constant:{name}={:?}\n", program.constants[name]));
    }
    let mut names: Vec<_> = program.functions.keys().collect();
    names.sort();
    for name in names {
        canonical.push_str(&format!("function:{name}={:?}\n", program.functions[name]));
    }
    for (module, bytes) in &program.foreign_modules {
        canonical.push_str(&format!(
            "foreign-module:{module}={:x}\n",
            Sha256::digest(bytes)
        ));
    }
    let mut keys: Vec<_> = program.impls.keys().collect();
    keys.sort_by(|a, b| {
        (&a.implementing_type, &a.interface).cmp(&(&b.implementing_type, &b.interface))
    });
    for key in keys {
        let implementation = &program.impls[key];
        canonical.push_str(&format!(
            "impl:{}::{}:args={:?}:params={:?}\n",
            key.implementing_type,
            key.interface,
            implementation.interface_args,
            implementation.impl_type_params
        ));
        let mut methods: Vec<_> = implementation.methods.keys().collect();
        methods.sort();
        for method in methods {
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
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn empty_program() -> LoweredProgram {
        LoweredProgram {
            statements: vec![],
            main_arg_bindings: vec![],
            constants: HashMap::new(),
            functions: HashMap::new(),
            impls: HashMap::new(),
            warnings: vec![],
            foreign_modules: BTreeMap::new(),
        }
    }
    #[test]
    fn emitted_program_is_byte_deterministic() {
        let p = empty_program();
        assert_eq!(emit_program_wasm(&p), emit_program_wasm(&p));
    }

    #[test]
    fn caller_owned_byte_buffers_require_uint8_values() {
        let ty = TypeRef::Array(Box::new(TypeRef::UInt8));
        assert_eq!(
            runtime_buffer_bytes(
                &RuntimeValue::Array(vec![RuntimeValue::Int(0), RuntimeValue::Int(255)]),
                &ty
            )
            .unwrap(),
            vec![0, 255]
        );
        let error = runtime_buffer_bytes(&RuntimeValue::Array(vec![RuntimeValue::Int(256)]), &ty)
            .unwrap_err();
        assert!(error.to_string().contains("outside 0..=255"));
    }

    #[test]
    fn caller_owned_strings_use_valid_utf8_bytes() {
        assert_eq!(
            runtime_buffer_bytes(&RuntimeValue::Str("Vï".into()), &TypeRef::Str).unwrap(),
            vec![86, 195, 175]
        );
    }
    #[test]
    fn emitted_program_fingerprint_covers_lowered_contents() {
        let a = empty_program();
        let mut b = empty_program();
        b.statements
            .push(Statement::Eval(Expr::Value(RuntimeValue::Int(1))));
        assert_ne!(emit_program_wasm(&a), emit_program_wasm(&b));
    }
    #[test]
    fn emitted_program_executes_through_vibra_v1() {
        run_lowered(&empty_program(), &RunConfig::default()).unwrap();
    }
    #[test]
    fn emitted_program_uses_fine_grained_versioned_host_imports() {
        let store = Store::default();
        let module = wasmer::Module::new(&store, emit_program_wasm(&empty_program())).unwrap();
        let imports: Vec<_> = module
            .imports()
            .map(|i| (i.module().to_string(), i.name().to_string()))
            .collect();
        assert!(!imports.iter().any(|(_, n)| n == RUN_PROGRAM_IMPORT));
        assert!(imports
            .iter()
            .all(|(m, _)| m == ABI_MODULE || m == "wasi_snapshot_preview1"));
        assert!(imports.iter().any(|(_, n)| n == "value_const"));
        assert!(imports.iter().any(|(_, n)| n == "host_call"));
        validate_imports(&module).unwrap();
    }

    #[test]
    fn import_validation_rejects_unknown_wasi_and_vibra_symbols() {
        fn imported(module_name: &str, import_name: &str) -> wasmer::Module {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            let mut imports = ImportSection::new();
            imports.import(module_name, import_name, EntityType::Function(0));
            let mut module = Module::new();
            module.section(&types).section(&imports);
            wasmer::Module::new(&Store::default(), module.finish()).unwrap()
        }
        validate_imports(&imported("wasi_snapshot_preview1", "fd_write")).unwrap();
        assert!(validate_imports(&imported("wasi_snapshot_preview1", "secret")).is_err());
        assert!(validate_imports(&imported(ABI_MODULE, "run_program")).is_err());
    }

    #[test]
    fn interpreter_and_wasm_match_representative_pure_output() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let loaded = crate::load::load_program(&root.join("examples/hello.vibra")).unwrap();
        let program = crate::lower::lower_program(&loaded).unwrap();
        let interpreted = SharedWriter::default();
        let wasm = SharedWriter::default();
        crate::execute::run_lowered_interpreted_with_io(
            &program,
            &RunConfig::default(),
            Box::new(interpreted.clone()),
            Box::new(SharedWriter::default()),
        )
        .unwrap();
        run_lowered_with_io(
            &program,
            &RunConfig::default(),
            Box::new(wasm.clone()),
            Box::new(SharedWriter::default()),
        )
        .unwrap();
        assert_eq!(*interpreted.0.lock().unwrap(), *wasm.0.lock().unwrap());
        assert_eq!(&*wasm.0.lock().unwrap(), b"Hello, World!\n");
    }

    #[test]
    fn emitted_wasm_executes_after_lowered_program_is_dropped() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let loaded = crate::load::load_program(&root.join("examples/hello.vibra")).unwrap();
        let program = crate::lower::lower_program(&loaded).unwrap();
        let wasm = emit_program_wasm(&program);
        drop(program);
        drop(loaded);
        let output = SharedWriter::default();
        run_wasm_inner(
            &wasm,
            &RunConfig::default(),
            Some((Box::new(output.clone()), Box::new(SharedWriter::default()))),
        )
        .unwrap();
        assert_eq!(&*output.0.lock().unwrap(), b"Hello, World!\n");
    }

    #[test]
    fn embedded_plan_contains_only_reachable_host_calls() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let loaded = crate::load::load_program(&root.join("examples/hello.vibra")).unwrap();
        let program = crate::lower::lower_program(&loaded).unwrap();
        let compiled = compile_program(&program);
        let module = wasmer::Module::new(&Store::default(), &compiled.wasm).unwrap();
        let bytes = module.custom_sections("vibra.plan.v1").next().unwrap();
        let plan: WasmPlan = serde_json::from_slice(&bytes).unwrap();
        let called: BTreeSet<_> = plan
            .calls
            .iter()
            .map(|call| call.callee_key.clone())
            .collect();
        let embedded: BTreeSet<_> = plan.host_functions.keys().cloned().collect();
        assert_eq!(embedded, called);
        let high_level_imports: Vec<_> = plan
            .host_functions
            .values()
            .filter_map(|sig| match &sig.body {
                FunctionBody::Wasm { import, .. } => Some(import),
                FunctionBody::User { .. } => None,
            })
            .collect();
        assert!(!high_level_imports.is_empty());
        assert!(high_level_imports
            .iter()
            .all(|import| import.module == ABI_MODULE));
    }

    #[test]
    fn compiler_monomorphizes_reachable_generics_and_drops_unreachable_functions() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("entry.vibra");
        std::fs::write(
            &entry,
            r#"(defn identity (value t) t (do (return value)) where: (t any))
(defn unused-one () void (do (let x 1)))
(defn unused-two () void (do (let y 2)))
(defn main () void (do (identity bool true) (identity int64 7)))
"#,
        )
        .unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        let program = crate::lower::lower_program(&loaded).unwrap();
        let compiler = Compiler::new(&program);
        assert_eq!(
            compiler.user_functions.len(),
            2,
            "one function per reachable concrete specialization"
        );
        assert!(compiler
            .user_functions
            .iter()
            .all(|function| function.name.contains("identity")));
    }
}
