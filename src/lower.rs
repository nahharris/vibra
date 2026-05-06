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
    Union {
        union_key: String,
        variant: String,
        payload: Option<Box<RuntimeValue>>,
    },
    Void,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Value(RuntimeValue),
    VarRef(String),
    Constructor {
        union_key: String,
        variant: String,
        payload: Option<Box<Expr>>,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Void,
    Named(String),
    Generic(String),
}

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub alias: String,
    pub symbol: String,
    pub arg_names: Vec<String>,
    pub arg_types: Vec<TypeRef>,
    pub return_type: TypeRef,
    pub import: ImportTarget,
    pub wasm_args: Vec<WasmArgSpec>,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub function: FunctionSig,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub variant: String,
    pub bind: Option<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum LetValue {
    Call(Call),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Call(Call),
    Let { var: String, value: LetValue },
    Match {
        target: Expr,
        union_key: String,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone)]
pub struct UnionVariantDef {
    pub payload_type: Option<TypeRef>,
}

#[derive(Debug, Clone)]
pub struct UnionDef {
    pub alias: String,
    pub name: String,
    pub variants: HashMap<String, UnionVariantDef>,
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
    let mut unions: HashMap<String, UnionDef> = HashMap::new();

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
        collect_import_defs(alias, imported, &mut sigs, &mut constants, &mut unions)?;
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
    let mut locals = HashMap::new();
    for step in steps {
        statements.push(lower_statement(
            step,
            &sigs,
            &constants,
            &unions,
            &mut locals,
        )?);
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
    unions: &mut HashMap<String, UnionDef>,
) -> Result<()> {
    let map = module_root
        .as_mapping()
        .context("imported module root must be mapping")?;
    for (k, v) in map {
        let name = k.as_str().context("imported key must be string")?;
        let Some(def_map) = v.as_mapping() else { continue };
        let Some(union_map) = map_get_str(def_map, "$union") else {
            continue;
        };
        let union = parse_union_decl(alias, name, union_map)
            .with_context(|| format!("invalid union declaration `{alias}.{name}`"))?;
        unions.insert(format!("{alias}.{name}"), union);
    }
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
        let (arg_names, arg_types) = parse_signature_args(args)
            .with_context(|| format!("{alias}.{name}: invalid function args"))?;
        let arg_types = arg_types
            .into_iter()
            .map(|t| qualify_named_type(alias, t, unions))
            .collect();
        let ret = map_get_str(body, "return").context("function missing return")?;
        let return_type = qualify_named_type(
            alias,
            parse_type_ref(ret)
                .with_context(|| format!("{alias}.{name}: invalid function return type"))?,
            unions,
        );
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
                arg_types,
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
    unions: &HashMap<String, UnionDef>,
    locals: &mut HashMap<String, TypeRef>,
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
        if looks_like_call(vv, sigs) {
            let call = parse_call(vv, sigs, constants, unions, locals)?;
            if call.function.return_type == TypeRef::Void {
                bail!("cannot bind void return in $let");
            }
            locals.insert(var.clone(), call.function.return_type.clone());
            Ok(Statement::Let {
                var,
                value: LetValue::Call(call),
            })
        } else {
            let expr = parse_expr(vv, constants, unions, locals)?;
            let expr_ty = infer_expr_type(&expr, constants, locals)
                .context("could not infer type for $let expression")?;
            if expr_ty == TypeRef::Void {
                bail!("cannot bind void expression in $let");
            }
            locals.insert(var.clone(), expr_ty);
            Ok(Statement::Let {
                var,
                value: LetValue::Expr(expr),
            })
        }
    } else if key == "$match" {
        parse_match_statement(v, sigs, constants, unions, locals)
    } else {
        let call = parse_call(step, sigs, constants, unions, locals)?;
        Ok(Statement::Call(call))
    }
}

fn parse_call(
    call_mapping_value: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    unions: &HashMap<String, UnionDef>,
    locals: &HashMap<String, TypeRef>,
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
    let args = parse_call_args(av, &function.arg_names, constants, unions, locals)?;
    for (idx, expr) in args.iter().enumerate() {
        let expected = &function.arg_types[idx];
        let Some(actual) = infer_expr_type(expr, constants, locals) else {
            continue;
        };
        if !type_compatible(expected, &actual) {
            bail!(
                "type mismatch in call `{sig_key}` arg `{}`: expected {:?}, got {:?}",
                function.arg_names[idx],
                expected,
                actual
            );
        }
    }
    Ok(Call { function, args })
}

fn parse_call_args(
    av: &Value,
    arg_names: &[String],
    constants: &HashMap<String, RuntimeValue>,
    unions: &HashMap<String, UnionDef>,
    locals: &HashMap<String, TypeRef>,
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
        return Ok(vec![parse_expr(av, constants, unions, locals)?]);
    }
    if arg_names.len() == 1 {
        if let Some(map) = av.as_mapping() {
            let only_key_constructor = map.len() == 1
                && map
                    .iter()
                    .next()
                    .and_then(|(k, _)| k.as_str())
                    .map(|s| s.starts_with('$'))
                    .unwrap_or(false);
            if only_key_constructor {
                return Ok(vec![parse_expr(av, constants, unions, locals)?]);
            }
        }
    }
    let map = av
        .as_mapping()
        .context("expected mapping arguments for multi-arg call")?;
    let mut out = Vec::with_capacity(arg_names.len());
    for n in arg_names {
        let v = map
            .get(Value::String(n.clone()))
            .with_context(|| format!("missing argument `{n}`"))?;
        out.push(parse_expr(v, constants, unions, locals)?);
    }
    Ok(out)
}

fn parse_expr(
    v: &Value,
    constants: &HashMap<String, RuntimeValue>,
    unions: &HashMap<String, UnionDef>,
    locals: &HashMap<String, TypeRef>,
) -> Result<Expr> {
    if let Some(m) = v.as_mapping() {
        if m.len() == 1 {
            let (k, payload_v) = m.iter().next().expect("one key");
            if let Some(constructor) = k.as_str() {
                if constructor.starts_with('$') {
                    let (union_key, variant) = parse_qualified_variant(constructor)?;
                    let union = unions
                        .get(&union_key)
                        .with_context(|| format!("unknown union `{union_key}` in constructor `{constructor}`"))?;
                    let variant_def = union
                        .variants
                        .get(variant)
                        .with_context(|| format!("unknown variant `{variant}` for union `{union_key}`"))?;
                    return match &variant_def.payload_type {
                        Some(expected_ty) => {
                            let payload_expr = parse_expr(payload_v, constants, unions, locals)?;
                            if let Some(actual_ty) = infer_expr_type(&payload_expr, constants, locals) {
                                if !type_compatible(expected_ty, &actual_ty) {
                                    bail!(
                                        "constructor `{constructor}` payload type mismatch: expected {:?}, got {:?}",
                                        expected_ty,
                                        actual_ty
                                    );
                                }
                            }
                            Ok(Expr::Constructor {
                                union_key,
                                variant: variant.to_string(),
                                payload: Some(Box::new(payload_expr)),
                            })
                        }
                        None => {
                            if !is_void_args(payload_v) {
                                bail!(
                                    "constructor `{constructor}` does not take payload; use `$void`"
                                );
                            }
                            Ok(Expr::Constructor {
                                union_key,
                                variant: variant.to_string(),
                                payload: None,
                            })
                        }
                    };
                }
            }
        }
    }

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
    bail!("unsupported expression: expected int/string/$var or constructor")
}

fn is_void_args(v: &Value) -> bool {
    matches!(v.as_str(), Some("$void"))
}

fn parse_signature_args(v: &Value) -> Result<(Vec<String>, Vec<TypeRef>)> {
    if is_void_args(v) {
        return Ok((Vec::new(), Vec::new()));
    }

    let args_map = v
        .as_mapping()
        .context("function args must be `$void` or a mapping of arg->type")?;
    if args_map.is_empty() {
        bail!("zero-arg functions must use `args: $void`");
    }

    let mut arg_names = Vec::with_capacity(args_map.len());
    let mut arg_types = Vec::with_capacity(args_map.len());
    for (k, t) in args_map {
        let arg_name = k.as_str().context("arg name must be string")?.to_string();
        let arg_type = parse_type_ref(t).with_context(|| format!("invalid type for arg `{arg_name}`"))?;
        arg_names.push(arg_name);
        arg_types.push(arg_type);
    }

    Ok((arg_names, arg_types))
}

fn parse_union_decl(alias: &str, name: &str, union_map: &Value) -> Result<UnionDef> {
    let union_body = union_map
        .as_mapping()
        .context("$union body must be mapping")?;
    let variants_v = map_get_str(union_body, "variants").context("$union missing variants")?;
    let variants_m = variants_v
        .as_mapping()
        .context("$union.variants must be mapping")?;
    if variants_m.is_empty() {
        bail!("$union.variants must not be empty");
    }

    let mut variants = HashMap::new();
    for (k, v) in variants_m {
        let variant = k.as_str().context("union variant name must be string")?;
        let payload_ty = parse_type_ref(v).with_context(|| format!("invalid payload type for variant `{variant}`"))?;
        variants.insert(
            variant.to_string(),
            UnionVariantDef {
                payload_type: if payload_ty == TypeRef::Void {
                    None
                } else {
                    Some(payload_ty)
                },
            },
        );
    }

    Ok(UnionDef {
        alias: alias.to_string(),
        name: name.to_string(),
        variants,
    })
}

fn parse_type_ref(v: &Value) -> Result<TypeRef> {
    let raw = v.as_str().context("type annotation must be string")?;
    let name = raw
        .strip_prefix('$')
        .with_context(|| format!("type annotation `{raw}` must start with `$`"))?;
    let ty = match name {
        "int8" => TypeRef::Int8,
        "int16" => TypeRef::Int16,
        "int32" => TypeRef::Int32,
        "int64" => TypeRef::Int64,
        "uint8" => TypeRef::UInt8,
        "uint16" => TypeRef::UInt16,
        "uint32" => TypeRef::UInt32,
        "uint64" => TypeRef::UInt64,
        "float32" => TypeRef::Float32,
        "float64" => TypeRef::Float64,
        "void" => TypeRef::Void,
        _ => {
            let first = name
                .chars()
                .next()
                .context("type annotation cannot be empty")?;
            if name.len() == 1 && first.is_ascii_uppercase() {
                TypeRef::Generic(name.to_string())
            } else {
                TypeRef::Named(name.to_string())
            }
        }
    };
    Ok(ty)
}

fn qualify_named_type(alias: &str, ty: TypeRef, unions: &HashMap<String, UnionDef>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => {
            if name.contains('.') {
                TypeRef::Named(name)
            } else {
                let maybe_union = format!("{alias}.{name}");
                if unions.contains_key(&maybe_union) {
                    TypeRef::Named(maybe_union)
                } else {
                    TypeRef::Named(name)
                }
            }
        }
        _ => ty,
    }
}

fn infer_expr_type(
    expr: &Expr,
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
) -> Option<TypeRef> {
    match expr {
        Expr::Value(RuntimeValue::Int(_)) => Some(TypeRef::Named("int".to_string())),
        Expr::Value(RuntimeValue::Str(_)) => Some(TypeRef::Named("str".to_string())),
        Expr::Value(RuntimeValue::Void) => Some(TypeRef::Void),
        Expr::Value(RuntimeValue::Union { union_key, .. }) => Some(TypeRef::Named(union_key.clone())),
        Expr::VarRef(v) => locals
            .get(v)
            .cloned()
            .or_else(|| constants.get(v).and_then(|rv| infer_expr_type(&Expr::Value(rv.clone()), constants, locals))),
        Expr::Constructor { union_key, .. } => Some(TypeRef::Named(union_key.clone())),
    }
}

fn is_numeric_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Int8
            | TypeRef::Int16
            | TypeRef::Int32
            | TypeRef::Int64
            | TypeRef::UInt8
            | TypeRef::UInt16
            | TypeRef::UInt32
            | TypeRef::UInt64
            | TypeRef::Float32
            | TypeRef::Float64
    )
}

fn type_compatible(expected: &TypeRef, actual: &TypeRef) -> bool {
    if expected == actual {
        return true;
    }
    match (expected, actual) {
        (TypeRef::Generic(_), _) => true,
        (TypeRef::Named(e), a) if e == "int" && is_numeric_type(a) => true,
        (e, TypeRef::Named(a)) if a == "int" && is_numeric_type(e) => true,
        (TypeRef::Named(e), TypeRef::Named(a))
            if strip_module_prefix(e) == strip_module_prefix(a) =>
        {
            true
        }
        (TypeRef::Named(e), TypeRef::Named(a)) if e == a => true,
        _ => false,
    }
}

fn strip_module_prefix(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn parse_match_statement(
    match_body: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    unions: &HashMap<String, UnionDef>,
    locals: &HashMap<String, TypeRef>,
) -> Result<Statement> {
    let m = match_body.as_mapping().context("$match must be mapping")?;
    let target_v = map_get_str(m, "target").context("$match missing target")?;
    let target = parse_expr(target_v, constants, unions, locals)?;
    let target_ty = infer_expr_type(&target, constants, locals)
        .context("$match target type could not be inferred; provide union variable context")?;
    let TypeRef::Named(union_key) = target_ty else {
        bail!("$match target must be a union type, got {target_ty:?}");
    };
    let union = unions
        .get(&union_key)
        .or_else(|| {
            unions
                .iter()
                .find(|(k, _)| strip_module_prefix(k) == strip_module_prefix(&union_key))
                .map(|(_, v)| v)
        })
        .with_context(|| format!("$match target `{union_key}` is not a known union"))?;

    let arms_v = map_get_str(m, "arms").context("$match missing arms")?;
    let arms_m = arms_v.as_mapping().context("$match arms must be mapping")?;
    let mut seen = HashMap::new();
    let mut arms = Vec::new();
    for (k, v) in arms_m {
        let variant = k.as_str().context("$match arm key must be string")?;
        let variant_def = union
            .variants
            .get(variant)
            .with_context(|| format!("unknown variant `{variant}` for union `{union_key}`"))?;
        if seen.insert(variant.to_string(), true).is_some() {
            bail!("duplicate $match arm for variant `{variant}`");
        }

        let arm_map = v
            .as_mapping()
            .with_context(|| format!("$match arm `{variant}` must be mapping"))?;
        let bind = map_get_str(arm_map, "bind")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        match (&variant_def.payload_type, &bind) {
            (Some(_), None) => bail!("$match arm `{variant}` must bind payload"),
            (None, Some(_)) => bail!("$match arm `{variant}` cannot bind payload (variant has none)"),
            _ => {}
        }

        let mut scoped_locals = locals.clone();
        if let (Some(payload_ty), Some(bind_name)) = (&variant_def.payload_type, &bind) {
            scoped_locals.insert(bind_name.clone(), payload_ty.clone());
        }

        let do_v = map_get_str(arm_map, "do")
            .with_context(|| format!("$match arm `{variant}` missing do"))?;
        let do_seq = do_v
            .as_sequence()
            .with_context(|| format!("$match arm `{variant}` do must be sequence"))?;
        let mut body = Vec::new();
        for step in do_seq {
            body.push(lower_statement(
                step,
                sigs,
                constants,
                unions,
                &mut scoped_locals,
            )?);
        }
        arms.push(MatchArm {
            variant: variant.to_string(),
            bind,
            body,
        });
    }

    for variant in union.variants.keys() {
        if !seen.contains_key(variant) {
            bail!("$match for union `{union_key}` missing arm for variant `{variant}`");
        }
    }

    Ok(Statement::Match {
        target,
        union_key,
        arms,
    })
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

fn parse_qualified_variant(raw: &str) -> Result<(String, &str)> {
    let rest = raw
        .strip_prefix('$')
        .context("constructor key must start with `$`")?;
    let mut parts = rest.split('.');
    let alias = parts
        .next()
        .context("constructor must be `$alias.Union.Variant`")?;
    let union = parts
        .next()
        .context("constructor must include union name")?;
    let variant = parts
        .next()
        .context("constructor must include variant name")?;
    if parts.next().is_some() {
        bail!("constructor `{raw}` has too many segments");
    }
    if alias.is_empty() || union.is_empty() || variant.is_empty() {
        bail!("invalid constructor `{raw}`");
    }
    Ok((format!("{alias}.{union}"), variant))
}

fn looks_like_call(v: &Value, sigs: &HashMap<String, FunctionSig>) -> bool {
    let Some(m) = v.as_mapping() else { return false };
    if m.len() != 1 {
        return false;
    }
    let Some((k, _)) = m.iter().next() else {
        return false;
    };
    let Some(raw) = k.as_str() else {
        return false;
    };
    let Ok((alias, symbol)) = parse_qualified_call(raw) else {
        return false;
    };
    sigs.contains_key(&format!("{alias}.{symbol}"))
}
