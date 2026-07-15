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
const HOST_FUNCTIONS: u32 = 13;

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
    main_grant_decls: Vec<crate::lower::GrantDecl>,
    host_functions: BTreeMap<String, crate::lower::FunctionSig>,
    impl_keys: Vec<crate::lower::ImplKey>,
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
    let imports = imports! { ABI_MODULE => {
        "seed" => seed, "value_const" => value_const, "value_read" => value_read,
        "frame_begin" => frame_begin, "frame_push" => frame_push, "value_construct" => construct,
        "host_call" => host_call, "value_set" => value_set, "value_bool" => value_bool,
        "pattern_match" => pattern_match, "pattern_binding" => pattern_binding,
        "status" => status, "no_match" => no_match,
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
            main_grant_decls: plan.main_grant_decls.clone(),
            constants: HashMap::new(),
            functions,
            impls,
            warnings: vec![],
        };
        let mut seed_env = HashMap::new();
        crate::execute::seed_main_args(&program, &config, &mut seed_env)?;
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
            Expr::PolicyNarrow { target, .. } => {
                if !matches!(values[0], RuntimeValue::Policy(_)) {
                    bail!("`$policy.narrow` expects a policy value")
                }
                let TypeRef::Policy(policy) = target else {
                    bail!("`$policy.narrow.into` must be a `$policy` type")
                };
                RuntimeValue::Policy(crate::lower::PolicyValue { policy })
            }
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
        call.args = values.into_iter().map(Expr::Value).collect();
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
                main_grant_decls: program.main_grant_decls.clone(),
                host_functions,
                impl_keys,
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
            let arity = self.program.functions[&function.name].arg_names.len();
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
        ] {
            imports.import(ABI_MODULE, name, EntityType::Function(ty));
        }
        let mut functions = FunctionSection::new();
        for function in &self.user_functions {
            functions
                .function(arity_types[&self.program.functions[&function.name].arg_names.len()]);
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
            .arg_names
            .iter()
            .map(|name| format!("args.{name}"))
            .collect();
        let mut context = FunctionCompiler::new(self, args, vec![], statements, false);
        context.emit_statements(statements);
        context.emit_const_value(RuntimeValue::Void);
        context.function.instruction(&Instruction::End);
        context.function
    }

    fn compile_main(&mut self) -> Function {
        let statements = self.program.statements.clone();
        let mut seeds: Vec<String> = self
            .program
            .main_arg_bindings
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        seeds.extend(
            self.program
                .main_grant_decls
                .iter()
                .map(|decl| format!("grants.{}", decl.name)),
        );
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
                self.emit_statements(then_body);
                self.function.instruction(&Instruction::Else);
                self.emit_statements(else_body);
                self.function.instruction(&Instruction::End);
            }
            Statement::While { cond, body } => {
                self.function
                    .instruction(&Instruction::Block(BlockType::Empty));
                self.function
                    .instruction(&Instruction::Loop(BlockType::Empty));
                self.emit_bool(cond);
                self.function.instruction(&Instruction::I32Eqz);
                self.function.instruction(&Instruction::BrIf(1));
                self.emit_statements(body);
                self.function.instruction(&Instruction::Br(0));
                self.function.instruction(&Instruction::End);
                self.function.instruction(&Instruction::End);
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
                for child in expr_children(expr) {
                    self.emit_raw_if_place(child);
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
    fn emit_const_value(&mut self, value: RuntimeValue) {
        self.emit_expr(&Expr::Value(value));
    }
}

fn expr_children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Mutable(v) | Expr::Cast { from: v, .. } | Expr::PolicyNarrow { from: v, .. } => {
            vec![v]
        }
        Expr::Reference { target, .. } => vec![target],
        Expr::EnumConstructor { payload, .. } => payload.iter().map(|v| v.as_ref()).collect(),
        Expr::Record(fields) => fields.values().collect(),
        Expr::Tuple(v) | Expr::Array(v) => v.iter().collect(),
        Expr::Map(v) => v.iter().flat_map(|(k, v)| [k, v]).collect(),
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
            _ => {}
        }
    }
    count
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
        "abi=vibra-v1\nstatements={:?}\nmain-args={:?}\ngrants={:?}\nwarnings={:?}\n",
        program.statements, program.main_arg_bindings, program.main_grant_decls, program.warnings
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
            main_grant_decls: vec![],
            constants: HashMap::new(),
            functions: HashMap::new(),
            impls: HashMap::new(),
            warnings: vec![],
        }
    }
    #[test]
    fn emitted_program_is_byte_deterministic() {
        let p = empty_program();
        assert_eq!(emit_program_wasm(&p), emit_program_wasm(&p));
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
    fn interpreter_and_wasm_match_privileged_policy_program() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let fs = std::fs::canonicalize(root.join("stdlib/src/fs.vibra")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data.txt");
        std::fs::write(&data, "secret").unwrap();
        let entry = temp.path().join("entry.vibra");
        let path = |value: &std::path::Path| value.display().to_string().replace('\\', "/");
        std::fs::write(
            &entry,
            format!(
                r#"fs:
  $import: "{}"
main:
  $function:
    policy:
      $policy:
        fs-read:
          - requirement: mandatory
            scopes:
              - dir: "{}"
  return: $void
  do:
    - $let:
        path:
          $fs.path.new: "{}"
    - $let:
        text:
          $fs.read-to-string: $path
          policy: $args.policy
"#,
                path(&fs),
                path(temp.path()),
                path(&data)
            ),
        )
        .unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        let program = crate::lower::lower_program(&loaded).unwrap();
        let config = RunConfig {
            approved_policy: Some(crate::lower::PolicyType {
                domains: BTreeMap::from([(
                    "fs-read".into(),
                    vec![crate::lower::PolicyGroup {
                        requirement: crate::lower::GrantRequirement::Mandatory,
                        scopes: vec![crate::lower::PolicyScope::Dir(path(temp.path()))],
                    }],
                )]),
            }),
            ..RunConfig::default()
        };
        crate::execute::run_lowered_interpreted(&program, &config).unwrap();
        run_lowered(&program, &config).unwrap();
        let interpreted = crate::execute::run_lowered_interpreted(&program, &RunConfig::default())
            .unwrap_err()
            .to_string();
        let guest = run_lowered(&program, &RunConfig::default())
            .unwrap_err()
            .to_string();
        assert_eq!(interpreted, guest);
    }

    #[test]
    fn compiler_monomorphizes_reachable_generics_and_drops_unreachable_functions() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("entry.vibra");
        std::fs::write(
            &entry,
            r#"identity:
  $function:
    value: $t
  =where:
    t: []
  return: $t
  do:
    - $return: $args.value
unused-one:
  $function: $void
  return: $void
  do:
    - $let:
        x: 1
unused-two:
  $function: $void
  return: $void
  do:
    - $let:
        y: 2
main:
  $function: $void
  return: $void
  do:
    - $identity: true
      t: $bool
    - $identity: 7
      t: $int64
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
