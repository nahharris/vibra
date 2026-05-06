//! Generic lowering for stdlib-qualified calls with `$wasm.import` metadata.

use crate::load::{map_get_str, LoadedProgram};
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    Str(String),
    Enum {
        enum_key: String,
        tag: String,
        payload: Option<Box<RuntimeValue>>,
    },
    Void,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Value(RuntimeValue),
    VarRef(String),
    EnumConstructor {
        enum_key: String,
        tag: String,
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
    /// Enum type at a use site with concrete or partially inferred type arguments (ordered like `EnumDef.type_params`).
    Instantiated {
        base: String,
        type_args: Vec<TypeRef>,
    },
    Union(Vec<TypeRef>),
    Enum(HashMap<String, TypeRef>),
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
    pub tag: String,
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
        enum_key: String,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub alias: String,
    pub name: String,
    /// Type parameter names from the innermost `$forall` wrapping this enum, in declaration order.
    pub type_params: Vec<String>,
    pub tags: HashMap<String, TypeRef>,
}

#[derive(Debug, Clone)]
pub struct LoweredProgram {
    pub statements: Vec<Statement>,
    pub constants: HashMap<String, RuntimeValue>,
    pub warnings: Vec<String>,
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
    let mut type_aliases: HashMap<String, TypeRef> = HashMap::new();
    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let mut warnings = Vec::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        if alias.starts_with('-') {
            continue;
        }
        maybe_warn_kebab(alias, "import alias", &mut warnings);
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else { continue };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = fs::canonicalize(parent.join(imp_s))
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        let imported = program
            .modules
            .get(&imported_path)
            .with_context(|| format!("imported module missing from graph `{alias}`"))?;
        collect_import_defs(
            alias,
            imported,
            &mut sigs,
            &mut constants,
            &mut type_aliases,
            &mut enums,
            &mut warnings,
        )?;
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
            &type_aliases,
            &enums,
            &mut locals,
            &mut warnings,
        )?);
    }

    Ok(LoweredProgram {
        statements,
        constants,
        warnings,
    })
}

/// Parses `$forall: { types: [...], in: ..., where?: ... }`. `where` is accepted and ignored.
fn parse_forall_header<'a>(
    fv: &'a Value,
    warnings: &mut Vec<String>,
) -> Result<(Vec<String>, &'a Value)> {
    let body = fv
        .as_mapping()
        .context("`$forall` value must be a mapping")?;
    let types_v = map_get_str(body, "types").context("`$forall` missing `types`")?;
    let types_seq = types_v
        .as_sequence()
        .context("`$forall.types` must be an array of strings")?;
    if types_seq.is_empty() {
        bail!("`$forall.types` must contain at least one type parameter");
    }
    let mut types = Vec::with_capacity(types_seq.len());
    for t in types_seq {
        let s = t.as_str().context("`$forall.types` entries must be strings")?;
        maybe_warn_kebab(s, "type parameter", warnings);
        types.push(s.to_string());
    }
    if map_get_str(body, "where").is_some() {
        // Reserved for future bounds; ignored in v1.
    }
    // Reject legacy field names explicitly.
    if map_get_str(body, "params").is_some()
        || map_get_str(body, "bounds").is_some()
        || map_get_str(body, "body").is_some()
    {
        bail!("`$forall` uses `types` and `in`; legacy keys `params`, `bounds`, and `body` are not supported");
    }
    let in_v = map_get_str(body, "in").context("`$forall` missing `in`")?;
    Ok((types, in_v))
}

/// Peels leading `$forall` layers. Returns `(scope_inner_first, innermost_forall_types, inner_mapping)`.
fn unwrap_forall_on_def<'a>(
    mut cur: &'a Value,
    warnings: &mut Vec<String>,
) -> Result<(Vec<String>, Vec<String>, &'a serde_yaml::Mapping)> {
    let mut scope: Vec<String> = Vec::new();
    let mut innermost_types: Vec<String> = Vec::new();
    loop {
        let m = cur
            .as_mapping()
            .context("imported type definition must be a mapping")?;
        if let Some(fv) = map_get_str(m, "$forall") {
            let (types, in_v) = parse_forall_header(fv, warnings)?;
            innermost_types = types.clone();
            for t in types.iter().rev() {
                scope.retain(|x| x != t);
                scope.insert(0, t.clone());
            }
            cur = in_v;
            continue;
        }
        return Ok((scope, innermost_types, m));
    }
}

fn substitute_type(ty: &TypeRef, subst: &HashMap<String, TypeRef>) -> TypeRef {
    match ty {
        TypeRef::Generic(n) => subst.get(n.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        TypeRef::Union(items) => TypeRef::Union(
            items
                .iter()
                .map(|t| substitute_type(t, subst))
                .collect(),
        ),
        TypeRef::Enum(tags) => {
            let mut out = HashMap::new();
            for (k, v) in tags {
                out.insert(k.clone(), substitute_type(v, subst));
            }
            TypeRef::Enum(out)
        }
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type(t, subst))
                .collect(),
        },
        _ => ty.clone(),
    }
}

/// Unify `expected` with `actual`, recording generic instantiations in `bindings`.
fn unify_types(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeRef>,
    bindings: &mut HashMap<String, TypeRef>,
) -> bool {
    let expected_n = normalize_type_ref(expected, aliases);
    let actual_n = normalize_type_ref(actual, aliases);

    if expected_n == actual_n {
        return true;
    }

    if let TypeRef::Generic(name) = &expected_n {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&bound, &actual_n, aliases, bindings);
        }
        bindings.insert(name.clone(), actual_n.clone());
        return true;
    }
    if let TypeRef::Generic(name) = &actual_n {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&expected_n, &bound, aliases, bindings);
        }
        bindings.insert(name.clone(), expected_n.clone());
        return true;
    }

    match (&expected_n, &actual_n) {
        (
            TypeRef::Instantiated {
                base: b1,
                type_args: a1,
            },
            TypeRef::Instantiated {
                base: b2,
                type_args: a2,
            },
        ) => {
            if b1 != b2 || a1.len() != a2.len() {
                return false;
            }
            a1.iter()
                .zip(a2.iter())
                .all(|(e, a)| unify_types(e, a, aliases, bindings))
        }
        (TypeRef::Union(opts), a) => opts
            .iter()
            .any(|c| unify_types(c, a, aliases, bindings)),
        (e, a) if is_numeric_type(e) && is_numeric_type(a) => true,
        (TypeRef::Named(e), TypeRef::Named(a))
            if strip_module_prefix(e) == strip_module_prefix(a) =>
        {
            true
        }
        (TypeRef::Enum(e), TypeRef::Enum(a)) => e == a,
        (TypeRef::Named(e), TypeRef::Named(a)) if e == a => true,
        _ => false,
    }
}

fn instantiated_type_for_constructor(
    enum_key: &str,
    enum_def: &EnumDef,
    tag: &str,
    payload_expr: Option<&Expr>,
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let payload_ty = enum_def.tags.get(tag)?;
    let mut bindings: HashMap<String, TypeRef> = HashMap::new();
    match (payload_expr, payload_ty) {
        (None, t) if *t == TypeRef::Void => {}
        (Some(pl), t) if *t != TypeRef::Void => {
            let actual = infer_expr_type(pl, constants, locals, aliases, enums)?;
            if !unify_types(t, &actual, aliases, &mut bindings) {
                return None;
            }
        }
        _ => return None,
    }
    let type_args: Vec<TypeRef> = enum_def
        .type_params
        .iter()
        .map(|p| {
            bindings
                .get(p)
                .cloned()
                .unwrap_or_else(|| TypeRef::Generic(p.clone()))
        })
        .collect();
    Some(TypeRef::Instantiated {
        base: enum_key.to_string(),
        type_args,
    })
}

fn collect_import_defs(
    alias: &str,
    module_root: &Value,
    sigs: &mut HashMap<String, FunctionSig>,
    constants: &mut HashMap<String, RuntimeValue>,
    type_aliases: &mut HashMap<String, TypeRef>,
    enums: &mut HashMap<String, EnumDef>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let map = module_root
        .as_mapping()
        .context("imported module root must be mapping")?;
    for (k, v) in map {
        let name = k.as_str().context("imported key must be string")?;
        maybe_warn_kebab(name, "top-level symbol", warnings);
        if let Some(def_map) = v.as_mapping() {
            let (scope, innermost_forall_types, inner_map) =
                unwrap_forall_on_def(v, warnings).with_context(|| {
                    format!("invalid `$forall` or type wrapper for `{alias}.{name}`")
                })?;
            if let Some(union_v) = map_get_str(inner_map, "$union") {
                let parsed = parse_union_type(union_v, &scope, warnings)
                    .with_context(|| format!("invalid union declaration `{alias}.{name}`"))?;
                let ty = qualify_named_type(alias, parsed, type_aliases);
                type_aliases.insert(format!("{alias}.{name}"), ty);
                continue;
            }
            if let Some(enum_v) = map_get_str(inner_map, "$enum") {
                let mut enum_def = parse_enum_decl(
                    alias,
                    name,
                    enum_v,
                    &innermost_forall_types,
                    &scope,
                    warnings,
                )
                .with_context(|| format!("invalid enum declaration `{alias}.{name}`"))?;
                for payload in enum_def.tags.values_mut() {
                    *payload = qualify_named_type(alias, payload.clone(), type_aliases);
                }
                type_aliases.insert(
                    format!("{alias}.{name}"),
                    TypeRef::Enum(enum_def.tags.clone()),
                );
                enums.insert(format!("{alias}.{name}"), enum_def);
                continue;
            }
            if !innermost_forall_types.is_empty()
                && map_get_str(inner_map, "$union").is_none()
                && map_get_str(inner_map, "$enum").is_none()
            {
                bail!(
                    "`$forall` on `{alias}.{name}` must wrap `$enum` or `$union` in `in`"
                );
            }
            if map_get_str(def_map, "variants").is_some() {
                bail!(
                    "legacy `variants` union syntax was removed; use `$union: [...]` or `$enum: {{...}}`"
                );
            }
        }
    }
    for (k, v) in map {
        let name = k.as_str().context("imported key must be string")?;
        if let Some(i) = v.as_i64() {
            constants.insert(format!("{alias}.{name}"), RuntimeValue::Int(i));
            continue;
        }
        if let Some(f) = v.as_f64() {
            constants.insert(format!("{alias}.{name}"), RuntimeValue::Float(f));
            continue;
        }
        if let Some(s) = v.as_str() {
            constants.insert(format!("{alias}.{name}"), RuntimeValue::Str(s.to_string()));
            continue;
        }
        let Some(def_map) = v.as_mapping() else { continue };
        let Some(fn_map) = map_get_str(def_map, "$function") else { continue };
        maybe_warn_kebab(name, "function name", warnings);
        let body = fn_map
            .as_mapping()
            .with_context(|| format!("`{alias}.{name}` function body must be mapping"))?;
        let args = map_get_str(body, "args").context("function missing args")?;
        let (arg_names, arg_types) = parse_signature_args(args, warnings)
            .with_context(|| format!("{alias}.{name}: invalid function args"))?;
        let arg_types = arg_types
            .into_iter()
            .map(|t| qualify_named_type(alias, t, type_aliases))
            .collect();
        let ret = map_get_str(body, "return").context("function missing return")?;
        let return_type = qualify_named_type(
            alias,
            parse_type_ref(ret, &[], warnings)
                .with_context(|| format!("{alias}.{name}: invalid function return type"))?,
            type_aliases,
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
    type_aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
    locals: &mut HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
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
        maybe_warn_kebab(&var, "local variable", warnings);
        if looks_like_call(vv, sigs) {
            let call = parse_call(vv, sigs, constants, type_aliases, enums, locals, warnings)?;
            if call.function.return_type == TypeRef::Void {
                bail!("cannot bind void return in $let");
            }
            locals.insert(var.clone(), call.function.return_type.clone());
            Ok(Statement::Let {
                var,
                value: LetValue::Call(call),
            })
        } else {
            let expr = parse_expr(vv, constants, type_aliases, enums, locals, warnings)?;
            let expr_ty = infer_expr_type(&expr, constants, locals, type_aliases, enums)
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
        parse_match_statement(
            v,
            sigs,
            constants,
            type_aliases,
            enums,
            locals,
            warnings,
        )
    } else {
        let call = parse_call(step, sigs, constants, type_aliases, enums, locals, warnings)?;
        Ok(Statement::Call(call))
    }
}

fn parse_call(
    call_mapping_value: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
    locals: &HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
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
    let args = parse_call_args(
        av,
        &function.arg_names,
        constants,
        type_aliases,
        enums,
        locals,
        warnings,
    )?;
    for (idx, expr) in args.iter().enumerate() {
        let expected = &function.arg_types[idx];
        let Some(actual) = infer_expr_type(expr, constants, locals, type_aliases, enums) else {
            continue;
        };
        if !type_compatible(expected, &actual, type_aliases) {
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
    type_aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
    locals: &HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
) -> Result<Vec<Expr>> {
    if arg_names.is_empty() {
        if av.is_null() {
            return Ok(Vec::new());
        }
        if let Some(m) = av.as_mapping() {
            if !m.is_empty() {
                bail!("expected no args");
            }
        }
        bail!("zero-arg call payload must be `null`");
    }
    if arg_names.len() == 1 && !av.is_mapping() {
        return Ok(vec![parse_expr(
            av,
            constants,
            type_aliases,
            enums,
            locals,
            warnings,
        )?]);
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
                return Ok(vec![parse_expr(
                    av,
                    constants,
                    type_aliases,
                    enums,
                    locals,
                    warnings,
                )?]);
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
        maybe_warn_kebab(n, "argument key", warnings);
        out.push(parse_expr(
            v,
            constants,
            type_aliases,
            enums,
            locals,
            warnings,
        )?);
    }
    Ok(out)
}

fn parse_expr(
    v: &Value,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
    locals: &HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
) -> Result<Expr> {
    if let Some(m) = v.as_mapping() {
        if m.len() == 1 {
            let (k, payload_v) = m.iter().next().expect("one key");
            if let Some(constructor) = k.as_str() {
                if constructor.starts_with('$') {
                    let (enum_key, tag) = resolve_enum_tag_ref(constructor, enums)?;
                    maybe_warn_kebab(&tag, "enum tag", warnings);
                    let enum_def = enums
                        .get(&enum_key)
                        .with_context(|| format!("unknown enum `{enum_key}` in constructor `{constructor}`"))?;
                    let payload_ty = enum_def
                        .tags
                        .get(&tag)
                        .with_context(|| format!("unknown enum tag `{tag}` for enum `{enum_key}`"))?;
                    return if *payload_ty == TypeRef::Void {
                        if !payload_v.is_null() {
                            bail!(
                                "constructor `{constructor}` tag `{tag}` does not take payload; use `null`"
                            );
                        }
                        Ok(Expr::EnumConstructor {
                            enum_key,
                            tag: tag.to_string(),
                            payload: None,
                        })
                    } else {
                        let payload_expr = parse_expr(
                            payload_v,
                            constants,
                            type_aliases,
                            enums,
                            locals,
                            warnings,
                        )?;
                        if let Some(actual_ty) = infer_expr_type(
                            &payload_expr,
                            constants,
                            locals,
                            type_aliases,
                            enums,
                        ) {
                            if !type_compatible(payload_ty, &actual_ty, type_aliases) {
                                bail!(
                                    "constructor `{constructor}` payload type mismatch: expected {:?}, got {:?}",
                                    payload_ty,
                                    actual_ty
                                );
                            }
                        }
                        Ok(Expr::EnumConstructor {
                            enum_key,
                            tag: tag.to_string(),
                            payload: Some(Box::new(payload_expr)),
                        })
                    };
                }
            }
        }
    }

    if v.is_null() {
        return Ok(Expr::Value(RuntimeValue::Void));
    }
    if let Some(i) = v.as_i64() {
        return Ok(Expr::Value(RuntimeValue::Int(i)));
    }
    if let Some(f) = v.as_f64() {
        return Ok(Expr::Value(RuntimeValue::Float(f)));
    }
    if let Some(s) = v.as_str() {
        if let Some(var) = s.strip_prefix('$') {
            maybe_warn_kebab_qualified(var, "symbol reference", warnings);
            if let Ok((enum_key, tag)) = resolve_enum_tag_ref(s, enums) {
                let enum_def = enums
                    .get(&enum_key)
                    .with_context(|| format!("unknown enum `{enum_key}` in constructor `{s}`"))?;
                let payload_ty = enum_def
                    .tags
                    .get(tag.as_str())
                    .with_context(|| format!("unknown enum tag `{tag}` for enum `{enum_key}`"))?;
                if *payload_ty == TypeRef::Void {
                    return Ok(Expr::EnumConstructor {
                        enum_key,
                        tag,
                        payload: None,
                    });
                }
                bail!(
                    "constructor `{s}` requires payload; use mapping form `{{{s}: ...}}`"
                );
            }
            if let Some(c) = constants.get(var) {
                return Ok(Expr::Value(c.clone()));
            }
            return Ok(Expr::VarRef(var.to_string()));
        }
        return Ok(Expr::Value(RuntimeValue::Str(s.to_string())));
    }
    bail!("unsupported expression: expected null/number/string/$var or constructor")
}

fn is_void_args(v: &Value) -> bool {
    matches!(v.as_str(), Some("$void"))
}

fn parse_union_type(
    v: &Value,
    scope: &[String],
    warnings: &mut Vec<String>,
) -> Result<TypeRef> {
    if let Some(m) = v.as_mapping() {
        if map_get_str(m, "variants").is_some() {
            bail!(
                "legacy `variants` union syntax was removed; use `$union: [...]` or `$enum: {{...}}`"
            );
        }
    }
    let items = v
        .as_sequence()
        .context("$union must be an array of type expressions")?;
    if items.len() < 2 {
        bail!("$union must contain at least two members");
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(parse_type_ref(item, scope, warnings)?);
    }
    Ok(TypeRef::Union(out))
}

fn parse_enum_decl(
    alias: &str,
    name: &str,
    v: &Value,
    type_params: &[String],
    scope: &[String],
    warnings: &mut Vec<String>,
) -> Result<EnumDef> {
    let m = v.as_mapping().context("$enum must be a mapping")?;
    if m.is_empty() {
        bail!("$enum must not be empty");
    }
    let mut tags = HashMap::new();
    for (k, tv) in m {
        let tag = k.as_str().context("$enum tag must be string")?;
        maybe_warn_kebab(tag, "enum tag", warnings);
        let ty = parse_type_ref(tv, scope, warnings)
            .with_context(|| format!("invalid type for enum tag `{tag}`"))?;
        tags.insert(tag.to_string(), ty);
    }
    Ok(EnumDef {
        alias: alias.to_string(),
        name: name.to_string(),
        type_params: type_params.to_vec(),
        tags,
    })
}

fn parse_type_ref(v: &Value, scope: &[String], warnings: &mut Vec<String>) -> Result<TypeRef> {
    if let Some(m) = v.as_mapping() {
        if let Some(fv) = map_get_str(m, "$forall") {
            let (types, in_v) = parse_forall_header(fv, warnings)?;
            let mut new_scope = scope.to_vec();
            for t in types.iter().rev() {
                new_scope.retain(|x| x != t);
                new_scope.insert(0, t.clone());
            }
            return parse_type_ref(in_v, &new_scope, warnings);
        }
        if let Some(union_v) = map_get_str(m, "$union") {
            return parse_union_type(union_v, scope, warnings);
        }
        if let Some(enum_v) = map_get_str(m, "$enum") {
            return Ok(TypeRef::Enum(
                parse_enum_decl("inline", "inline", enum_v, &[], scope, warnings)?.tags,
            ));
        }
        bail!("unsupported type expression mapping; expected `$forall`, `$union`, or `$enum`");
    }
    let raw = v.as_str().context("type annotation must be string or type expression object")?;
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
        "int" | "float" => {
            bail!("type alias `${name}` was removed; use explicit numeric primitives")
        }
        _ => {
            maybe_warn_kebab_qualified(name, "type reference", warnings);
            if scope.iter().any(|s| s == name) {
                TypeRef::Generic(name.to_string())
            } else {
                TypeRef::Named(name.to_string())
            }
        }
    };
    Ok(ty)
}

fn infer_expr_type(
    expr: &Expr,
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    match expr {
        Expr::Value(RuntimeValue::Int(_)) => Some(TypeRef::Int64),
        Expr::Value(RuntimeValue::Float(_)) => Some(TypeRef::Float64),
        Expr::Value(RuntimeValue::Str(_)) => Some(TypeRef::Named("str".to_string())),
        Expr::Value(RuntimeValue::Void) => Some(TypeRef::Void),
        Expr::Value(RuntimeValue::Enum { enum_key, .. }) => Some(TypeRef::Named(enum_key.clone())),
        Expr::VarRef(v) => locals
            .get(v)
            .cloned()
            .or_else(|| {
                constants.get(v).and_then(|rv| {
                    infer_expr_type(
                        &Expr::Value(rv.clone()),
                        constants,
                        locals,
                        aliases,
                        enums,
                    )
                })
            }),
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let def = enums.get(enum_key)?;
            instantiated_type_for_constructor(
                enum_key,
                def,
                tag,
                payload.as_deref(),
                constants,
                locals,
                aliases,
                enums,
            )
        }
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

fn normalize_type_ref(ty: &TypeRef, aliases: &HashMap<String, TypeRef>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => aliases.get(name).cloned().unwrap_or_else(|| ty.clone()),
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect(),
        },
        TypeRef::Union(items) => TypeRef::Union(
            items
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect(),
        ),
        TypeRef::Enum(tags) => {
            let mut out = HashMap::new();
            for (k, v) in tags {
                out.insert(k.clone(), normalize_type_ref(v, aliases));
            }
            TypeRef::Enum(out)
        }
        _ => ty.clone(),
    }
}

fn type_compatible(expected: &TypeRef, actual: &TypeRef, aliases: &HashMap<String, TypeRef>) -> bool {
    let mut bindings = HashMap::new();
    unify_types(expected, actual, aliases, &mut bindings)
}

fn strip_module_prefix(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn parse_match_statement(
    match_body: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeRef>,
    enums: &HashMap<String, EnumDef>,
    locals: &HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
) -> Result<Statement> {
    let m = match_body.as_mapping().context("$match must be mapping")?;
    let target_v = map_get_str(m, "target").context("$match missing target")?;
    let target = parse_expr(target_v, constants, type_aliases, enums, locals, warnings)?;
    let target_ty = infer_expr_type(&target, constants, locals, type_aliases, enums)
        .context("$match target type could not be inferred; provide enum variable context")?;
    let TypeRef::Instantiated { base: enum_key, type_args } = target_ty else {
        bail!("$match target must be an instantiated enum type, got {target_ty:?}");
    };
    let enum_def = enums
        .get(&enum_key)
        .or_else(|| {
            enums
                .iter()
                .find(|(k, _)| strip_module_prefix(k) == strip_module_prefix(&enum_key))
                .map(|(_, v)| v)
        })
        .with_context(|| format!("$match target `{enum_key}` is not a known enum"))?;

    if enum_def.type_params.len() != type_args.len() {
        bail!(
            "internal: enum `{enum_key}` type arg count {} does not match params {}",
            type_args.len(),
            enum_def.type_params.len()
        );
    }
    let mut subst: HashMap<String, TypeRef> = HashMap::new();
    for (p, a) in enum_def.type_params.iter().zip(type_args.iter()) {
        subst.insert(p.clone(), a.clone());
    }

    let arms_v = map_get_str(m, "arms").context("$match missing arms")?;
    let arms_m = arms_v.as_mapping().context("$match arms must be mapping")?;
    let mut seen = HashMap::new();
    let mut arms = Vec::new();
    for (k, v) in arms_m {
        let tag = k.as_str().context("$match arm key must be string")?;
        maybe_warn_kebab(tag, "match arm tag", warnings);
        let payload_ty = enum_def
            .tags
            .get(tag)
            .with_context(|| format!("unknown tag `{tag}` for enum `{enum_key}`"))?;
        if seen.insert(tag.to_string(), true).is_some() {
            bail!("duplicate $match arm for tag `{tag}`");
        }

        let arm_map = v
            .as_mapping()
            .with_context(|| format!("$match arm `{tag}` must be mapping"))?;
        let bind = map_get_str(arm_map, "bind")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        if let Some(bind_name) = &bind {
            maybe_warn_kebab(bind_name, "match bind", warnings);
        }

        if *payload_ty == TypeRef::Void && bind.is_some() {
            bail!("$match arm `{tag}` cannot bind payload (tag has `$void` payload)");
        }
        if *payload_ty != TypeRef::Void && bind.is_none() {
            bail!("$match arm `{tag}` must bind payload");
        }

        let mut scoped_locals = locals.clone();
        if let Some(bind_name) = &bind {
            let bind_ty = substitute_type(payload_ty, &subst);
            scoped_locals.insert(bind_name.clone(), bind_ty);
        }

        let do_v = map_get_str(arm_map, "do")
            .with_context(|| format!("$match arm `{tag}` missing do"))?;
        let do_seq = do_v
            .as_sequence()
            .with_context(|| format!("$match arm `{tag}` do must be sequence"))?;
        let mut body = Vec::new();
        for step in do_seq {
            body.push(lower_statement(
                step,
                sigs,
                constants,
                type_aliases,
                enums,
                &mut scoped_locals,
                warnings,
            )?);
        }
        arms.push(MatchArm {
            tag: tag.to_string(),
            bind,
            body,
        });
    }

    for tag in enum_def.tags.keys() {
        if !seen.contains_key(tag) {
            bail!("$match for enum `{enum_key}` missing arm for tag `{tag}`");
        }
    }

    Ok(Statement::Match {
        target,
        enum_key,
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

fn resolve_enum_tag_ref(raw: &str, enums: &HashMap<String, EnumDef>) -> Result<(String, String)> {
    let rest = raw
        .strip_prefix('$')
        .context("constructor key must start with `$`")?;
    let parts: Vec<&str> = rest.split('.').collect();
    if parts.len() == 3 {
        let alias = parts[0];
        let enum_name = parts[1];
        let tag = parts[2];
        if alias.is_empty() || enum_name.is_empty() || tag.is_empty() {
            bail!("invalid constructor `{raw}`");
        }
        return Ok((format!("{alias}.{enum_name}"), tag.to_string()));
    }
    if parts.len() == 2 {
        let enum_name = parts[0];
        let tag = parts[1];
        if enum_name.is_empty() || tag.is_empty() {
            bail!("invalid constructor `{raw}`");
        }
        if enums.contains_key(enum_name) {
            return Ok((enum_name.to_string(), tag.to_string()));
        }
        let matches: Vec<String> = enums
            .keys()
            .filter(|k| strip_module_prefix(k) == enum_name)
            .cloned()
            .collect();
        match matches.as_slice() {
            [single] => Ok((single.clone(), tag.to_string())),
            [] => bail!("unknown enum reference `{enum_name}` in `{raw}`"),
            _ => bail!(
                "ambiguous enum reference `{enum_name}` in `{raw}`; use `$alias.{enum_name}.{tag}`"
            ),
        }
    } else {
        bail!("constructor `{raw}` must be `$alias.enum.tag` or `$enum.tag`")
    }
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

fn parse_signature_args(v: &Value, warnings: &mut Vec<String>) -> Result<(Vec<String>, Vec<TypeRef>)> {
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
        maybe_warn_kebab(&arg_name, "function argument", warnings);
        let arg_type = parse_type_ref(t, &[], warnings)
            .with_context(|| format!("invalid type for arg `{arg_name}`"))?;
        arg_names.push(arg_name);
        arg_types.push(arg_type);
    }
    Ok((arg_names, arg_types))
}

fn qualify_named_type(alias: &str, ty: TypeRef, aliases: &HashMap<String, TypeRef>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => {
            if name.contains('.') {
                TypeRef::Named(name)
            } else {
                let maybe_alias = format!("{alias}.{name}");
                if aliases.contains_key(&maybe_alias) {
                    TypeRef::Named(maybe_alias)
                } else {
                    TypeRef::Named(name)
                }
            }
        }
        TypeRef::Union(items) => TypeRef::Union(
            items
                .into_iter()
                .map(|item| qualify_named_type(alias, item, aliases))
                .collect(),
        ),
        TypeRef::Enum(tags) => {
            let mut out = HashMap::new();
            for (tag, ty) in tags {
                out.insert(tag, qualify_named_type(alias, ty, aliases));
            }
            TypeRef::Enum(out)
        }
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args
                .into_iter()
                .map(|t| qualify_named_type(alias, t, aliases))
                .collect(),
        },
        _ => ty,
    }
}

fn maybe_warn_kebab(name: &str, context: &str, warnings: &mut Vec<String>) {
    if !is_kebab_case(name) {
        warnings.push(format!(
            "non-kebab-case {context}: `{name}` (recommended: kebab-case)"
        ));
    }
}

fn maybe_warn_kebab_qualified(name: &str, context: &str, warnings: &mut Vec<String>) {
    for segment in name.split('.') {
        if segment.is_empty() {
            continue;
        }
        maybe_warn_kebab(segment, context, warnings);
    }
}

fn is_kebab_case(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
