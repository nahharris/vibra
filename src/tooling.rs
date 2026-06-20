//! YAML-first formatter and linter support for the Vibra CLI.

use crate::{load, lower};
use anyhow::{bail, Context, Result};
use glob::glob;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use yaml_edit::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutputFormat {
    Yaml,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintOutputFormat {
    Yaml,
    Json,
    Sarif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Style,
    Syntax,
    Compile,
}

#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub uri: String,
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<Vec<RelatedDiagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Vec<serde_json::Value>>,
    #[serde(skip)]
    pub category: Category,
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
        let formatted =
            format_source(&original).with_context(|| format!("format {}", path.display()))?;
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

fn format_source(source: &str) -> Result<String> {
    let _ = Document::from_str(source).context("parse Vibra code document")?;
    let value: serde_yaml::Value = serde_yaml::from_str(source).context("parse Vibra YAML")?;
    serde_yaml::to_string(&value).context("emit canonical Vibra YAML")
}

pub fn run_lint(options: LintOptions) -> Result<bool> {
    let files = discover_vibra_files(&options.inputs)?;
    let active_categories = active_categories(&options.categories);
    let mut diagnostics = Vec::new();

    for path in &files {
        let source =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let suppressions = Suppressions::parse(&source);
        let mut file_diagnostics = Vec::new();
        let mut yaml_subset_ok = true;
        if active_categories.contains(&Category::Syntax) {
            for violation in crate::yaml_subset::validate_yaml_subset(&source) {
                yaml_subset_ok = false;
                file_diagnostics.push(yaml_subset_diagnostic(path, &violation));
            }
        }
        let syntax_ok = if active_categories.contains(&Category::Syntax) && yaml_subset_ok {
            match serde_yaml::from_str::<serde_yaml::Value>(&source) {
                Ok(_) => true,
                Err(err) => {
                    file_diagnostics.push(yaml_diagnostic(path, &err));
                    false
                }
            }
        } else if yaml_subset_ok {
            serde_yaml::from_str::<serde_yaml::Value>(&source).is_ok()
        } else {
            false
        };

        if syntax_ok && active_categories.contains(&Category::Style) {
            file_diagnostics.extend(style_diagnostics(path, &source));
        }

        if syntax_ok && active_categories.contains(&Category::Compile) {
            file_diagnostics.extend(compile_diagnostics(path));
        }

        diagnostics.extend(file_diagnostics.into_iter().filter(|diagnostic| {
            !suppressions.suppresses(&diagnostic.code, diagnostic.span.start.line)
        }));
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
        summary: lint_summary(files.len(), &diagnostics),
        diagnostics,
    };
    print_lint_report(&report, options.format)?;
    let has_errors = report.summary.errors > 0;
    let denied_warnings = options.deny_warnings && report.summary.warnings > 0;
    Ok(!has_errors && !denied_warnings)
}

fn print_structured_report<T: Serialize>(report: &T, format: ToolOutputFormat) -> Result<()> {
    match format {
        ToolOutputFormat::Yaml => {
            print!("{}", serde_yaml::to_string(report)?);
        }
        ToolOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
    }
    Ok(())
}

fn print_lint_report(report: &LintReport, format: LintOutputFormat) -> Result<()> {
    match format {
        LintOutputFormat::Yaml => {
            print!("{}", serde_yaml::to_string(report)?);
        }
        LintOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        LintOutputFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(&sarif_report(report))?);
        }
    }
    Ok(())
}

fn lint_summary(files: usize, diagnostics: &[Diagnostic]) -> LintSummary {
    let mut summary = LintSummary {
        files,
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

fn sarif_report(report: &LintReport) -> serde_json::Value {
    let mut rules = BTreeMap::new();
    for diagnostic in &report.diagnostics {
        rules.entry(diagnostic.code.clone()).or_insert_with(|| {
            json!({
                "id": diagnostic.code,
                "shortDescription": { "text": rule_summary(&diagnostic.code) },
            })
        });
    }
    let results: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "ruleId": diagnostic.code,
                "level": sarif_level(diagnostic.severity),
                "message": { "text": diagnostic.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": diagnostic.span.uri },
                        "region": {
                            "startLine": diagnostic.span.start.line + 1,
                            "startColumn": diagnostic.span.start.column + 1,
                            "endLine": diagnostic.span.end.line + 1,
                            "endColumn": diagnostic.span.end.column + 1,
                        }
                    }
                }]
            })
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
        "E-YAML-001" => "YAML parse or strict-subset violation",
        "E-COMPILE-001" => "Vibra compile diagnostic",
        "E-ONE-001" => "Function declaration is not canonical labeled shorthand",
        "E-MUT-001" => "Malformed `$mut` wrapper",
        "E-SET-001" => "Malformed `$set` statement",
        "E-SET-002" => "`$set` target is not writable",
        "E-SET-003" => "`$set` value has the wrong type",
        "E-REF-001" => "Malformed `$ref` wrapper",
        "E-REF-002" => "`$ref` target cannot be resolved",
        "E-REF-003" => "Invalid reference access mode",
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
        "E-CAP-001" => "Capability values cannot be created from source",
        "E-SELF-001" => "Reserved `$self` type used outside allowed positions",
        "E-DEFS-001" => "Invalid `=defs` annotation",
        "E-IMPL-001" => "Invalid `=impl` annotation",
        "E-IMPL-002" => "`=impl` interface key does not resolve to an interface",
        "E-IMPL-003" => "`=impl` block is missing a required binding",
        "E-IMPL-004" => "`=impl` payload contains an unexpected key",
        "E-IMPL-005" => "`=impl` method signature does not match interface declaration",
        "E-IMPL-006" => "`=impl` method alias does not resolve",
        "E-OPTION-001" => "Noncanonical option representation",
        _ => "Vibra diagnostic",
    }
}

fn active_categories(categories: &[Category]) -> BTreeSet<Category> {
    if categories.is_empty() {
        return [Category::Style, Category::Syntax, Category::Compile]
            .into_iter()
            .collect();
    }
    categories.iter().copied().collect()
}

fn yaml_diagnostic(path: &Path, err: &serde_yaml::Error) -> Diagnostic {
    let (line, column) = err
        .location()
        .map(|location| {
            (
                location.line().saturating_sub(1),
                location.column().saturating_sub(1),
            )
        })
        .unwrap_or((0, 0));
    Diagnostic {
        code: "E-YAML-001".to_string(),
        message: err.to_string(),
        severity: Severity::Error,
        span: point_span(path, line, column),
        related: None,
        fix: None,
        category: Category::Syntax,
    }
}

fn yaml_subset_diagnostic(path: &Path, violation: &crate::yaml_subset::YamlSubsetViolation) -> Diagnostic {
    Diagnostic {
        code: violation.code.to_string(),
        message: violation.message.clone(),
        severity: Severity::Error,
        span: point_span(path, violation.line, violation.column),
        related: None,
        fix: None,
        category: Category::Syntax,
    }
}

fn style_diagnostics(path: &Path, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            if let Some((key, column)) = simple_mapping_key(line) {
                if is_symbol_key(key) && !is_kebab_case(key) {
                    diagnostics.push(Diagnostic {
                        code: "W-STYLE-001".to_string(),
                        message: format!(
                            "non-kebab-case top-level symbol: `{key}` (recommended: kebab-case)"
                        ),
                        severity: Severity::Warning,
                        span: span_for_text(path, line_index, column, key.len()),
                        related: None,
                        fix: None,
                        category: Category::Style,
                    });
                }
            }
        }
    }
    diagnostics
}

fn compile_diagnostics(path: &Path) -> Vec<Diagnostic> {
    let result = load::load_program(path).and_then(|program| {
        let Some(entry) = program.modules.get(&program.entry) else {
            return Ok(());
        };
        if contains_noncanonical_option(entry) {
            anyhow::bail!(
                "E-OPTION-001: noncanonical option representation; use the tagged stdlib option enum"
            );
        }
        let Some(map) = entry.as_mapping() else {
            return Ok(());
        };
        if !map.contains_key(serde_yaml::Value::String("main".to_string())) {
            return Ok(());
        }
        lower::lower_program(&program).map(|_| ())
    });
    match result {
        Ok(()) => Vec::new(),
        Err(err) => {
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

fn contains_noncanonical_option(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Mapping(map) => {
            if let Some(option) = map.get(serde_yaml::Value::String("$option".to_string())) {
                if !option.as_mapping().is_some_and(|type_args| {
                    type_args
                        .keys()
                        .all(|key| key.as_str().is_some_and(|name| !name.starts_with('$')))
                }) {
                    return true;
                }
            }
            if let Some(union) = map.get(serde_yaml::Value::String("$union".to_string())) {
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
        serde_yaml::Value::Sequence(items) => items.iter().any(contains_noncanonical_option),
        serde_yaml::Value::Tagged(tagged) => contains_noncanonical_option(&tagged.value),
        _ => false,
    }
}

fn extract_diagnostic_code(message: &str) -> Option<&'static str> {
    const KNOWN_CODES: &[&str] = &[
        "E-YAML-001",
        "E-YAML-002",
        "E-YAML-003",
        "E-SYN-001",
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
        "E-SELF-001",
        "E-DEFS-001",
        "E-IMPL-001",
        "E-IMPL-002",
        "E-IMPL-003",
        "E-IMPL-004",
        "E-IMPL-005",
        "E-IMPL-006",
        "E-OPTION-001",
    ];
    KNOWN_CODES
        .iter()
        .copied()
        .find(|code| message.contains(code))
}

fn simple_mapping_key(line: &str) -> Option<(&str, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return None;
    }
    let column = line.len() - trimmed.len();
    let colon = trimmed.find(':')?;
    let key = &trimmed[..colon];
    if key.is_empty() || key.contains(' ') || key.contains('\t') {
        return None;
    }
    Some((key, column))
}

fn is_symbol_key(key: &str) -> bool {
    !key.starts_with('$') && !key.starts_with('=') && !key.starts_with('-')
}

fn is_kebab_case(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
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
            if name == ".git" || name == "target" {
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
    s.ends_with(".vibra") || s.ends_with(".vibra.yaml")
}

fn display_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn file_uri(path: &Path) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut s = absolute.display().to_string().replace('\\', "/");
    if !s.starts_with('/') {
        s = format!("/{s}");
    }
    format!("file://{}", percent_encode_uri_path(&s))
}

fn percent_encode_uri_path(path: &str) -> String {
    let mut out = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                out.push(char::from(*byte))
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Debug, Default)]
struct Suppressions {
    current_line: BTreeMap<usize, BTreeSet<String>>,
    next_line: BTreeMap<usize, BTreeSet<String>>,
    from_line: Vec<(usize, BTreeSet<String>)>,
}

impl Suppressions {
    fn parse(source: &str) -> Self {
        let mut suppressions = Self::default();
        for (line_index, line) in source.lines().enumerate() {
            let Some(comment_start) = line.find('#') else {
                continue;
            };
            let comment = line[comment_start + 1..].trim();
            if let Some(rest) = comment.strip_prefix("vibra-lint-disable-next-line") {
                suppressions
                    .next_line
                    .insert(line_index + 1, parse_suppression_codes(rest));
            } else if let Some(rest) = comment.strip_prefix("vibra-lint-disable-line") {
                suppressions
                    .current_line
                    .insert(line_index, parse_suppression_codes(rest));
            } else if let Some(rest) = comment.strip_prefix("vibra-lint-disable") {
                suppressions
                    .from_line
                    .push((line_index, parse_suppression_codes(rest)));
            }
        }
        suppressions
    }

    fn suppresses(&self, code: &str, line: usize) -> bool {
        self.current_line
            .get(&line)
            .is_some_and(|codes| code_matches(codes, code))
            || self
                .next_line
                .get(&line)
                .is_some_and(|codes| code_matches(codes, code))
            || self
                .from_line
                .iter()
                .any(|(start, codes)| line >= *start && code_matches(codes, code))
    }
}

fn parse_suppression_codes(raw: &str) -> BTreeSet<String> {
    let codes: BTreeSet<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|part| {
            let part = part.trim();
            (!part.is_empty()).then(|| part.to_string())
        })
        .collect();
    if codes.is_empty() {
        ["all".to_string()].into_iter().collect()
    } else {
        codes
    }
}

fn code_matches(codes: &BTreeSet<String>, code: &str) -> bool {
    codes.contains("all") || codes.contains(code)
}
