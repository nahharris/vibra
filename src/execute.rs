//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::lower::{
    Call, Expr, FunctionBody, LetValue, LoweredProgram, Pattern, RuntimeValue, Statement, TypeRef,
    WasmArgSpec,
};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    let mut files = FileTable::default();
    for stmt in &program.statements {
        if exec_statement(stmt, program, &mut env, &mut files, config)?.is_some() {
            bail!("unexpected `$return` at top level");
        }
    }
    Ok(())
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

fn eval_expr(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<RuntimeValue> {
    match expr {
        Expr::Value(v) => Ok(v.clone()),
        Expr::VarRef(v) => env
            .get(v)
            .cloned()
            .with_context(|| format!("unknown variable `${v}`")),
        Expr::Cast { from, target } => Ok(RuntimeValue::Typed {
            type_ref: target.clone(),
            value: Box::new(eval_expr(from, env)?),
        }),
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let payload_value = payload
                .as_ref()
                .map(|p| eval_expr(p, env))
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
            .map(|(name, expr)| Ok((name.clone(), eval_expr(expr, env)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>>>()
            .map(RuntimeValue::Record),
        Expr::Tuple(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Tuple),
        Expr::Array(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Array),
        Expr::Map(items) => items
            .iter()
            .map(|(k, v)| Ok((eval_expr(k, env)?, eval_expr(v, env)?)))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Map),
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
        Statement::Return(expr) => Ok(Some(eval_expr(expr, env)?)),
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
                LetValue::Expr(e) => eval_expr(e, env)?,
            };
            env.insert(var.clone(), value);
            Ok(None)
        }
        Statement::Match { target, arms } => {
            let value = eval_expr(target, env)?;
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

fn eval_i64(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<i64> {
    match eval_expr(expr, env)? {
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

fn eval_handle(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<u64> {
    let raw = eval_i64(expr, env)?;
    u64::try_from(raw).context("file handle < 0")
}

fn eval_string(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<String> {
    match eval_expr(expr, env)? {
        RuntimeValue::Str(s) => Ok(s),
        RuntimeValue::Typed { value, .. } => match *value {
            RuntimeValue::Str(s) => Ok(s),
            other => bail!("expected string, got {other:?}"),
        },
        other => bail!("expected string, got {other:?}"),
    }
}

fn eval_domain_string(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    domain: &str,
) -> Result<String> {
    match eval_expr(expr, env)? {
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
) -> Result<Vec<RuntimeValue>> {
    let FunctionBody::Wasm { wasm_args, .. } = &sig.body else {
        bail!(
            "internal: forwarded_args on non-wasm function `{}`",
            sig.symbol
        );
    };
    let mut named: HashMap<&str, RuntimeValue> = HashMap::new();
    for (idx, name) in sig.arg_names.iter().enumerate() {
        named.insert(name.as_str(), eval_expr(&call.args[idx], env)?);
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

fn resolve_preopen(path: &str, config: &RunConfig) -> Result<PathBuf> {
    fn norm(p: &Path) -> String {
        let mut s = p.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
        if let Some(rest) = s.strip_prefix("//?/") {
            s = rest.to_string();
        }
        s
    }

    let p = Path::new(path);
    let abs = if p.is_absolute() {
        PathBuf::from(p)
    } else {
        std::env::current_dir()
            .context("current dir")?
            .join(p)
            .canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap().join(p))
    };
    if config.preopen_host_dirs.is_empty() {
        return Ok(abs);
    }
    let canon = abs.canonicalize().unwrap_or(abs.clone());
    let canon_s = norm(&canon);
    for root in &config.preopen_host_dirs {
        let r = root.canonicalize().unwrap_or(root.clone());
        let r_s = norm(&r);
        if canon_s.starts_with(&r_s) {
            return Ok(canon);
        }
    }
    bail!("path `{}` is outside configured preopens", path)
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
                let val = eval_expr(&call.args[idx], env)?;
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
            let _forwarded = forwarded_args(call, sig, env)?;
            let sym = sig.symbol.as_str();

            match sym {
                "print" => {
                    let msg = eval_string(&call.args[0], env)?;
                    print!("{msg}");
                    std::io::stdout().flush().ok();
                    Ok(RuntimeValue::Void)
                }
                "println" => {
                    let msg = eval_string(&call.args[0], env)?;
                    println!("{msg}");
                    Ok(RuntimeValue::Void)
                }
                "eprint" => {
                    let msg = eval_string(&call.args[0], env)?;
                    eprint!("{msg}");
                    std::io::stderr().flush().ok();
                    Ok(RuntimeValue::Void)
                }
                "eprintln" => {
                    let msg = eval_string(&call.args[0], env)?;
                    eprintln!("{msg}");
                    Ok(RuntimeValue::Void)
                }
                "flush-stdout" => {
                    std::io::stdout().flush().context("flush stdout")?;
                    Ok(RuntimeValue::Void)
                }
                "read-line" => {
                    let fd = eval_i64(&call.args[0], env)?;
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
                    let fd = eval_i64(&call.args[0], env)?;
                    let len = eval_i64(&call.args[1], env)?;
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
                    let fd = eval_i64(&call.args[0], env)?;
                    let bytes = eval_string(&call.args[1], env)?;
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
                    let path = eval_string(&call.args[0], env)?;
                    Ok(RuntimeValue::Str(path))
                }
                "open-read" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::OpenOptions::new().read(true).open(p),
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "open-write" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
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
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::OpenOptions::new().create(true).append(true).open(p),
                        |file| {
                            RuntimeValue::Int(i64::try_from(files.insert(file)).unwrap_or(i64::MAX))
                        },
                    ))
                }
                "open-read-write" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
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
                "readln" => {
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
                s if s.ends_with(".readable.read-string") => {
                    let handle = eval_handle(&call.args[0], env)?;
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
                    let handle = eval_handle(&call.args[0], env)?;
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
                    let handle = eval_handle(&call.args[0], env)?;
                    let contents = eval_string(&call.args[1], env)?;
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
                    let handle = eval_handle(&call.args[0], env)?;
                    let contents = eval_string(&call.args[1], env)?;
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
                    let handle = eval_handle(&call.args[0], env)?;
                    let result = match files.get_mut(handle)? {
                        FileHandle::Stdout => std::io::stdout().flush(),
                        FileHandle::Stderr => std::io::stderr().flush(),
                        FileHandle::File(file) => file.flush(),
                        FileHandle::Stdin => Ok(()),
                    };
                    Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
                }
                s if s.ends_with(".closeable.close") => {
                    let handle = eval_handle(&call.args[0], env)?;
                    files.close(handle);
                    Ok(result_ok(sig, RuntimeValue::Void))
                }

                "create-dir-all" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::create_dir_all(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "remove-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::remove_file(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "remove-dir" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::remove_dir_all(p),
                        |_| RuntimeValue::Void,
                    ))
                }
                "exists" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(RuntimeValue::Int(if p.exists() { 1 } else { 0 }))
                }
                "canonicalize" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || p.canonicalize(),
                        |c| wrap_domain_string("path", c.display().to_string()),
                    ))
                }
                "metadata" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::metadata(p),
                        |md| RuntimeValue::Str(format!("size={},is_dir={}", md.len(), md.is_dir())),
                    ))
                }
                "read-to-string" | "read-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(sig, || fs::read_to_string(p), RuntimeValue::Str))
                }
                "write-string-all" | "write-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let contents = eval_string(&call.args[1], env)?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(fs_result(
                        sig,
                        || fs::write(p, contents),
                        |_| RuntimeValue::Void,
                    ))
                }
                "append-string" | "append-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let contents = eval_string(&call.args[1], env)?;
                    let p = resolve_preopen(&path, config)?;
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
                    let path = eval_domain_string(&call.args[0], env, "Path")?;
                    let p = resolve_preopen(&path, config)?;
                    let _ = fs::File::create(p).context("create-file")?;
                    Ok(wrap_domain_string("file", path))
                }
                "open-path" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
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
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
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
