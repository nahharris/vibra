//! Formatter and JSON-first linter support for the Vibra CLI.

pub use crate::diagnostics::{
    file_uri, Category, Diagnostic, Fix, FixApplicability, Position, RelatedDiagnostic, Severity,
    Span, TextEdit,
};
use crate::{load, lower};
use anyhow::{bail, Context, Result};
use glob::glob;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutputFormat {
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintOutputFormat {
    Json,
    Sarif,
}

#[derive(Debug, Serialize)]
pub struct FmtFileReport {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct FmtSummary {
    pub files: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub written: usize,
}

#[derive(Debug, Serialize)]
pub struct FmtReport {
    pub files: Vec<FmtFileReport>,
    pub summary: FmtSummary,
}

#[derive(Debug, Serialize)]
pub struct LintSummary {
    pub files: usize,
    pub fixed: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}

#[derive(Debug, Serialize)]
pub struct LintReport {
    pub diagnostics: Vec<Diagnostic>,
    pub summary: LintSummary,
}

#[derive(Debug)]
pub struct FmtOptions {
    pub inputs: Vec<PathBuf>,
    pub write: bool,
    pub output: ToolOutputFormat,
}

#[derive(Debug)]
pub struct LintOptions {
    pub inputs: Vec<PathBuf>,
    pub format: LintOutputFormat,
    pub categories: Vec<Category>,
    pub severity: Option<Severity>,
    pub deny_warnings: bool,
    pub fix: bool,
}

pub fn run_fmt(options: FmtOptions) -> Result<bool> {
    let files = discover_vibra_files(&options.inputs)?;
    let mut reports = Vec::new();
    let mut changed = 0;
    let mut written = 0;
    let mut unchanged = 0;

    for path in files {
        let original =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let formatted = format_source(&path, &original)
            .with_context(|| format!("format {}", path.display()))?;
        let is_changed = formatted != original;
        let status = if is_changed && options.write {
            fs::write(&path, formatted).with_context(|| format!("write {}", path.display()))?;
            written += 1;
            "written"
        } else if is_changed {
            changed += 1;
            "changed"
        } else {
            unchanged += 1;
            "unchanged"
        };
        reports.push(FmtFileReport {
            path: display_path(&path),
            status: status.to_string(),
        });
    }

    let report = FmtReport {
        summary: FmtSummary {
            files: reports.len(),
            changed,
            unchanged,
            written,
        },
        files: reports,
    };
    print_structured_report(&report, options.output)?;
    Ok(options.write || report.summary.changed == 0)
}

/// Source is always S-expression (see `src/load.rs` module docs): this
/// delegates to `sexpr_tooling::staged_format_sexpr`, the reader-typed,
/// serde-free canonical printer, rather than the legacy YAML-subset path.
pub fn format_source(path: &Path, source: &str) -> Result<String> {
    if path.file_name().and_then(|name| name.to_str()) == Some(crate::project::MANIFEST_FILE) {
        let document = crate::syntax::parse(source)?;
        return Ok(crate::syntax::print(&document));
    }
    crate::sexpr_tooling::staged_format_sexpr(path, source)
        .map_err(|diagnostic| anyhow::anyhow!("{}: {}", diagnostic.code, diagnostic.message))
}

/// Produce syntax and style diagnostics for an in-memory editor document.
/// Compile diagnostics intentionally remain workspace/disk based.
pub fn diagnostics_for_source(path: &Path, source: &str) -> Vec<Diagnostic> {
    let suppressions = Suppressions::parse(source);
    let mut diagnostics = crate::sexpr_tooling::staged_sexpr_diagnostics(path, source);
    if diagnostics.is_empty() {
        diagnostics.extend(crate::sexpr_tooling::staged_lint_sexpr(path, source));
    }
    diagnostics.retain(|diagnostic| {
        diagnostic.severity == Severity::Error
            || !suppressions.suppresses(&diagnostic.code, diagnostic.span.start.line)
    });
    diagnostics
}

pub fn run_lint(options: LintOptions) -> Result<bool> {
    let files = discover_vibra_files(&options.inputs)?;
    let mut active_categories = active_categories(&options.categories);
    if options.fix {
        // `--fix` is explicitly an opt-in write operation and includes the
        // two compiler-backed safe edits as well as style edits.
        active_categories.insert(Category::Compile);
    }
    let mut diagnostics = Vec::new();
    let mut fixed = 0;

    for path in &files {
        let source =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        if path.file_name().and_then(|name| name.to_str()) == Some(crate::project::MANIFEST_FILE) {
            continue;
        }
        let source = if options.fix {
            let (updated, applied) = apply_safe_fixes(path, &source, &active_categories)?;
            if updated != source {
                fs::write(path, &updated)
                    .with_context(|| format!("write lint fixes to {}", path.display()))?;
            }
            fixed += applied;
            updated
        } else {
            source
        };
        diagnostics.extend(collect_file_diagnostics(
            path,
            &source,
            &active_categories,
            active_categories.contains(&Category::Compile),
        ));
    }

    if let Some(min_severity) = options.severity {
        diagnostics.retain(|diagnostic| diagnostic.severity <= min_severity);
    }

    diagnostics.sort_by(|a, b| {
        a.span
            .uri
            .cmp(&b.span.uri)
            .then(a.span.start.line.cmp(&b.span.start.line))
            .then(a.span.start.column.cmp(&b.span.start.column))
            .then(a.code.cmp(&b.code))
    });

    let report = LintReport {
        summary: lint_summary(files.len(), fixed, &diagnostics),
        diagnostics,
    };
    print_lint_report(&report, options.format)?;
    let has_errors = report.summary.errors > 0;
    let denied_warnings = options.deny_warnings && report.summary.warnings > 0;
    Ok(!has_errors && !denied_warnings)
}

fn print_structured_report<T: Serialize>(report: &T, format: ToolOutputFormat) -> Result<()> {
    match format {
        ToolOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
    }
    Ok(())
}

fn print_lint_report(report: &LintReport, format: LintOutputFormat) -> Result<()> {
    match format {
        LintOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        LintOutputFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(&sarif_report(report))?);
        }
    }
    Ok(())
}

fn lint_summary(files: usize, fixed: usize, diagnostics: &[Diagnostic]) -> LintSummary {
    let mut summary = LintSummary {
        files,
        fixed,
        errors: 0,
        warnings: 0,
        infos: 0,
        hints: 0,
    };
    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Error => summary.errors += 1,
            Severity::Warning => summary.warnings += 1,
            Severity::Info => summary.infos += 1,
            Severity::Hint => summary.hints += 1,
        }
    }
    summary
}

fn collect_file_diagnostics(
    path: &Path,
    source: &str,
    active_categories: &BTreeSet<Category>,
    include_compile: bool,
) -> Vec<Diagnostic> {
    let suppressions = Suppressions::parse(source);
    // Source is always S-expression: style and compile passes are gated on a
    // valid reader/typed surface even when syntax is not selected for output.
    let syntax_diagnostics = crate::sexpr_tooling::staged_sexpr_diagnostics(path, source);
    let syntax_ok = syntax_diagnostics.is_empty();
    let mut file_diagnostics = Vec::new();
    if active_categories.contains(&Category::Syntax) {
        file_diagnostics.extend(syntax_diagnostics);
    }
    if syntax_ok && active_categories.contains(&Category::Style) {
        file_diagnostics.extend(crate::sexpr_tooling::staged_lint_sexpr(path, source));
    }
    if syntax_ok && active_categories.contains(&Category::Compile) && include_compile {
        file_diagnostics.extend(compile_diagnostics(path));
    }
    file_diagnostics
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.severity == Severity::Error
                || !suppressions.suppresses(&diagnostic.code, diagnostic.span.start.line)
        })
        .collect()
}

fn apply_safe_fixes(
    path: &Path,
    source: &str,
    active_categories: &BTreeSet<Category>,
) -> Result<(String, usize)> {
    let mut current = source.to_string();
    let mut applied = 0;
    let mut seen = HashSet::from([current.clone()]);
    let mut iterations = 0usize;
    let baseline_compile_errors = compile_diagnostics(path)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.code)
        .collect::<BTreeSet<_>>();
    loop {
        iterations += 1;
        if iterations > 1024 {
            bail!(
                "lint autofix iteration limit exceeded for {}",
                path.display()
            );
        }
        // Compile diagnostics read the current on-disk source. They are
        // included on the first pass (before any candidate write); subsequent
        // passes use the in-memory candidate and therefore only need the
        // syntax/style stages whose spans are derived from that candidate.
        let include_compile = current == source;
        let diagnostics =
            collect_file_diagnostics(path, &current, active_categories, include_compile);
        let (next, count) = apply_diagnostic_fixes(path, &current, &diagnostics);
        if count == 0 || next == current {
            break;
        }
        if !seen.insert(next.clone()) {
            bail!("lint autofix cycle detected for {}", path.display());
        }
        validate_candidate_source(path, &next, &baseline_compile_errors)?;
        current = next;
        applied += count;
    }
    Ok((current, applied))
}

fn validate_candidate_source(
    path: &Path,
    source: &str,
    baseline_compile_errors: &BTreeSet<String>,
) -> Result<()> {
    let syntax_errors = crate::sexpr_tooling::staged_sexpr_diagnostics(path, source)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    if !syntax_errors.is_empty() {
        bail!("safe lint fix validation failed for {}", path.display());
    }

    // Type-check the candidate in a sibling temporary file so relative
    // imports resolve exactly as they do for the user's source.  Existing
    // compile errors are allowed to remain; a safe fix must not introduce a
    // new error when the original source was otherwise valid.
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let suffix = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_else(|| ".vib".to_string());
    let mut temporary = tempfile::Builder::new()
        .prefix(".vibra-lint-fix-")
        .suffix(&suffix)
        .tempfile_in(parent)
        .with_context(|| format!("create lint validation file beside {}", path.display()))?;
    temporary
        .as_file_mut()
        .set_len(0)
        .context("truncate lint validation file")?;
    temporary
        .as_file_mut()
        .seek(SeekFrom::Start(0))
        .context("seek lint validation file")?;
    temporary
        .as_file_mut()
        .write_all(source.as_bytes())
        .context("write lint validation file")?;
    temporary
        .as_file_mut()
        .flush()
        .context("flush lint validation file")?;
    let compile_errors = compile_diagnostics(temporary.path())
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    if compile_errors
        .iter()
        .any(|diagnostic| !baseline_compile_errors.contains(&diagnostic.code))
    {
        bail!("safe lint fix validation failed for {}", path.display());
    }
    Ok(())
}

fn apply_diagnostic_fixes(
    path: &Path,
    source: &str,
    diagnostics: &[Diagnostic],
) -> (String, usize) {
    #[derive(Clone)]
    struct Candidate {
        edits: Vec<TextEdit>,
        start: usize,
    }

    let uri = file_uri(path);
    let mut candidates = diagnostics
        .iter()
        .flat_map(|diagnostic| diagnostic.fix.as_ref().into_iter().flatten())
        .filter_map(|fix| {
            if fix.edits.iter().any(|edit| edit.span.uri != uri) {
                return None;
            }
            let edits = fix.edits.clone();
            let start = edits
                .iter()
                .filter_map(|edit| edit.span.start.offset)
                .min()?;
            (!edits.is_empty()).then_some(Candidate { edits, start })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.start.cmp(&left.start));

    let mut selected = Vec::new();
    let mut selected_ranges = Vec::<(usize, usize)>::new();
    for candidate in candidates {
        let mut ranges = Vec::new();
        let mut valid = true;
        for edit in &candidate.edits {
            let Some(start) = edit.span.start.offset else {
                valid = false;
                break;
            };
            let Some(end) = edit.span.end.offset else {
                valid = false;
                break;
            };
            if start > end || end > source.len() {
                valid = false;
                break;
            }
            if ranges
                .iter()
                .any(|(other_start, other_end)| start < *other_end && *other_start < end)
                || selected_ranges
                    .iter()
                    .any(|(other_start, other_end)| start < *other_end && *other_start < end)
            {
                valid = false;
                break;
            }
            ranges.push((start, end));
        }
        if valid {
            selected_ranges.extend(ranges);
            selected.push(candidate.edits);
        }
    }
    let selected_count = selected.len();
    let mut edits = selected.into_iter().flatten().collect::<Vec<_>>();
    edits.sort_by(|left, right| {
        right
            .span
            .start
            .offset
            .unwrap_or_default()
            .cmp(&left.span.start.offset.unwrap_or_default())
    });
    let count = selected_count;
    let mut output = source.to_string();
    for edit in edits {
        let (Some(start), Some(end)) = (edit.span.start.offset, edit.span.end.offset) else {
            continue;
        };
        output.replace_range(start..end, &edit.replacement);
    }
    (output, count)
}

fn sarif_report(report: &LintReport) -> serde_json::Value {
    let mut rules = BTreeMap::new();
    for diagnostic in &report.diagnostics {
        rules.entry(diagnostic.code.clone()).or_insert_with(|| {
            json!({
                "id": diagnostic.code,
                "shortDescription": { "text": rule_summary(&diagnostic.code) },
                "fullDescription": { "text": rule_summary(&diagnostic.code) },
            })
        });
    }
    let results: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let mut result = json!({
                "ruleId": diagnostic.code,
                "level": sarif_level(diagnostic.severity),
                "message": { "text": diagnostic.message },
                "locations": [{
                    "physicalLocation": sarif_physical_location(&diagnostic.span)
                }]
            });
            if let Some(related) = &diagnostic.related {
                let related_locations: Vec<_> = related
                    .iter()
                    .enumerate()
                    .map(|(id, related)| {
                        json!({
                            "id": id,
                            "message": { "text": related.message },
                            "physicalLocation": sarif_physical_location(&related.span),
                        })
                    })
                    .collect();
                if !related_locations.is_empty() {
                    result["relatedLocations"] = serde_json::Value::Array(related_locations);
                }
            }
            result
        })
        .collect();

    json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "vibra lint",
                    "rules": rules.into_values().collect::<Vec<_>>()
                }
            },
            "results": results
        }]
    })
}

fn sarif_physical_location(span: &Span) -> serde_json::Value {
    json!({
        "artifactLocation": { "uri": span.uri },
        "region": {
            "startLine": span.start.line + 1,
            "startColumn": span.start.column + 1,
            "endLine": span.end.line + 1,
            "endColumn": span.end.column + 1,
        }
    })
}

fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
        Severity::Hint => "note",
    }
}

fn rule_summary(code: &str) -> &'static str {
    match code {
        "W-STYLE-001" => "Symbol-like key is not kebab-case",
        "W-STYLE-003" => "Redundant sequencing do block",
        "W-EFFECT-001" => "Declaration includes nominal effects not inferred from its body",
        "W-RESULT-001" => "Result or option value is unhandled in statement position",
        "W-BIND-001" => "Binding is never read",
        "W-HANDLE-001" => "Handle binding is used after a same-binding close",
        "W-HANDLE-002" => "Handle binding is closed more than once",
        "E-SCOPE-001" => "Lexical binding shadows an enclosing binding",
        "E-YAML-001" => "YAML parse or strict-subset violation",
        "E-YAML-002" => "YAML comments are forbidden",
        "E-COMMENT-001" => "`=comment` must be a string scalar",
        "E-LINT-001" => "Malformed or invalid `=lint` annotation",
        "E-ANNO-003" => "Annotation has no syntax in its mapping",
        "E-COMPILE-001" => "Vibra compile diagnostic",
        "E-ONE-001" => "Function declaration is not canonical labeled shorthand",
        "E-MUT-001" => "Malformed `$mut` wrapper",
        "E-SET-001" => "Malformed `$set` statement",
        "E-SET-002" => "`$set` target is not writable",
        "E-SET-003" => "`$set` value has the wrong type",
        "E-REF-001" => "Malformed `$ref` wrapper",
        "E-REF-002" => "`$ref` target cannot be resolved",
        "E-REF-003" => "Invalid reference access mode",
        "E-TASK-001" => "Structured task capture is not alias-safe",
        "E-TASK-002" => "Structured task body is malformed",
        "E-MOD-003" => "Import cycle detected",
        "E-MOD-004" => "Import alias must be declared directly",
        "E-ONE-007" => "Structured `$match` form is not canonical",
        "E-ONE-008" => "`$match` arm must use `case` instead of `pattern`",
        "E-ANNO-001" => "Unknown annotation key on a top-level definition",
        "E-ANNO-002" => "Legacy un-prefixed annotation key",
        "E-WHERE-002" => "`=where` bound list element does not resolve to an interface",
        "E-BOUND-001" => "Generic or interface bound is not satisfied",
        "E-CALL-IFACE-NOSELF" => "Interface-qualified call cannot dispatch without `$self`",
        "E-DISPATCH-001" => "Interface-qualified dispatch on generic static type is unsupported",
        "E-DOC-001" => "`=doc` annotation must be a string scalar",
        "E-GEN-001" => "Generic type alias requires explicit instantiation",
        "E-GEN-002" => "Generic alias instantiation is malformed",
        "E-NEWTYPE-001" => "Implicit newtype coercion is forbidden",
        "E-NEWTYPE-002" => "Malformed `$newtype` definition body",
        "E-CAST-001" => "`$cast` has no valid v1 cast path",
        "E-CAST-002" => "Malformed `$cast` payload",
        "E-CAP-001" => "Opaque host handles cannot be created from source",
        "E-CAP-005" => "Host-backed endpoint cannot be forged or cast",
        "E-SELF-001" => "Reserved `$self` type used outside allowed positions",
        "E-DEFS-001" => "Invalid `=defs` annotation",
        "E-IMPL-001" => "Invalid `=impl` annotation",
        "E-IMPL-002" => "`=impl` interface key does not resolve to an interface",
        "E-IMPL-003" => "`=impl` block is missing a required binding",
        "E-IMPL-004" => "`=impl` payload contains an unexpected key",
        "E-IMPL-005" => "`=impl` method signature does not match interface declaration",
        "E-IMPL-006" => "`=impl` method alias does not resolve",
        "E-OPTION-001" => "Noncanonical option representation",
        "E-EFFECT-001" => "Body performs a nominal effect outside its declared row",
        "E-EFFECT-002" => "Name in `effects:` does not resolve to an effect",
        "E-EFFECT-003" => "Impl method exceeds its interface's declared effects",
        "E-EFFECT-005" => "Host-import declaration does not cover registry effects",
        "E-EFFECT-004" => "Malformed effect construction",
        "E-EFFECT-006" => "`effects:` is only valid on a function declaration",
        "E-EFFECT-007" => "Reserved effect syntax (handler operands or row tail)",
        "E-DEFFECT-001" => "Malformed deffect declaration",
        "E-DEFFECT-002" => "A deffect body may contain only effect operations",
        "E-DEFFECT-003" => "Duplicate deffect operation",
        "E-DEFFECT-004" => "Deffect root collides with a top-level symbol",
        "E-DEFFECT-005" => "Invalid native deffect owner metadata",
        "E-INTRINSIC-001" => "Effectful intrinsic expressions are only valid in deffect operations",
        "E-INTRINSIC-002" => "Intrinsic name is missing",
        "E-INTRINSIC-003" => "Unknown closed intrinsic",
        "E-INTRINSIC-004" => "Intrinsic argument arity mismatch",
        "E-INTRINSIC-005" => "Intrinsic argument type mismatch",
        "E-EFFECT-008" => "Structural `(effect @domain @action)` syntax is removed",
        "E-WASM-008" => "Raw wasm is only valid in a deffect operation",
        "E-TY-001" => "Call argument type does not match the parameter type",
        "E-TY-002" => "Returned value type does not match the declared return type",
        "E-MATCH-001" => "`match` over an enum is missing an arm for a tag",
        "E-MATCH-002" => "`match` over an open-ended type requires a `_` arm",
        "E-SYN-013" => "Labelled operands must precede remainder or variadic operands",
        _ => "Vibra diagnostic",
    }
}

fn active_categories(categories: &[Category]) -> BTreeSet<Category> {
    if categories.is_empty() {
        return [Category::Style, Category::Syntax].into_iter().collect();
    }
    categories.iter().copied().collect()
}

/// Compile a saved entry path and return its structured compiler diagnostics.
/// Embedders that use in-memory documents should call this against an isolated
/// workspace mirror rather than modifying the user's files.
pub fn compile_diagnostics(path: &Path) -> Vec<Diagnostic> {
    compile_diagnostics_with_flags(path, &load::CompilationFlags::default())
}

pub fn compile_diagnostics_with_flags(
    path: &Path,
    flags: &load::CompilationFlags,
) -> Vec<Diagnostic> {
    let result = load::load_legacy_yaml_program(path, flags).and_then(|program| {
        let Some(entry) = program.modules.get(&program.entry) else {
            return Ok(Vec::new());
        };
        if contains_noncanonical_option(entry) {
            anyhow::bail!(
                "E-OPTION-001: noncanonical option representation; use the tagged stdlib option enum"
            );
        }
        let has_main = program
            .modules
            .get(&program.entry)
            .and_then(crate::legacy_value::Value::as_mapping)
            .is_some_and(|module| {
                module.contains_key(&crate::legacy_value::Value::String("main".to_string()))
            });
        let lower = if has_main {
            lower::lower_program(&program)
        } else {
            lower::lower_library_program(&program)
        };
        lower.map(|lowered| {
            lowered
                .body_diagnostics
                .into_iter()
                .map(|warning| body_warning_diagnostic(path, warning))
                .collect()
        })
    });
    match result {
        Ok(diagnostics) => diagnostics,
        Err(err) => {
            if let Some(error) = err.downcast_ref::<crate::frontend::ScopeValidationError>() {
                return vec![scope_diagnostic(error, path)];
            }
            let message = format!("{err:#}");
            let code = extract_diagnostic_code(&message).unwrap_or("E-COMPILE-001");
            vec![Diagnostic {
                code: code.to_string(),
                message,
                severity: Severity::Error,
                span: point_span(path, 0, 0),
                related: None,
                fix: None,
                category: Category::Compile,
            }]
        }
    }
}

fn body_warning_diagnostic(
    fallback_path: &Path,
    warning: crate::body_semantics::BodyDiagnostic,
) -> Diagnostic {
    let crate::body_semantics::BodyDiagnostic {
        code,
        message,
        span,
        fix,
    } = warning;
    let diagnostic_span = span
        .as_ref()
        .map(|location| {
            let source = fs::read_to_string(&location.path).unwrap_or_default();
            crate::sexpr_tooling::tooling_span(&location.path, &source, location.span)
        })
        .unwrap_or_else(|| point_span(fallback_path, 0, 0));
    let fixes = fix.map(|location| {
        let source = fs::read_to_string(&location.path).unwrap_or_default();
        let span = crate::sexpr_tooling::tooling_span(&location.path, &source, location.span);
        let replacement = match code {
            "W-BIND-001" => "_".to_string(),
            "W-RESULT-001" => source
                .get(location.span.range())
                .map(|expression| format!("(let _ {})", expression.trim()))
                .unwrap_or_default(),
            _ => String::new(),
        };
        let title = match code {
            "W-BIND-001" => "Replace unused binder with `_`",
            "W-RESULT-001" => "Discard result with an explicit `let`",
            _ => "Apply safe fix",
        };
        vec![Fix::safe(title, vec![TextEdit { span, replacement }])]
    });
    Diagnostic {
        code: code.to_string(),
        message,
        severity: Severity::Warning,
        span: diagnostic_span,
        related: None,
        fix: fixes,
        category: Category::Compile,
    }
}

fn contains_noncanonical_option(value: &crate::legacy_value::Value) -> bool {
    match value {
        crate::legacy_value::Value::Mapping(map) => {
            if let Some(option) =
                map.get(&crate::legacy_value::Value::String("$option".to_string()))
            {
                if !option.as_mapping().is_some_and(|type_args| {
                    type_args
                        .keys()
                        .all(|key| key.as_str().is_some_and(|name| !name.starts_with('$')))
                }) {
                    return true;
                }
            }
            if let Some(union) = map.get(&crate::legacy_value::Value::String("$union".to_string()))
            {
                if union.as_sequence().is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| item.as_str().is_some_and(|s| s == "$void"))
                }) {
                    return true;
                }
            }
            map.iter().any(|(key, value)| {
                contains_noncanonical_option(key) || contains_noncanonical_option(value)
            })
        }
        crate::legacy_value::Value::Sequence(items) => {
            items.iter().any(contains_noncanonical_option)
        }
        crate::legacy_value::Value::Tagged(tagged) => contains_noncanonical_option(&tagged.value),
        _ => false,
    }
}

fn extract_diagnostic_code(message: &str) -> Option<&'static str> {
    const KNOWN_CODES: &[&str] = &[
        "E-YAML-001",
        "E-YAML-002",
        "E-COMMENT-001",
        "E-LINT-001",
        "E-ANNO-003",
        "E-YAML-002",
        "E-YAML-003",
        "E-SYN-001",
        "E-SYN-013",
        "E-ONE-001",
        "E-ONE-002",
        "E-ONE-003",
        "E-ONE-004",
        "E-ONE-005",
        "E-ONE-006",
        "E-ONE-007",
        "E-ONE-008",
        "E-MOD-003",
        "E-MOD-004",
        "E-WASM-001",
        "E-ANNO-001",
        "E-ANNO-002",
        "E-WHERE-002",
        "E-BOUND-001",
        "E-CALL-IFACE-NOSELF",
        "E-DISPATCH-001",
        "E-DOC-001",
        "E-GEN-001",
        "E-GEN-002",
        "E-NEWTYPE-001",
        "E-NEWTYPE-002",
        "E-CAST-001",
        "E-CAST-002",
        "E-CAP-001",
        "E-CAP-005",
        "E-SELF-001",
        "E-DEFS-001",
        "E-IMPL-001",
        "E-IMPL-002",
        "E-IMPL-003",
        "E-IMPL-004",
        "E-IMPL-005",
        "E-IMPL-006",
        "E-OPTION-001",
        "E-MATCH-001",
        "E-MATCH-002",
        "E-TY-001",
        "E-TY-002",
        "E-EFFECT-001",
        "E-EFFECT-002",
        "E-EFFECT-003",
        "E-EFFECT-004",
        "E-EFFECT-005",
        "E-EFFECT-006",
        "E-EFFECT-007",
        "E-DEFFECT-001",
        "E-DEFFECT-002",
        "E-DEFFECT-003",
        "E-DEFFECT-004",
        "E-DEFFECT-005",
        "E-INTRINSIC-001",
        "E-INTRINSIC-002",
        "E-INTRINSIC-003",
        "E-INTRINSIC-004",
        "E-INTRINSIC-005",
        "E-EFFECT-008",
        "E-WASM-008",
        "E-SCOPE-001",
        "W-RESULT-001",
        "W-BIND-001",
    ];
    KNOWN_CODES
        .iter()
        .copied()
        .find(|code| message.contains(code))
}

fn scope_diagnostic(
    error: &crate::frontend::ScopeValidationError,
    fallback_path: &Path,
) -> Diagnostic {
    let primary = error.primary_location();
    let primary_path = error.path_for(primary).unwrap_or(fallback_path);
    let primary_source = fs::read_to_string(primary_path).unwrap_or_default();
    let related = error
        .error
        .related
        .iter()
        .map(|(message, location)| {
            let path = error.path_for(*location).unwrap_or(fallback_path);
            let source = fs::read_to_string(path).unwrap_or_default();
            RelatedDiagnostic {
                message: message.clone(),
                span: crate::sexpr_tooling::tooling_span(path, &source, location.span),
            }
        })
        .collect();
    Diagnostic {
        code: error.error.code.to_string(),
        message: error.error.to_string(),
        severity: Severity::Error,
        span: crate::sexpr_tooling::tooling_span(primary_path, &primary_source, primary.span),
        related: Some(related),
        fix: None,
        category: Category::Compile,
    }
}

fn point_span(path: &Path, line: usize, column: usize) -> Span {
    span_for_text(path, line, column, 1)
}

fn span_for_text(path: &Path, line: usize, column: usize, len: usize) -> Span {
    Span {
        uri: file_uri(path),
        start: Position {
            line,
            column,
            offset: None,
        },
        end: Position {
            line,
            column: column + len.max(1),
            offset: None,
        },
    }
}

fn discover_vibra_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let inputs: Vec<PathBuf> = if inputs.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        inputs.to_vec()
    };
    let mut files = BTreeSet::new();
    for input in inputs {
        let raw = input.to_string_lossy();
        if has_glob_meta(&raw) {
            for entry in glob(&raw).with_context(|| format!("invalid glob `{raw}`"))? {
                let path = entry.with_context(|| format!("read glob entry `{raw}`"))?;
                collect_path(&path, &mut files)?;
            }
        } else {
            collect_path(&input, &mut files)?;
        }
    }
    Ok(files.into_iter().collect())
}

fn collect_path(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    if path.is_dir() {
        collect_dir(path, files)
    } else if path.is_file() {
        if is_vibra_file(path) {
            files.insert(
                fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))?,
            );
        }
        Ok(())
    } else {
        bail!("path does not exist: {}", path.display())
    }
}

fn collect_dir(dir: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(
                name,
                ".git" | ".worktrees" | ".agents" | "target" | "tmp" | "dep"
            ) {
                continue;
            }
            collect_dir(&path, files)?;
        } else if is_vibra_file(&path) {
            files.insert(
                fs::canonicalize(&path).with_context(|| format!("resolve {}", path.display()))?,
            );
        }
    }
    Ok(())
}

fn has_glob_meta(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

fn is_vibra_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".vib")
}

#[cfg(test)]
mod discovery_tests {
    use super::{diagnostics_for_source, is_vibra_file};
    use std::path::Path;

    #[test]
    fn only_discovers_sexpression_vibra_sources() {
        assert!(is_vibra_file(Path::new("main.vib")));
        assert!(!is_vibra_file(Path::new("main.vibra")));
        assert!(!is_vibra_file(Path::new("main.vib.yaml")));
    }

    #[test]
    fn editor_diagnostics_use_sexpression_surface() {
        assert!(
            diagnostics_for_source(Path::new("main.vib"), "(const good-name int64 1)\n",)
                .is_empty()
        );
    }
}

fn display_path(path: &Path) -> String {
    path.display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

#[derive(Debug, Default)]
struct Suppressions {
    subtrees: Vec<(usize, usize, BTreeSet<String>)>,
}

impl Suppressions {
    fn parse(source: &str) -> Self {
        let mut suppressions = Self::default();
        let lines: Vec<_> = source.lines().collect();
        for (line_index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("=lint:") {
                continue;
            }
            let annotation_indent = line.len() - trimmed.len();
            let owner_start = if annotation_indent == 0 {
                0
            } else {
                (0..line_index)
                    .rev()
                    .find(|index| {
                        let candidate = lines[*index];
                        !candidate.trim().is_empty()
                            && candidate.len() - candidate.trim_start().len() < annotation_indent
                    })
                    .unwrap_or(0)
            };
            let owner_indent = lines[owner_start].len() - lines[owner_start].trim_start().len();
            let owner_end = if annotation_indent == 0 {
                lines.len()
            } else {
                ((line_index + 1)..lines.len())
                    .find(|index| {
                        let candidate = lines[*index];
                        !candidate.trim().is_empty()
                            && candidate.len() - candidate.trim_start().len() <= owner_indent
                    })
                    .unwrap_or(lines.len())
            };
            let lint_end = ((line_index + 1)..lines.len())
                .find(|index| {
                    let candidate = lines[*index];
                    !candidate.trim().is_empty()
                        && candidate.len() - candidate.trim_start().len() <= annotation_indent
                })
                .unwrap_or(lines.len());
            let inline = trimmed.trim_start_matches("=lint:").trim();
            let _lint_source = if inline.is_empty() {
                lines[line_index + 1..lint_end]
                    .iter()
                    .map(|line| {
                        line.get((annotation_indent + 2).min(line.len())..)
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                inline.to_string()
            };
            let codes = BTreeSet::new();
            suppressions.subtrees.push((owner_start, owner_end, codes));
        }
        suppressions
    }

    fn suppresses(&self, code: &str, line: usize) -> bool {
        self.subtrees
            .iter()
            .any(|(start, end, codes)| line >= *start && line < *end && code_matches(codes, code))
    }
}

fn code_matches(codes: &BTreeSet<String>, code: &str) -> bool {
    codes.contains("all") || codes.contains(code)
}
