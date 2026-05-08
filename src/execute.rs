//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::lower::{
    Call, Expr, FunctionBody, LetValue, LoweredProgram, RuntimeValue, Statement, TypeRef,
    WasmArgSpec,
};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    for stmt in &program.statements {
        if exec_statement(stmt, program, &mut env, config)?.is_some() {
            bail!("unexpected `$return` at top level");
        }
    }
    Ok(())
}

fn eval_expr(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<RuntimeValue> {
    match expr {
        Expr::Value(v) => Ok(v.clone()),
        Expr::VarRef(v) => env
            .get(v)
            .cloned()
            .with_context(|| format!("unknown variable `${v}`")),
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
    }
}

/// Runs one statement; `Some` means a `$return` was executed.
fn exec_statement(
    stmt: &Statement,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    match stmt {
        Statement::Return(expr) => Ok(Some(eval_expr(expr, env)?)),
        Statement::Call(call) => {
            let _ = exec_call(call, program, env, config)?;
            Ok(None)
        }
        Statement::Let {
            var,
            value: binding,
        } => {
            let value = match binding {
                LetValue::Call(c) => exec_call(c, program, env, config)?,
                LetValue::Expr(e) => eval_expr(e, env)?,
            };
            env.insert(var.clone(), value);
            Ok(None)
        }
        Statement::Match {
            target,
            enum_key,
            arms,
        } => {
            let value = eval_expr(target, env)?;
            let RuntimeValue::Enum {
                enum_key: actual_enum,
                tag,
                payload,
            } = value
            else {
                bail!("$match target did not evaluate to enum value");
            };
            if &actual_enum != enum_key {
                bail!("$match target enum mismatch: expected `{enum_key}`, got `{actual_enum}`");
            }
            let arm = arms
                .iter()
                .find(|a| a.tag == tag)
                .with_context(|| format!("missing runtime $match arm for tag `{tag}`"))?;
            let mut scoped = env.clone();
            if let Some(bind_name) = &arm.bind {
                let payload_value = payload
                    .map(|p| *p)
                    .with_context(|| format!("tag `{tag}` had no payload for binding"))?;
                scoped.insert(bind_name.clone(), payload_value);
            }
            if let Some(v) = run_block(&arm.body, program, &mut scoped, config)? {
                return Ok(Some(v));
            }
            for (k, v) in scoped {
                env.insert(k, v);
            }
            Ok(None)
        }
    }
}

fn run_block(
    stmts: &[Statement],
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    for stmt in stmts {
        if let Some(v) = exec_statement(stmt, program, env, config)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn eval_i64(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<i64> {
    match eval_expr(expr, env)? {
        RuntimeValue::Int(i) => Ok(i),
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

fn eval_string(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<String> {
    match eval_expr(expr, env)? {
        RuntimeValue::Str(s) => Ok(s),
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

fn forwarded_args(
    call: &Call,
    sig: &crate::lower::FunctionSig,
    env: &HashMap<String, RuntimeValue>,
) -> Result<Vec<RuntimeValue>> {
    let FunctionBody::Wasm { wasm_args, .. } = &sig.body else {
        bail!("internal: forwarded_args on non-wasm function `{}`", sig.symbol);
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
            if let Some(v) = run_block(statements, program, &mut fn_env, config)? {
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
                    std::io::stdin().read_line(&mut line).context("stdin read_line")?;
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
                    Ok(RuntimeValue::Str(String::from_utf8_lossy(&buf[..n]).to_string()))
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
                    Ok(RuntimeValue::Int(i64::try_from(bytes.len()).unwrap_or(i64::MAX)))
                }

                "create-dir-all" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    fs::create_dir_all(p).context("create-dir-all")?;
                    Ok(RuntimeValue::Void)
                }
                "remove-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    fs::remove_file(p).context("remove-file")?;
                    Ok(RuntimeValue::Void)
                }
                "remove-dir" => {
                    let path = eval_domain_string(&call.args[0], env, "dir")?;
                    let p = resolve_preopen(&path, config)?;
                    fs::remove_dir_all(p).context("remove-dir")?;
                    Ok(RuntimeValue::Void)
                }
                "exists" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    Ok(RuntimeValue::Int(if p.exists() { 1 } else { 0 }))
                }
                "canonicalize" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    let c = p.canonicalize().context("canonicalize")?;
                    Ok(wrap_domain_string("path", c.display().to_string()))
                }
                "metadata" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    let md = fs::metadata(p).context("metadata")?;
                    Ok(RuntimeValue::Str(format!(
                        "size={},is_dir={}",
                        md.len(),
                        md.is_dir()
                    )))
                }
                "read-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let p = resolve_preopen(&path, config)?;
                    let s = fs::read_to_string(p).context("read-file")?;
                    Ok(RuntimeValue::Str(s))
                }
                "write-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let contents = eval_string(&call.args[1], env)?;
                    let p = resolve_preopen(&path, config)?;
                    fs::write(p, contents).context("write-file")?;
                    Ok(RuntimeValue::Void)
                }
                "append-file" => {
                    let path = eval_domain_string(&call.args[0], env, "path")?;
                    let contents = eval_string(&call.args[1], env)?;
                    let p = resolve_preopen(&path, config)?;
                    let mut f = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(p)
                        .context("append-file open")?;
                    f.write_all(contents.as_bytes())
                        .context("append-file write")?;
                    Ok(RuntimeValue::Void)
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
                        .read(true)
                        .write(true)
                        .open(p)
                        .context("open-path")?;
                    Ok(wrap_domain_int("fd", 1))
                }
                "read-dir" => {
                    let path = eval_domain_string(&call.args[0], env, "dir")?;
                    let p = resolve_preopen(&path, config)?;
                    let mut names = Vec::new();
                    for e in fs::read_dir(p).context("read-dir")? {
                        let e = e.context("read-dir entry")?;
                        names.push(e.file_name().to_string_lossy().to_string());
                    }
                    Ok(RuntimeValue::Str(names.join("\n")))
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
