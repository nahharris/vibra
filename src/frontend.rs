//! Staged typed S-expression frontend foundation.
//!
//! This module owns physical source selection, parsing, typed surface
//! validation, deterministic module-part merging, import discovery, and typed
//! macro expansion. Post-expansion symbol/import validation runs before the
//! remaining compile-time phases. This module deliberately has no conversion
//! to the legacy YAML value tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::ast::{
    self, DocumentId, Expr, ExprKind, Module, Pattern, PatternKind, TopLevel, TypeExpr,
    TypeExprKind,
};
use crate::load::{is_compilation_flag, CompilationFlags};
use crate::project_context::{self, ProjectImportContext};
use crate::syntax::{self, Document};

#[derive(Debug, Clone)]
pub struct SourcePart {
    pub path: PathBuf,
    pub source: String,
    pub document: Document,
    pub module: Module,
}

#[derive(Debug, Clone)]
pub struct SourceModule {
    pub path: PathBuf,
    pub parts: Vec<SourcePart>,
}

impl SourceModule {
    /// Iterate the logical module surface without erasing each physical
    /// document's identity or source-relative spans.
    pub fn forms(&self) -> impl Iterator<Item = &TopLevel> {
        self.parts.iter().flat_map(|part| part.module.forms.iter())
    }
}

#[derive(Debug)]
pub struct SurfaceProgram {
    pub entry: PathBuf,
    pub modules: BTreeMap<PathBuf, SourceModule>,
    pub module_parts: BTreeMap<PathBuf, Vec<PathBuf>>,
}

pub fn load_surface_program(entry: &Path, flags: &CompilationFlags) -> Result<SurfaceProgram> {
    let entry = canonical_module_path(entry)?;
    let project = project_context::discover_project_import_context(&entry)?;
    let mut modules = BTreeMap::new();
    let mut visiting = Vec::new();
    load_recursive(&entry, project.as_ref(), flags, &mut modules, &mut visiting)?;
    let imports = modules
        .iter()
        .map(|(path, module)| {
            Ok((
                path.clone(),
                resolved_module_imports(module, project.as_ref())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let typed_modules = modules
        .iter()
        .map(|(path, module)| {
            (
                path.clone(),
                module
                    .parts
                    .iter()
                    .map(|part| part.module.clone())
                    .collect(),
            )
        })
        .collect();
    let mut expanded = ast::expand_typed_macro_program(typed_modules, &imports)
        .context("expand typed S-expression macros")?;
    for (path, module) in &mut modules {
        let expanded_parts = expanded
            .remove(path)
            .with_context(|| format!("missing expanded module {}", path.display()))?;
        for (part, expanded_part) in module.parts.iter_mut().zip(expanded_parts) {
            part.module = expanded_part;
        }
    }
    validate_unique_symbols(&modules)?;
    validate_direct_import_aliases(&modules)?;
    for module in modules.values() {
        for part in &module.parts {
            reject_uncut_phases(&part.module, &part.path)?;
        }
    }
    let module_parts = modules
        .iter()
        .map(|(path, module)| {
            (
                path.clone(),
                module.parts.iter().map(|part| part.path.clone()).collect(),
            )
        })
        .collect();
    Ok(SurfaceProgram {
        entry,
        modules,
        module_parts,
    })
}

pub fn load_surface_entry_for_test_discovery(entry: &Path) -> Result<(PathBuf, SourceModule)> {
    let entry = canonical_module_path(entry)?;
    let module = load_source_module(&entry, &CompilationFlags::new(["test"]))?;
    Ok((entry, module))
}

fn load_recursive(
    path: &Path,
    project: Option<&ProjectImportContext>,
    flags: &CompilationFlags,
    modules: &mut BTreeMap<PathBuf, SourceModule>,
    visiting: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = canonical_module_path(path)?;
    if modules.contains_key(&path) {
        return Ok(());
    }
    if visiting.contains(&path) {
        bail!("E-MOD-003: import cycle includes `{}`", path.display());
    }
    visiting.push(path.clone());
    let module = load_source_module(&path, flags)?;
    let imports = module_imports(&module, project)?;
    for import in imports {
        load_recursive(&import, project, flags, modules, visiting)?;
    }
    visiting.pop();
    modules.insert(path, module);
    Ok(())
}

fn load_source_module(path: &Path, flags: &CompilationFlags) -> Result<SourceModule> {
    let part_paths = module_part_paths(path, flags)?;
    let mut parts = Vec::with_capacity(part_paths.len());
    let mut symbols = BTreeSet::new();
    for part_path in part_paths {
        let source = fs::read_to_string(&part_path)
            .with_context(|| format!("read {}", part_path.display()))?;
        let document = syntax::parse(&source)
            .with_context(|| format!("parse S-expression source {}", part_path.display()))?;
        let document_id = DocumentId::from_path(&part_path);
        let module = ast::lower_document_with_id(&document, document_id)
            .with_context(|| format!("validate typed source {}", part_path.display()))?;
        for form in &module.forms {
            if let Some(name) = top_level_name(form) {
                if !symbols.insert(name.to_string()) {
                    bail!(
                        "E-MOD-002: duplicate top-level symbol `{name}` across module parts of {}",
                        path.display()
                    );
                }
            }
        }
        parts.push(SourcePart {
            path: part_path,
            source,
            document,
            module,
        });
    }
    Ok(SourceModule {
        path: path.to_path_buf(),
        parts,
    })
}

fn reject_uncut_phases(module: &Module, path: &Path) -> Result<()> {
    for form in &module.forms {
        visit_top_level_exprs(form, &mut |expr| {
            let phase = match &expr.value {
                ExprKind::Embed { .. } => Some("embed"),
                ExprKind::Template { .. } => Some("template"),
                _ => None,
            };
            if let Some(phase) = phase {
                bail!(
                    "E-FRONT-002: `{phase}` expansion is not active in the S-expression frontend ({})",
                    path.display()
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn visit_top_level_exprs(
    form: &TopLevel,
    visitor: &mut impl FnMut(&Expr) -> Result<()>,
) -> Result<()> {
    match form {
        TopLevel::Constant(value) => visit_expr(&value.value, visitor),
        TopLevel::Function(function) => visit_exprs(&function.body, visitor),
        TopLevel::Test(test) => visit_exprs(&test.body, visitor),
        TopLevel::Import(_) | TopLevel::Definition(_) | TopLevel::Macro(_) => Ok(()),
    }
}

fn visit_exprs(exprs: &[Expr], visitor: &mut impl FnMut(&Expr) -> Result<()>) -> Result<()> {
    for expr in exprs {
        visit_expr(expr, visitor)?;
    }
    Ok(())
}

fn visit_expr(expr: &Expr, visitor: &mut impl FnMut(&Expr) -> Result<()>) -> Result<()> {
    visitor(expr)?;
    match &expr.value {
        ExprKind::Call { arguments, .. }
        | ExprKind::Do(arguments)
        | ExprKind::Tuple(arguments)
        | ExprKind::Array(arguments) => visit_exprs(arguments, visitor),
        ExprKind::Let { value, .. }
        | ExprKind::Set { value, .. }
        | ExprKind::Mutable(value)
        | ExprKind::ReferenceOf(value) => visit_expr(value, visitor),
        ExprKind::Return(value) => value
            .as_deref()
            .map_or(Ok(()), |value| visit_expr(value, visitor)),
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            visit_expr(condition, visitor)?;
            visit_exprs(then_body, visitor)?;
            visit_exprs(else_body, visitor)
        }
        ExprKind::While { condition, body } => {
            visit_expr(condition, visitor)?;
            visit_exprs(body, visitor)
        }
        ExprKind::For { source, body, .. } => {
            visit_expr(source, visitor)?;
            visit_exprs(body, visitor)
        }
        ExprKind::Match { target, cases } => {
            visit_expr(target, visitor)?;
            for case in cases {
                visit_exprs(&case.body, visitor)?;
            }
            Ok(())
        }
        ExprKind::Record(fields) => {
            for field in fields {
                visit_expr(&field.value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                visit_expr(key, visitor)?;
                visit_expr(value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Range(start, end, step) => {
            visit_expr(start, visitor)?;
            visit_expr(end, visitor)?;
            visit_expr(step, visitor)
        }
        ExprKind::Convert { value, .. } | ExprKind::Cast { value, .. } => {
            visit_expr(value, visitor)
        }
        ExprKind::Template { bindings, .. } => {
            for binding in bindings {
                visit_expr(&binding.value, visitor)?;
            }
            Ok(())
        }
        ExprKind::Task { body, .. } => visit_exprs(body, visitor),
        ExprKind::Spawn { value, .. } => visit_expr(value, visitor),
        ExprKind::Literal(_)
        | ExprKind::Reference(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Embed { .. }
        | ExprKind::Wasm { .. }
        | ExprKind::Join { .. } => Ok(()),
    }
}

fn module_imports(
    module: &SourceModule,
    project: Option<&ProjectImportContext>,
) -> Result<Vec<PathBuf>> {
    let mut imports = resolved_module_imports(module, project)?
        .into_values()
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    Ok(imports)
}

fn resolved_module_imports(
    module: &SourceModule,
    project: Option<&ProjectImportContext>,
) -> Result<BTreeMap<String, PathBuf>> {
    let parent = module
        .path
        .parent()
        .with_context(|| format!("{} has no parent directory", module.path.display()))?;
    let mut imports = BTreeMap::new();
    for form in module.forms() {
        let TopLevel::Import(import) = form else {
            continue;
        };
        let resolved = if import.path.value.starts_with('@') {
            let project = project.with_context(|| {
                format!(
                    "{}: @ import `{}` requires project.vibra",
                    module.path.display(),
                    import.path.value
                )
            })?;
            project_context::resolve_project_import(project, &import.path.value)?
        } else {
            parent.join(&import.path.value)
        };
        let resolved = canonical_module_path(&resolved).with_context(|| {
            format!(
                "{}: cannot resolve import `{}`",
                module.path.display(),
                import.path.value
            )
        })?;
        imports.insert(import.alias.value.clone(), resolved);
    }
    Ok(imports)
}

fn validate_unique_symbols(modules: &BTreeMap<PathBuf, SourceModule>) -> Result<()> {
    for module in modules.values() {
        let mut symbols = BTreeSet::new();
        for form in module.forms() {
            if let Some(name) = top_level_name(form) {
                if !symbols.insert(name) {
                    bail!(
                        "E-MOD-002: duplicate top-level symbol `{name}` across module parts of {}",
                        module.path.display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_direct_import_aliases(modules: &BTreeMap<PathBuf, SourceModule>) -> Result<()> {
    let known_import_aliases = modules
        .values()
        .flat_map(SourceModule::forms)
        .filter_map(|form| match form {
            TopLevel::Import(import) => Some(import.alias.value.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for module in modules.values() {
        let direct_imports = module
            .forms()
            .filter_map(|form| match form {
                TopLevel::Import(import) => Some(import.alias.value.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let local_symbols = module
            .forms()
            .filter_map(top_level_name)
            .filter(|symbol| !direct_imports.contains(symbol))
            .collect::<BTreeSet<_>>();
        let self_alias = module_self_alias(&module.path);

        for form in module.forms() {
            validate_top_level_aliases(
                form,
                &module.path,
                &known_import_aliases,
                &direct_imports,
                &local_symbols,
                self_alias.as_deref(),
            )?;
        }
    }
    Ok(())
}

fn validate_top_level_aliases(
    form: &TopLevel,
    path: &Path,
    known: &BTreeSet<&str>,
    direct: &BTreeSet<&str>,
    local: &BTreeSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    let mut validate_name =
        |name: &str| validate_reference_alias(name, path, known, direct, local, self_alias);
    match form {
        TopLevel::Definition(definition) => visit_type(&definition.body, &mut validate_name),
        TopLevel::Constant(constant) => {
            visit_type(&constant.ty, &mut validate_name)?;
            visit_expr_aliases(&constant.value, &mut validate_name)
        }
        TopLevel::Function(function) => {
            for parameter in &function.parameters {
                visit_type(&parameter.ty, &mut validate_name)?;
            }
            visit_type(&function.return_type, &mut validate_name)?;
            for expr in &function.body {
                visit_expr_aliases(expr, &mut validate_name)?;
            }
            Ok(())
        }
        TopLevel::Test(test) => {
            for expr in &test.body {
                visit_expr_aliases(expr, &mut validate_name)?;
            }
            Ok(())
        }
        TopLevel::Import(_) | TopLevel::Macro(_) => Ok(()),
    }
}

fn visit_expr_aliases(expr: &Expr, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    visit_expr(expr, &mut |expr| {
        match &expr.value {
            ExprKind::Reference(name) => visitor(name)?,
            ExprKind::Call { callee, .. } => visitor(&callee.value)?,
            ExprKind::Let { ty: Some(ty), .. } => visit_type(ty, visitor)?,
            ExprKind::Match { cases, .. } => {
                for case in cases {
                    visit_pattern(&case.pattern, visitor)?;
                }
            }
            ExprKind::Convert { into, .. } | ExprKind::Cast { into, .. } => {
                visit_type(into, visitor)?
            }
            _ => {}
        }
        Ok(())
    })
}

fn visit_type(ty: &TypeExpr, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    match &ty.value {
        TypeExprKind::Named(name) => visitor(name),
        TypeExprKind::Application {
            constructor,
            arguments,
        } => {
            visitor(&constructor.value)?;
            for argument in arguments {
                visit_type(argument, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Record(members)
        | TypeExprKind::Enum(members)
        | TypeExprKind::Interface(members) => {
            for member in members {
                visit_type(&member.ty, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Tuple(types)
        | TypeExprKind::Union(types)
        | TypeExprKind::Function {
            parameters: types, ..
        } => {
            for ty in types {
                visit_type(ty, visitor)?;
            }
            if let TypeExprKind::Function { result, .. } = &ty.value {
                visit_type(result, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Array(ty)
        | TypeExprKind::Newtype(ty)
        | TypeExprKind::Mutable(ty)
        | TypeExprKind::Reference(ty)
        | TypeExprKind::MutableReference(ty) => visit_type(ty, visitor),
        TypeExprKind::Map(key, value) => {
            visit_type(key, visitor)?;
            visit_type(value, visitor)
        }
        TypeExprKind::Intersect(types) => {
            for ty in types {
                visit_type(ty, visitor)?;
            }
            Ok(())
        }
        TypeExprKind::Policy(domains) => {
            for domain in domains {
                visitor(&domain.name.value)?;
            }
            Ok(())
        }
        TypeExprKind::Capability { domain, .. } | TypeExprKind::WasmValue(domain) => {
            visitor(&domain.value)
        }
        TypeExprKind::Handle(_) => Ok(()),
    }
}

fn visit_pattern(pattern: &Pattern, visitor: &mut impl FnMut(&str) -> Result<()>) -> Result<()> {
    match &pattern.value {
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            visitor(&constructor.value)?;
            for pattern in arguments {
                visit_pattern(pattern, visitor)?;
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                visit_pattern(&field.pattern, visitor)?;
            }
        }
        PatternKind::Tuple(patterns) | PatternKind::Array(patterns) => {
            for pattern in patterns {
                visit_pattern(pattern, visitor)?;
            }
        }
        PatternKind::Map(entries) => {
            for (key, value) in entries {
                visit_pattern(key, visitor)?;
                visit_pattern(value, visitor)?;
            }
        }
        PatternKind::Newtype { ty, pattern } | PatternKind::Interface { ty, pattern } => {
            visit_type(ty, visitor)?;
            visit_pattern(pattern, visitor)?;
        }
        PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Bind(_) => {}
    }
    Ok(())
}

fn validate_reference_alias(
    reference: &str,
    path: &Path,
    known: &BTreeSet<&str>,
    direct: &BTreeSet<&str>,
    local: &BTreeSet<&str>,
    self_alias: Option<&str>,
) -> Result<()> {
    let Some((alias, _)) = reference.split_once('.') else {
        return Ok(());
    };
    if direct.contains(alias)
        || local.contains(alias)
        || self_alias == Some(alias)
        || matches!(alias, "args" | "const" | "grants" | "policy" | "self")
        || !known.contains(alias)
    {
        return Ok(());
    }
    bail!(
        "E-MOD-004: module {} uses import alias `{alias}` without declaring it directly",
        path.display()
    )
}

fn module_self_alias(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    Some(
        file_name
            .strip_suffix(".vibra")
            .unwrap_or(file_name)
            .split('.')
            .next()
            .unwrap_or(file_name)
            .to_string(),
    )
}

fn top_level_name(form: &TopLevel) -> Option<&str> {
    match form {
        TopLevel::Import(value) => Some(&value.alias.value),
        TopLevel::Definition(value) => Some(&value.name.value),
        TopLevel::Constant(value) => Some(&value.name.value),
        TopLevel::Function(value) => Some(&value.name.value),
        TopLevel::Macro(value) => Some(&value.name.value),
        TopLevel::Test(value) => Some(&value.name.value),
    }
}

fn module_part_paths(module_path: &Path, flags: &CompilationFlags) -> Result<Vec<PathBuf>> {
    let mut paths = vec![module_path.to_path_buf()];
    let Some(parent) = module_path.parent() else {
        return Ok(paths);
    };
    let Some(file_name) = module_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(paths);
    };
    let Some(base) = file_name.strip_suffix(".vibra") else {
        bail!(
            "E-SYN-012: S-expression source must use the `.vibra` extension: {}",
            module_path.display()
        );
    };
    let prefix = format!("{base}.");
    for entry in fs::read_dir(parent).with_context(|| format!("read {}", parent.display()))? {
        let path = entry?.path();
        if path == module_path {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".vibra") else {
            continue;
        };
        let Some(suffix) = stem.strip_prefix(&prefix) else {
            continue;
        };
        let suffix_flags = suffix.split('.').collect::<Vec<_>>();
        if suffix.is_empty() || suffix_flags.iter().any(|flag| !is_compilation_flag(flag)) {
            bail!("E-FLAG-002: malformed conditional source suffix in `{file_name}`");
        }
        if suffix_flags.iter().all(|flag| flags.contains(flag)) {
            paths.push(fs::canonicalize(path)?);
        }
    }
    paths.sort();
    Ok(paths)
}

fn canonical_module_path(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 file name", path.display()))?;
    if file_name.ends_with(".vibra.yaml") {
        bail!("E-SYN-012: `.vibra.yaml` is not supported by the S-expression frontend");
    }
    if !file_name.ends_with(".vibra") {
        bail!(
            "E-SYN-012: S-expression source must use the `.vibra` extension: {}",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
    }

    #[test]
    fn loads_import_graph_and_merges_selected_parts_deterministically() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helper \"helper.vibra\")\n(fn main () void (do (helper.run)))\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(test main-runs core (do unit))\n",
        );
        write(
            &temp.path().join("main.unix.vibra"),
            "(const platform str \"unix\")\n",
        );
        write(
            &temp.path().join("helper.vibra"),
            "(fn run () void (do unit))\n",
        );
        let program = load_surface_program(&entry, &CompilationFlags::new(["test"])).unwrap();
        assert_eq!(program.modules.len(), 2);
        let entry_module = &program.modules[&program.entry];
        assert_eq!(entry_module.parts.len(), 2);
        assert_eq!(entry_module.forms().count(), 3);
        assert!(entry_module.parts[0].path.ends_with("main.test.vibra"));
        assert!(entry_module.parts[1].path.ends_with("main.vibra"));
        assert_ne!(
            entry_module.parts[0].module.document_id,
            entry_module.parts[1].module.document_id
        );
        for part in &entry_module.parts {
            assert_eq!(
                part.module.document_id,
                DocumentId::from_path(&part.path),
                "{}",
                part.path.display()
            );
        }
    }

    #[test]
    fn duplicate_symbols_across_parts_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(const answer int64 1)\n");
        write(
            &temp.path().join("main.test.vibra"),
            "(const answer int64 2)\n",
        );
        let error = load_surface_program(&entry, &CompilationFlags::new(["test"]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-002"), "{error}");
        assert!(error.contains("answer"), "{error}");
    }

    #[test]
    fn cycles_have_a_stable_error() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a.vibra");
        write(&a, "(import b \"b.vibra\")\n");
        write(&temp.path().join("b.vibra"), "(import a \"a.vibra\")\n");
        let error = load_surface_program(&a, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-003"), "{error}");
    }

    #[test]
    fn rejects_phases_that_are_not_cut_over() {
        for (source, code) in [
            ("(const x str (embed \"x.txt\"))\n", "E-FRONT-002"),
            (
                "(fn f () str (do (template \"x\" with: (record))))\n",
                "E-FRONT-002",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let entry = temp.path().join("main.vibra");
            write(&entry, source);
            let error = load_surface_program(&entry, &CompilationFlags::default())
                .unwrap_err()
                .to_string();
            assert!(error.contains(code), "{source}: {error}");
        }
    }

    #[test]
    fn expands_local_cross_part_and_imported_macros_before_validation() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helpers \"helpers.vibra\")\n\
             (fn main ((caller int64)) int64\n\
               (do (local caller) (helpers.use-helper caller)))\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(macro local ((value expr-syntax)) expr-syntax\n\
               (do (quote expr-syntax (unquote value))))\n",
        );
        write(
            &temp.path().join("helpers.vibra"),
            "(fn helper ((value int64)) int64 (do value))\n\
             (macro use-helper ((value expr-syntax)) expr-syntax\n\
               (do (quote expr-syntax (helper (unquote value)))))\n",
        );
        let program = load_surface_program(&entry, &CompilationFlags::new(["test"])).unwrap();
        let module = &program.modules[&program.entry];
        assert!(module
            .forms()
            .all(|form| !matches!(form, TopLevel::Macro(_))));
        let TopLevel::Function(function) = module
            .forms()
            .find(|form| matches!(form, TopLevel::Function(_)))
            .unwrap()
        else {
            unreachable!()
        };
        let ExprKind::Call { callee, .. } = &function.body[1].value else {
            panic!("expected imported macro expansion call");
        };
        assert_eq!(callee.value, "helpers.helper");
    }

    #[test]
    fn imported_private_macros_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helpers \"helpers.vibra\")\n\
             (fn main () void (do (helpers.secret unit)))\n",
        );
        write(
            &temp.path().join("helpers.vibra"),
            "(private (macro secret ((value expr-syntax)) expr-syntax\n\
               (do (unquote value))))\n",
        );
        let error = format!(
            "{:#}",
            load_surface_program(&entry, &CompilationFlags::default()).unwrap_err()
        );
        assert!(error.contains("E-MACRO-010"), "{error}");
    }

    #[test]
    fn entry_discovery_selects_test_parts_without_import_loading() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(&entry, "(import missing \"missing.vibra\")\n");
        write(
            &temp.path().join("main.test.vibra"),
            "(test selected core (do unit))\n",
        );
        let (_, module) = load_surface_entry_for_test_discovery(&entry).unwrap();
        assert_eq!(module.parts.len(), 2);
        assert!(module.forms().any(|form| matches!(form, TopLevel::Test(_))));
    }

    #[test]
    fn rejects_transitive_import_alias_references() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import middle \"middle.vibra\")\n(fn main () void (do (leaf.run)))\n",
        );
        write(
            &temp.path().join("middle.vibra"),
            "(import leaf \"leaf.vibra\")\n(fn run () void (do (leaf.run)))\n",
        );
        write(
            &temp.path().join("leaf.vibra"),
            "(fn run () void (do unit))\n",
        );

        let error = load_surface_program(&entry, &CompilationFlags::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("E-MOD-004"), "{error}");
        assert!(error.contains("leaf"), "{error}");
    }
}
