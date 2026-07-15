use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use serde::Serialize;
use serde_yaml::{Mapping, Value};

use super::{
    ChangeSet, CodeError, CodeErrorKind, Form, Node, NodeKind, Path, Pattern, Query, Segment,
    SemanticIndex, SemanticKind, SourceDatabase,
};

#[derive(Debug, Clone)]
pub struct CodeRunOptions {
    pub workspace: PathBuf,
    pub pipeline: String,
    pub write: bool,
    pub lint: bool,
    pub test: bool,
}

#[derive(Debug, Clone)]
enum PipelineValue {
    Workspace,
    File(PathBuf),
    Nodes(Vec<Node>),
    Value(Value),
}

#[derive(Debug)]
struct PipelineState {
    original: SourceDatabase,
    database: SourceDatabase,
    current: PipelineValue,
    saved: BTreeMap<String, PipelineValue>,
    changed: BTreeSet<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CodeReport {
    status: &'static str,
    files: Vec<CodeFileReport>,
    diff: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeFileReport {
    path: String,
    before_revision: String,
    after_revision: String,
}

pub fn run_pipeline(options: CodeRunOptions) -> Result<String, CodeError> {
    let original = SourceDatabase::discover(&options.workspace)?;
    let mut state = PipelineState {
        database: original.clone(),
        original,
        current: PipelineValue::Workspace,
        saved: BTreeMap::new(),
        changed: BTreeSet::new(),
    };
    let pipeline: Value = serde_yaml::from_str(&options.pipeline).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("parse `vibra code` pipeline: {error}"),
        )
    })?;
    let stages = pipeline.as_sequence().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            "`vibra code` input must be a sequence of pipeline stages",
        )
    })?;
    for (index, stage) in stages.iter().enumerate() {
        execute_stage(&mut state, stage).map_err(|error| {
            CodeError::new(
                error.kind,
                format!("pipeline stage {}: {}", index + 1, error.message),
            )
        })?;
    }

    if state.changed.is_empty() {
        return render_pipeline_value(&state.current);
    }
    if let Some(path) = state.changed.iter().find(|path| {
        matches!(
            path.components().next(),
            Some(std::path::Component::Normal(name)) if name == "dep" || name == "stdlib"
        )
    }) {
        return Err(CodeError::new(
            CodeErrorKind::EditConflict,
            format!(
                "dependency and stdlib source is read-only to code transactions: `{}`",
                path.display()
            ),
        ));
    }
    validate_transaction(&options.workspace, &state, options.lint, options.test)?;
    let report = build_report(&state, options.write)?;
    if options.write {
        write_atomically(&options.workspace, &state)?;
    }
    serde_yaml::to_string(&report).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("render `vibra code` report: {error}"),
        )
    })
}

fn execute_stage(state: &mut PipelineState, stage: &Value) -> Result<(), CodeError> {
    let mapping = stage.as_mapping().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            "pipeline stage must be a singleton mapping",
        )
    })?;
    if mapping.len() != 1 {
        return Err(CodeError::new(
            CodeErrorKind::InvalidForm,
            "pipeline stage must contain exactly one operation",
        ));
    }
    let (operation, argument) = mapping.iter().next().expect("length checked");
    let operation = operation.as_str().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            "pipeline operation name must be a string",
        )
    })?;
    match operation {
        "$code.file" => {
            let path = argument.as_str().ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidForm,
                    "$code.file expects a path string",
                )
            })?;
            let path = normalize_relative_path(path);
            state.database.document(&path)?;
            state.current = PipelineValue::File(path);
        }
        "$code.root" => {
            let file = current_file(&state.current)?;
            state.current = PipelineValue::Nodes(vec![state.database.document(file)?.root()?]);
        }
        "$code.at" => {
            let relative = compact_path(argument)?;
            let nodes = match &state.current {
                PipelineValue::File(file) => {
                    vec![state.database.document(file)?.at(&relative)?]
                }
                PipelineValue::Nodes(nodes) => nodes
                    .iter()
                    .map(|node| {
                        state
                            .database
                            .document(node.document())?
                            .at(&append_path(node.path(), &relative))
                    })
                    .collect::<Result<Vec<_>, CodeError>>()?,
                _ => {
                    return Err(CodeError::new(
                        CodeErrorKind::EditConflict,
                        "$code.at requires a file or node selection",
                    ));
                }
            };
            state.current = PipelineValue::Nodes(nodes);
        }
        "$code.parent" => {
            let nodes = current_nodes(&state.current)?;
            state.current = PipelineValue::Nodes(
                nodes
                    .iter()
                    .filter_map(|node| {
                        node.path()
                            .parent()
                            .map(|path| state.database.document(node.document())?.at(&path))
                    })
                    .collect::<Result<Vec<_>, CodeError>>()?,
            );
        }
        "$code.children" => {
            let nodes = current_nodes(&state.current)?;
            state.current = PipelineValue::Nodes(
                nodes
                    .iter()
                    .map(Node::children)
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
            );
        }
        "$code.find" => {
            let roots = selection_roots(state)?;
            if let Value::Mapping(options) = argument {
                if let Some(semantic) = get_string(options, "semantic")? {
                    let kind = parse_semantic_kind(&semantic)?;
                    let symbol = get_string(options, "symbol")?;
                    let index = SemanticIndex::build(&state.database)?;
                    let facts = index.query(Some(kind), symbol.as_deref());
                    let mut nodes = Vec::new();
                    for fact in facts {
                        if roots.iter().any(|root| {
                            root.document() == fact.document && root.path().is_prefix_of(&fact.path)
                        }) {
                            nodes.push(state.database.document(&fact.document)?.at(&fact.path)?);
                        }
                    }
                    state.current = PipelineValue::Nodes(nodes);
                    return Ok(());
                }
            }
            let mut output = Vec::new();
            for root in roots {
                let mut query = Query::subtree(root.path().clone());
                match argument {
                    Value::String(key) => query = query.with_mapping_key(key),
                    Value::Mapping(options) => {
                        if let Some(key) = get_string(options, "key")? {
                            query = query.with_mapping_key(key);
                        }
                        if let Some(scalar) = options.get(Value::String("scalar".into())) {
                            query = query.with_pattern(Pattern::Exact(Form::from_yaml_value(
                                scalar.clone(),
                            )?));
                        }
                        if let Some(kind) = get_string(options, "kind")? {
                            query = query.with_kind(parse_kind(&kind)?);
                        }
                        if let Some(limit) = options
                            .get(Value::String("limit".into()))
                            .and_then(Value::as_u64)
                        {
                            query = query.with_limit(limit as usize);
                        }
                    }
                    _ => {
                        return Err(CodeError::new(
                            CodeErrorKind::InvalidForm,
                            "$code.find expects a key string or query mapping",
                        ));
                    }
                }
                output.extend(
                    query
                        .execute(&state.database.document(root.document())?)?
                        .into_iter()
                        .map(|result| result.node),
                );
            }
            state.current = PipelineValue::Nodes(output);
        }
        "$code.project" => {
            let projection = argument.as_str().ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidForm,
                    "$code.project expects `summary`, `form`, or `source`",
                )
            })?;
            state.current = PipelineValue::Value(project_value(&state.current, projection)?);
        }
        "$code.save" => {
            let name = argument.as_str().ok_or_else(|| {
                CodeError::new(CodeErrorKind::InvalidForm, "$code.save expects a name")
            })?;
            state.saved.insert(name.to_string(), state.current.clone());
        }
        "$code.load" => {
            let name = argument.as_str().ok_or_else(|| {
                CodeError::new(CodeErrorKind::InvalidForm, "$code.load expects a name")
            })?;
            state.current = state.saved.get(name).cloned().ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidPath,
                    format!("saved pipeline value `{name}` does not exist"),
                )
            })?;
        }
        "$code.replace" => {
            let value = Form::from_yaml_value(argument.clone())?;
            apply_node_edits(state, |changes, node| {
                changes.replace(node.locator(), value.clone())
            })?;
        }
        "$code.delete" => {
            apply_node_edits(state, |changes, node| changes.delete(node.locator()))?;
            state.current = PipelineValue::Nodes(Vec::new());
        }
        "$code.rename-key" => {
            let new_key = argument.as_str().ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidForm,
                    "$code.rename-key expects a string",
                )
            })?;
            let old_nodes = current_nodes(&state.current)?.to_vec();
            let old_paths: Vec<_> = old_nodes
                .iter()
                .map(|node| {
                    let mut path = node.path().parent().expect("selected key has parent");
                    path.push(Segment::key(new_key));
                    (node.document().to_path_buf(), path)
                })
                .collect();
            apply_node_edits(state, |changes, node| {
                changes.rename_mapping_key(node.locator(), new_key)
            })?;
            state.current = PipelineValue::Nodes(
                old_paths
                    .into_iter()
                    .map(|(document, path)| state.database.document(document)?.at(&path))
                    .collect::<Result<Vec<_>, CodeError>>()?,
            );
        }
        "$code.upsert" => {
            let options = required_mapping(argument, "$code.upsert")?;
            let key = required_string(options, "key")?;
            let value = options.get(Value::String("value".into())).ok_or_else(|| {
                CodeError::new(CodeErrorKind::InvalidForm, "$code.upsert requires `value`")
            })?;
            let form = Form::from_yaml_value(value.clone())?;
            apply_node_edits(state, |changes, node| {
                changes.upsert_mapping(node.locator(), key.clone(), form.clone())
            })?;
        }
        "$code.insert-mapping" => {
            let options = required_mapping(argument, "$code.insert-mapping")?;
            let key = required_string(options, "key")?;
            let value = options.get(Value::String("value".into())).ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidForm,
                    "$code.insert-mapping requires `value`",
                )
            })?;
            let form = Form::from_yaml_value(value.clone())?;
            apply_node_edits(state, |changes, node| {
                changes.insert_mapping(node.locator(), key.clone(), form.clone())
            })?;
        }
        "$code.insert" => {
            let options = required_mapping(argument, "$code.insert")?;
            let index = required_index(options, "index")?;
            let value = options.get(Value::String("value".into())).ok_or_else(|| {
                CodeError::new(CodeErrorKind::InvalidForm, "$code.insert requires `value`")
            })?;
            let form = Form::from_yaml_value(value.clone())?;
            apply_node_edits(state, |changes, node| {
                changes.insert_sequence(node.locator(), index, form.clone())
            })?;
        }
        "$code.splice" => {
            let options = required_mapping(argument, "$code.splice")?;
            let start = required_index(options, "start")?;
            let delete_count = required_index(options, "delete")?;
            let values = options
                .get(Value::String("values".into()))
                .and_then(Value::as_sequence)
                .ok_or_else(|| {
                    CodeError::new(
                        CodeErrorKind::InvalidForm,
                        "$code.splice requires a `values` sequence",
                    )
                })?
                .iter()
                .cloned()
                .map(Form::from_yaml_value)
                .collect::<Result<Vec<_>, _>>()?;
            apply_node_edits(state, |changes, node| {
                changes.splice_sequence(node.locator(), start, delete_count, values.clone())
            })?;
        }
        "$code.copy" | "$code.move" => {
            let source = current_nodes(&state.current)?;
            if source.len() != 1 {
                return Err(CodeError::new(
                    CodeErrorKind::EditConflict,
                    format!("{operation} requires exactly one selected source node"),
                ));
            }
            let source = source[0].clone();
            let options = required_mapping(argument, operation)?;
            let file = get_string(options, "file")?
                .map(|path| normalize_relative_path(&path))
                .unwrap_or_else(|| source.document().to_path_buf());
            let destination_path = options
                .get(Value::String("to".into()))
                .map(compact_path)
                .transpose()?
                .unwrap_or_else(Path::root);
            let target = state.database.document(&file)?.at(&destination_path)?;
            let destination = if let Some(key) = get_string(options, "key")? {
                Segment::key(key)
            } else if let Some(index) = options
                .get(Value::String("index".into()))
                .and_then(Value::as_u64)
            {
                Segment::index(index as usize)
            } else {
                return Err(CodeError::new(
                    CodeErrorKind::InvalidForm,
                    format!("{operation} requires either `key` or `index`"),
                ));
            };
            let changes = if operation == "$code.copy" {
                ChangeSet::new().copy(source.locator(), target.locator(), destination.clone())
            } else {
                ChangeSet::new().move_node(source.locator(), target.locator(), destination.clone())
            };
            apply_changes(state, changes)?;
            state.current = PipelineValue::Nodes(vec![state
                .database
                .document(&file)?
                .at(&destination_path.child(destination))?]);
        }
        "$code.rename-symbol" => {
            let options = required_mapping(argument, "$code.rename-symbol")?;
            let file = required_string(options, "file")?;
            let from = required_string(options, "from")?;
            let to = required_string(options, "to")?;
            let changes = SemanticIndex::build(&state.database)?.rename_symbol(
                &state.database,
                file,
                &from,
                &to,
            )?;
            apply_changes(state, changes)?;
            state.current = PipelineValue::Workspace;
        }
        "$code.rename-import" => {
            let options = required_mapping(argument, "$code.rename-import")?;
            let file = required_string(options, "file")?;
            let from = required_string(options, "from")?;
            let to = required_string(options, "to")?;
            let changes = SemanticIndex::build(&state.database)?.rename_import_alias(
                &state.database,
                file,
                &from,
                &to,
            )?;
            apply_changes(state, changes)?;
            state.current = PipelineValue::Workspace;
        }
        other => {
            return Err(CodeError::new(
                CodeErrorKind::InvalidForm,
                format!("unknown `vibra code` operation `{other}`"),
            ));
        }
    }
    Ok(())
}

fn apply_node_edits(
    state: &mut PipelineState,
    build: impl FnMut(ChangeSet, &Node) -> ChangeSet,
) -> Result<(), CodeError> {
    let nodes = current_nodes(&state.current)?.to_vec();
    let paths: Vec<_> = nodes
        .iter()
        .map(|node| (node.document().to_path_buf(), node.path().clone()))
        .collect();
    let changes = nodes.iter().fold(ChangeSet::new(), build);
    apply_changes(state, changes)?;
    state.current = PipelineValue::Nodes(
        paths
            .into_iter()
            .filter_map(|(document, path)| {
                state
                    .database
                    .document(document)
                    .and_then(|document| document.at(&path))
                    .ok()
            })
            .collect(),
    );
    Ok(())
}

fn apply_changes(state: &mut PipelineState, changes: ChangeSet) -> Result<(), CodeError> {
    let applied = state.database.apply(&changes)?;
    state.changed.extend(applied.changed_files);
    state.database = applied.database;
    Ok(())
}

fn current_nodes(current: &PipelineValue) -> Result<&[Node], CodeError> {
    match current {
        PipelineValue::Nodes(nodes) => Ok(nodes),
        _ => Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "operation requires a node selection",
        )),
    }
}

fn current_file(current: &PipelineValue) -> Result<&FsPath, CodeError> {
    match current {
        PipelineValue::File(path) => Ok(path),
        _ => Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "operation requires a selected file",
        )),
    }
}

fn selection_roots(state: &PipelineState) -> Result<Vec<Node>, CodeError> {
    match &state.current {
        PipelineValue::File(path) => Ok(vec![state.database.document(path)?.root()?]),
        PipelineValue::Nodes(nodes) => Ok(nodes.clone()),
        _ => Err(CodeError::new(
            CodeErrorKind::EditConflict,
            "$code.find requires a file or node selection",
        )),
    }
}

fn compact_path(value: &Value) -> Result<Path, CodeError> {
    let segments = value.as_sequence().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            "$code.at expects a compact path sequence",
        )
    })?;
    segments
        .iter()
        .map(|segment| {
            if let Some(key) = segment.as_str() {
                return Ok(Segment::key(key));
            }
            if let Some(index) = segment.as_u64() {
                return Ok(Segment::index(index as usize));
            }
            Err(CodeError::new(
                CodeErrorKind::InvalidForm,
                "path segments must be strings or non-negative integers",
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Path::new)
}

fn append_path(base: &Path, relative: &Path) -> Path {
    let mut result = base.clone();
    for segment in relative.segments() {
        result.push(segment.clone());
    }
    result
}

fn project_value(current: &PipelineValue, projection: &str) -> Result<Value, CodeError> {
    let nodes = current_nodes(current)?;
    match projection {
        "form" => project_many(nodes, |node| node.form().to_yaml_value()),
        "source" => project_many(nodes, |node| Value::String(node.source().to_string())),
        "summary" => {
            let values = nodes
                .iter()
                .map(|node| {
                    let mut mapping = Mapping::new();
                    mapping.insert(
                        Value::String("document".into()),
                        Value::String(node.document().display().to_string()),
                    );
                    mapping.insert(
                        Value::String("path".into()),
                        Value::Sequence(
                            node.path()
                                .segments()
                                .iter()
                                .map(|segment| match segment {
                                    Segment::Key(key) => Value::String(key.clone()),
                                    Segment::Index(index) => Value::Number((*index as u64).into()),
                                })
                                .collect(),
                        ),
                    );
                    mapping.insert(
                        Value::String("kind".into()),
                        Value::String(format!("{:?}", node.kind()).to_lowercase()),
                    );
                    mapping.insert(
                        Value::String("fingerprint".into()),
                        Value::String(node.fingerprint().to_string()),
                    );
                    Value::Mapping(mapping)
                })
                .collect();
            Ok(Value::Sequence(values))
        }
        _ => Err(CodeError::new(
            CodeErrorKind::InvalidForm,
            "$code.project expects `summary`, `form`, or `source`",
        )),
    }
}

fn project_many(nodes: &[Node], project: impl Fn(&Node) -> Value) -> Result<Value, CodeError> {
    if nodes.len() == 1 {
        Ok(project(&nodes[0]))
    } else {
        Ok(Value::Sequence(nodes.iter().map(project).collect()))
    }
}

fn render_pipeline_value(value: &PipelineValue) -> Result<String, CodeError> {
    let value = match value {
        PipelineValue::Value(value) => value.clone(),
        PipelineValue::Nodes(_) => project_value(value, "summary")?,
        PipelineValue::File(path) => Value::String(path.display().to_string()),
        PipelineValue::Workspace => Value::String("workspace".into()),
    };
    serde_yaml::to_string(&value).map_err(|error| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("render `vibra code` output: {error}"),
        )
    })
}

fn build_report(state: &PipelineState, write: bool) -> Result<CodeReport, CodeError> {
    let mut files = Vec::new();
    let mut diff = String::new();
    for path in &state.changed {
        let before = state.original.document(path)?;
        let after = state.database.document(path)?;
        files.push(CodeFileReport {
            path: path.display().to_string(),
            before_revision: before.revision().to_string(),
            after_revision: after.revision().to_string(),
        });
        diff.push_str(&full_diff(path, before.source(), after.source()));
    }
    Ok(CodeReport {
        status: if write { "written" } else { "preview" },
        files,
        diff,
    })
}

fn full_diff(path: &FsPath, before: &str, after: &str) -> String {
    let display = path.display();
    let mut diff = format!("--- a/{display}\n+++ b/{display}\n@@\n");
    for line in before.lines() {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in after.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn write_atomically(workspace: &FsPath, state: &PipelineState) -> Result<(), CodeError> {
    let current = SourceDatabase::discover(workspace)?;
    for path in &state.changed {
        if current.document(path)?.revision() != state.original.document(path)?.revision() {
            return Err(CodeError::new(
                CodeErrorKind::StaleRevision,
                format!("document `{}` changed before write", path.display()),
            ));
        }
    }
    let nonce = std::process::id();
    let mut staged = Vec::new();
    for path in &state.changed {
        let target = workspace.join(path);
        let temporary = target.with_extension(format!("vibra-code-{nonce}.tmp"));
        fs::write(&temporary, state.database.document(path)?.source()).map_err(|error| {
            CodeError::new(
                CodeErrorKind::EditConflict,
                format!("stage {}: {error}", target.display()),
            )
        })?;
        staged.push((target, temporary));
    }

    let mut committed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (target, temporary) in staged {
        let backup = target.with_extension(format!("vibra-code-{nonce}.bak"));
        if let Err(error) =
            fs::rename(&target, &backup).and_then(|_| fs::rename(&temporary, &target))
        {
            for (committed_target, committed_backup) in committed.into_iter().rev() {
                let _ = fs::remove_file(&committed_target);
                let _ = fs::rename(&committed_backup, &committed_target);
            }
            let _ = fs::rename(&backup, &target);
            let _ = fs::remove_file(&temporary);
            return Err(CodeError::new(
                CodeErrorKind::EditConflict,
                format!("commit {}: {error}", target.display()),
            ));
        }
        committed.push((target, backup));
    }
    for (_, backup) in committed {
        fs::remove_file(&backup).map_err(|error| {
            CodeError::new(
                CodeErrorKind::EditConflict,
                format!("remove transaction backup {}: {error}", backup.display()),
            )
        })?;
    }
    Ok(())
}

fn validate_transaction(
    workspace: &FsPath,
    state: &PipelineState,
    lint: bool,
    test: bool,
) -> Result<(), CodeError> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = std::env::temp_dir().join(format!(
        "vibra-code-validation-{}-{nonce}",
        std::process::id()
    ));
    let validation = (|| -> Result<(), CodeError> {
        copy_workspace(workspace, &staging)?;
        for path in &state.changed {
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    CodeError::new(
                        CodeErrorKind::EditConflict,
                        format!("validation create {}: {error}", parent.display()),
                    )
                })?;
            }
            fs::write(&target, state.database.document(path)?.source()).map_err(|error| {
                CodeError::new(
                    CodeErrorKind::EditConflict,
                    format!("validation write {}: {error}", target.display()),
                )
            })?;
        }
        validate_compilation(&staging, &state.changed)?;
        if lint {
            validate_with_command(&staging, "lint")?;
        }
        if test {
            validate_with_command(&staging, "test")?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    validation.map_err(|error| {
        CodeError::new(error.kind, format!("validation failed: {}", error.message))
    })
}

fn copy_workspace(source: &FsPath, destination: &FsPath) -> Result<(), CodeError> {
    fs::create_dir_all(destination).map_err(|error| {
        CodeError::new(
            CodeErrorKind::EditConflict,
            format!("validation create {}: {error}", destination.display()),
        )
    })?;
    let mut entries = fs::read_dir(source)
        .map_err(|error| {
            CodeError::new(
                CodeErrorKind::EditConflict,
                format!("validation read {}: {error}", source.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CodeError::new(
                CodeErrorKind::EditConflict,
                format!("validation read directory entry: {error}"),
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | ".worktrees" | "target")) {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&name);
        if from.is_dir() {
            copy_workspace(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|error| {
                CodeError::new(
                    CodeErrorKind::EditConflict,
                    format!("validation copy {}: {error}", from.display()),
                )
            })?;
        }
    }
    Ok(())
}

fn validate_compilation(staging: &FsPath, changed: &BTreeSet<PathBuf>) -> Result<(), CodeError> {
    let manifest = staging.join(crate::project::MANIFEST_FILE);
    if manifest.exists() {
        let project = crate::project::load_project(&manifest).map_err(validation_error)?;
        for target in &project.manifest.targets.libs {
            let entry = project.root.join(&target.root).join(&target.entry);
            let loaded = crate::load::load_program(&entry)
                .map_err(|error| validation_path_error(&entry, error))?;
            crate::lower::lower_library(&loaded)
                .map_err(|error| validation_path_error(&entry, error))?;
        }
        for target in &project.manifest.targets.bins {
            let entry = project.root.join(&target.root).join(&target.entry);
            let loaded = crate::load::load_program(&entry)
                .map_err(|error| validation_path_error(&entry, error))?;
            crate::lower::lower_program(&loaded)
                .map_err(|error| validation_path_error(&entry, error))?;
        }
        return Ok(());
    }

    for path in changed {
        let entry = staging.join(path);
        let loaded = crate::load::load_program(&entry)
            .map_err(|error| validation_path_error(path, error))?;
        let has_main = loaded
            .modules
            .get(&loaded.entry)
            .and_then(Value::as_mapping)
            .is_some_and(|mapping| mapping.contains_key(Value::String("main".into())));
        if has_main {
            crate::lower::lower_program(&loaded)
                .map_err(|error| validation_path_error(path, error))?;
        } else {
            crate::lower::lower_library(&loaded)
                .map_err(|error| validation_path_error(path, error))?;
        }
    }
    Ok(())
}

fn validate_with_command(staging: &FsPath, command: &str) -> Result<(), CodeError> {
    let executable = std::env::current_exe().map_err(validation_error)?;
    let mut process = std::process::Command::new(executable);
    process.arg(command).arg(staging);
    if command == "lint" {
        process.arg("--deny-warnings");
    }
    let output = process.output().map_err(validation_error)?;
    if output.status.success() {
        return Ok(());
    }
    Err(CodeError::new(
        CodeErrorKind::EditConflict,
        format!(
            "{command} validation failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    ))
}

fn validation_error(error: impl std::fmt::Display) -> CodeError {
    CodeError::new(CodeErrorKind::EditConflict, error.to_string())
}

fn validation_path_error(path: &FsPath, error: impl std::fmt::Display) -> CodeError {
    CodeError::new(
        CodeErrorKind::EditConflict,
        format!("compile {}: {error}", path.display()),
    )
}

fn required_mapping<'a>(value: &'a Value, operation: &str) -> Result<&'a Mapping, CodeError> {
    value.as_mapping().ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("{operation} expects a mapping"),
        )
    })
}

fn required_string(mapping: &Mapping, key: &str) -> Result<String, CodeError> {
    get_string(mapping, key)?.ok_or_else(|| {
        CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("operation requires string field `{key}`"),
        )
    })
}

fn get_string(mapping: &Mapping, key: &str) -> Result<Option<String>, CodeError> {
    mapping
        .get(Value::String(key.into()))
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                CodeError::new(
                    CodeErrorKind::InvalidForm,
                    format!("field `{key}` must be a string"),
                )
            })
        })
        .transpose()
}

fn required_index(mapping: &Mapping, key: &str) -> Result<usize, CodeError> {
    mapping
        .get(Value::String(key.into()))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| {
            CodeError::new(
                CodeErrorKind::InvalidForm,
                format!("operation requires non-negative integer field `{key}`"),
            )
        })
}

fn parse_kind(value: &str) -> Result<NodeKind, CodeError> {
    match value {
        "scalar" => Ok(NodeKind::Scalar),
        "mapping" => Ok(NodeKind::Mapping),
        "sequence" => Ok(NodeKind::Sequence),
        _ => Err(CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("unknown node kind `{value}`"),
        )),
    }
}

fn parse_semantic_kind(value: &str) -> Result<SemanticKind, CodeError> {
    match value {
        "definition" => Ok(SemanticKind::Definition),
        "import" => Ok(SemanticKind::Import),
        "reference" => Ok(SemanticKind::Reference),
        "call" => Ok(SemanticKind::Call),
        _ => Err(CodeError::new(
            CodeErrorKind::InvalidForm,
            format!("unknown semantic kind `{value}`"),
        )),
    }
}

fn normalize_relative_path(path: &str) -> PathBuf {
    PathBuf::from(path.replace('/', std::path::MAIN_SEPARATOR_STR))
        .components()
        .collect()
}
