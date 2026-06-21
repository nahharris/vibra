//! Deterministic pre-typecheck expansion for function-shaped Vibra macros.

use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_EXPANSION_DEPTH: usize = 64;
pub const MAX_EVALUATION_STEPS: usize = 1_000_000;
pub const MAX_GENERATED_NODES: usize = 100_000;

#[derive(Debug, Clone)]
struct MacroDefinition {
    module: PathBuf,
    name: String,
    input: String,
    return_category: String,
    body: Vec<Value>,
}

#[derive(Debug, Default)]
struct ExpansionState {
    evaluation_steps: usize,
    generated_nodes: usize,
    hygiene_id: usize,
}

pub fn expand_program_modules(
    entry: &Path,
    mut modules: HashMap<PathBuf, Value>,
) -> Result<HashMap<PathBuf, Value>> {
    let imports = collect_imports(&modules)?;
    let definitions = collect_module_symbols(&modules)?;
    let macros = collect_macros(&mut modules)?;
    let mut state = ExpansionState::default();
    let module_paths: Vec<_> = modules.keys().cloned().collect();
    for module in module_paths {
        let value = modules
            .remove(&module)
            .with_context(|| format!("missing module {}", module.display()))?;
        let expanded = expand_value(
            value,
            &module,
            &macros,
            &imports,
            &definitions,
            &mut state,
            0,
            entry,
        )?;
        modules.insert(module, expanded);
    }
    Ok(modules)
}

fn collect_module_symbols(
    modules: &HashMap<PathBuf, Value>,
) -> Result<HashMap<PathBuf, HashSet<String>>> {
    modules
        .iter()
        .map(|(module, root)| {
            let mapping = root
                .as_mapping()
                .with_context(|| format!("{}: root must be a mapping", module.display()))?;
            Ok((
                module.clone(),
                mapping
                    .keys()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ))
        })
        .collect()
}

fn collect_macros(
    modules: &mut HashMap<PathBuf, Value>,
) -> Result<HashMap<(PathBuf, String), MacroDefinition>> {
    let mut macros = HashMap::new();
    for (module, root) in modules {
        let mapping = root
            .as_mapping_mut()
            .with_context(|| format!("{}: root must be a mapping", module.display()))?;
        let names: Vec<String> = mapping
            .iter()
            .filter_map(|(key, definition)| {
                definition
                    .as_mapping()
                    .filter(|definition| map_get(definition, "$macro").is_some())
                    .and_then(|_| key.as_str().map(str::to_string))
            })
            .collect();
        for name in names {
            let definition = mapping
                .remove(Value::String(name.clone()))
                .expect("macro name collected from mapping");
            let envelope = definition
                .as_mapping()
                .context("macro definition must be a mapping")?;
            let signature = map_get(envelope, "$macro")
                .and_then(Value::as_mapping)
                .context("`$macro` must declare one labeled input")?;
            if signature.len() != 1 {
                bail!("macro `{name}` must declare exactly one labeled input");
            }
            let input = signature
                .keys()
                .next()
                .and_then(Value::as_str)
                .context("macro input name must be a string")?
                .to_string();
            let return_category = map_get(envelope, "return")
                .and_then(Value::as_str)
                .context("macro must declare a syntax return category")?
                .to_string();
            let body = map_get(envelope, "do")
                .and_then(Value::as_sequence)
                .context("macro must declare a `do` sequence")?
                .clone();
            let key = (module.clone(), name.clone());
            if macros
                .insert(
                    key,
                    MacroDefinition {
                        module: module.clone(),
                        name,
                        input,
                        return_category,
                        body,
                    },
                )
                .is_some()
            {
                bail!("duplicate macro definition");
            }
        }
    }
    Ok(macros)
}

fn collect_imports(
    modules: &HashMap<PathBuf, Value>,
) -> Result<HashMap<(PathBuf, String), PathBuf>> {
    let mut imports = HashMap::new();
    for (module, root) in modules {
        let mapping = root
            .as_mapping()
            .with_context(|| format!("{}: root must be a mapping", module.display()))?;
        for (alias, definition) in mapping {
            let Some(alias) = alias.as_str() else {
                continue;
            };
            let Some(import) = definition
                .as_mapping()
                .and_then(|definition| map_get(definition, "$import"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            if import.starts_with('@') {
                continue;
            }
            let path = module
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(import);
            let path = fs::canonicalize(&path)
                .with_context(|| format!("resolve macro import `{import}`"))?;
            imports.insert(
                (module.clone(), alias.to_string()),
                canonical_module_path(path),
            );
        }
    }
    Ok(imports)
}

#[allow(clippy::too_many_arguments)]
fn expand_value(
    value: Value,
    module: &Path,
    macros: &HashMap<(PathBuf, String), MacroDefinition>,
    imports: &HashMap<(PathBuf, String), PathBuf>,
    definitions: &HashMap<PathBuf, HashSet<String>>,
    state: &mut ExpansionState,
    depth: usize,
    entry: &Path,
) -> Result<Value> {
    state.generated_nodes = state.generated_nodes.saturating_add(1);
    if state.generated_nodes > MAX_GENERATED_NODES {
        bail!(
            "macro expansion generated more than {MAX_GENERATED_NODES} nodes while expanding {}",
            entry.display()
        );
    }
    match value {
        Value::Mapping(mapping) => {
            if let Some((definition, argument, call_name, definition_alias)) =
                resolve_macro_call(&mapping, module, macros, imports)
            {
                if depth >= MAX_EXPANSION_DEPTH {
                    bail!(
                        "macro expansion depth exceeded {MAX_EXPANSION_DEPTH} at `{call_name}` in {} (defined in {})",
                        module.display(),
                        definition.module.display()
                    );
                }
                let expanded = evaluate_macro(
                    definition,
                    argument,
                    state,
                    definition_alias.as_deref(),
                    definitions.get(&definition.module),
                )
                .with_context(|| {
                    format!(
                        "expand macro `{}` invoked in {} (defined in {})",
                        definition.name,
                        module.display(),
                        definition.module.display()
                    )
                })?;
                validate_category(&definition.return_category, &expanded).with_context(|| {
                    format!(
                        "macro `{}` returned invalid `{}` syntax",
                        definition.name, definition.return_category
                    )
                })?;
                return expand_value(
                    expanded,
                    module,
                    macros,
                    imports,
                    definitions,
                    state,
                    depth + 1,
                    entry,
                );
            }
            mapping
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        key,
                        expand_value(
                            value,
                            module,
                            macros,
                            imports,
                            definitions,
                            state,
                            depth,
                            entry,
                        )?,
                    ))
                })
                .collect::<Result<Mapping>>()
                .map(Value::Mapping)
        }
        Value::Sequence(values) => values
            .into_iter()
            .map(|value| {
                expand_value(
                    value,
                    module,
                    macros,
                    imports,
                    definitions,
                    state,
                    depth,
                    entry,
                )
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Sequence),
        other => Ok(other),
    }
}

fn resolve_macro_call<'a>(
    mapping: &'a Mapping,
    module: &Path,
    macros: &'a HashMap<(PathBuf, String), MacroDefinition>,
    imports: &HashMap<(PathBuf, String), PathBuf>,
) -> Option<(&'a MacroDefinition, &'a Value, String, Option<String>)> {
    for (key, argument) in mapping {
        let key = key.as_str()?.strip_prefix('$')?;
        let (target_module, name, definition_alias): (PathBuf, &str, Option<String>) =
            if let Some((alias, name)) = key.split_once('.') {
                (
                    imports
                        .get(&(module.to_path_buf(), alias.to_string()))?
                        .clone(),
                    name,
                    Some(alias.to_string()),
                )
            } else {
                (module.to_path_buf(), key, None)
            };
        if let Some(definition) = macros.get(&(target_module, name.to_string())) {
            return Some((definition, argument, format!("${key}"), definition_alias));
        }
    }
    None
}

fn evaluate_macro(
    definition: &MacroDefinition,
    argument: &Value,
    state: &mut ExpansionState,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Result<Value> {
    let mut environment = HashMap::new();
    environment.insert(format!("args.{}", definition.input), argument.clone());
    for statement in &definition.body {
        step(state)?;
        let mapping = statement
            .as_mapping()
            .context("compile-time macro statements must be mappings")?;
        if let Some(binding) = map_get(mapping, "$let").and_then(Value::as_mapping) {
            for (name, value) in binding {
                let name = name
                    .as_str()
                    .context("compile-time `$let` binding name must be a string")?;
                let value = evaluate_expression(
                    value,
                    &environment,
                    state,
                    definition_alias,
                    definition_symbols,
                )?;
                environment.insert(name.to_string(), value);
            }
            continue;
        }
        if let Some(value) = map_get(mapping, "$return") {
            return evaluate_expression(
                value,
                &environment,
                state,
                definition_alias,
                definition_symbols,
            );
        }
        bail!(
            "macro `{}` uses unsupported compile-time statement",
            definition.name
        );
    }
    bail!("macro `{}` completed without `$return`", definition.name)
}

fn evaluate_expression(
    value: &Value,
    environment: &HashMap<String, Value>,
    state: &mut ExpansionState,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Result<Value> {
    step(state)?;
    if let Some(reference) = value.as_str().and_then(|value| value.strip_prefix('$')) {
        if let Some(value) = environment.get(reference) {
            return Ok(value.clone());
        }
    }
    if let Some(mapping) = value.as_mapping() {
        if mapping.len() == 1 {
            if let Some(template) = map_get(mapping, "$quote") {
                state.hygiene_id = state.hygiene_id.saturating_add(1);
                let mut bindings = HashMap::new();
                collect_quoted_bindings(template, state.hygiene_id, &mut bindings);
                return quote_value(
                    template,
                    environment,
                    state,
                    &bindings,
                    definition_alias,
                    definition_symbols,
                );
            }
        }
    }
    Ok(value.clone())
}

fn collect_quoted_bindings(
    value: &Value,
    hygiene_id: usize,
    bindings: &mut HashMap<String, String>,
) {
    match value {
        Value::Mapping(mapping) => {
            if mapping.len() == 1
                && (map_get(mapping, "$unquote").is_some()
                    || map_get(mapping, "$splice").is_some()
                    || map_get(mapping, "$capture").is_some())
            {
                return;
            }
            if let Some(binding) = map_get(mapping, "$let").and_then(Value::as_mapping) {
                for name in binding.keys().filter_map(Value::as_str) {
                    bindings
                        .entry(name.to_string())
                        .or_insert_with(|| format!("{name}--macro-{hygiene_id}"));
                }
            }
            for value in mapping.values() {
                collect_quoted_bindings(value, hygiene_id, bindings);
            }
        }
        Value::Sequence(values) => {
            for value in values {
                collect_quoted_bindings(value, hygiene_id, bindings);
            }
        }
        _ => {}
    }
}

fn quote_value(
    value: &Value,
    environment: &HashMap<String, Value>,
    state: &mut ExpansionState,
    bindings: &HashMap<String, String>,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Result<Value> {
    step(state)?;
    match value {
        Value::Mapping(mapping) if mapping.len() == 1 => {
            if let Some(value) = map_get(mapping, "$unquote") {
                return evaluate_expression(
                    value,
                    environment,
                    state,
                    definition_alias,
                    definition_symbols,
                );
            }
            if let Some(value) = map_get(mapping, "$capture") {
                return Ok(value.clone());
            }
            quote_mapping(
                mapping,
                environment,
                state,
                bindings,
                definition_alias,
                definition_symbols,
            )
        }
        Value::Mapping(mapping) => quote_mapping(
            mapping,
            environment,
            state,
            bindings,
            definition_alias,
            definition_symbols,
        ),
        Value::Sequence(values) => {
            let mut output = Vec::new();
            for value in values {
                if let Some(splice) = value
                    .as_mapping()
                    .filter(|mapping| mapping.len() == 1)
                    .and_then(|mapping| map_get(mapping, "$splice"))
                {
                    let value = evaluate_expression(
                        splice,
                        environment,
                        state,
                        definition_alias,
                        definition_symbols,
                    )?;
                    let values = value
                        .as_sequence()
                        .context("`$splice` requires a sequence value")?;
                    output.extend(values.iter().cloned());
                } else {
                    output.push(quote_value(
                        value,
                        environment,
                        state,
                        bindings,
                        definition_alias,
                        definition_symbols,
                    )?);
                }
            }
            Ok(Value::Sequence(output))
        }
        Value::String(reference) if reference.starts_with('$') => {
            let name = reference.trim_start_matches('$');
            Ok(if let Some(name) = bindings.get(name) {
                Value::String(format!("${name}"))
            } else {
                qualify_reference(value, definition_alias, definition_symbols)
            })
        }
        other => Ok(other.clone()),
    }
}

fn quote_mapping(
    mapping: &Mapping,
    environment: &HashMap<String, Value>,
    state: &mut ExpansionState,
    bindings: &HashMap<String, String>,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Result<Value> {
    let is_let = map_get(mapping, "$let").is_some();
    mapping
        .iter()
        .map(|(key, value)| {
            let key = if is_let {
                key.clone()
            } else {
                rename_quoted_key(key, bindings, definition_alias, definition_symbols)
            };
            let value = if key.as_str() == Some("$let") {
                let binding = value
                    .as_mapping()
                    .context("quoted `$let` must bind a mapping")?;
                let mut renamed = Mapping::new();
                for (name, value) in binding {
                    let name_string = name.as_str().unwrap_or_default();
                    let renamed_name = bindings
                        .get(name_string)
                        .cloned()
                        .unwrap_or_else(|| name_string.to_string());
                    renamed.insert(
                        Value::String(renamed_name),
                        quote_value(
                            value,
                            environment,
                            state,
                            bindings,
                            definition_alias,
                            definition_symbols,
                        )?,
                    );
                }
                Value::Mapping(renamed)
            } else {
                quote_value(
                    value,
                    environment,
                    state,
                    bindings,
                    definition_alias,
                    definition_symbols,
                )?
            };
            Ok((key, value))
        })
        .collect::<Result<Mapping>>()
        .map(Value::Mapping)
}

fn rename_quoted_key(
    key: &Value,
    bindings: &HashMap<String, String>,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Value {
    let Some(reference) = key.as_str().and_then(|key| key.strip_prefix('$')) else {
        return key.clone();
    };
    bindings
        .get(reference)
        .map(|name| Value::String(format!("${name}")))
        .unwrap_or_else(|| qualify_reference(key, definition_alias, definition_symbols))
}

fn qualify_reference(
    value: &Value,
    definition_alias: Option<&str>,
    definition_symbols: Option<&HashSet<String>>,
) -> Value {
    let (Some(alias), Some(symbols), Some(reference)) = (
        definition_alias,
        definition_symbols,
        value.as_str().and_then(|value| value.strip_prefix('$')),
    ) else {
        return value.clone();
    };
    let symbol = reference.split('.').next().unwrap_or(reference);
    if symbols.contains(symbol) {
        Value::String(format!("${alias}.{reference}"))
    } else {
        value.clone()
    }
}

fn validate_category(category: &str, value: &Value) -> Result<()> {
    if category.ends_with("statement-syntax") && !value.is_mapping() {
        bail!("statement macro must return a mapping");
    }
    if category.ends_with("definition-syntax") && !value.is_mapping() {
        bail!("definition macro must return a mapping");
    }
    Ok(())
}

fn step(state: &mut ExpansionState) -> Result<()> {
    state.evaluation_steps = state.evaluation_steps.saturating_add(1);
    if state.evaluation_steps > MAX_EVALUATION_STEPS {
        bail!("macro compile-time evaluation exceeded {MAX_EVALUATION_STEPS} steps");
    }
    Ok(())
}

fn map_get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_string()))
}

fn canonical_module_path(path: PathBuf) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path;
    };
    let without_extension = file_name
        .strip_suffix(".vibra.yaml")
        .or_else(|| file_name.strip_suffix(".vibra"))
        .unwrap_or(file_name);
    let Some((base, _)) = without_extension.split_once('.') else {
        return path;
    };
    let candidate = path.with_file_name(format!("{base}.vibra"));
    if candidate.exists() {
        fs::canonicalize(candidate).unwrap_or(path)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_step_limit_is_enforced() {
        let mut state = ExpansionState {
            evaluation_steps: MAX_EVALUATION_STEPS,
            ..ExpansionState::default()
        };
        let error = step(&mut state).unwrap_err();
        assert!(error.to_string().contains("evaluation exceeded"));
    }

    #[test]
    fn generated_node_limit_is_enforced() {
        let mut state = ExpansionState {
            generated_nodes: MAX_GENERATED_NODES,
            ..ExpansionState::default()
        };
        let error = expand_value(
            Value::Null,
            Path::new("module.vibra"),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &mut state,
            0,
            Path::new("module.vibra"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("generated more than"));
    }
}
