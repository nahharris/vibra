use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
};
use vibra::ast::{DocumentId, Module, TopLevel};
use vibra::typed_body;
use vibra::typed_lower::{self, TypedModuleInput};

#[derive(Clone, Debug)]
struct CallSignature {
    type_args: Vec<String>,
    value_args: Vec<String>,
}

thread_local! {
    static CALL_SIGNATURES: RefCell<BTreeMap<String, CallSignature>> = RefCell::new(BTreeMap::new());
    static TYPE_SIGNATURES: RefCell<BTreeMap<String, Vec<String>>> = RefCell::new(BTreeMap::new());
}

#[derive(Default)]
struct SignatureIndex {
    calls: BTreeMap<PathBuf, BTreeMap<String, CallSignature>>,
    types: BTreeMap<PathBuf, BTreeMap<String, Vec<String>>>,
}

#[derive(Default)]
struct Report {
    scanned: usize,
    already_sexpr: usize,
    converted: usize,
    /// Conversion (legacy-YAML-to-S-expression) failures. Renamed from the
    /// original single-tier `unsupported` bucket now that validation is
    /// staged: this bucket only covers the syntactic rewrite, not
    /// compileability.
    conversion_failures: BTreeMap<String, usize>,
    /// `project.vibra` package manifests use their own top-level grammar
    /// (`(project ...)`, parsed by `project_context.rs`), not the module
    /// grammar `ast::lower_document` expects. They are excluded from the
    /// module-lowering tiers below rather than counted as surface failures:
    /// that would be a validator/category mismatch, not a real language-path
    /// gap. Step 5 ("Projects and packages") is already done and validated
    /// elsewhere.
    project_manifests: usize,
    /// Tier 1: the S-expression source parses (reader/CST) and lowers into a
    /// well-shaped typed surface AST. Proves shape only, not compileability.
    surface_valid: usize,
    surface_failures: BTreeMap<String, usize>,
    /// Tier 2: typed signature lowering succeeds for the file treated as the
    /// entry point of its own program, pulling in its transitive relative
    /// imports so cross-module type references resolve as they would for a
    /// real build.
    signature_valid: usize,
    signature_failures: BTreeMap<String, usize>,
    /// Tier 3: typed body lowering succeeds against the signature index from
    /// tier 2. This is the tier that actually proves cutover readiness; it is
    /// expected to fail widely until `Expr::Primitive` lands on the typed
    /// path (tracked on a separate branch).
    body_valid: usize,
    materialized_valid: usize,
    body_failures: BTreeMap<String, usize>,
    materialize_failures: BTreeMap<String, usize>,
}

/// `--write` is the explicit opt-in for the migrator to touch disk at all. Dry
/// run (report only, no `fs::write`) stays the default so a plain invocation
/// can never mutate the corpus by accident; only this flag switches the tool
/// into its write mode. Positional argument order does not matter.
struct Args {
    root: PathBuf,
    write: bool,
}

fn parse_args() -> Result<Args> {
    let mut root = None;
    let mut write = false;
    for arg in env::args().skip(1) {
        if arg == "--write" {
            write = true;
        } else if root.is_none() {
            root = Some(PathBuf::from(arg));
        } else {
            bail!("unexpected extra argument `{arg}`");
        }
    }
    Ok(Args {
        root: match root {
            Some(root) => root,
            None => env::current_dir()?,
        },
        write,
    })
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let root = args.root;
    let mut files = Vec::new();
    collect(&root, &mut files)?;
    files.sort();
    let signature_index = build_signature_index(&files)?;

    let mut report = Report::default();
    let mut displays: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut sexpr_sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    // Canonical path -> real on-disk path, and the set of canonical paths
    // that were freshly converted this run (as opposed to files that were
    // already S-expression and therefore have nothing to write back).
    let mut originals: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut freshly_converted: BTreeSet<PathBuf> = BTreeSet::new();

    for path in &files {
        report.scanned += 1;
        let display = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let canonical = fs::canonicalize(path)?;
        displays.insert(canonical.clone(), display.clone());
        originals.insert(canonical.clone(), path.clone());
        let source = fs::read_to_string(path)?;
        if source.trim_start().starts_with('(') {
            report.already_sexpr += 1;
            sexpr_sources.insert(canonical, source);
            continue;
        }
        match migrate_for_path(&source, path, &signature_index) {
            Ok(output) => {
                report.converted += 1;
                sexpr_sources.insert(canonical.clone(), output);
                freshly_converted.insert(canonical);
            }
            Err(error) => record_issue(
                &mut report.conversion_failures,
                format!("{display}: {error:#}"),
            ),
        }
    }

    // Tier 1 (surface-valid): the S-expression source parses and lowers into
    // a well-shaped typed surface AST. This is the check the migrator used
    // to call "typed-valid"; it proves shape only, not compileability.
    let mut modules: BTreeMap<PathBuf, Module> = BTreeMap::new();
    for (canonical, source) in &sexpr_sources {
        if is_project_manifest(canonical) {
            report.project_manifests += 1;
            continue;
        }
        let display = displays.get(canonical).cloned().unwrap_or_default();
        match surface_module(source, canonical) {
            Ok(mut module) => {
                report.surface_valid += 1;
                // A real build expands compile-time `embed` and `template`
                // forms in `load_surface_program` before any body lowering.
                // This tool drives `lower_typed_bodies` directly, so without
                // running expansion here it reports files as failing on
                // compile-time data the compiler handles correctly.
                if let Err(error) =
                    vibra::frontend::expand_module_compile_time_data(&mut module, canonical)
                {
                    record_issue(
                        &mut report.body_failures,
                        format!(
                            "{display}: compile-time expansion: {}",
                            first_line(&format!("{error:#}"))
                        ),
                    );
                    continue;
                }
                modules.insert(canonical.clone(), module);
            }
            Err(error) => record_issue(
                &mut report.surface_failures,
                format!("{display}: {}", first_line(&format!("{error:#}"))),
            ),
        }
    }

    // `--write` mode: rewrite a source file in place only if it was both
    // converted (the YAML-to-S-expression rewrite succeeded) and validated
    // (tier 1: the emitted S-expression parses and lowers into a well-shaped
    // typed surface AST). A file that fails either check is left untouched on
    // disk -- never a partially-converted file. Files that were already
    // S-expression are never rewritten here; there is nothing to migrate.
    //
    // Each write is immediately re-canonicalized through the staged
    // S-expression printer (`sexpr_tooling::staged_format_sexpr`), the same
    // parse-validate-print path `vibra fmt` will use once it is repointed at
    // S-expression source. Formatting is best-effort: if it fails for any
    // reason the already-valid, already-written (merely non-canonically
    // spaced) source is left in place rather than aborting the run, since the
    // write already passed its own tier-1 validation.
    let mut written = 0usize;
    let mut formatted = 0usize;
    if args.write {
        for canonical in &freshly_converted {
            if !modules.contains_key(canonical) {
                continue;
            }
            let Some(original) = originals.get(canonical) else {
                continue;
            };
            let Some(source) = sexpr_sources.get(canonical) else {
                continue;
            };
            fs::write(original, source)
                .with_context(|| format!("write converted source {}", original.display()))?;
            written += 1;
            match vibra::sexpr_tooling::staged_format_sexpr(original, source) {
                Ok(canonical_source) if &canonical_source != source => {
                    fs::write(original, &canonical_source).with_context(|| {
                        format!("write canonically formatted source {}", original.display())
                    })?;
                    formatted += 1;
                }
                Ok(_) => {}
                Err(diagnostic) => {
                    eprintln!(
                        "warning: {} did not reformat cleanly ({:?}); kept unformatted-but-valid output",
                        original.display(),
                        diagnostic
                    );
                }
            }
        }
    }

    // Tier 2 (signature-valid) and tier 3 (body-valid): only attempted for
    // files whose surface AST is valid. Each file is validated as the entry
    // point of its own program, pulling in its transitive relative imports so
    // cross-module type and call references resolve the same way a real build
    // would see them.
    //
    // The entry module is mounted under an alias derived from its file stem
    // rather than under the empty alias. A module refers to its own exports
    // through its module name — `option.vibra` calls `option.empty` — and with
    // an empty alias `qualify("", name)` is a no-op, so those self-qualified
    // references resolve against neither the qualified nor the bare key. The
    // legacy path has the same requirement: its self-export resolution is
    // guarded by a non-empty home module, because stdlib modules are always
    // mounted rather than compiled bare. Mounting here matches a real build
    // instead of testing a configuration that never occurs.
    for canonical in modules.keys() {
        let display = displays.get(canonical).cloned().unwrap_or_default();
        let graph = match build_typed_graph(canonical, &modules) {
            Ok(graph) => graph,
            Err(error) => {
                record_issue(
                    &mut report.signature_failures,
                    format!("{display}: {}", first_line(&format!("{error:#}"))),
                );
                continue;
            }
        };
        let inputs: Vec<TypedModuleInput> = graph
            .iter()
            .map(|(alias, path)| TypedModuleInput {
                alias: alias.as_str(),
                module: &modules[path],
            })
            .collect();
        match typed_lower::lower_typed_signatures(inputs.iter().copied()) {
            Ok(signature_index) => {
                report.signature_valid += 1;
                match typed_body::lower_typed_bodies(inputs.iter().copied(), &signature_index) {
                    Ok(bodies) => {
                        report.body_valid += 1;
                        // Tier 4 (materialized-valid): body lowering alone does
                        // not prove a program compiles. It produces staged
                        // statements; only materialization turns them into
                        // executable `FunctionSig`s, which is what the real
                        // compiler needs. Reporting body-valid as readiness
                        // overstated it by an order of magnitude.
                        match vibra::typed_body::materialize_typed_functions(
                            &signature_index,
                            &bodies,
                        ) {
                            Ok(_) => report.materialized_valid += 1,
                            Err(error) => record_issue(
                                &mut report.materialize_failures,
                                format!("{display}: {}", first_line(&format!("{error:#}"))),
                            ),
                        }
                    }
                    Err(error) => record_issue(
                        &mut report.body_failures,
                        format!("{display}: {}", first_line(&format!("{error:#}"))),
                    ),
                }
            }
            Err(error) => record_issue(
                &mut report.signature_failures,
                format!("{display}: {}", first_line(&format!("{error:#}"))),
            ),
        }
    }

    let candidates = report.converted + report.already_sexpr - report.project_manifests;
    println!("scanned: {}", report.scanned);
    println!("already-sexpr: {}", report.already_sexpr);
    println!("converted: {}", report.converted);
    println!(
        "written: {} ({}); canonically reformatted: {}",
        written,
        if args.write {
            "--write requested"
        } else {
            "dry run; pass --write to rewrite in place"
        },
        formatted
    );
    println!(
        "unsupported: {}",
        report.conversion_failures.values().sum::<usize>()
    );
    for (reason, count) in &report.conversion_failures {
        println!("  {count:>3}  {reason}");
    }
    println!(
        "project-manifests-excluded: {} (own grammar, validated by project_context.rs, not a module-lowering tier)",
        report.project_manifests
    );
    println!();
    println!("surface-valid: {}/{}", report.surface_valid, candidates);
    println!(
        "surface-invalid: {}",
        report.surface_failures.values().sum::<usize>()
    );
    for (reason, count) in &report.surface_failures {
        println!("  {count:>3}  {reason}");
    }
    println!();
    println!(
        "signature-valid: {}/{}",
        report.signature_valid, report.surface_valid
    );
    println!(
        "signature-invalid: {}",
        report.signature_failures.values().sum::<usize>()
    );
    for (reason, count) in &report.signature_failures {
        println!("  {count:>3}  {reason}");
    }
    println!();
    println!(
        "body-valid: {}/{}",
        report.body_valid, report.signature_valid
    );
    println!(
        "body-invalid: {}",
        report.body_failures.values().sum::<usize>()
    );
    for (reason, count) in &report.body_failures {
        println!("  {count:>3}  {reason}");
    }
    println!();
    println!(
        "materialized-valid: {}/{}",
        report.materialized_valid, report.body_valid
    );
    println!(
        "materialized-invalid: {}",
        report.materialize_failures.values().sum::<usize>()
    );
    for (reason, count) in &report.materialize_failures {
        println!("  {count:>3}  {reason}");
    }
    Ok(())
}

/// `project.vibra` is the fixed manifest file name recognized by
/// `project_context::PROJECT_MANIFEST`. It is a package descriptor, not a
/// language module, and uses a different top-level grammar.
fn is_project_manifest(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("project.vibra")
}

/// Parse and lower a single S-expression source into a typed surface AST
/// module, identified by its own canonical path (tier 1: surface-valid).
fn surface_module(source: &str, path: &Path) -> Result<Module> {
    let document = vibra::syntax::parse(source).map_err(|errors| anyhow::anyhow!("{errors:?}"))?;
    let document_id = DocumentId::from_path(path);
    vibra::ast::lower_document_with_id(&document, document_id)
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// Build the transitive typed-module graph for `entry`, treating it as the
/// entry point of its own program: entry gets the empty (root) alias, and
/// each relatively-imported module is added under the exact alias its
/// importer declared for it. The same physical file can legitimately appear
/// more than once under different aliases (the corpus does this, e.g.
/// `stdlib/src/fs.vibra` imports `./error.vibra` twice, as `error` and
/// `error-lib`), so edges are deduplicated on `(alias, path)`, not on `path`
/// alone. `@`-prefixed project imports are skipped: this standalone tool has
/// no `project.vibra` resolution, matching the same limitation the migrator
/// already accepts for named-call signature discovery.
fn build_typed_graph(
    entry: &Path,
    modules: &BTreeMap<PathBuf, Module>,
) -> Result<Vec<(String, PathBuf)>> {
    let mut order = Vec::new();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    let entry_alias = entry
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    queue.push_back((entry_alias, entry.to_path_buf()));
    while let Some((alias, path)) = queue.pop_front() {
        if !visited.insert((alias.clone(), path.clone())) {
            continue;
        }
        if visited.len() > 2000 {
            bail!(
                "typed module graph exceeded its safety cap while resolving {}",
                entry.display()
            );
        }
        let module = modules
            .get(&path)
            .with_context(|| format!("module {} is not surface-valid", path.display()))?;
        let Some(parent) = path.parent() else {
            order.push((alias, path));
            continue;
        };
        for form in &module.forms {
            let TopLevel::Import(import) = form else {
                continue;
            };
            if import.path.value.starts_with('@') {
                continue;
            }
            let Ok(target) = fs::canonicalize(parent.join(&import.path.value)) else {
                continue;
            };
            if !modules.contains_key(&target) {
                continue;
            }
            queue.push_back((import.alias.value.clone(), target));
        }
        order.push((alias, path));
    }
    Ok(order)
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if !is_excluded_dir(&path) {
                collect(&path, files)?;
            }
        } else if path.extension().and_then(|v| v.to_str()) == Some("vibra") {
            files.push(path);
        }
    }
    Ok(())
}

fn is_excluded_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("target" | ".git" | ".worktrees" | "worktrees")
    )
}

#[cfg(test)]
fn migrate(source: &str) -> Result<String> {
    let root: Value = serde_yaml::from_str(source).context("yaml-parse")?;
    let map = root.as_mapping().context("root-not-mapping")?;
    let signatures = discover_local_signatures(map)?;
    let type_signatures = discover_local_type_signatures(map)?;
    migrate_with_signatures(map, signatures, type_signatures)
}

fn migrate_for_path(source: &str, path: &Path, signature_index: &SignatureIndex) -> Result<String> {
    let root: Value = serde_yaml::from_str(source).context("yaml-parse")?;
    let map = root.as_mapping().context("root-not-mapping")?;
    let mut signatures = discover_local_signatures(map)?;
    let mut type_signatures = discover_local_type_signatures(map)?;
    if let Some(module_name) = path.file_stem().and_then(|name| name.to_str()) {
        for (name, signature) in signatures.clone() {
            signatures.insert(format!("{module_name}.{name}"), signature);
        }
        for (name, parameters) in type_signatures.clone() {
            type_signatures.insert(format!("{module_name}.{name}"), parameters);
        }
    }
    for (alias, definition) in map {
        let Some(alias) = alias.as_str() else {
            continue;
        };
        let Some(import) = definition
            .as_mapping()
            .and_then(|definition| get(definition, "$import"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if import.starts_with('@') {
            continue;
        }
        let Some(parent) = path.parent() else {
            continue;
        };
        let Ok(imported_path) = fs::canonicalize(parent.join(import)) else {
            continue;
        };
        let Some(imported) = signature_index.calls.get(&imported_path) else {
            continue;
        };
        for (name, signature) in imported {
            signatures.insert(format!("{alias}.{name}"), signature.clone());
        }
        if let Some(imported_types) = signature_index.types.get(&imported_path) {
            for (name, parameters) in imported_types {
                type_signatures.insert(format!("{alias}.{name}"), parameters.clone());
            }
        }
    }
    migrate_with_signatures(map, signatures, type_signatures)
}

fn migrate_with_signatures(
    map: &Mapping,
    signatures: BTreeMap<String, CallSignature>,
    type_signatures: BTreeMap<String, Vec<String>>,
) -> Result<String> {
    CALL_SIGNATURES.with(|slot| *slot.borrow_mut() = signatures);
    TYPE_SIGNATURES.with(|slot| *slot.borrow_mut() = type_signatures);
    let mut forms = Vec::new();
    for (key, value) in map {
        let name = key.as_str().context("non-string-top-level-name")?;
        if matches!(name, "=comment" | "=doc" | "=lint") {
            continue;
        }
        forms.push(top_level(name, value).with_context(|| format!("top-level `{name}`"))?);
    }
    Ok(forms.join("\n"))
}

fn build_signature_index(files: &[PathBuf]) -> Result<SignatureIndex> {
    let mut index = SignatureIndex::default();
    for path in files {
        let source = fs::read_to_string(path)?;
        if source.trim_start().starts_with('(') {
            continue;
        }
        let Ok(root) = serde_yaml::from_str::<Value>(&source) else {
            continue;
        };
        let Some(map) = root.as_mapping() else {
            continue;
        };
        let Ok(signatures) = discover_local_signatures(map) else {
            continue;
        };
        let canonical = fs::canonicalize(path)?;
        index.calls.insert(canonical.clone(), signatures);
        index
            .types
            .insert(canonical, discover_local_type_signatures(map)?);
    }
    Ok(index)
}

fn discover_local_type_signatures(map: &Mapping) -> Result<BTreeMap<String, Vec<String>>> {
    let mut signatures = BTreeMap::new();
    for (name, value) in map {
        let (Some(name), Some(definition)) = (name.as_str(), value.as_mapping()) else {
            continue;
        };
        let parameters = where_names(definition)?;
        if !parameters.is_empty()
            && definition
                .keys()
                .any(|key| key.as_str().is_some_and(|key| key.starts_with('$')))
        {
            signatures.insert(name.to_string(), parameters);
        }
    }
    Ok(signatures)
}

fn where_names(map: &Mapping) -> Result<Vec<String>> {
    match get(map, "=where") {
        Some(value) => value
            .as_mapping()
            .context("where-not-mapping")?
            .keys()
            .map(|name| Ok(as_str(name, "type-parameter")?.to_string()))
            .collect(),
        None => Ok(Vec::new()),
    }
}

fn discover_local_signatures(map: &Mapping) -> Result<BTreeMap<String, CallSignature>> {
    let mut signatures = BTreeMap::new();
    for (name, value) in map {
        let Some(name) = name.as_str() else {
            continue;
        };
        let Some(definition) = value.as_mapping() else {
            continue;
        };
        if let Some(primary) = get(definition, "$function").or_else(|| get(definition, "$fn")) {
            let value_args = parameter_entries(primary, get(definition, "args"))?
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            let type_args = match get(definition, "=where") {
                Some(value) => value
                    .as_mapping()
                    .context("where-not-mapping")?
                    .keys()
                    .map(|name| Ok(as_str(name, "type-parameter")?.to_string()))
                    .collect::<Result<Vec<_>>>()?,
                None => Vec::new(),
            };
            signatures.insert(
                name.to_string(),
                CallSignature {
                    type_args,
                    value_args,
                },
            );
        }
        if let Some(definitions) = get(definition, "=defs").and_then(Value::as_mapping) {
            let enclosing_type_args = where_names(definition)?;
            for (method_name, method) in definitions {
                let method_name = as_str(method_name, "def-name")?;
                let method = method.as_mapping().context("def-not-mapping")?;
                let primary = get(method, "$function")
                    .or_else(|| get(method, "$fn"))
                    .context("def-not-function")?;
                signatures.insert(
                    format!("{name}.{method_name}"),
                    CallSignature {
                        type_args: enclosing_type_args
                            .iter()
                            .cloned()
                            .chain(where_names(method)?)
                            .collect(),
                        value_args: parameter_entries(primary, get(method, "args"))?
                            .into_iter()
                            .map(|(name, _)| name)
                            .collect(),
                    },
                );
            }
        }
        if let Some(interface) = get(definition, "$interface").and_then(Value::as_mapping) {
            let enclosing_type_args = where_names(definition)?;
            for (method_name, method_type) in interface {
                let method_name = as_str(method_name, "interface-method-name")?;
                let method_type = method_type
                    .as_mapping()
                    .and_then(|method| get(method, "$fn-type"))
                    .and_then(Value::as_mapping)
                    .context("interface-method-not-fn-type")?;
                let args = get(method_type, "args").context("interface-method-missing-args")?;
                let value_args = if let Some(args) = args
                    .as_mapping()
                    .and_then(|args| get(args, "$record"))
                    .and_then(Value::as_mapping)
                {
                    args.keys()
                        .map(|name| Ok(as_str(name, "interface-method-arg")?.to_string()))
                        .collect::<Result<Vec<_>>>()?
                } else {
                    vec!["input".to_string()]
                };
                signatures.insert(
                    format!("{name}.{method_name}"),
                    CallSignature {
                        type_args: enclosing_type_args.clone(),
                        value_args,
                    },
                );
            }
        }
    }
    Ok(signatures)
}

fn top_level(name: &str, value: &Value) -> Result<String> {
    let Some(map) = value.as_mapping() else {
        return Ok(format!(
            "(const {} {} {})",
            sym(name),
            infer_type(value)?,
            expr(value)?
        ));
    };
    if let Some(import) = get(map, "$import") {
        only_known(map, &["$import"])?;
        return Ok(format!(
            "(import {} {})",
            sym(name),
            quoted(as_str(import, "$import")?)
        ));
    }
    if let Some(profile) = get(map, "$test") {
        return test_form(name, profile, map);
    }
    if let Some(primary) = get(map, "$function").or_else(|| get(map, "$fn")) {
        return function_form(name, primary, map);
    }
    let dollar = map
        .iter()
        .filter_map(|(key, value)| {
            key.as_str()
                .filter(|key| key.starts_with('$'))
                .map(|key| (key, value))
        })
        .collect::<Vec<_>>();
    if dollar.len() == 1 {
        let (head, payload) = dollar[0];
        let annotations = declaration_annotations(map)?;
        return Ok(format!(
            "(def {} {}{})",
            sym(name),
            type_form(head, payload)?,
            annotations
        ));
    }
    bail!("unsupported-top-level-envelope")
}

fn function_form(name: &str, primary: &Value, map: &Mapping) -> Result<String> {
    only_known(
        map,
        &["$function", "$fn", "args", "return", "do", "=doc", "=where"],
    )?;
    let params = parameters(primary, get(map, "args"))?;
    let result = get(map, "return").context("function-missing-return")?;
    let body = get(map, "do").context("function-missing-do")?;
    Ok(format!(
        "(fn {} {} {} {}{})",
        sym(name),
        params,
        ty(result)?,
        body_form(body)?,
        declaration_annotations(map)?
    ))
}

fn test_form(name: &str, profile: &Value, map: &Mapping) -> Result<String> {
    only_known(
        map,
        &[
            "$test",
            "do",
            "tags",
            "timeout-ms",
            "random-seed",
            "skip",
            "expect-error",
            "clock",
            "workspace",
            "policy",
        ],
    )?;
    let body = body_form(get(map, "do").context("test-missing-do")?)?;
    let mut attrs = Vec::new();
    for key in ["tags", "timeout-ms", "random-seed", "skip", "workspace"] {
        if let Some(value) = get(map, key) {
            attrs.push(format!("{key}: {}", metadata_value(key, value)?));
        }
    }
    if let Some(value) = get(map, "expect-error") {
        attrs.push(format!("expect-error: {}", expected_error(value)?));
    }
    if let Some(value) = get(map, "clock") {
        attrs.push(format!("clock: {}", clock(value)?));
    }
    if let Some(value) = get(map, "policy") {
        attrs.push(format!("policy: {}", ty(value)?));
    }
    Ok(format!(
        "(test {} {} {}{})",
        sym(name),
        sym(as_str(profile, "$test")?),
        body,
        attrs
            .into_iter()
            .map(|v| format!(" {v}"))
            .collect::<String>()
    ))
}

fn expected_error(value: &Value) -> Result<String> {
    let map = value.as_mapping().context("expect-error-not-mapping")?;
    only_known(map, &["phase", "code", "message-contains"])?;
    let phase = as_str(
        get(map, "phase").context("expect-error-missing-phase")?,
        "phase",
    )?;
    match phase {
        "load" | "compile" => {
            let code = sym(as_str(
                get(map, "code").context("expect-error-missing-code")?,
                "code",
            )?);
            Ok(match get(map, "message-contains") {
                Some(message) => format!(
                    "({phase} {code} {})",
                    quoted(as_str(message, "message-contains")?)
                ),
                None => format!("({phase} {code})"),
            })
        }
        "runtime" => Ok(format!(
            "(runtime {})",
            quoted(as_str(
                get(map, "message-contains").context("runtime-missing-message")?,
                "message-contains"
            )?)
        )),
        _ => bail!("unknown-expect-error-phase-{phase}"),
    }
}

fn clock(value: &Value) -> Result<String> {
    let map = value.as_mapping().context("clock-not-mapping")?;
    only_known(map, &["unix-millis", "monotonic-millis"])?;
    Ok(format!(
        "(fixed {} {})",
        expr(get(map, "unix-millis").context("clock-missing-unix-millis")?)?,
        expr(get(map, "monotonic-millis").context("clock-missing-monotonic-millis")?)?
    ))
}

fn metadata_value(key: &str, value: &Value) -> Result<String> {
    match key {
        "tags" => Ok(format!(
            "({})",
            value
                .as_sequence()
                .context("tags-not-sequence")?
                .iter()
                .map(|v| Ok(sym(as_str(v, "tag")?)))
                .collect::<Result<Vec<_>>>()?
                .join(" ")
        )),
        "workspace" => Ok(sym(as_str(value, "workspace")?)),
        _ => expr(value),
    }
}

fn parameters(primary: &Value, additional: Option<&Value>) -> Result<String> {
    Ok(format!(
        "({})",
        parameter_entries(primary, additional)?
            .into_iter()
            .map(|(name, value)| Ok(format!("({} {})", sym(&name), ty(value)?)))
            .collect::<Result<Vec<_>>>()?
            .join(" ")
    ))
}

fn parameter_entries<'a>(
    primary: &'a Value,
    additional: Option<&'a Value>,
) -> Result<Vec<(String, &'a Value)>> {
    let mut entries = Vec::new();
    if matches!(primary.as_str(), Some("$self" | "self")) {
        entries.push(("self".to_string(), primary));
    } else if !matches!(primary.as_str(), Some("$void" | "void")) {
        let map = primary.as_mapping().context("parameters-not-mapping")?;
        for (name, value) in map {
            entries.push((as_str(name, "parameter")?.to_string(), value));
        }
    }
    if let Some(additional) = additional {
        let map = additional
            .as_mapping()
            .context("additional-parameters-not-mapping")?;
        for (name, value) in map {
            let name = as_str(name, "parameter")?.to_string();
            if entries.iter().any(|(existing, _)| existing == &name) {
                bail!("duplicate-parameter-{name}");
            }
            entries.push((name, value));
        }
    }
    Ok(entries)
}

fn body_form(value: &Value) -> Result<String> {
    let values = value.as_sequence().context("body-not-sequence")?;
    Ok(format!(
        "(do{})",
        values
            .iter()
            .map(statement)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|value| format!(" {value}"))
            .collect::<String>()
    ))
}

fn statement(value: &Value) -> Result<String> {
    let Some(map) = value.as_mapping() else {
        return expr(value);
    };
    if let Some(condition) = get(map, "$if") {
        only_known(map, &["$if", "then", "else"])?;
        return Ok(format!(
            "(if {} {} {})",
            expr(condition)?,
            body_form(get(map, "then").context("if-missing-then")?)?,
            body_form(get(map, "else").context("if-missing-else")?)?
        ));
    }
    if let Some(condition) = get(map, "$while") {
        only_known(map, &["$while", "do"])?;
        return Ok(format!(
            "(while {} {})",
            expr(condition)?,
            body_form(get(map, "do").context("while-missing-do")?)?
        ));
    }
    if let Some(binding) = get(map, "$for") {
        only_known(map, &["$for", "in", "do"])?;
        return Ok(format!(
            "(for {} {} {})",
            sym(as_str(binding, "for-binding")?),
            expr(get(map, "in").context("for-missing-in")?)?,
            body_form(get(map, "do").context("for-missing-do")?)?
        ));
    }
    if let Some(target) = get(map, "$match") {
        only_known(map, &["$match", "when"])?;
        let cases = get(map, "when")
            .and_then(Value::as_sequence)
            .context("match-when-not-sequence")?
            .iter()
            .map(match_case)
            .collect::<Result<Vec<_>>>()?
            .join(" ");
        return Ok(format!("(match {} {cases})", expr(target)?));
    }
    if let Some(captures) = get(map, "$task") {
        only_known(map, &["$task", "do"])?;
        let captures = captures
            .as_sequence()
            .context("task-captures-not-sequence")?;
        return Ok(format!(
            "(task (captures{}) {})",
            captures
                .iter()
                .map(|capture| Ok(format!(" {}", reference(as_str(capture, "task-capture")?))))
                .collect::<Result<String>>()?,
            body_form(get(map, "do").context("task-missing-do")?)?
        ));
    }
    if let Some(handle) = get(map, "$spawn") {
        only_known(map, &["$spawn", "captures", "value"])?;
        let captures = get(map, "captures")
            .and_then(Value::as_sequence)
            .context("spawn-captures-not-sequence")?
            .iter()
            .map(|capture| Ok(reference(as_str(capture, "spawn-capture")?)))
            .collect::<Result<Vec<_>>>()?
            .join(" ");
        return Ok(format!(
            "(spawn {} (captures {}) {})",
            sym(as_str(handle, "spawn-handle")?),
            captures,
            expr(get(map, "value").context("spawn-missing-value")?)?
        ));
    }
    if let Some(handle) = get(map, "$join") {
        only_known(map, &["$join", "into"])?;
        return Ok(format!(
            "(join {} {})",
            sym(as_str(handle, "join-handle")?),
            sym(as_str(
                get(map, "into").context("join-missing-result")?,
                "join-result"
            )?)
        ));
    }
    if map.len() != 1 {
        return expr(value);
    }
    let (key, payload) = map.iter().next().unwrap();
    let head = as_str(key, "statement-key")?.trim_start_matches('$');
    match head {
        "let" => {
            let binding = payload.as_mapping().context("let-not-mapping")?;
            if binding.len() != 1 {
                bail!("let-not-single-binding")
            }
            let (name, value) = binding.iter().next().unwrap();
            Ok(format!(
                "(let {} {})",
                sym(as_str(name, "let-name")?),
                expr(value)?
            ))
        }
        "set" => {
            let binding = payload.as_mapping().context("set-not-mapping")?;
            if binding.len() != 1 {
                bail!("set-not-single-binding")
            }
            let (name, value) = binding.iter().next().unwrap();
            Ok(format!(
                "(set {} {})",
                reference(as_str(name, "set-name")?),
                expr(value)?
            ))
        }
        "return" => Ok(format!("(return {})", expr(payload)?)),
        "break" | "continue" if payload.is_null() => Ok(format!("({head})")),
        _ => expr(value),
    }
}

fn match_case(value: &Value) -> Result<String> {
    let map = value.as_mapping().context("match-case-not-mapping")?;
    only_known(map, &["case", "do"])?;
    Ok(format!(
        "(case {} {})",
        pattern(get(map, "case").context("match-case-missing-pattern")?)?,
        body_form(get(map, "do").context("match-case-missing-do")?)?
    ))
}

fn pattern(value: &Value) -> Result<String> {
    if let Some(map) = value.as_mapping() {
        if map.len() != 1 {
            bail!("pattern-multi-key")
        }
        let (key, payload) = map.iter().next().unwrap();
        let head = as_str(key, "pattern-head")?.trim_start_matches('$');
        return match head {
            "$wildcard" => Ok("_".into()),
            "$bind" => Ok(format!("(bind {})", sym(as_str(payload, "bind-name")?))),
            "wildcard" => Ok("_".into()),
            "bind" => Ok(format!("(bind {})", sym(as_str(payload, "bind-name")?))),
            "array" | "tuple" => Ok(format!(
                "({head}{})",
                payload
                    .as_sequence()
                    .context("pattern-items-not-sequence")?
                    .iter()
                    .map(pattern)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(|item| format!(" {item}"))
                    .collect::<String>()
            )),
            "record" => {
                let fields = payload.as_mapping().context("record-pattern-not-mapping")?;
                Ok(format!(
                    "(record{})",
                    fields
                        .iter()
                        .map(|(name, value)| Ok(format!(
                            " ({} {})",
                            sym(as_str(name, "record-pattern-field")?),
                            pattern(value)?
                        )))
                        .collect::<Result<String>>()?
                ))
            }
            "map" => {
                let pairs = payload.as_sequence().context("map-pattern-not-sequence")?;
                Ok(format!(
                    "(map{})",
                    pairs
                        .iter()
                        .map(|pair| {
                            let pair = pair.as_mapping().context("map-pattern-pair-not-mapping")?;
                            only_known(pair, &["key", "value"])?;
                            Ok(format!(
                                " ({} {})",
                                pattern(get(pair, "key").context("map-pattern-missing-key")?)?,
                                pattern(get(pair, "value").context("map-pattern-missing-value")?)?
                            ))
                        })
                        .collect::<Result<String>>()?
                ))
            }
            "newtype" | "interface" => {
                let fields = payload
                    .as_mapping()
                    .context("wrapped-pattern-not-mapping")?;
                Ok(format!(
                    "({head} {} {})",
                    ty(get(fields, "type").context("wrapped-pattern-missing-type")?)?,
                    pattern(
                        get(fields, "value")
                            .or_else(|| get(fields, "inner"))
                            .context("wrapped-pattern-missing-value")?
                    )?
                ))
            }
            _ => {
                let arguments = if payload.is_null() {
                    String::new()
                } else if let Some(values) = payload.as_sequence() {
                    values
                        .iter()
                        .map(pattern)
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(|item| format!(" {item}"))
                        .collect()
                } else {
                    format!(" {}", pattern(payload)?)
                };
                Ok(format!("({}{} )", sym(head), arguments).replace(" )", ")"))
            }
        };
    }
    expr(value)
}

fn expr(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("unit".into()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) if value.starts_with('$') => Ok(reference(value)),
        Value::String(value) => Ok(quoted(value)),
        Value::Sequence(values) => Ok(format!(
            "(array{})",
            values
                .iter()
                .map(expr)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|value| format!(" {value}"))
                .collect::<String>()
        )),
        Value::Mapping(map) => expression_mapping(map),
        Value::Tagged(_) => bail!("yaml-tag"),
    }
}

fn expression_mapping(map: &Mapping) -> Result<String> {
    if map.len() != 1 {
        let calls = map
            .iter()
            .filter_map(|(key, payload)| {
                key.as_str()
                    .filter(|key| key.starts_with('$'))
                    .map(|key| (key, payload))
            })
            .collect::<Vec<_>>();
        if calls.len() == 1 {
            let (call, primary) = calls[0];
            let head = sym(call);
            if matches!(head.as_str(), "cast" | "policy.narrow") {
                only_known(map, &[call, "into"])?;
                return Ok(format!(
                    "({head} {} {})",
                    expr(primary)?,
                    ty(get(map, "into").context("typed-operation-missing-into")?)?
                ));
            }
            if head == "convert" {
                only_known(map, &[call, "into", "or"])?;
                return Ok(format!(
                    "(convert {} {} {})",
                    expr(primary)?,
                    ty(get(map, "into").context("convert-missing-into")?)?,
                    expr(get(map, "or").context("convert-missing-fallback")?)?
                ));
            }
            let signature = CALL_SIGNATURES
                .with(|slot| slot.borrow().get(&head).cloned())
                .with_context(|| format!("multi-key-call-unresolved-callee-{head}"))?;
            let Some(primary_name) = signature.value_args.first() else {
                bail!("multi-key-call-has-no-primary-argument-{head}");
            };
            let mut arguments = Mapping::new();
            arguments.insert(Value::String(primary_name.clone()), primary.clone());
            for (key, value) in map {
                if key.as_str() != Some(call) {
                    arguments.insert(key.clone(), value.clone());
                }
            }
            return named_call(&head, &arguments);
        }
        return record(map);
    }
    let (key, payload) = map.iter().next().unwrap();
    let Some(key) = key.as_str() else {
        return record(map);
    };
    if !key.starts_with('$') {
        return record(map);
    }
    let head = sym(key.trim_start_matches('$'));
    match head.as_str() {
        "if" | "while" | "for" | "match" | "task" | "spawn" | "join" | "convert" => {
            bail!("unsupported-expression-{head}")
        }
        "range" => {
            if let Some(values) = payload.as_sequence() {
                if !(2..=3).contains(&values.len()) {
                    bail!("range-arity")
                }
                let step = if values.len() == 3 {
                    expr(&values[2])?
                } else {
                    "1".into()
                };
                Ok(format!(
                    "(range {} {} {step})",
                    expr(&values[0])?,
                    expr(&values[1])?
                ))
            } else {
                let fields = payload
                    .as_mapping()
                    .context("range-not-sequence-or-mapping")?;
                only_known(fields, &["start", "end", "step"])?;
                Ok(format!(
                    "(range {} {} {})",
                    expr(get(fields, "start").context("range-missing-start")?)?,
                    expr(get(fields, "end").context("range-missing-end")?)?,
                    expr(get(fields, "step").unwrap_or(&Value::Number(1.into())))?
                ))
            }
        }
        "mutable" | "mut" => Ok(format!("(mut {})", expr(payload)?)),
        "ref" => Ok(format!("(ref {})", expr(payload)?)),
        "not" => Ok(format!("(not {})", expr(payload)?)),
        "record" => {
            let fields = payload.as_mapping().context("record-not-mapping")?;
            record(fields)
        }
        "map" => {
            let pairs = payload.as_sequence().context("map-not-sequence")?;
            Ok(format!(
                "(map{})",
                pairs
                    .iter()
                    .map(|pair| {
                        let pair = pair.as_mapping().context("map-pair-not-mapping")?;
                        only_known(pair, &["key", "value"])?;
                        Ok(format!(
                            " ({} {})",
                            expr(get(pair, "key").context("map-missing-key")?)?,
                            expr(get(pair, "value").context("map-missing-value")?)?
                        ))
                    })
                    .collect::<Result<String>>()?
            ))
        }
        "template" => template_expression(payload),
        "wasm" => wasm_expression(payload),
        _ if payload.as_mapping().is_some_and(|arguments| {
            arguments
                .keys()
                .any(|key| key.as_str().is_some_and(|key| key.starts_with('$')))
        }) =>
        {
            Ok(format!("({head} {})", expr(payload)?))
        }
        _ => match payload {
            Value::Null => Ok(format!("({head})")),
            Value::Sequence(values) => Ok(format!(
                "({head}{})",
                values
                    .iter()
                    .map(expr)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(|v| format!(" {v}"))
                    .collect::<String>()
            )),
            Value::Mapping(arguments) => named_call(&head, arguments),
            _ => Ok(format!("({head} {})", expr(payload)?)),
        },
    }
}

fn template_expression(payload: &Value) -> Result<String> {
    let fields = payload.as_mapping().context("template-not-mapping")?;
    only_known(fields, &["path", "with"])?;
    let path = quoted(as_str(
        get(fields, "path").context("template-missing-path")?,
        "template-path",
    )?);
    let bindings = get(fields, "with")
        .and_then(Value::as_mapping)
        .context("template-with-not-mapping")?;
    Ok(format!("(template {path} with: {})", record(bindings)?))
}

fn wasm_expression(payload: &Value) -> Result<String> {
    let fields = payload.as_mapping().context("wasm-not-mapping")?;
    only_known(fields, &["import", "args"])?;
    let import = get(fields, "import")
        .and_then(Value::as_mapping)
        .context("wasm-import-not-mapping")?;
    only_known(import, &["module", "name"])?;
    let module = quoted(as_str(
        get(import, "module").context("wasm-import-missing-module")?,
        "wasm-module",
    )?);
    let name = quoted(as_str(
        get(import, "name").context("wasm-import-missing-name")?,
        "wasm-name",
    )?);
    let args = get(fields, "args")
        .and_then(Value::as_sequence)
        .context("wasm-args-not-sequence")?
        .iter()
        .map(|value| match value {
            Value::String(value) if value.starts_with('$') => {
                Ok(format!("(arg {})", reference(value)))
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                Ok(format!("(const {})", expr(value)?))
            }
            _ => bail!("wasm-arg-not-reference-or-literal"),
        })
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    Ok(format!(
        "(wasm import: (import {module} {name}) args: ({args}))"
    ))
}

fn named_call(head: &str, arguments: &Mapping) -> Result<String> {
    let signature = CALL_SIGNATURES
        .with(|slot| slot.borrow().get(head).cloned())
        .with_context(|| {
            if head.contains('.') {
                format!("named-call-unresolved-qualified-callee-{head}")
            } else {
                format!("named-call-unresolved-local-callee-{head}")
            }
        })?;
    let ordered = signature
        .type_args
        .iter()
        .chain(&signature.value_args)
        .collect::<Vec<_>>();
    if signature.type_args.is_empty()
        && signature.value_args.len() == 1
        && arguments
            .keys()
            .any(|key| key.as_str().is_some_and(|key| key.starts_with('$')))
    {
        return Ok(format!("({head} {})", expression_mapping(arguments)?));
    }
    for key in arguments.keys() {
        let key = as_str(key, "named-call-argument")?;
        if !ordered.iter().any(|expected| expected.as_str() == key) {
            bail!("named-call-unknown-argument-{head}-{key}");
        }
    }
    let mut values = Vec::with_capacity(ordered.len());
    for name in ordered {
        let value = get(arguments, name)
            .with_context(|| format!("named-call-missing-argument-{head}-{name}"))?;
        values.push(expr(value)?);
    }
    Ok(format!(
        "({head}{})",
        values
            .into_iter()
            .map(|value| format!(" {value}"))
            .collect::<String>()
    ))
}

fn record(map: &Mapping) -> Result<String> {
    Ok(format!(
        "(record{})",
        map.iter()
            .map(|(key, value)| Ok(format!(
                " ({} {})",
                sym(as_str(key, "record-key")?),
                expr(value)?
            )))
            .collect::<Result<Vec<_>>>()?
            .join("")
    ))
}

fn ty(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(sym(value.trim_start_matches('$'))),
        Value::Mapping(map) if map.len() == 1 => {
            let (key, payload) = map.iter().next().unwrap();
            type_form(as_str(key, "type-head")?, payload)
        }
        _ => bail!("unsupported-type"),
    }
}

fn type_form(head: &str, payload: &Value) -> Result<String> {
    let head = sym(head.trim_start_matches('$'));
    if head == "fn-type" {
        let fields = payload.as_mapping().context("fn-type-not-mapping")?;
        only_known(fields, &["args", "return"])?;
        let args = get(fields, "args").context("fn-type-missing-args")?;
        let args = if matches!(args.as_str(), Some("$void" | "void")) {
            Vec::new()
        } else if let Some(record) = args
            .as_mapping()
            .and_then(|args| get(args, "$record"))
            .and_then(Value::as_mapping)
        {
            record.values().map(ty).collect::<Result<Vec<_>>>()?
        } else {
            vec![ty(args)?]
        };
        return Ok(format!(
            "(fn-type ({}) {})",
            args.join(" "),
            ty(get(fields, "return").context("fn-type-missing-return")?)?
        ));
    }
    if matches!(
        head.as_str(),
        "newtype" | "array" | "mut" | "ref" | "mut-ref"
    ) {
        return Ok(format!("({head} {})", ty(payload)?));
    }
    if head == "policy" {
        return policy_type(payload);
    }
    if head == "capability" {
        if let Some(domain) = payload.as_str() {
            return Ok(format!("(capability {})", sym(domain)));
        }
        let map = payload.as_mapping().context("capability-not-mapping")?;
        if map.len() != 1 {
            bail!("capability-domain-count")
        }
        let (domain, groups) = map.iter().next().unwrap();
        return Ok(format!(
            "(capability {}{})",
            sym(as_str(domain, "capability-domain")?),
            policy_groups(groups)?
        ));
    }
    // The corpus spells capability and handle types with the domain or access
    // fused into the head: `$capability.clock: null`, `$handle.read: null`.
    // Legacy strips the prefix and parses the remainder as the domain/access
    // (`src/lower.rs`, `strip_prefix("$capability.")` and `"$handle."`). Both
    // must become one head with positional children, since the contract allows
    // no form to accept both a compact and an expanded spelling.
    if let Some(domain) = head.strip_prefix("capability.") {
        if domain.is_empty() {
            bail!("capability-domain-missing")
        }
        return Ok(format!(
            "(capability {}{})",
            sym(domain),
            policy_groups(payload)?
        ));
    }
    if let Some(access) = head.strip_prefix("handle.") {
        if access.is_empty() {
            bail!("handle-access-missing")
        }
        if !payload.is_null() {
            bail!("handle-body-not-null")
        }
        return Ok(format!("(handle {})", sym(access)));
    }
    match payload {
        Value::Null => Ok(head),
        Value::String(value) if value == "$void" => Ok(format!("({head})")),
        Value::Sequence(values) => Ok(format!(
            "({head}{})",
            values
                .iter()
                .map(ty)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|v| format!(" {v}"))
                .collect::<String>()
        )),
        Value::Mapping(fields)
            if matches!(head.as_str(), "record" | "enum" | "iface" | "interface") =>
        {
            let canonical = if head == "iface" { "interface" } else { &head };
            Ok(format!(
                "({canonical}{})",
                fields
                    .iter()
                    .map(|(key, value)| Ok(format!(
                        " ({} {})",
                        sym(as_str(key, "type-member")?),
                        ty(value)?
                    )))
                    .collect::<Result<Vec<_>>>()?
                    .join("")
            ))
        }
        Value::Mapping(arguments) => {
            let parameters = TYPE_SIGNATURES.with(|slot| slot.borrow().get(&head).cloned());
            let parameters = match parameters {
                Some(parameters) => parameters,
                None if head == "map" => vec!["key".to_string(), "value".to_string()],
                None => bail!("type-application-unresolved-parameters-{head}"),
            };
            for key in arguments.keys() {
                let key = as_str(key, "type-argument")?;
                if !parameters.iter().any(|parameter| parameter == key) {
                    bail!("type-application-unknown-argument-{head}-{key}");
                }
            }
            let values = parameters
                .iter()
                .map(|parameter| {
                    ty(get(arguments, parameter).with_context(|| {
                        format!("type-application-missing-argument-{head}-{parameter}")
                    })?)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "({head}{})",
                values
                    .into_iter()
                    .map(|value| format!(" {value}"))
                    .collect::<String>()
            ))
        }
        _ => Ok(format!("({head} {})", ty(payload)?)),
    }
}

fn policy_type(payload: &Value) -> Result<String> {
    let domains = payload.as_mapping().context("policy-domains-not-mapping")?;
    Ok(format!(
        "(policy{})",
        domains
            .iter()
            .map(|(domain, groups)| {
                Ok(format!(
                    " ({}{})",
                    sym(as_str(domain, "policy-domain")?),
                    policy_groups(groups)?
                ))
            })
            .collect::<Result<String>>()?
    ))
}

fn policy_groups(value: &Value) -> Result<String> {
    if value.is_null() {
        return Ok(String::new());
    }
    value
        .as_sequence()
        .context("policy-groups-not-sequence")?
        .iter()
        .map(|group| {
            let map = group.as_mapping().context("policy-group-not-mapping")?;
            only_known(map, &["requirement", "scopes"])?;
            Ok(format!(
                " (group requirement: {} scopes: {})",
                sym(as_str(
                    get(map, "requirement").context("policy-group-missing-requirement")?,
                    "requirement"
                )?),
                policy_scopes(get(map, "scopes").context("policy-group-missing-scopes")?)?
            ))
        })
        .collect()
}

fn policy_scopes(value: &Value) -> Result<String> {
    if value.as_str() == Some("any") {
        return Ok("((any))".into());
    }
    let scopes = value.as_sequence().context("policy-scopes-not-sequence")?;
    Ok(format!(
        "({})",
        scopes
            .iter()
            .map(|scope| {
                let map = scope.as_mapping().context("policy-scope-not-mapping")?;
                if map.len() != 1 {
                    bail!("policy-scope-selector-count")
                }
                let (selector, value) = map.iter().next().unwrap();
                Ok(format!(
                    "({} {})",
                    sym(as_str(selector, "policy-scope-selector")?),
                    quoted(as_str(value, "policy-scope-value")?)
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(" ")
    ))
}

fn declaration_annotations(map: &Mapping) -> Result<String> {
    let mut output = String::new();
    if let Some(doc) = get(map, "=doc") {
        output.push_str(&format!(" doc: {}", quoted(as_str(doc, "=doc")?)));
    }
    if let Some(where_clause) = get(map, "=where") {
        let parameters = where_clause.as_mapping().context("where-not-mapping")?;
        output.push_str(&format!(
            " where: ({})",
            parameters
                .iter()
                .map(|(name, bounds)| {
                    let name = sym(as_str(name, "type-parameter")?);
                    let bounds = bounds.as_sequence().context("where-bounds-not-sequence")?;
                    Ok(format!(
                        "({}{})",
                        name,
                        bounds
                            .iter()
                            .map(ty)
                            .collect::<Result<Vec<_>>>()?
                            .into_iter()
                            .map(|bound| format!(" {bound}"))
                            .collect::<String>()
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(" ")
        ));
    }
    if let Some(definitions) = get(map, "=defs") {
        let definitions = definitions.as_mapping().context("defs-not-mapping")?;
        output.push_str(&format!(
            " defs: ({})",
            definitions
                .iter()
                .map(|(name, definition)| {
                    let name = as_str(name, "def-name")?;
                    let definition = definition.as_mapping().context("def-not-mapping")?;
                    let primary = get(definition, "$function")
                        .or_else(|| get(definition, "$fn"))
                        .context("def-not-function")?;
                    function_form(name, primary, definition)
                })
                .collect::<Result<Vec<_>>>()?
                .join(" ")
        ));
    }
    if let Some(implementations) = get(map, "=impl") {
        let implementations = implementations.as_mapping().context("impl-not-mapping")?;
        output.push_str(&format!(
            " impls: ({})",
            implementations
                .iter()
                .map(|(interface, methods)| {
                    let interface = ty(interface)?;
                    let methods = methods.as_mapping().context("impl-methods-not-mapping")?;
                    Ok(format!(
                        "(impl {interface} methods: ({}))",
                        methods
                            .iter()
                            .map(|(name, method)| {
                                let name = as_str(name, "method-name")?;
                                if let Some(reference) = method.as_str() {
                                    return Ok(format!(
                                        "(method {} {})",
                                        sym(name),
                                        sym(reference)
                                    ));
                                }
                                let method = method.as_mapping().context("method-not-mapping")?;
                                let primary = get(method, "$function")
                                    .or_else(|| get(method, "$fn"))
                                    .context("method-not-function")?;
                                Ok(format!(
                                    "(method {} {})",
                                    sym(name),
                                    function_form(name, primary, method)?
                                ))
                            })
                            .collect::<Result<Vec<_>>>()?
                            .join(" ")
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(" ")
        ));
    }
    Ok(output)
}

fn infer_type(value: &Value) -> Result<&'static str> {
    match value {
        Value::Bool(_) => Ok("bool"),
        Value::Number(number) if number.is_i64() || number.is_u64() => Ok("int64"),
        Value::Number(_) => Ok("float64"),
        Value::String(_) => Ok("str"),
        _ => bail!("cannot-infer-constant-type"),
    }
}

/// Surface-tier check used only by the unit tests below, which validate
/// in-memory conversion output that has no real file path (and therefore no
/// meaningful document identity for the tiered pipeline `main` runs). Kept
/// separate from `surface_module`, which the CLI report uses and which needs
/// a path to derive a stable `DocumentId`.
#[cfg(test)]
fn validate(source: &str) -> Result<()> {
    let document = vibra::syntax::parse(source).map_err(|errors| anyhow::anyhow!("{errors:?}"))?;
    vibra::ast::lower_document(&document).map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(())
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}

fn only_known(map: &Mapping, known: &[&str]) -> Result<()> {
    for key in map.keys() {
        let key = as_str(key, "mapping-key")?;
        if !known.contains(&key) {
            bail!("unsupported-key-{key}")
        }
    }
    Ok(())
}

fn as_str<'a>(value: &'a Value, context: &str) -> Result<&'a str> {
    value
        .as_str()
        .with_context(|| format!("{context}-not-string"))
}

fn sym(value: &str) -> String {
    value.trim().trim_start_matches('$').to_string()
}

/// A legacy reference to a function's own parameter or a `$set` mutation
/// target -- `$args.name`, or the bare `args.name` spelling a `$set` target
/// key uses (YAML keys are never `$`-prefixed). The contract removes the
/// `$args.` envelope entirely (spec line 110: "`$args.name` is removed";
/// migration table: `| $args.x | x |`), so every occurrence strips to the
/// bare declared name, never `args.<name>` -- that spelling is not valid
/// S-expression surface. Ordinary `$foo`/`foo` references that never had the
/// `args.` prefix pass through unchanged, so this is safe to apply anywhere
/// a value or `$set` target reference is converted, not just wasm
/// forwarding.
fn reference(value: &str) -> String {
    let bare = sym(value);
    bare.strip_prefix("args.")
        .map(str::to_string)
        .unwrap_or(bare)
}

fn quoted(value: &str) -> String {
    format!("{value:?}")
}

fn record_issue(bucket: &mut BTreeMap<String, usize>, reason: String) {
    *bucket.entry(reason).or_default() += 1;
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_direct_generic_heads_calls_and_trailing_metadata() {
        let output = migrate(
            r#"
test:
  $import: ../stdlib/src/test.vibra
answer:
  $function:
    value: $int64
  return: $array
  do:
    - $return:
        $array: [$args.value]
works:
  $test: core
  tags: [fast]
  do:
    - $test.assert: true
"#,
        )
        .unwrap();
        assert!(output.contains("(import test \"../stdlib/src/test.vibra\")"));
        assert!(output.contains("(fn answer ((value int64)) array"));
        // `$args.value` is the removed legacy parameter envelope (spec line
        // 110; migration table `| $args.x | x |`) -- it converts to the bare
        // declared name, never `args.value`.
        assert!(output.contains("(array value)"));
        assert!(output.contains("tags: (fast)"));
        validate(&output).unwrap();
    }

    #[test]
    fn migrates_capability_and_handle_shorthand_heads() {
        // The corpus never spells these expanded: it fuses the domain or access
        // into the head, as `$capability.clock` and `$handle.read`. Emitting the
        // head verbatim produced a bare symbol reference, which the typed path
        // then correctly rejected as a malformed capability target.
        let output = migrate(
            r#"
clock-capability:
  $capability.clock: null
read-handle:
  $handle.read: null
"#,
        )
        .unwrap();
        assert!(
            output.contains("(capability clock)"),
            "capability shorthand not converted: {output}"
        );
        assert!(
            output.contains("(handle read)"),
            "handle shorthand not converted: {output}"
        );
        assert!(
            !output.contains("capability.clock") && !output.contains("handle.read"),
            "fused head leaked into output: {output}"
        );
        validate(&output).unwrap();
    }

    #[test]
    fn strips_the_removed_args_envelope_from_parameter_references() {
        // `$args.name` was the legacy parameter-forwarding envelope; the
        // contract removes it entirely (spec line 110: "`$args.name` is
        // removed"; migration table: `| $args.x | x |`). Every position that
        // can reference a function's own parameter must strip the whole
        // `args.` token, not just the `$` sigil -- an ordinary expression
        // reference, a `$wasm` forwarding spec, and a `$set` mutation target
        // (whose YAML key is bare `args.value`, with no `$` at all).
        let output = migrate(
            r#"
increment:
  $function:
    value:
      $mut: $int64
  return: $int64
  do:
  - $set:
      args.value: 4
  - $return: $args.value
scalar-len:
  $function:
    text: $str
  return: $uint64
  do:
  - $wasm:
      import:
        module: vibra_v1
        name: str_scalar_len
      args: [$args.text]
"#,
        )
        .unwrap();
        assert!(
            output.contains("(set value 4)"),
            "$set target kept the args. envelope: {output}"
        );
        assert!(
            output.contains("(return value)"),
            "expression reference kept the args. envelope: {output}"
        );
        assert!(
            output.contains("args: ((arg text))"),
            "wasm forwarding kept the args. envelope: {output}"
        );
        assert!(
            !output.contains("args."),
            "removed $args. envelope leaked into output: {output}"
        );
        validate(&output).unwrap();
    }

    #[test]
    fn capability_shorthand_keeps_explicit_policy_groups() {
        let output = migrate(
            r#"
scoped-reader:
  $capability.fs-read:
  - requirement: mandatory
    scopes:
    - exact: /tmp
"#,
        )
        .unwrap();
        assert!(
            output.contains("(capability fs-read (group"),
            "capability groups lost: {output}"
        );
        assert!(
            output.contains("requirement: mandatory"),
            "group requirement lost: {output}"
        );
        validate(&output).unwrap();
    }

    #[test]
    fn migrates_test_authority_with_explicit_policy_domains() {
        let output = migrate(
            r#"
privileged:
  $test: fs
  policy: {$policy: {fs-read: null}}
  do: []
"#,
        )
        .unwrap();
        assert!(output.contains("policy: (policy (fs-read))"));
        validate(&output).unwrap();
    }

    #[test]
    fn named_calls_follow_declared_type_and_value_argument_order() {
        let output = migrate(
            r#"
combine:
  =where:
    item: []
  $function:
    left: $int64
  args:
    right: $int64
  return: $int64
  do:
    - $return: $args.left
main:
  $function: $void
  return: $int64
  do:
    - $return:
        $combine:
          right: 2
          item: $int64
          left: 1
"#,
        )
        .unwrap();
        assert!(output.contains("(fn combine ((left int64) (right int64)) int64"));
        assert!(output.contains("(combine int64 1 2)"));
        assert!(!output.contains("(combine (record"));
        validate(&output).unwrap();
    }

    #[test]
    fn named_calls_fail_closed_without_a_safe_signature() {
        let qualified = migrate(
            r#"
main:
  $function: $void
  return: $void
  do:
    - $remote.call: {value: 1}
"#,
        )
        .unwrap_err();
        let qualified = format!("{qualified:#}");
        assert!(qualified.contains("named-call-unresolved-qualified-callee-remote.call"));

        let local = migrate(
            r#"
main:
  $function: $void
  return: $void
  do:
    - $unknown: {value: 1}
"#,
        )
        .unwrap_err();
        let local = format!("{local:#}");
        assert!(local.contains("named-call-unresolved-local-callee-unknown"));
    }

    #[test]
    fn qualified_calls_use_scanned_relative_import_signatures() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("lib.vibra");
        let main = temp.path().join("main.vibra");
        fs::write(
            &library,
            r#"
combine:
  $function:
    left: $int64
  args:
    right: $int64
  return: $int64
  do:
    - $return: $args.left
"#,
        )
        .unwrap();
        let source = r#"
lib:
  $import: ./lib.vibra
main:
  $function: $void
  return: $int64
  do:
    - $return:
        $lib.combine:
          right: 2
          left: 1
"#;
        fs::write(&main, source).unwrap();
        let index = build_signature_index(&[library, main.clone()]).unwrap();
        let output = migrate_for_path(source, &main, &index).unwrap();
        assert!(output.contains("(lib.combine 1 2)"));
        assert!(!output.contains("(lib.combine (record"));
        validate(&output).unwrap();
    }

    #[test]
    fn migrates_schema_known_record_template_and_wasm_expressions() {
        let output = migrate(
            r#"
assets:
  $function: $void
  return: $void
  do:
    - $record: {name: Vibra}
    - $template:
        path: greeting.txt
        with: {name: Vibra}
    - $wasm:
        import: {module: vibra_v1, name: io_write}
        args: [$args.output, 1, suffix]
"#,
        )
        .unwrap();
        assert!(output.contains("(record (name \"Vibra\"))"));
        assert!(output.contains("(template \"greeting.txt\" with: (record (name \"Vibra\")))"));
        // `$args.output` strips to the bare `output`, same as any other
        // `$args.` reference -- wasm forwarding is not a special case.
        assert!(output.contains(
            "(wasm import: (import \"vibra_v1\" \"io_write\") args: ((arg output) (const 1) (const \"suffix\")))"
        ));
        validate(&output).unwrap();
    }

    #[test]
    fn generic_type_and_owned_method_arguments_follow_declaration_order() {
        let output = migrate(
            r#"
pairing:
  $tuple: [$left, $right]
  =where:
    left: []
    right: []
box:
  $newtype: $t
  =where:
    t: []
  =defs:
    replace:
      =where:
        u: []
      $function: $self
      args:
        value: $u
      return: $u
      do:
        - $return: $args.value
mapper:
  $interface:
    map:
      $fn-type:
        args:
          $record:
            self: $self
            value: $t
        return: $t
  =where:
    t: []
demo:
  $function: $void
  return:
    $pairing:
      right: $str
      left: $int64
  do:
    - $box.replace:
        value: ok
        u: $str
        self: $boxed
        t: $int64
    - $mapper.map:
        value: 1
        self: $mapped
        t: $int64
"#,
        )
        .unwrap();
        assert!(output.contains("(pairing int64 str)"));
        assert!(output.contains("(box.replace int64 str boxed \"ok\")"));
        assert!(output.contains("(mapper.map int64 mapped 1)"));
        validate(&output).unwrap();
    }

    #[test]
    fn scan_covers_repository_source_kinds_and_excludes_work_areas() {
        let temp = tempfile::tempdir().unwrap();
        let included = [
            "tests/case.vibra",
            "examples/demo.vibra",
            "stdlib/src/core.vibra",
            "fixtures/input.vibra",
            "templates/app.vibra",
            "macros/expanded.vibra",
            "generated/source.vibra",
            "dep/package/lib.vibra",
        ];
        let excluded = [
            "target/debug/build.vibra",
            ".git/objects/fake.vibra",
            ".worktrees/other/test.vibra",
            "worktrees/other/test.vibra",
        ];
        for relative in included.iter().chain(excluded.iter()) {
            let path = temp.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "(const x int64 1)").unwrap();
        }
        let mut files = Vec::new();
        collect(temp.path(), &mut files).unwrap();
        files.sort();
        let relative = files
            .iter()
            .map(|path| {
                path.strip_prefix(temp.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        for expected in included {
            assert!(
                relative.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
        for unexpected in excluded {
            assert!(
                !relative.contains(&unexpected.to_string()),
                "included {unexpected}"
            );
        }
    }
}
