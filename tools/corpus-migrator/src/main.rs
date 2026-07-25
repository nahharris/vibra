use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Report {
    scanned: usize,
    already_sexpr: usize,
    converted: usize,
    valid: usize,
    unsupported: BTreeMap<String, usize>,
}

fn main() -> Result<()> {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    let mut files = Vec::new();
    collect(&root.join("tests"), &mut files)?;
    collect(&root.join("examples"), &mut files)?;
    files.sort();

    let mut report = Report::default();
    for path in files {
        report.scanned += 1;
        let display = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let source = fs::read_to_string(&path)?;
        if source.trim_start().starts_with('(') {
            report.already_sexpr += 1;
            continue;
        }
        match migrate(&source) {
            Ok(output) => {
                report.converted += 1;
                match validate(&output) {
                    Ok(()) => report.valid += 1,
                    Err(error) => record_issue(
                        &mut report,
                        format!(
                            "{display}: typed-validation: {}",
                            first_line(&error.to_string())
                        ),
                    ),
                }
            }
            Err(error) => record_issue(&mut report, format!("{display}: {error:#}")),
        }
    }

    println!("scanned: {}", report.scanned);
    println!("already-sexpr: {}", report.already_sexpr);
    println!("converted: {}", report.converted);
    println!("typed-valid: {}", report.valid);
    println!(
        "unsupported: {}",
        report.unsupported.values().sum::<usize>()
    );
    for (reason, count) in report.unsupported {
        println!("  {count:>3}  {reason}");
    }
    Ok(())
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().and_then(|v| v.to_str()) != Some("dep") {
                collect(&path, files)?;
            }
        } else if path.extension().and_then(|v| v.to_str()) == Some("vibra") {
            files.push(path);
        }
    }
    Ok(())
}

fn migrate(source: &str) -> Result<String> {
    let root: Value = serde_yaml::from_str(source).context("yaml-parse")?;
    let map = root.as_mapping().context("root-not-mapping")?;
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
    let params = if primary.as_str().is_some() {
        match get(map, "args") {
            Some(args) => parameters(args)?,
            None => parameters(primary)?,
        }
    } else {
        parameters(primary)?
    };
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

fn parameters(value: &Value) -> Result<String> {
    if matches!(value.as_str(), Some("$void" | "void")) {
        return Ok("()".into());
    }
    let map = value.as_mapping().context("parameters-not-mapping")?;
    Ok(format!(
        "({})",
        map.iter()
            .map(|(name, value)| Ok(format!(
                "({} {})",
                sym(as_str(name, "parameter")?),
                ty(value)?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(" ")
    ))
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
                .map(|capture| Ok(format!(" {}", sym(as_str(capture, "task-capture")?))))
                .collect::<Result<String>>()?,
            body_form(get(map, "do").context("task-missing-do")?)?
        ));
    }
    if map.len() != 1 {
        bail!("multi-key-statement")
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
                sym(as_str(name, "set-name")?),
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
            "newtype" | "interface" => {
                let fields = payload
                    .as_mapping()
                    .context("wrapped-pattern-not-mapping")?;
                Ok(format!(
                    "({head} {} {})",
                    ty(get(fields, "type").context("wrapped-pattern-missing-type")?)?,
                    pattern(get(fields, "value").context("wrapped-pattern-missing-value")?)?
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
        Value::String(value) if value.starts_with('$') => Ok(sym(value.trim_start_matches('$'))),
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
            Value::Mapping(arguments) => Ok(format!("({head} {})", record(arguments)?)),
            _ => Ok(format!("({head} {})", expr(payload)?)),
        },
    }
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
        Value::Mapping(arguments) => Ok(format!(
            "({head}{})",
            arguments
                .iter()
                .map(|(_, value)| ty(value))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|v| format!(" {v}"))
                .collect::<String>()
        )),
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
    if get(map, "=where").is_some() || get(map, "=defs").is_some() || get(map, "=impl").is_some() {
        bail!("unsupported-declaration-annotation");
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

fn quoted(value: &str) -> String {
    format!("{value:?}")
}

fn record_issue(report: &mut Report, reason: String) {
    *report.unsupported.entry(reason).or_default() += 1;
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
        assert!(output.contains("(array args.value)"));
        assert!(output.contains("tags: (fast)"));
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
}
