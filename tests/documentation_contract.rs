use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::tempdir;

#[derive(Debug)]
struct FencedBlock {
    path: PathBuf,
    opening_line: usize,
    tag: String,
    body: String,
}

#[test]
fn decision_document_source_blocks_match_live_grammars() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let decisions = root.join("docs/decisions");
    let mut paths = fs::read_dir(&decisions)
        .unwrap_or_else(|error| panic!("read {}: {error}", decisions.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    paths.sort();

    let blocks = paths
        .iter()
        .flat_map(|path| read_fenced_blocks(path))
        .collect::<Vec<_>>();
    let mut failures = Vec::new();

    for block in &blocks {
        let result = match block.tag.as_str() {
            "vibra" => parse_module(&block.body),
            "vibra-expr" => parse_module(&format!("(defn __doc () void (do {}))\n", block.body)),
            "vibra-project" => parse_project(&block.body),
            "text" | "ebnf" => Ok(()),
            tag => Err(format!("unrecognized documentation fence tag `{tag}`")),
        };
        if let Err(error) = result {
            let relative = block.path.strip_prefix(root).unwrap_or(&block.path);
            failures.push(format!(
                "{}:{} ({}): {error}",
                relative.display(),
                block.opening_line,
                block.tag
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "documentation source blocks do not match live grammars:\n{}",
        failures.join("\n")
    );
}

fn read_fenced_blocks(path: &Path) -> Vec<FencedBlock> {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut blocks = Vec::new();
    let mut current: Option<PartialBlock> = None;

    for (index, line) in source.lines().enumerate() {
        if let Some(block) = &mut current {
            if line.trim() == "```" {
                blocks.push(FencedBlock {
                    path: path.to_path_buf(),
                    opening_line: block.opening_line,
                    tag: block.tag.clone(),
                    body: std::mem::take(&mut block.body),
                });
                current = None;
            } else {
                block.body.push_str(line);
                block.body.push('\n');
            }
            continue;
        }

        let Some(rest) = line.trim_start().strip_prefix("```") else {
            continue;
        };
        current = Some(PartialBlock {
            opening_line: index + 1,
            tag: rest.trim().to_owned(),
            body: String::new(),
        });
    }

    assert!(
        current.is_none(),
        "unclosed documentation fence in {}",
        path.display()
    );
    blocks
}

struct PartialBlock {
    opening_line: usize,
    tag: String,
    body: String,
}

fn parse_module(source: &str) -> Result<(), String> {
    let document = vibra::syntax::parse(source).map_err(|error| error.to_string())?;
    vibra::ast::lower_document(&document)
        .map(|_| ())
        .map_err(|error| error.to_string())?;
    let printed = vibra::syntax::print(&document);
    vibra::syntax::parse(&printed)
        .map(|_| ())
        .map_err(|error| format!("printer output is not readable: {error}"))
}

fn parse_project(source: &str) -> Result<(), String> {
    let directory = tempdir().map_err(|error| error.to_string())?;
    let path = directory.path().join("project.vib");
    fs::write(&path, source).map_err(|error| error.to_string())?;
    vibra::project::load_project(&path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[test]
fn secure_compilation_contract_separates_semantics_from_attack_prevention() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("docs/decisions/secure-compilation.md");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(source.contains("semantic preservation"), "{path:?}");
    assert!(source.contains("attack prevention"), "{path:?}");
    assert!(
        source.contains("does not claim") || source.contains("not claim"),
        "the contract must disclaim attack-prevention claims: {path:?}"
    );
}
