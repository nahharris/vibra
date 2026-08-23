//! Format-neutral diagnostic data transfer objects shared by tooling paths.

use std::fs;
use std::path::Path;

use serde::Serialize;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Position {
    pub line: usize,
    /// Zero-based column. LSP-facing producers use UTF-16 code units.
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub uri: String,
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub span: Span,
}

/// A single source replacement in a diagnostic fix.
///
/// Offsets are carried by the typed `Span` positions so all consumers (CLI,
/// MCP, and LSP) can apply the same edit without interpreting an ad-hoc JSON
/// object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextEdit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixApplicability {
    Safe,
}

/// A safe, source-preserving edit offered by a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fix {
    pub title: String,
    pub applicability: FixApplicability,
    pub edits: Vec<TextEdit>,
}

impl Fix {
    pub fn safe(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            title: title.into(),
            applicability: FixApplicability::Safe,
            edits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<Vec<RelatedDiagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Vec<Fix>>,
    #[serde(skip)]
    pub category: Category,
}

pub fn file_uri(path: &Path) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut value = absolute
        .display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    if !value.starts_with('/') {
        value = format!("/{value}");
    }
    format!("file://{}", percent_encode_uri_path(&value))
}

fn percent_encode_uri_path(path: &str) -> String {
    let mut output = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                output.push(char::from(*byte))
            }
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}
