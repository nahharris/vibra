//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::lower::{
    Call, CapabilityGrant, Expr, FunctionBody, LetValue, LoweredExec, LoweredProgram, Pattern,
    RuntimeValue, Statement, TypeRef, WasmArgSpec,
};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    seed_main_args(program, config, &mut env);
    let mut files = FileTable::default();
    for stmt in &program.statements {
        if exec_statement(stmt, program, &mut env, &mut files, config)?.is_some() {
            bail!("unexpected `$return` at top level");
        }
    }
    Ok(())
}

pub fn eval_lowered_exec(
    exec: &LoweredExec,
    bindings: &HashMap<String, RuntimeValue>,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    let env = bindings.clone();
    let mut files = FileTable::default();
    eval_expr(&exec.expr, &env, &exec.program, &mut files, config)
}

fn seed_main_args(
    program: &LoweredProgram,
    config: &RunConfig,
    env: &mut HashMap<String, RuntimeValue>,
) {
    for (name, ty) in &program.main_arg_bindings {
        if let Some(value) = grant_status_value(ty, config) {
            env.insert(name.clone(), value);
        }
    }
}

fn grant_status_value(ty: &TypeRef, config: &RunConfig) -> Option<RuntimeValue> {
    let TypeRef::Instantiated { base, type_args } = ty else {
        return None;
    };
    if !base.ends_with("grant-status") || type_args.len() != 1 {
        return None;
    }
    let TypeRef::Named(grant_type) = &type_args[0] else {
        return None;
    };
    let scopes = grant_scopes(grant_type, config);
    if scopes.is_empty() {
        Some(RuntimeValue::Enum {
            enum_key: base.clone(),
            tag: "denied".to_string(),
            payload: Some(Box::new(RuntimeValue::Enum {
                enum_key: denial_reason_key(base),
                tag: "not-granted".to_string(),
                payload: None,
            })),
        })
    } else {
        Some(RuntimeValue::Enum {
            enum_key: base.clone(),
            tag: "granted".to_string(),
            payload: Some(Box::new(RuntimeValue::Capability(CapabilityGrant {
                type_key: grant_type.clone(),
                scopes,
            }))),
        })
    }
}

fn denial_reason_key(grant_status_key: &str) -> String {
    grant_status_key
        .rsplit_once('.')
        .map(|(prefix, _)| format!("{prefix}.denial-reason"))
        .unwrap_or_else(|| "denial-reason".to_string())
}

fn grant_scopes(grant_type: &str, config: &RunConfig) -> Vec<String> {
    let mut scopes = Vec::new();
    if grant_type.ends_with("fs-read-grant") {
        scopes.extend(config.preopen_host_dirs.iter().map(|p| path_scope(p)));
        scopes.extend(config.allow_read.iter().map(|p| path_scope(p)));
    } else if grant_type.ends_with("fs-write-grant") {
        scopes.extend(config.preopen_host_dirs.iter().map(|p| path_scope(p)));
        scopes.extend(config.allow_write.iter().map(|p| path_scope(p)));
    } else if grant_type.ends_with("stdin-read-grant") {
        if config.allow_stdin {
            scopes.push("*".to_string());
        }
    } else if grant_type.ends_with("env-read-grant") {
        scopes.extend(config.allow_env.clone());
    } else if grant_type.ends_with("env-write-grant") {
        scopes.extend(config.allow_env_write.clone());
    } else if grant_type.ends_with("net-connect-grant") {
        scopes.extend(config.allow_net.clone());
    } else if grant_type.ends_with("net-listen-grant") {
        scopes.extend(config.allow_net_listen.clone());
    } else if grant_type.ends_with("process-run-grant") {
        scopes.extend(config.allow_run.clone());
    } else if (grant_type.ends_with("clock-grant") && config.allow_clock)
        || (grant_type.ends_with("random-grant") && config.allow_random)
        || (grant_type.ends_with("system-info-grant") && config.allow_system_info)
    {
        scopes.push("*".to_string());
    }
    scopes
}

fn path_scope(p: &Path) -> String {
    normalize_absolute_path(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

enum FileHandle {
    Stdin,
    Stdout,
    Stderr,
    File(File),
}

struct FileTable {
    next: u64,
    handles: HashMap<u64, FileHandle>,
}

impl Default for FileTable {
    fn default() -> Self {
        let mut handles = HashMap::new();
        handles.insert(0, FileHandle::Stdin);
        handles.insert(1, FileHandle::Stdout);
        handles.insert(2, FileHandle::Stderr);
        Self { next: 3, handles }
    }
}

impl FileTable {
    fn insert(&mut self, file: File) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::File(file));
        id
    }

    fn get_mut(&mut self, id: u64) -> Result<&mut FileHandle> {
        self.handles
            .get_mut(&id)
            .with_context(|| format!("invalid file handle `{id}`"))
    }

    fn close(&mut self, id: u64) {
        if id > 2 {
            self.handles.remove(&id);
        }
    }
}

fn eval_expr(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    match expr {
        Expr::Value(v) => Ok(v.clone()),
        Expr::VarRef(v) => env
            .get(v)
            .cloned()
            .with_context(|| format!("unknown variable `${v}`")),
        Expr::Call { call, .. } => exec_call(call, program, env, files, config),
        Expr::Cast { from, target } => Ok(RuntimeValue::Typed {
            type_ref: target.clone(),
            value: Box::new(eval_expr(from, env, program, files, config)?),
        }),
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let payload_value = payload
                .as_ref()
                .map(|p| eval_expr(p, env, program, files, config))
                .transpose()?
                .map(Box::new);
            Ok(RuntimeValue::Enum {
                enum_key: enum_key.clone(),
                tag: tag.clone(),
                payload: payload_value,
            })
        }
        Expr::Record(fields) => fields
            .iter()
            .map(|(name, expr)| Ok((name.clone(), eval_expr(expr, env, program, files, config)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>>>()
            .map(RuntimeValue::Record),
        Expr::Tuple(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env, program, files, config))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Tuple),
        Expr::Array(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env, program, files, config))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Array),
        Expr::Map(items) => items
            .iter()
            .map(|(k, v)| {
                Ok((
                    eval_expr(k, env, program, files, config)?,
                    eval_expr(v, env, program, files, config)?,
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Map),
        Expr::If {
            cond,
            then_e,
            else_e,
        } => match eval_expr(cond, env, program, files, config)? {
            RuntimeValue::Bool(true) => eval_expr(then_e, env, program, files, config),
            RuntimeValue::Bool(false) => eval_expr(else_e, env, program, files, config),
            other => bail!("`$if` condition must be `$bool`, got {other:?}"),
        },
    }
}

/// Runs one statement; `Some` means a `$return` was executed.
fn exec_statement(
    stmt: &Statement,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    match stmt {
        Statement::Return(expr) => Ok(Some(eval_expr(expr, env, program, files, config)?)),
        Statement::Call(call) => {
            let _ = exec_call(call, program, env, files, config)?;
            Ok(None)
        }
        Statement::Let {
            var,
            value: binding,
        } => {
            let value = match binding {
                LetValue::Call(c) => exec_call(c, program, env, files, config)?,
                LetValue::Expr(e) => eval_expr(e, env, program, files, config)?,
            };
            env.insert(var.clone(), value);
            Ok(None)
        }
        Statement::Match { target, arms } => {
            let value = eval_expr(target, env, program, files, config)?;
            for arm in arms {
                let mut scoped = env.clone();
                if pattern_matches(&arm.pattern, &value, program, &mut scoped)? {
                    if let Some(v) = run_block(&arm.body, program, &mut scoped, files, config)? {
                        return Ok(Some(v));
                    }
                    return Ok(None);
                }
            }
            bail!("non-exhaustive $match reached runtime with value `{value:?}`")
        }
        Statement::Eval(expr) => {
            eval_expr(expr, env, program, files, config)?;
            Ok(None)
        }
        Statement::If {
            cond,
            then_body,
            else_body,
        } => match eval_expr(cond, env, program, files, config)? {
            RuntimeValue::Bool(true) => run_block(then_body, program, env, files, config),
            RuntimeValue::Bool(false) => run_block(else_body, program, env, files, config),
            other => bail!("`$if` condition must be `$bool`, got {other:?}"),
        },
        Statement::While { cond, body } => loop {
            match eval_expr(cond, env, program, files, config)? {
                RuntimeValue::Bool(true) => {
                    if let Some(v) = run_block(body, program, env, files, config)? {
                        return Ok(Some(v));
                    }
                }
                RuntimeValue::Bool(false) => return Ok(None),
                other => bail!("`$while` condition must be `$bool`, got {other:?}"),
            }
        },
    }
}

fn pattern_matches(
    pattern: &Pattern,
    value: &RuntimeValue,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
) -> Result<bool> {
    match pattern {
        Pattern::Wildcard => Ok(true),
        Pattern::Bind(name) => {
            env.insert(name.clone(), strip_type_tag(value.clone()));
            Ok(true)
        }
        Pattern::Literal(expected) => Ok(runtime_value_eq(expected, value)),
        Pattern::Enum {
            enum_key,
            tag,
            payload,
        } => {
            let RuntimeValue::Enum {
                enum_key: actual_enum,
                tag: actual_tag,
                payload: actual_payload,
            } = untyped(value)
            else {
                return Ok(false);
            };
            if actual_enum != enum_key || actual_tag != tag {
                return Ok(false);
            }
            match (payload, actual_payload.as_deref()) {
                (None, None) => Ok(true),
                (None, Some(RuntimeValue::Void)) => Ok(true),
                (Some(p), Some(v)) => pattern_matches(p, v, program, env),
                _ => Ok(false),
            }
        }
        Pattern::Record(fields) => {
            let RuntimeValue::Record(actual) = untyped(value) else {
                return Ok(false);
            };
            for (name, pat) in fields {
                let Some(v) = actual.get(name) else {
                    return Ok(false);
                };
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Tuple(items) => {
            let RuntimeValue::Tuple(actual) = untyped(value) else {
                return Ok(false);
            };
            if actual.len() != items.len() {
                return Ok(false);
            }
            for (pat, v) in items.iter().zip(actual.iter()) {
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Array(items) => {
            let RuntimeValue::Array(actual) = untyped(value) else {
                return Ok(false);
            };
            if actual.len() != items.len() {
                return Ok(false);
            }
            for (pat, v) in items.iter().zip(actual.iter()) {
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Map(entries) => {
            let RuntimeValue::Map(actual) = untyped(value) else {
                return Ok(false);
            };
            for (kp, vp) in entries {
                let mut found = false;
                for (ak, av) in actual {
                    let mut key_env = env.clone();
                    if pattern_matches(kp, ak, program, &mut key_env)?
                        && pattern_matches(vp, av, program, &mut key_env)?
                    {
                        *env = key_env;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Newtype { type_ref, inner } => {
            let RuntimeValue::Typed {
                type_ref: actual_ty,
                value,
            } = value
            else {
                return Ok(false);
            };
            if actual_ty != type_ref {
                return Ok(false);
            }
            pattern_matches(inner, value, program, env)
        }
        Pattern::Interface(iface) => {
            let Some(actual_ty) = runtime_type(value) else {
                return Ok(false);
            };
            let (TypeRef::Named(type_name)
            | TypeRef::Instantiated {
                base: type_name, ..
            }) = actual_ty
            else {
                return Ok(false);
            };
            let (TypeRef::Named(iface_name)
            | TypeRef::Instantiated {
                base: iface_name, ..
            }) = iface
            else {
                return Ok(false);
            };
            Ok(program
                .impls
                .keys()
                .any(|k| k.implementing_type == *type_name && k.interface == *iface_name))
        }
    }
}

fn runtime_type(value: &RuntimeValue) -> Option<&TypeRef> {
    match value {
        RuntimeValue::Typed { type_ref, .. } => Some(type_ref),
        _ => None,
    }
}

fn untyped(value: &RuntimeValue) -> &RuntimeValue {
    match value {
        RuntimeValue::Typed { value, .. } => value,
        _ => value,
    }
}

fn strip_type_tag(value: RuntimeValue) -> RuntimeValue {
    match value {
        RuntimeValue::Typed { value, .. } => *value,
        other => other,
    }
}

fn runtime_value_eq(expected: &RuntimeValue, actual: &RuntimeValue) -> bool {
    expected == untyped(actual)
}

fn run_block(
    stmts: &[Statement],
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    for stmt in stmts {
        if let Some(v) = exec_statement(stmt, program, env, files, config)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn eval_i64(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<i64> {
    match eval_expr(expr, env, program, files, config)? {
        RuntimeValue::Int(i) => Ok(i),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Int(i) => Ok(i),
            RuntimeValue::Float(_) => bail!("expected integer, got float"),
            other => bail!("expected integer, got {other:?}"),
        },
        RuntimeValue::Float(_) => bail!("expected integer, got float"),
        RuntimeValue::Enum {
            enum_key,
            payload: Some(payload),
            ..
        } if domain_type_matches(&enum_key, "fd") => match *payload {
            RuntimeValue::Int(i) => Ok(i),
            RuntimeValue::Float(_) => bail!("expected fd payload integer, got float"),
            other => bail!("expected fd payload int, got {other:?}"),
        },
        other => bail!("expected integer, got {other:?}"),
    }
}

fn eval_handle(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<u64> {
    let raw = eval_i64(expr, env, program, files, config)?;
    u64::try_from(raw).context("file handle < 0")
}

fn eval_string(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<String> {
    match eval_expr(expr, env, program, files, config)? {
        RuntimeValue::Str(s) => Ok(s),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Str(s) => Ok(s),
            other => bail!("expected string, got {other:?}"),
        },
        other => bail!("expected string, got {other:?}"),
    }
}

fn eval_bool(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<bool> {
    match eval_expr(expr, env, program, files, config)? {
        RuntimeValue::Bool(b) => Ok(b),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Bool(b) => Ok(b),
            other => bail!("expected bool, got {other:?}"),
        },
        other => bail!("expected bool, got {other:?}"),
    }
}

fn eval_capability(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<CapabilityGrant> {
    match eval_expr(expr, env, program, files, config)? {
        RuntimeValue::Capability(grant) => Ok(grant),
        other => bail!("expected capability grant, got {other:?}"),
    }
}

fn eval_domain_string(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
    domain: &str,
) -> Result<String> {
    match eval_expr(expr, env, program, files, config)? {
        RuntimeValue::Str(s) => Ok(s),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Str(s) => Ok(s),
            other => bail!("expected {domain} payload string, got {other:?}"),
        },
        RuntimeValue::Enum {
            enum_key,
            payload: Some(payload),
            ..
        } if domain_type_matches(&enum_key, domain) => match *payload {
            RuntimeValue::Str(s) => Ok(s),
            other => bail!("expected {domain} payload string, got {other:?}"),
        },
        other => bail!("expected {domain} or string wrapper, got {other:?}"),
    }
}

fn domain_type_matches(union_key: &str, expected: &str) -> bool {
    union_key == expected || union_key.ends_with(&format!(".{expected}"))
}

fn wrap_domain_string(domain: &str, value: String) -> RuntimeValue {
    RuntimeValue::Enum {
        enum_key: format!("types.{domain}"),
        tag: "str".to_string(),
        payload: Some(Box::new(RuntimeValue::Str(value))),
    }
}

fn wrap_domain_int(domain: &str, value: i64) -> RuntimeValue {
    RuntimeValue::Enum {
        enum_key: format!("types.{domain}"),
        tag: "int".to_string(),
        payload: Some(Box::new(RuntimeValue::Int(value))),
    }
}

fn result_enum_key(sig: &crate::lower::FunctionSig) -> String {
    match &sig.return_type {
        TypeRef::Instantiated { base, .. } => base.clone(),
        TypeRef::Named(name) => name.clone(),
        other => format!("{other:?}"),
    }
}

fn result_ok(sig: &crate::lower::FunctionSig, value: RuntimeValue) -> RuntimeValue {
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "ok".to_string(),
        payload: Some(Box::new(value)),
    }
}

fn result_err(
    sig: &crate::lower::FunctionSig,
    error_tag: &str,
    message: Option<String>,
) -> RuntimeValue {
    let fs_error_key = match &sig.return_type {
        TypeRef::Instantiated { type_args, .. } if type_args.len() >= 2 => match &type_args[1] {
            TypeRef::Named(name) => name.clone(),
            other => format!("{other:?}"),
        },
        _ => "fs-error".to_string(),
    };
    let payload = message.map(RuntimeValue::Str).map(Box::new);
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "err".to_string(),
        payload: Some(Box::new(RuntimeValue::Enum {
            enum_key: fs_error_key,
            tag: error_tag.to_string(),
            payload,
        })),
    }
}

fn fs_result<T>(
    sig: &crate::lower::FunctionSig,
    op: impl FnOnce() -> std::io::Result<T>,
    ok: impl FnOnce(T) -> RuntimeValue,
) -> RuntimeValue {
    match op() {
        Ok(value) => result_ok(sig, ok(value)),
        Err(err) => {
            let tag = match err.kind() {
                std::io::ErrorKind::NotFound => "not-found",
                std::io::ErrorKind::PermissionDenied => "permission-denied",
                std::io::ErrorKind::AlreadyExists => "already-exists",
                std::io::ErrorKind::InvalidInput => "invalid-path",
                _ => "io",
            };
            result_err(sig, tag, Some(err.to_string()))
        }
    }
}

fn forwarded_args(
    call: &Call,
    sig: &crate::lower::FunctionSig,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<Vec<RuntimeValue>> {
    let FunctionBody::Wasm { wasm_args, .. } = &sig.body else {
        bail!(
            "internal: forwarded_args on non-wasm function `{}`",
            sig.symbol
        );
    };
    let mut named: HashMap<&str, RuntimeValue> = HashMap::new();
    for (idx, name) in sig.arg_names.iter().enumerate() {
        named.insert(
            name.as_str(),
            eval_expr(&call.args[idx], env, program, files, config)?,
        );
    }

    let mut out = Vec::new();
    for spec in wasm_args {
        let v = match spec {
            WasmArgSpec::Arg(a) => named
                .get(a.as_str())
                .cloned()
                .with_context(|| format!("missing forwarded arg `{a}`"))?,
            WasmArgSpec::ConstInt(i) => RuntimeValue::Int(*i),
            WasmArgSpec::ConstStr(s) => RuntimeValue::Str(s.clone()),
        };
        out.push(v);
    }
    Ok(out)
}

fn resolve_granted_path(
    path: &str,
    grant: &CapabilityGrant,
    required_suffix: &str,
) -> Result<PathBuf> {
    if !grant.type_key.ends_with(required_suffix) {
        bail!(
            "grant `{}` cannot authorize `{required_suffix}` filesystem access",
            grant.type_key
        );
    }
    let abs = normalize_absolute_path(Path::new(path))?;
    let auth_path = nearest_existing_path(&abs)?;
    let canon_auth = auth_path.canonicalize().unwrap_or(auth_path);
    for root in &grant.scopes {
        if root == "*" {
            return Ok(abs);
        }
        let root_path = PathBuf::from(root);
        if let Ok(canon_root) = root_path.canonicalize() {
            if canon_auth.starts_with(&canon_root) {
                return Ok(abs);
            }
        } else {
            let normalized_root = normalize_absolute_path(&root_path)?;
            if abs.starts_with(&normalized_root) {
                return Ok(abs);
            }
        }
    }
    bail!("path `{}` is outside configured grants", path)
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().context("current dir")?
    };

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    Ok(normalized)
}

fn nearest_existing_path(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Ok(current);
        }
        if !current.pop() {
            bail!("path `{}` has no existing ancestor", path.display());
        }
    }
}

fn narrow_path_grant(
    grant: &CapabilityGrant,
    path: &str,
    required_suffix: &str,
) -> Result<CapabilityGrant> {
    let narrowed = resolve_granted_path(path, grant, required_suffix)?;
    let canon = narrowed
        .canonicalize()
        .or_else(|_| normalize_absolute_path(&narrowed))
        .unwrap_or(narrowed);
    Ok(CapabilityGrant {
        type_key: grant.type_key.clone(),
        scopes: vec![canon.display().to_string()],
    })
}

fn ensure_scope(grant: &CapabilityGrant, required_suffix: &str, requested: &str) -> Result<()> {
    if !grant.type_key.ends_with(required_suffix) {
        bail!(
            "grant `{}` cannot authorize `{required_suffix}`",
            grant.type_key
        );
    }
    if grant
        .scopes
        .iter()
        .any(|scope| scope == "*" || scope.eq_ignore_ascii_case(requested))
    {
        return Ok(());
    }
    bail!("requested scope `{requested}` is outside configured grants")
}

fn env_get(name: &str) -> std::io::Result<String> {
    std::env::var(name).map_err(|err| {
        let kind = match err {
            std::env::VarError::NotPresent => std::io::ErrorKind::NotFound,
            std::env::VarError::NotUnicode(_) => std::io::ErrorKind::InvalidData,
        };
        std::io::Error::new(kind, err.to_string())
    })
}

fn is_valid_env_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

fn exec_call(
    call: &Call,
    program: &LoweredProgram,
    env: &HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    let sig = program
        .functions
        .get(&call.callee_key)
        .with_context(|| format!("missing function `{}`", call.callee_key))?;

    match &sig.body {
        FunctionBody::User { statements } => {
            let mut fn_env: HashMap<String, RuntimeValue> = HashMap::new();
            for (idx, name) in sig.arg_names.iter().enumerate() {
                let val = eval_expr(&call.args[idx], env, program, files, config)?;
                fn_env.insert(name.clone(), val.clone());
                fn_env.insert(format!("args.{name}"), val);
            }
            if let Some(v) = run_block(statements, program, &mut fn_env, files, config)? {
                return Ok(v);
            }
            if sig.return_type != TypeRef::Void {
                bail!(
                    "function `{}` finished without a value (expected non-void return)",
                    sig.symbol
                );
            }
            Ok(RuntimeValue::Void)
        }
        FunctionBody::Wasm { import, .. } => {
            let _forwarded = forwarded_args(call, sig, env, program, files, config)?;
            let sym = sig.symbol.as_str();

            if import.module == "vibra_test" {
                return match import.name.as_str() {
                    "assert" => {
                        if eval_bool(&call.args[0], env, program, files, config)? {
                            Ok(RuntimeValue::Void)
                        } else {
                            bail!("assertion failed")
                        }
                    }
                    "fail" => bail!(
                        "{}",
                        eval_string(&call.args[0], env, program, files, config)?
                    ),
                    other => bail!("unsupported vibra_test import `{other}`"),
                };
            }

            if import.module == "vibra_code" {
                return match import.name.as_str() {
                    "parse" => {
                        let source = eval_string(&call.args[0], env, program, files, config)?;
                        let doc = crate::code_doc::parse(&source)?;
                        Ok(RuntimeValue::Typed {
                            type_ref: sig.return_type.clone(),
                            value: Box::new(RuntimeValue::Str(doc)),
                        })
                    }
                    "emit" => {
                        let doc = eval_string(&call.args[0], env, program, files, config)?;
                        Ok(RuntimeValue::Str(crate::code_doc::emit(&doc)?))
                    }
                    "get" => {
                        let doc = eval_string(&call.args[0], env, program, files, config)?;
                        let path = eval_string(&call.args[1], env, program, files, config)?;
                        Ok(RuntimeValue::Str(crate::code_doc::get(&doc, &path)?))
                    }
                    "set" => {
                        let doc = eval_string(&call.args[0], env, program, files, config)?;
                        let path = eval_string(&call.args[1], env, program, files, config)?;
                        let value = eval_string(&call.args[2], env, program, files, config)?;
                        let doc = crate::code_doc::set(&doc, &path, &value)?;
                        Ok(RuntimeValue::Typed {
                            type_ref: sig.return_type.clone(),
                            value: Box::new(RuntimeValue::Str(doc)),
                        })
                    }
                    "remove" => {
                        let doc = eval_string(&call.args[0], env, program, files, config)?;
                        let path = eval_string(&call.args[1], env, program, files, config)?;
                        let doc = crate::code_doc::remove(&doc, &path)?;
                        Ok(RuntimeValue::Typed {
                            type_ref: sig.return_type.clone(),
                            value: Box::new(RuntimeValue::Str(doc)),
                        })
                    }
                    "append" => {
                        let doc = eval_string(&call.args[0], env, program, files, config)?;
                        let path = eval_string(&call.args[1], env, program, files, config)?;
                        let value = eval_string(&call.args[2], env, program, files, config)?;
                        let doc = crate::code_doc::append(&doc, &path, &value)?;
                        Ok(RuntimeValue::Typed {
                            type_ref: sig.return_type.clone(),
                            value: Box::new(RuntimeValue::Str(doc)),
                        })
                    }
                    other => bail!("unsupported vibra_code import `{other}`"),
                };
            }

            match sym {
                "print" => {
                    let msg = eval_string(&call.args[0], env, program, files, config)?;
                    print!("{msg}");
                    std::io::stdout().flush().ok();
                    Ok(RuntimeValue::Void)
                }
                "println" => {
                    let msg = eval_string(&call.args[0], env, program, files, config)?;
                    println!("{msg}");
                    Ok(RuntimeValue::Void)
                }
                "eprint" => {
                    let msg = eval_string(&call.args[0], env, program, files, config)?;
                    eprint!("{msg}");
                    std::io::stderr().flush().ok();
                    Ok(RuntimeValue::Void)
                }
                "eprintln" => {
                    let msg = eval_string(&call.args[0], env, program, files, config)?;
                    eprintln!("{msg}");
                    Ok(RuntimeValue::Void)
                }
                "flush-stdout" => {
                    std::io::stdout().flush().context("flush stdout")?;
                    Ok(RuntimeValue::Void)
                }
                "read-line" => {
                    let fd = eval_i64(&call.args[0], env, program, files, config)?;
                    if fd != 0 {
                        bail!("read-line currently supports stdin fd 0 only");
                    }
                    let mut line = String::new();
                    std::io::stdin()
                        .read_line(&mut line)
                        .context("stdin read_line")?;
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }
                    Ok(RuntimeValue::Str(line))
                }
                "read-raw" => {
                    let fd = eval_i64(&call.args[0], env, program, files, config)?;
                    let len = eval_i64(&call.args[1], env, program, files, config)?;
                    if fd != 0 {
                        bail!("read-raw currently supports stdin fd 0 only");
                    }
                    let mut buf = vec![0u8; usize::try_from(len).context("len < 0")?];
                    let n = std::io::stdin().read(&mut buf).context("stdin read")?;
                    Ok(RuntimeValue::Str(
                        String::from_utf8_lossy(&buf[..n]).to_string(),
                    ))
                }
                "write-raw" | "write-all" => {
                    let fd = eval_i64(&call.args[0], env, program, files, config)?;
                    let bytes = eval_string(&call.args[1], env, program, files, config)?;
                    if fd == 2 {
                        eprint!("{bytes}");
                        std::io::stderr().flush().ok();
                    } else {
                        print!("{bytes}");
                        std::io::stdout().flush().ok();
                    }
                    Ok(RuntimeValue::Int(
                        i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                    ))
                }

                "path.new" => {
                    let path = eval_string(&call.args[0], env, program, files, config)?;
                    Ok(RuntimeValue::Str(path))
                }
                "open-read" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::OpenOptions::new().read(true).open(p),
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "open-write" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || {
                            fs::OpenOptions::new()
                                .create(true)
                                .truncate(true)
                                .write(true)
                                .open(p)
                        },
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "open-append" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::OpenOptions::new().create(true).append(true).open(p),
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "open-read-write" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let read_grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let write_grant = eval_capability(&call.args[2], env, program, files, config)?;
                    let _ = resolve_granted_path(&path, &read_grant, "fs-read-grant")?;
                    let p = resolve_granted_path(&path, &write_grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || {
                            fs::OpenOptions::new()
                                .create(true)
                                .truncate(false)
                                .read(true)
                                .write(true)
                                .open(p)
                        },
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "narrow-read" => {
                    let grant = eval_capability(&call.args[0], env, program, files, config)?;
                    let path =
                        eval_domain_string(&call.args[1], env, program, files, config, "path")?;
                    let narrowed = narrow_path_grant(&grant, &path, "fs-read-grant")?;
                    Ok(result_ok(sig, RuntimeValue::Capability(narrowed)))
                }
                "narrow-write" => {
                    let grant = eval_capability(&call.args[0], env, program, files, config)?;
                    let path =
                        eval_domain_string(&call.args[1], env, program, files, config, "path")?;
                    let narrowed = narrow_path_grant(&grant, &path, "fs-write-grant")?;
                    Ok(result_ok(sig, RuntimeValue::Capability(narrowed)))
                }
                "readln" => {
                    let grant = eval_capability(&call.args[0], env, program, files, config)?;
                    ensure_scope(&grant, "stdin-read-grant", "*")?;
                    let mut line = String::new();
                    match std::io::stdin().read_line(&mut line) {
                        Ok(_) => {
                            if line.ends_with('\n') {
                                line.pop();
                                if line.ends_with('\r') {
                                    line.pop();
                                }
                            }
                            Ok(result_ok(sig, RuntimeValue::Str(line)))
                        }
                        Err(err) => Ok(result_err(sig, "io", Some(err.to_string()))),
                    }
                }
                "get" if sig.alias.ends_with("env") => {
                    let name = eval_string(&call.args[0], env, program, files, config)?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    ensure_scope(&grant, "env-read-grant", &name)?;
                    Ok(fs_result(sig, || env_get(&name), RuntimeValue::Str))
                }
                "set" if sig.alias.ends_with("env") => {
                    let name = eval_string(&call.args[0], env, program, files, config)?;
                    let value = eval_string(&call.args[1], env, program, files, config)?;
                    let grant = eval_capability(&call.args[2], env, program, files, config)?;
                    ensure_scope(&grant, "env-write-grant", &name)?;
                    if !is_valid_env_name(&name) {
                        return Ok(result_err(sig, "invalid-name", None));
                    }
                    std::env::set_var(name, value);
                    Ok(result_ok(sig, RuntimeValue::Void))
                }
                "connect" if sig.alias.ends_with("net") => {
                    let target = eval_string(&call.args[0], env, program, files, config)?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    ensure_scope(&grant, "net-connect-grant", &target)?;
                    Ok(result_err(
                        sig,
                        "unsupported",
                        Some("network runtime is not implemented yet".to_string()),
                    ))
                }
                "listen" if sig.alias.ends_with("net") => {
                    let target = eval_string(&call.args[0], env, program, files, config)?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    ensure_scope(&grant, "net-listen-grant", &target)?;
                    Ok(result_err(
                        sig,
                        "unsupported",
                        Some("network runtime is not implemented yet".to_string()),
                    ))
                }
                "run" if sig.alias.ends_with("process") => {
                    let command = eval_string(&call.args[0], env, program, files, config)?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    ensure_scope(&grant, "process-run-grant", &command)?;
                    Ok(result_err(
                        sig,
                        "unsupported",
                        Some("process runtime is not implemented yet".to_string()),
                    ))
                }
                "now-unix-millis" => {
                    let grant = eval_capability(&call.args[0], env, program, files, config)?;
                    ensure_scope(&grant, "clock-grant", "*")?;
                    let millis = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .context("system clock before unix epoch")?
                        .as_millis();
                    Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)))
                }
                "bytes" if sig.alias.ends_with("random") => {
                    let len = eval_i64(&call.args[0], env, program, files, config)?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    ensure_scope(&grant, "random-grant", "*")?;
                    let len = usize::try_from(len).context("random len < 0")?;
                    Ok(RuntimeValue::Array(vec![RuntimeValue::Int(0); len]))
                }
                "info" if sig.alias.ends_with("sys") => {
                    let grant = eval_capability(&call.args[0], env, program, files, config)?;
                    ensure_scope(&grant, "system-info-grant", "*")?;
                    Ok(RuntimeValue::Str(format!(
                        "{}-{}",
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    )))
                }
                s if s.ends_with(".readable.read-string") => {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    let value = match files.get_mut(handle)? {
                        FileHandle::Stdin => {
                            let mut line = String::new();
                            std::io::stdin().read_line(&mut line).map(|_| {
                                if line.ends_with('\n') {
                                    line.pop();
                                    if line.ends_with('\r') {
                                        line.pop();
                                    }
                                }
                                line
                            })
                        }
                        FileHandle::File(file) => {
                            let mut s = String::new();
                            file.read_to_string(&mut s).map(|_| s)
                        }
                        FileHandle::Stdout | FileHandle::Stderr => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "handle is not readable",
                        )),
                    };
                    Ok(fs_result(sig, || value, RuntimeValue::Str))
                }
                s if s.ends_with(".readable.read-bytes") => {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    let value = match files.get_mut(handle)? {
                        FileHandle::Stdin => {
                            let mut s = String::new();
                            std::io::stdin().read_to_string(&mut s).map(|_| s)
                        }
                        FileHandle::File(file) => {
                            let mut buf = Vec::new();
                            file.read_to_end(&mut buf)
                                .map(|_| String::from_utf8_lossy(&buf).to_string())
                        }
                        FileHandle::Stdout | FileHandle::Stderr => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "handle is not readable",
                        )),
                    };
                    Ok(fs_result(sig, || value, RuntimeValue::Str))
                }
                s if s.ends_with(".writable.write-string")
                    || s.ends_with(".appendable.append-string") =>
                {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    let contents = eval_string(&call.args[1], env, program, files, config)?;
                    let result = match files.get_mut(handle)? {
                        FileHandle::Stdout => {
                            print!("{contents}");
                            std::io::stdout().flush()
                        }
                        FileHandle::Stderr => {
                            eprint!("{contents}");
                            std::io::stderr().flush()
                        }
                        FileHandle::File(file) => file.write_all(contents.as_bytes()),
                        FileHandle::Stdin => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "handle is not writable",
                        )),
                    };
                    Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
                }
                s if s.ends_with(".writable.write-bytes")
                    || s.ends_with(".appendable.append-bytes") =>
                {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    let contents = eval_string(&call.args[1], env, program, files, config)?;
                    let result = match files.get_mut(handle)? {
                        FileHandle::Stdout => {
                            print!("{contents}");
                            std::io::stdout().flush()
                        }
                        FileHandle::Stderr => {
                            eprint!("{contents}");
                            std::io::stderr().flush()
                        }
                        FileHandle::File(file) => file.write_all(contents.as_bytes()),
                        FileHandle::Stdin => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "handle is not writable",
                        )),
                    };
                    Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
                }
                s if s.ends_with(".writable.flush") => {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    let result = match files.get_mut(handle)? {
                        FileHandle::Stdout => std::io::stdout().flush(),
                        FileHandle::Stderr => std::io::stderr().flush(),
                        FileHandle::File(file) => file.flush(),
                        FileHandle::Stdin => Ok(()),
                    };
                    Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
                }
                s if s.ends_with(".closeable.close") => {
                    let handle = eval_handle(&call.args[0], env, program, files, config)?;
                    files.close(handle);
                    Ok(result_ok(sig, RuntimeValue::Void))
                }

                "create-dir-all" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::create_dir_all(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "remove-file" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::remove_file(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "remove-dir" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::remove_dir_all(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "exists" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(RuntimeValue::Bool(p.exists()))
                }
                "canonicalize" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(fs_result(
                        sig,
                        || p.canonicalize(),
                        |c| wrap_domain_string("path", c.display().to_string()),
                    ))
                }
                "metadata" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::metadata(p),
                        |md| RuntimeValue::Str(format!("size={},is_dir={}", md.len(), md.is_dir())),
                    ))
                }
                "read-to-string" | "read-file" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(fs_result(sig, || fs::read_to_string(p), RuntimeValue::Str))
                }
                "write-string-all" | "write-file" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let contents = eval_string(&call.args[1], env, program, files, config)?;
                    let grant = eval_capability(&call.args[2], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || fs::write(p, contents),
                        |_| RuntimeValue::Void,
                    ))
                }
                "append-string" | "append-file" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let contents = eval_string(&call.args[1], env, program, files, config)?;
                    let grant = eval_capability(&call.args[2], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    Ok(fs_result(
                        sig,
                        || {
                            let mut f = fs::OpenOptions::new().create(true).append(true).open(p)?;
                            f.write_all(contents.as_bytes())
                        },
                        |_| RuntimeValue::Void,
                    ))
                }
                "create-file" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "Path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    let _ = fs::File::create(p).context("create-file")?;
                    Ok(wrap_domain_string("file", path))
                }
                "open-path" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-write-grant")?;
                    let _ = fs::OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .read(true)
                        .write(true)
                        .open(p)
                        .context("open-path")?;
                    Ok(wrap_domain_int("fd", 1))
                }
                "read-dir" => {
                    let path =
                        eval_domain_string(&call.args[0], env, program, files, config, "path")?;
                    let grant = eval_capability(&call.args[1], env, program, files, config)?;
                    let p = resolve_granted_path(&path, &grant, "fs-read-grant")?;
                    Ok(fs_result(
                        sig,
                        || {
                            let mut names = Vec::new();
                            for entry in fs::read_dir(p)? {
                                let entry = entry?;
                                names.push(entry.file_name().to_string_lossy().to_string());
                            }
                            Ok(names)
                        },
                        |names| {
                            RuntimeValue::Array(
                                names.into_iter().map(RuntimeValue::Str).collect::<Vec<_>>(),
                            )
                        },
                    ))
                }
                _ => {
                    bail!(
                        "unsupported function `{}` import {}.{}",
                        sym,
                        import.module,
                        import.name
                    )
                }
            }
        }
    }
}
