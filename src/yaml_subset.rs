//! Strict YAML subset checks required by the Vibra language spec (DRAFT §2).

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlSubsetViolation {
    pub code: &'static str,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

pub fn validate_yaml_subset(source: &str) -> Vec<YamlSubsetViolation> {
    let mut violations = Vec::new();
    let mut block_scalar_indent = None;
    for (line_idx, line) in source.lines().enumerate() {
        let line_no = line_idx;
        let indent = line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        if block_scalar_indent
            .is_some_and(|block_indent| line.trim().is_empty() || indent > block_indent)
        {
            continue;
        }
        block_scalar_indent = None;
        if let Some(column) = find_comment(line) {
            violations.push(YamlSubsetViolation {
                code: "E-YAML-002",
                message: "YAML `#` comments are forbidden; use an attached `=comment` annotation"
                    .to_string(),
                line: line_no,
                column,
            });
        }
        let content = strip_line_comment(line);
        if content.contains("<<:") {
            violations.push(YamlSubsetViolation {
                code: "E-YAML-001",
                message: "merge keys (`<<`) are forbidden in the Vibra YAML subset".to_string(),
                line: line_no,
                column: content.find("<<:").unwrap_or(0),
            });
        }
        if content.contains("!!") {
            violations.push(YamlSubsetViolation {
                code: "E-YAML-001",
                message: "explicit YAML tags (`!!`) are forbidden in the Vibra YAML subset"
                    .to_string(),
                line: line_no,
                column: content.find("!!").unwrap_or(0),
            });
        }
        if let Some(column) = find_anchor(content) {
            violations.push(YamlSubsetViolation {
                code: "E-YAML-001",
                message: "YAML anchors (`&`) are forbidden in the Vibra YAML subset".to_string(),
                line: line_no,
                column,
            });
        }
        if let Some(column) = find_alias(content) {
            violations.push(YamlSubsetViolation {
                code: "E-YAML-001",
                message: "YAML aliases (`*`) are forbidden in the Vibra YAML subset".to_string(),
                line: line_no,
                column,
            });
        }
        if starts_block_scalar(content) {
            block_scalar_indent = Some(indent);
        }
    }
    violations
}

fn find_comment(line: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_double => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single
                && !in_double
                && (index == 0 || line.as_bytes()[index - 1].is_ascii_whitespace()) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

fn starts_block_scalar(content: &str) -> bool {
    let trimmed = content.trim_end();
    trimmed.ends_with('|')
        || trimmed.ends_with('>')
        || trimmed.ends_with("|-")
        || trimmed.ends_with("|+")
        || trimmed.ends_with(">-")
        || trimmed.ends_with(">+")
}

pub fn validate_yaml_subset_or_err(source: &str, path: &Path) -> anyhow::Result<()> {
    let violations = validate_yaml_subset(source);
    if let Some(first) = violations.first() {
        anyhow::bail!(
            "{}:{}:{}: {}: {}",
            path.display(),
            first.line + 1,
            first.column + 1,
            first.code,
            first.message
        );
    }
    Ok(())
}

fn strip_line_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn find_anchor(content: &str) -> Option<usize> {
    for (idx, _) in content.match_indices('&') {
        if idx > 0 {
            let prev = content.as_bytes()[idx - 1];
            if prev != b' ' && prev != b'\t' && prev != b':' && prev != b'-' && prev != b'[' {
                continue;
            }
        }
        let rest = &content[idx + 1..];
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            return Some(idx);
        }
    }
    None
}

fn find_alias(content: &str) -> Option<usize> {
    for (idx, _) in content.match_indices('*') {
        if idx > 0 {
            let prev = content.as_bytes()[idx - 1];
            if prev != b' ' && prev != b'\t' && prev != b':' && prev != b'-' && prev != b'[' {
                continue;
            }
        }
        let rest = &content[idx + 1..];
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_anchors_and_aliases() {
        let violations = validate_yaml_subset("a: &x 1\nb: *x\n");
        assert!(violations
            .iter()
            .any(|v| v.code == "E-YAML-001" && v.message.contains("anchor")));
        assert!(violations
            .iter()
            .any(|v| v.code == "E-YAML-001" && v.message.contains("alias")));
    }

    #[test]
    fn rejects_merge_keys_and_tags() {
        let violations = validate_yaml_subset("defaults: &base\n  x: 1\nchild:\n  <<: *base\n");
        assert!(violations.iter().any(|v| v.message.contains("merge")));
        let tagged = validate_yaml_subset("value: !!str hello\n");
        assert!(tagged.iter().any(|v| v.message.contains("tag")));
    }

    #[test]
    fn rejects_yaml_comments_but_allows_hash_text() {
        for source in [
            "# leading\nvalue: ok\n",
            "value: ok # trailing\n",
            "value: ok\n  # nested\n",
        ] {
            let violations = validate_yaml_subset(source);
            assert!(
                violations.iter().any(|violation| {
                    violation.code == "E-YAML-002"
                        && violation.message.contains("comments are forbidden")
                }),
                "expected comment rejection for {source:?}"
            );
        }

        for source in [
            "value: '# quoted'\n",
            "value: \"# quoted\"\n",
            "value: issue#50\n",
            "value: |\n  # block scalar text\n",
        ] {
            assert!(
                validate_yaml_subset(source).is_empty(),
                "hash text should remain valid: {source:?}"
            );
        }
    }
}
