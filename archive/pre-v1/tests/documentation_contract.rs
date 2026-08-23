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

/// Every normative document — accepted contracts under `docs/decisions/` and
/// the operational guides under `docs/reference/` — must contain only Vibra
/// source that the live grammars still accept. Both directories are gated so
/// that a language change cannot silently strand either one.
#[test]
fn contract_document_source_blocks_match_live_grammars() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for directory in ["docs/decisions", "docs/reference"] {
        let directory = root.join(directory);
        let mut paths = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect::<Vec<_>>();
        paths.sort();

        for block in paths.iter().flat_map(|path| read_fenced_blocks(path)) {
            let result = match block.tag.as_str() {
                "vibra" => parse_module(&block.body),
                "vibra-expr" => {
                    parse_module(&format!("(defn __doc () void (do {}))\n", block.body))
                }
                "vibra-project" => parse_project(&block.body),
                // A manifest fragment shown on its own (one or more
                // `(dependency ...)` forms). Spliced into a minimal valid
                // manifest so the fragment's own grammar is still checked.
                "vibra-project-fragment" => parse_project(&format!(
                    "(project\n  (package \"doc\" \"0.0.0\")\n  (target doc kind: @lib root: \"src\" entry: \"lib.vib\")\n{})\n",
                    block.body.trim_end()
                )),
                // Prose, grammar productions, and genuinely foreign source
                // (shell transcripts, CI workflows, editor config, tooling
                // payloads). These are not Vibra and have no Vibra grammar to
                // drift against.
                "text" | "ebnf" | "sh" | "json" | "yaml" | "lua" => Ok(()),
                // Syntax a document proposes but the compiler does not yet
                // accept. Deliberately ungated: gating it would force either a
                // false claim of implementation or the deletion of the
                // proposal. The tag is what keeps that choice explicit, so a
                // block only carries it while its document says the design is
                // unimplemented.
                "vibra-proposed" => Ok(()),
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
        // `{:#}` keeps the whole context chain; the outermost layer is only
        // ever "parse <tempfile>", which names no actual defect.
        .map_err(|error| format!("{error:#}"))
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
