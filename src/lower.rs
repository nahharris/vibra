//! Generic lowering for stdlib-qualified calls with `$wasm.import` metadata.

use crate::load::{map_get_str, LoadedProgram};
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    Str(String),
    Void,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Value(RuntimeValue),
    VarRef(String),
}

#[derive(Debug, Clone)]
pub struct ImportTarget {
    pub module: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum WasmArgSpec {
    Arg(String),
    ConstInt(i64),
    ConstStr(String),
}

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub alias: String,
    pub symbol: String,
    pub arg_names: Vec<String>,
    pub return_type: String,
    pub import: ImportTarget,
    pub wasm_args: Vec<WasmArgSpec>,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub function: FunctionSig,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Call(Call),
    Let { var: String, call: Call },
}

#[derive(Debug, Clone)]
pub struct LoweredProgram {
    pub statements: Vec<Statement>,
    pub constants: HashMap<String, RuntimeValue>,
}

pub fn lower_program(program: &LoadedProgram) -> Result<LoweredProgram> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;

    let parent = program.entry.parent().context("entry path has no parent")?;
    let mut sigs: HashMap<String, FunctionSig> = HashMap::new();
    let mut constants: HashMap<String, RuntimeValue> = HashMap::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        if alias.starts_with('-') {
            continue;
        }
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else { continue };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = fs::canonicalize(parent.join(imp_s))
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        let imported = program
            .modules
            .get(&imported_path)
            .with_context(|| format!("imported module missing from graph `{alias}`"))?;
        collect_import_defs(alias, imported, &mut sigs, &mut constants)?;
    }

    let main = map_get_str(entry_map, "main").context("missing top-level `main`")?;
    let main_fn = map_get_str(
        main.as_mapping().context("`main` must be a mapping")?,
        "$function",
    )
    .context("`main` must be a `$function`")?;
    let fn_body = main_fn
        .as_mapping()
        .context("`$function` body must be mapping")?;

    let args = map_get_str(fn_body, "args").context("missing `args` on main")?;
    if !is_void_args(args) {
        let args_map = args
            .as_mapping()
            .context("`args` must be `$void` or an empty mapping for main")?;
        if !args_map.is_empty() {
            bail!("main must have no args (`args: $void`)");
        }
    }
    let ret = map_get_str(fn_body, "return").context("missing `return` on main")?;
    if ret.as_str() != Some("$void") {
        bail!("main must have return: $void");
    }

    let do_seq = map_get_str(fn_body, "do").context("missing `do` on main")?;
    let steps = do_seq.as_sequence().context("`do` must be sequence")?;
    let mut statements = Vec::new();
    for step in steps {
        statements.push(lower_statement(step, &sigs, &constants)?);
    }

    Ok(LoweredProgram {
        statements,
        constants,
    })
}

fn collect_import_defs(
    alias: &str,
    module_root: &Value,
    sigs: &mut HashMap<String, FunctionSig>,
    constants: &mut HashMap<String, RuntimeValue>,
) -> Result<()> {
    let map = module_root
        .as_mapping()
        .context("imported module root must be mapping")?;
    for (k, v) in map {
        let name = k.as_str().context("imported key must be string")?;
        if let Some(i) = v.as_i64() {
            constants.insert(format!("{alias}.{name}"), RuntimeValue::Int(i));
            continue;
        }
        if let Some(s) = v.as_str() {
            constants.insert(format!("{alias}.{name}"), RuntimeValue::Str(s.to_string()));
            continue;
        }
        let Some(def_map) = v.as_mapping() else { continue };
        let Some(fn_map) = map_get_str(def_map, "$function") else { continue };
        let body = fn_map
            .as_mapping()
            .with_context(|| format!("`{alias}.{name}` function body must be mapping"))?;
        let args = map_get_str(body, "args").context("function missing args")?;
        let mut arg_names = Vec::new();
        if !is_void_args(args) {
            let args_map = args
                .as_mapping()
                .context("function args must be `$void` or a mapping")?;
            for (ak, _) in args_map {
                arg_names.push(ak.as_str().context("arg name must be string")?.to_string());
            }
        }
        let ret = map_get_str(body, "return").context("function missing return")?;
        let return_type = ret
            .as_str()
            .context("return type must be string")?
            .to_string();
        let do_seq = map_get_str(body, "do").context("function missing do")?;
        let steps = do_seq.as_sequence().context("function do must be sequence")?;
        if steps.len() != 1 {
            bail!("{alias}.{name}: function do must contain one $wasm statement");
        }
        let stmt = steps[0].as_mapping().context("function statement must be mapping")?;
        let wasm = map_get_str(stmt, "$wasm").context("function do must contain $wasm")?;
        let wm = wasm.as_mapping().context("$wasm body must be mapping")?;
        let import = map_get_str(wm, "import").context("$wasm missing import")?;
        let im = import
            .as_mapping()
            .context("$wasm.import must be mapping")?;
        let module = map_get_str(im, "module")
            .context("$wasm.import missing module")?
            .as_str()
            .context("$wasm.import.module must be string")?
            .to_string();
        let import_name = map_get_str(im, "name")
            .context("$wasm.import missing name")?
            .as_str()
            .context("$wasm.import.name must be string")?
            .to_string();
        let wasm_args_v = map_get_str(wm, "args").context("$wasm missing args")?;
        let wasm_args_seq = wasm_args_v
            .as_sequence()
            .context("$wasm.args must be sequence")?;
        let mut wasm_args = Vec::new();
        for a in wasm_args_seq {
            wasm_args.push(parse_wasm_arg_spec(a)?);
        }
        sigs.insert(
            format!("{alias}.{name}"),
            FunctionSig {
                alias: alias.to_string(),
                symbol: name.to_string(),
                arg_names,
                return_type,
                import: ImportTarget {
                    module,
                    name: import_name,
                },
                wasm_args,
            },
        );
    }
    Ok(())
}

fn parse_wasm_arg_spec(v: &Value) -> Result<WasmArgSpec> {
    if let Some(i) = v.as_i64() {
        return Ok(WasmArgSpec::ConstInt(i));
    }
    let s = v
        .as_str()
        .context("wasm arg spec must be string or int literal")?;
    if let Some(name) = s.strip_prefix("$args.") {
        return Ok(WasmArgSpec::Arg(name.to_string()));
    }
    if let Some(n) = s.strip_prefix("$const.") {
        let i = n
            .parse::<i64>()
            .with_context(|| format!("invalid $const int `{n}`"))?;
        return Ok(WasmArgSpec::ConstInt(i));
    }
    Ok(WasmArgSpec::ConstStr(s.to_string()))
}

fn lower_statement(
    step: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
) -> Result<Statement> {
    let stmt = step
        .as_mapping()
        .context("statement must be a mapping")?;
    if stmt.len() != 1 {
        bail!("statement must have exactly one key");
    }
    let (k, v) = stmt.iter().next().expect("one key");
    let key = k.as_str().context("statement key must be string")?;
    if key == "$let" {
        let lm = v.as_mapping().context("$let must be mapping")?;
        if lm.len() != 1 {
            bail!("$let must define one variable");
        }
        let (vk, vv) = lm.iter().next().expect("let one");
        let var = vk.as_str().context("$let variable must be string")?.to_string();
        let call = parse_call(vv, sigs, constants)?;
        if call.function.return_type == "$void" {
            bail!("cannot bind void return in $let");
        }
        Ok(Statement::Let { var, call })
    } else {
        let call = parse_call(step, sigs, constants)?;
        Ok(Statement::Call(call))
    }
}

fn parse_call(
    call_mapping_value: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
) -> Result<Call> {
    let m = call_mapping_value
        .as_mapping()
        .context("call must be mapping")?;
    if m.len() != 1 {
        bail!("call mapping must contain one function invocation");
    }
    let (ck, av) = m.iter().next().expect("call one");
    let call_key = ck.as_str().context("call key must be string")?;
    let (alias, symbol) = parse_qualified_call(call_key)?;
    let sig_key = format!("{alias}.{symbol}");
    let function = sigs
        .get(&sig_key)
        .with_context(|| format!("unknown imported symbol `{sig_key}`"))?
        .clone();
    let args = parse_call_args(av, &function.arg_names, constants)?;
    Ok(Call { function, args })
}

fn parse_call_args(
    av: &Value,
    arg_names: &[String],
    constants: &HashMap<String, RuntimeValue>,
) -> Result<Vec<Expr>> {
    if arg_names.is_empty() {
        if is_void_args(av) {
            return Ok(Vec::new());
        }
        if let Some(m) = av.as_mapping() {
            if !m.is_empty() {
                bail!("expected no args");
            }
        }
        return Ok(Vec::new());
    }
    if arg_names.len() == 1 && !av.is_mapping() {
        return Ok(vec![parse_expr(av, constants)?]);
    }
    let map = av
        .as_mapping()
        .context("expected mapping arguments for multi-arg call")?;
    let mut out = Vec::with_capacity(arg_names.len());
    for n in arg_names {
        let v = map
            .get(Value::String(n.clone()))
            .with_context(|| format!("missing argument `{n}`"))?;
        out.push(parse_expr(v, constants)?);
    }
    Ok(out)
}

fn parse_expr(v: &Value, constants: &HashMap<String, RuntimeValue>) -> Result<Expr> {
    if let Some(i) = v.as_i64() {
        return Ok(Expr::Value(RuntimeValue::Int(i)));
    }
    if let Some(s) = v.as_str() {
        if let Some(var) = s.strip_prefix('$') {
            if let Some(c) = constants.get(var) {
                return Ok(Expr::Value(c.clone()));
            }
            return Ok(Expr::VarRef(var.to_string()));
        }
        return Ok(Expr::Value(RuntimeValue::Str(s.to_string())));
    }
    bail!("unsupported expression: expected int/string/$var")
}

fn is_void_args(v: &Value) -> bool {
    matches!(v.as_str(), Some("$void"))
}

fn parse_qualified_call(call: &str) -> Result<(&str, &str)> {
    let rest = call
        .strip_prefix('$')
        .context("call key must start with `$`")?;
    let (a, b) = rest
        .split_once('.')
        .context("expected qualified call `$alias.symbol`")?;
    if a.is_empty() || b.is_empty() {
        bail!("invalid qualified call `{call}`");
    }
    Ok((a, b))
}
