//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::lower::{Call, Expr, LoweredProgram, RuntimeValue, Statement, WasmArgSpec};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    for stmt in &program.statements {
        match stmt {
            Statement::Call(call) => {
                let _ = exec_call(call, &env, config)?;
            }
            Statement::Let { var, call } => {
                let value = exec_call(call, &env, config)?;
                env.insert(var.clone(), value);
            }
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
    }
}

fn eval_i64(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<i64> {
    match eval_expr(expr, env)? {
        RuntimeValue::Int(i) => Ok(i),
        other => bail!("expected int, got {other:?}"),
    }
}

fn eval_string(expr: &Expr, env: &HashMap<String, RuntimeValue>) -> Result<String> {
    match eval_expr(expr, env)? {
        RuntimeValue::Str(s) => Ok(s),
        other => bail!("expected string, got {other:?}"),
    }
}

fn forwarded_args(call: &Call, env: &HashMap<String, RuntimeValue>) -> Result<Vec<RuntimeValue>> {
    let mut named: HashMap<&str, RuntimeValue> = HashMap::new();
    for (idx, name) in call.function.arg_names.iter().enumerate() {
        named.insert(name.as_str(), eval_expr(&call.args[idx], env)?);
    }

    let mut out = Vec::new();
    for spec in &call.function.wasm_args {
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

fn exec_call(call: &Call, env: &HashMap<String, RuntimeValue>, config: &RunConfig) -> Result<RuntimeValue> {
    let _forwarded = forwarded_args(call, env)?;
    let sym = call.function.symbol.as_str();

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
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            fs::create_dir_all(p).context("create-dir-all")?;
            Ok(RuntimeValue::Void)
        }
        "remove-file" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            fs::remove_file(p).context("remove-file")?;
            Ok(RuntimeValue::Void)
        }
        "remove-dir" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            fs::remove_dir_all(p).context("remove-dir")?;
            Ok(RuntimeValue::Void)
        }
        "exists" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            Ok(RuntimeValue::Int(if p.exists() { 1 } else { 0 }))
        }
        "canonicalize" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            let c = p.canonicalize().context("canonicalize")?;
            Ok(RuntimeValue::Str(c.display().to_string()))
        }
        "metadata" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            let md = fs::metadata(p).context("metadata")?;
            Ok(RuntimeValue::Str(format!(
                "size={},is_dir={}",
                md.len(),
                md.is_dir()
            )))
        }
        "read-file" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            let s = fs::read_to_string(p).context("read-file")?;
            Ok(RuntimeValue::Str(s))
        }
        "write-file" => {
            let path = eval_string(&call.args[0], env)?;
            let contents = eval_string(&call.args[1], env)?;
            let p = resolve_preopen(&path, config)?;
            fs::write(p, contents).context("write-file")?;
            Ok(RuntimeValue::Void)
        }
        "append-file" => {
            let path = eval_string(&call.args[0], env)?;
            let contents = eval_string(&call.args[1], env)?;
            let p = resolve_preopen(&path, config)?;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .context("append-file open")?;
            f.write_all(contents.as_bytes()).context("append-file write")?;
            Ok(RuntimeValue::Void)
        }
        "create-file" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            let _ = fs::File::create(p).context("create-file")?;
            Ok(RuntimeValue::Int(1))
        }
        "open-path" => {
            let path = eval_string(&call.args[0], env)?;
            let p = resolve_preopen(&path, config)?;
            let _ = fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(p)
                .context("open-path")?;
            Ok(RuntimeValue::Int(1))
        }
        "read-dir" => {
            let path = eval_string(&call.args[0], env)?;
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
                call.function.import.module,
                call.function.import.name
            )
        }
    }
}
