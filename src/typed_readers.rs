//! Staged read-only consumers for the typed S-expression frontend.
//!
//! Existing public CLI commands remain on their legacy readers until the
//! source corpus is cut over. These APIs consume typed physical module parts
//! directly and never adapt the AST through YAML.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::ast::{
    Annotation, AnnotationKind, DocumentId, Function, ImplItem, MethodBinding, TestMeta, TopLevel,
    TypeExpr, TypeExprKind, Visibility,
};
use crate::frontend::{SourceModule, SurfaceProgram};
use crate::project_context::{self, ProjectImportContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedTypedDocumentation {
    pub symbol: String,
    pub kind: String,
    pub documentation: String,
    pub visibility: Visibility,
    pub module: PathBuf,
    pub source_part: PathBuf,
    pub source: StagedSourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedSourceLocation {
    pub document: DocumentId,
    pub span: crate::syntax::Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedExpectedError {
    Load {
        code: String,
        message: Option<String>,
    },
    Compile {
        code: String,
        message: Option<String>,
    },
    Runtime {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedTestClock {
    pub unix_millis: i64,
    pub monotonic_millis: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StagedTypedTest {
    pub name: String,
    pub profile: String,
    pub tags: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub skip: Option<String>,
    pub random_seed: Option<u64>,
    pub expect_error: Option<StagedExpectedError>,
    pub clock: Option<StagedTestClock>,
    pub workspace: Option<String>,
    pub module: PathBuf,
    pub source_part: PathBuf,
    pub source: StagedSourceLocation,
    pub metadata: Vec<StagedTestMetadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StagedTestMetadata {
    Tags(Vec<StagedMetadataValue<String>>),
    TimeoutMillis(StagedMetadataValue<i64>),
    RandomSeed(StagedMetadataValue<i64>),
    Skip(StagedMetadataValue<String>),
    LoadError {
        code: StagedMetadataValue<String>,
        message: Option<StagedMetadataValue<String>>,
    },
    CompileError {
        code: StagedMetadataValue<String>,
        message: Option<StagedMetadataValue<String>>,
    },
    RuntimeError {
        message: StagedMetadataValue<String>,
    },
    Clock {
        unix_millis: StagedMetadataValue<i64>,
        monotonic_millis: StagedMetadataValue<i64>,
    },
    Workspace(StagedMetadataValue<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedMetadataValue<T> {
    pub value: T,
    pub source: StagedSourceLocation,
}

/// Collect typed declaration documentation through the entry import graph.
pub fn staged_collect_typed_docs(
    program: &SurfaceProgram,
) -> Result<Vec<StagedTypedDocumentation>> {
    let project = project_context::discover_project_import_context(&program.entry)?;
    let mut docs = BTreeMap::new();
    let mut visited = BTreeSet::new();
    collect_module_docs(
        program,
        &program.entry,
        "",
        project.as_ref(),
        &mut visited,
        &mut docs,
    )?;
    Ok(docs.into_values().collect())
}

/// Discover tests declared by the entry logical module across its selected
/// physical parts. Imported modules are intentionally not test roots.
pub fn staged_discover_typed_tests(program: &SurfaceProgram) -> Result<Vec<StagedTypedTest>> {
    let module = program
        .modules
        .get(&program.entry)
        .with_context(|| format!("entry module {} is missing", program.entry.display()))?;
    let mut tests = Vec::new();
    for part in &module.parts {
        for form in &part.module.forms {
            let cases = match form {
                TopLevel::Test(test) => vec![(test.name.value.clone(), test)],
                TopLevel::TestScenario(scenario) => scenario
                    .cases
                    .iter()
                    .map(|test| {
                        (
                            format!("{}::{}", scenario.name.value, test.name.value),
                            test,
                        )
                    })
                    .collect(),
                _ => continue,
            };
            for (name, test) in cases {
                let mut discovered = StagedTypedTest {
                    name,
                    profile: test.profile.value.clone(),
                    tags: Vec::new(),
                    timeout_ms: None,
                    skip: None,
                    random_seed: None,
                    expect_error: None,
                    clock: None,
                    workspace: None,
                    module: module.path.clone(),
                    source_part: part.path.clone(),
                    source: StagedSourceLocation {
                        document: part.module.document_id,
                        span: test.span,
                    },
                    metadata: Vec::new(),
                };
                for metadata in &test.metadata {
                    match metadata {
                        TestMeta::Tags(tags) => {
                            discovered.tags = tags.iter().map(|tag| tag.value.clone()).collect();
                            discovered.metadata.push(StagedTestMetadata::Tags(
                                tags.iter()
                                    .map(|tag| metadata_value(part.module.document_id, tag))
                                    .collect(),
                            ));
                        }
                        TestMeta::ExpectError(expected) => match expected {
                            crate::ast::ExpectedError::Load { code, message } => {
                                discovered.expect_error = Some(StagedExpectedError::Load {
                                    code: code.value.clone(),
                                    message: message.as_ref().map(|value| value.value.clone()),
                                });
                                discovered.metadata.push(StagedTestMetadata::LoadError {
                                    code: metadata_value(part.module.document_id, code),
                                    message: message.as_ref().map(|value| {
                                        metadata_value(part.module.document_id, value)
                                    }),
                                });
                            }
                            crate::ast::ExpectedError::Compile { code, message } => {
                                discovered.expect_error = Some(StagedExpectedError::Compile {
                                    code: code.value.clone(),
                                    message: message.as_ref().map(|value| value.value.clone()),
                                });
                                discovered.metadata.push(StagedTestMetadata::CompileError {
                                    code: metadata_value(part.module.document_id, code),
                                    message: message.as_ref().map(|value| {
                                        metadata_value(part.module.document_id, value)
                                    }),
                                });
                            }
                            crate::ast::ExpectedError::Runtime { message } => {
                                discovered.expect_error = Some(StagedExpectedError::Runtime {
                                    message: message.value.clone(),
                                });
                                discovered.metadata.push(StagedTestMetadata::RuntimeError {
                                    message: metadata_value(part.module.document_id, message),
                                });
                            }
                        },
                        TestMeta::Clock {
                            unix_millis,
                            monotonic_millis,
                        } => {
                            discovered.clock = Some(StagedTestClock {
                                unix_millis: unix_millis.value,
                                monotonic_millis: monotonic_millis.value,
                            });
                            discovered.metadata.push(StagedTestMetadata::Clock {
                                unix_millis: metadata_value(part.module.document_id, unix_millis),
                                monotonic_millis: metadata_value(
                                    part.module.document_id,
                                    monotonic_millis,
                                ),
                            });
                        }
                        TestMeta::Workspace(name) => {
                            discovered.workspace = Some(name.value.clone());
                            discovered.metadata.push(StagedTestMetadata::Workspace(
                                metadata_value(part.module.document_id, name),
                            ));
                        }
                        TestMeta::TimeoutMillis(value) => {
                            discovered.timeout_ms = Some(value.value as u64);
                            discovered.metadata.push(StagedTestMetadata::TimeoutMillis(
                                metadata_value(part.module.document_id, value),
                            ));
                        }
                        TestMeta::RandomSeed(value) => {
                            discovered.random_seed = Some(value.value as u64);
                            discovered.metadata.push(StagedTestMetadata::RandomSeed(
                                metadata_value(part.module.document_id, value),
                            ));
                        }
                        TestMeta::Skip(value) => {
                            discovered.skip = Some(value.value.clone());
                            discovered
                                .metadata
                                .push(StagedTestMetadata::Skip(metadata_value(
                                    part.module.document_id,
                                    value,
                                )));
                        }
                    }
                }
                tests.push(discovered);
            }
        }
    }
    tests.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.source_part.cmp(&right.source_part))
    });
    Ok(tests)
}

fn metadata_value<T: Clone>(
    document: DocumentId,
    value: &crate::ast::Spanned<T>,
) -> StagedMetadataValue<T> {
    StagedMetadataValue {
        value: value.value.clone(),
        source: StagedSourceLocation {
            document,
            span: value.span,
        },
    }
}

fn collect_module_docs(
    program: &SurfaceProgram,
    module_path: &Path,
    qualifier: &str,
    project: Option<&ProjectImportContext>,
    visited: &mut BTreeSet<(PathBuf, String)>,
    docs: &mut BTreeMap<String, StagedTypedDocumentation>,
) -> Result<()> {
    if !visited.insert((module_path.to_path_buf(), qualifier.to_string())) {
        return Ok(());
    }
    let module = program
        .modules
        .get(module_path)
        .with_context(|| format!("typed module {} is missing", module_path.display()))?;
    for part in &module.parts {
        for form in &part.module.forms {
            match form {
                TopLevel::Import(import) => {
                    let imported = resolve_import(module, &import.path.value, project)?;
                    collect_module_docs(
                        program,
                        &imported,
                        &qualify(qualifier, &import.alias.value),
                        project,
                        visited,
                        docs,
                    )?;
                }
                TopLevel::Definition(definition) => {
                    let symbol = qualify(qualifier, &definition.name.value);
                    collect_annotated_docs(
                        &symbol,
                        definition_kind(&definition.body),
                        definition.visibility,
                        &definition.annotations,
                        module_path,
                        &part.path,
                        StagedSourceLocation {
                            document: part.module.document_id,
                            span: definition.span,
                        },
                        docs,
                    );
                }
                TopLevel::Constant(constant) => {
                    let symbol = qualify(qualifier, &constant.name.value);
                    collect_annotated_docs(
                        &symbol,
                        "constant",
                        constant.visibility,
                        &constant.annotations,
                        module_path,
                        &part.path,
                        StagedSourceLocation {
                            document: part.module.document_id,
                            span: constant.span,
                        },
                        docs,
                    );
                }
                TopLevel::Function(function) => {
                    let symbol = qualify(qualifier, &function.name.value);
                    collect_function_docs(
                        &symbol,
                        function,
                        module_path,
                        &part.path,
                        part.module.document_id,
                        docs,
                    );
                }
                TopLevel::Macro(macro_definition) => {
                    let symbol = qualify(qualifier, &macro_definition.name.value);
                    collect_annotated_docs(
                        &symbol,
                        "macro",
                        macro_definition.visibility,
                        &macro_definition.annotations,
                        module_path,
                        &part.path,
                        StagedSourceLocation {
                            document: part.module.document_id,
                            span: macro_definition.span,
                        },
                        docs,
                    );
                }
                TopLevel::Test(_) | TopLevel::TestScenario(_) => {}
            }
        }
    }
    Ok(())
}

fn collect_function_docs(
    symbol: &str,
    function: &Function,
    module: &Path,
    source_part: &Path,
    document: DocumentId,
    docs: &mut BTreeMap<String, StagedTypedDocumentation>,
) {
    collect_annotated_docs(
        symbol,
        "function",
        function.visibility,
        &function.annotations,
        module,
        source_part,
        StagedSourceLocation {
            document,
            span: function.span,
        },
        docs,
    );
}

fn collect_annotated_docs(
    symbol: &str,
    kind: &str,
    visibility: Visibility,
    annotations: &[Annotation],
    module: &Path,
    source_part: &Path,
    source: StagedSourceLocation,
    docs: &mut BTreeMap<String, StagedTypedDocumentation>,
) {
    if let Some(documentation) = annotations
        .iter()
        .find_map(|annotation| match &annotation.value {
            AnnotationKind::Doc(documentation) => Some(documentation),
            _ => None,
        })
    {
        docs.insert(
            symbol.to_string(),
            StagedTypedDocumentation {
                symbol: symbol.to_string(),
                kind: kind.to_string(),
                documentation: documentation.clone(),
                visibility,
                module: module.to_path_buf(),
                source_part: source_part.to_path_buf(),
                source,
            },
        );
    }
    for annotation in annotations {
        if let AnnotationKind::Definitions(functions) = &annotation.value {
            for function in functions {
                collect_function_docs(
                    &format!("{symbol}.{}", function.name.value),
                    function,
                    module,
                    source_part,
                    source.document,
                    docs,
                );
            }
        }
        if let AnnotationKind::Implementation { interface, items } = &annotation.value {
            let interface_name = type_name(interface);
            for item in items {
                let ImplItem::Method {
                    name,
                    binding: MethodBinding::Function(function),
                    ..
                } = item
                else {
                    continue;
                };
                collect_function_docs(
                    &format!("{symbol}.{interface_name}.{}", name.value),
                    function,
                    module,
                    source_part,
                    source.document,
                    docs,
                );
            }
        }
    }
}

fn definition_kind(body: &TypeExpr) -> &'static str {
    match &body.value {
        TypeExprKind::Record(_) => "record",
        TypeExprKind::Enum(_) => "enum",
        TypeExprKind::Interface(_) => "interface",
        TypeExprKind::Newtype(_) => "newtype",
        TypeExprKind::Function { .. } => "function-type",
        TypeExprKind::Effect { .. } => "effect",
        _ => "type",
    }
}

fn type_name(ty: &TypeExpr) -> &str {
    match &ty.value {
        TypeExprKind::Named(name) => name,
        TypeExprKind::Application { constructor, .. } => &constructor.value,
        _ => "implementation",
    }
}

fn resolve_import(
    module: &SourceModule,
    import: &str,
    project: Option<&ProjectImportContext>,
) -> Result<PathBuf> {
    let path = if import.starts_with('@') {
        project_context::resolve_project_import(
            project.context("@ import requires a project.vibra")?,
            import,
        )?
    } else {
        module
            .path
            .parent()
            .with_context(|| format!("{} has no parent directory", module.path.display()))?
            .join(import)
    };
    fs::canonicalize(&path).with_context(|| {
        format!(
            "{}: cannot resolve typed documentation import `{import}`",
            module.path.display()
        )
    })
}

fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::load::CompilationFlags;

    use super::*;

    fn write(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
    }

    #[test]
    fn docs_follow_imports_and_physical_parts_without_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helper \"helper.vibra\")\n\
             (defn main () void (do unit) doc: \"Entry docs\")\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(const fixture int64 1 doc: \"Fixture docs\")\n",
        );
        write(
            &temp.path().join("helper.vibra"),
            "(def box (record (value int64))\n\
             doc: \"Box docs\"\n\
             defs: ((defn get (input self) int64 (do (return 0)) doc: \"Getter docs\"))\n\
             impls: ((impl display methods: ((method show\n\
               (fn (input self) str (do (return \"box\")) doc: \"Show docs\"))))))\n",
        );
        let program =
            crate::frontend::load_surface_program(&entry, &CompilationFlags::new(["test"]))
                .unwrap();
        let docs = staged_collect_typed_docs(&program).unwrap();
        assert_eq!(
            docs.iter()
                .map(|doc| doc.symbol.as_str())
                .collect::<Vec<_>>(),
            [
                "fixture",
                "helper.box",
                "helper.box.display.show",
                "helper.box.get",
                "main"
            ]
        );
        assert!(docs
            .iter()
            .find(|doc| doc.symbol == "fixture")
            .unwrap()
            .source_part
            .ends_with("main.test.vibra"));
        let box_docs = docs.iter().find(|doc| doc.symbol == "helper.box").unwrap();
        assert!(box_docs.module.ends_with("helper.vibra"));
        assert_eq!(box_docs.kind, "record");
        assert_eq!(
            box_docs.source.document,
            DocumentId::from_path(&box_docs.source_part)
        );
    }

    #[test]
    fn tests_preserve_all_label_metadata_across_entry_parts() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.vibra");
        write(
            &entry,
            "(import helper \"helper.vibra\")\n\
             (test.scenario \"base\" (test.case \"base\" unit tags: (@fast)))\n",
        );
        write(
            &temp.path().join("main.test.vibra"),
            "(test.scenario \"measured\" (test.case \"measured\" unit\n\
             tags: (@slow @arithmetic)\n\
             expect-error: (@compile E-OP-002 \"overflow\")\n\
             clock: (@fixed 42 7)\n\
             workspace: @temp))\n",
        );
        write(
            &temp.path().join("helper.vibra"),
            "(test.scenario \"imported\" (test.case \"imported\" unit))\n",
        );
        let program =
            crate::frontend::load_surface_program(&entry, &CompilationFlags::new(["test"]))
                .unwrap();
        let tests = staged_discover_typed_tests(&program).unwrap();
        assert_eq!(
            tests
                .iter()
                .map(|test| test.name.as_str())
                .collect::<Vec<_>>(),
            ["base::base", "measured::measured"]
        );
        let measured = tests
            .iter()
            .find(|test| test.name == "measured::measured")
            .unwrap();
        assert_eq!(measured.tags, ["slow", "arithmetic"]);
        assert_eq!(
            measured.expect_error,
            Some(StagedExpectedError::Compile {
                code: "E-OP-002".into(),
                message: Some("overflow".into()),
            })
        );
        assert_eq!(
            measured.clock,
            Some(StagedTestClock {
                unix_millis: 42,
                monotonic_millis: 7,
            })
        );
        assert_eq!(measured.workspace.as_deref(), Some("temp"));
        assert!(measured.source_part.ends_with("main.test.vibra"));
        assert_eq!(
            measured.source.document,
            DocumentId::from_path(&measured.source_part)
        );
        assert!(measured.metadata.iter().all(|metadata| {
            let source = match metadata {
                StagedTestMetadata::Tags(values) => values[0].source,
                StagedTestMetadata::TimeoutMillis(value) => value.source,
                StagedTestMetadata::RandomSeed(value) => value.source,
                StagedTestMetadata::Skip(value) => value.source,
                StagedTestMetadata::LoadError { code, .. }
                | StagedTestMetadata::CompileError { code, .. } => code.source,
                StagedTestMetadata::RuntimeError { message } => message.source,
                StagedTestMetadata::Clock { unix_millis, .. } => unix_millis.source,
                StagedTestMetadata::Workspace(value) => value.source,
            };
            source.document == measured.source.document && source.span.end > source.span.start
        }));
    }
}
