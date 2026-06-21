//! Validation and semantic normalization for universal source annotations.

use anyhow::{bail, Result};
use serde_yaml::Value;

pub const IGNORED_ANNOTATIONS: &[&str] = &["=comment", "=lint"];
pub const SUPPRESSIBLE_LINT_CODES: &[&str] = &["W-STYLE-001"];

pub fn validate(value: &Value) -> Result<()> {
    match value {
        Value::Mapping(mapping) => {
            let has_annotation = mapping.keys().any(is_ignored_annotation);
            let has_subject = mapping.keys().any(|key| !is_ignored_annotation(key));
            if has_annotation && !has_subject {
                bail!("E-ANNO-003: annotations must share a mapping with the syntax they describe");
            }
            if let Some(comment) = mapping.get(Value::String("=comment".into())) {
                if !comment.is_string() {
                    bail!("E-COMMENT-001: `=comment` must be a string scalar");
                }
            }
            if let Some(lint) = mapping.get(Value::String("=lint".into())) {
                validate_lint(lint)?;
            }
            for (key, child) in mapping {
                if !is_ignored_annotation(key) {
                    validate(child)?;
                }
            }
        }
        Value::Sequence(items) => {
            for item in items {
                validate(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn strip(value: &mut Value) {
    match value {
        Value::Mapping(mapping) => {
            mapping.remove(Value::String("=comment".into()));
            mapping.remove(Value::String("=lint".into()));
            for child in mapping.values_mut() {
                strip(child);
            }
        }
        Value::Sequence(items) => {
            for item in items {
                strip(item);
            }
        }
        _ => {}
    }
}

pub fn lint_codes(value: &Value) -> Result<Vec<String>> {
    validate_lint(value)?;
    let disable = value
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String("disable".into())))
        .and_then(Value::as_sequence)
        .expect("validated lint annotation");
    Ok(disable
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect())
}

fn validate_lint(value: &Value) -> Result<()> {
    let mapping = value
        .as_mapping()
        .ok_or_else(|| anyhow::anyhow!("E-LINT-001: `=lint` must be a mapping"))?;
    if mapping.len() != 1 || !mapping.contains_key(Value::String("disable".into())) {
        bail!("E-LINT-001: `=lint` accepts exactly one `disable` key");
    }
    let disable = mapping
        .get(Value::String("disable".into()))
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow::anyhow!("E-LINT-001: `=lint.disable` must be an array"))?;
    if disable.is_empty() {
        bail!("E-LINT-001: `=lint.disable` must not be empty");
    }
    for code in disable {
        let code = code
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("E-LINT-001: lint codes must be strings"))?;
        if code != "all" && !SUPPRESSIBLE_LINT_CODES.contains(&code) {
            bail!("E-LINT-001: unknown or unsuppressible lint code `{code}`");
        }
    }
    Ok(())
}

fn is_ignored_annotation(key: &Value) -> bool {
    key.as_str()
        .is_some_and(|key| IGNORED_ANNOTATIONS.contains(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_strips_universal_annotations() {
        let mut value: Value = serde_yaml::from_str(
            r#"=comment: module
main:
  =lint:
    disable: [W-STYLE-001]
  $function: $void
  do:
    - =comment: call
      $io.println: hello
"#,
        )
        .unwrap();

        validate(&value).unwrap();
        strip(&mut value);
        let rendered = serde_yaml::to_string(&value).unwrap();
        assert!(!rendered.contains("=comment"));
        assert!(!rendered.contains("=lint"));
        assert!(rendered.contains("$io.println"));
    }

    #[test]
    fn rejects_malformed_or_unattached_annotations() {
        for (source, code) in [
            ("=comment: 1\nvalue: ok\n", "E-COMMENT-001"),
            ("=lint:\n  disable: [E-YAML-001]\nvalue: ok\n", "E-LINT-001"),
            ("items:\n  - =comment: unattached\n", "E-ANNO-003"),
        ] {
            let value: Value = serde_yaml::from_str(source).unwrap();
            let error = validate(&value).unwrap_err().to_string();
            assert!(error.contains(code), "{error}");
        }
    }
}
