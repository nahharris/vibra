//! `vibra fmt PATH [--write]` through the command line.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::support::{LIBRARY, TempDir, envelope_in, run_in};

const MESSY: &str = "fn  id takes {x:I32} returns r:I32 do return x endfn";
const CANONICAL: &str =
    "fn id\n  takes {x: I32}\n  returns r: I32\n  do\n    return x\nendfn\n";

fn messy_project(label: &str) -> TempDir {
    TempDir::project(label, LIBRARY, &[("src/app/main.vib", MESSY)])
}

#[test]
fn fmt_preview_leaves_file() {
    let root = messy_project("fmt-preview");
    let (code, document) = envelope_in(
        root.path(),
        &["--format", "json", "fmt", "src/app/main.vib"],
    );
    assert_eq!(code, 0);
    assert_eq!(document["command"], "fmt");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["result"], "@command.ok");
    assert!(document["workspaceRevision"].is_string());
    assert_eq!(
        document["payload"],
        json!({"path": "src/app/main.vib", "changed": true, "written": false,
            "text": CANONICAL})
    );
    assert_eq!(
        root.read("src/app/main.vib"),
        MESSY,
        "a preview writes nothing"
    );
}

#[test]
fn fmt_preview_in_human_format_prints_the_text() {
    let root = messy_project("fmt-human");
    let outcome = run_in(root.path(), &["fmt", "src/app/main.vib"]);
    assert_eq!(outcome.code, 0);
    assert_eq!(outcome.stdout, CANONICAL);
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
}

#[test]
fn fmt_write() {
    let root = messy_project("fmt-write");
    let (code, document) = envelope_in(
        root.path(),
        &["--format", "json", "fmt", "src/app/main.vib", "--write"],
    );
    assert_eq!(code, 0);
    assert_eq!(
        document["payload"],
        json!({"path": "src/app/main.vib", "changed": true, "written": true,
            "text": null})
    );
    assert_eq!(root.read("src/app/main.vib"), CANONICAL);
    let (code, again) = envelope_in(
        root.path(),
        &["--format", "json", "fmt", "src/app/main.vib", "--write"],
    );
    assert_eq!(code, 0);
    assert_eq!(again["payload"]["changed"], false);
    assert_eq!(again["payload"]["written"], false);
    assert_eq!(again["payload"]["text"], CANONICAL);
    assert_ne!(document["workspaceRevision"], again["workspaceRevision"]);
}

#[test]
fn fmt_write_is_refused_by_a_semantic_error() {
    let source = "fn  f returns r:I32 do return \"a\" endfn";
    let root =
        TempDir::project("fmt-semantic", LIBRARY, &[("src/app/main.vib", source)]);
    let (code, document) = envelope_in(
        root.path(),
        &["--format", "json", "fmt", "src/app/main.vib", "--write"],
    );
    assert_eq!(code, 1);
    assert_eq!(document["result"], "@command.diagnostics");
    assert_eq!(document["status"], "refuted");
    let planned = "fn f\n  returns r: I32\n  do\n    return \"a\"\nendfn\n";
    assert_eq!(
        document["payload"],
        json!({"path": "src/app/main.vib", "changed": true, "written": false,
            "text": planned})
    );
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(diagnostic["code"], "@type.mismatch");
    // The span is in the planned text: `"a"` on line 4.
    assert_eq!(diagnostic["primarySpan"]["sourceId"], "src/app/main.vib");
    assert_eq!(diagnostic["primarySpan"]["start"], 38);
    assert_eq!(diagnostic["primarySpan"]["end"], 41);
    assert_eq!(
        diagnostic["primarySpan"]["startPosition"],
        json!({"line": 4, "column": 12})
    );
    assert_eq!(root.read("src/app/main.vib"), source);
}

#[test]
fn fmt_extension() {
    let root = messy_project("fmt-extension");
    root.write("README.md", "# readme\n");
    let (code, document) =
        envelope_in(root.path(), &["--format", "json", "fmt", "README.md"]);
    assert_eq!(code, 2);
    assert_eq!(document["result"], "@command.invalidInput");
    assert_eq!(document["payload"], json!({}));
    assert_eq!(document["diagnostics"], json!([]));
}

#[test]
fn fmt_escape() {
    let parent = TempDir::new("fmt-escape-parent");
    parent.write("other/a.vib", MESSY);
    parent.write("work/project.vibon", LIBRARY);
    parent.write("work/src/app/main.vib", MESSY);
    let workspace = parent.path().join("work");
    let (code, document) =
        envelope_in(&workspace, &["--format", "json", "fmt", "../other/a.vib"]);
    assert_eq!(code, 1);
    assert_eq!(document["result"], "@command.diagnostics");
    let diagnostics = document["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "@module.pathEscape");
    assert_eq!(diagnostics[0]["primarySpan"]["start"], 0);
    assert_eq!(document["payload"]["text"], json!(null));
    assert_eq!(document["payload"]["written"], false);
}

#[test]
fn fmt_missing_file() {
    let root = messy_project("fmt-missing");
    let (code, document) = envelope_in(
        root.path(),
        &["--format", "json", "fmt", "src/app/none.vib"],
    );
    assert_eq!(code, 1);
    let diagnostics = document["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "@module.ioError");
    assert_eq!(
        diagnostics[0]["primarySpan"]["sourceId"],
        "src/app/none.vib"
    );
    assert_eq!(document["payload"]["path"], "src/app/none.vib");
}

fn files_below(directory: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            files_below(&path, extension, found);
        } else if path.extension().and_then(|text| text.to_str()) == Some(extension) {
            found.push(path);
        }
    }
}

/// The `formatted` snapshot of every accepted case: the canonical texts.
fn canonical_corpus_texts(cases: &Path, found: &mut Vec<PathBuf>) {
    let mut manifests = Vec::new();
    files_below(cases, "toml", &mut manifests);
    for manifest in manifests {
        let text = std::fs::read_to_string(&manifest).expect("a manifest");
        if !text.contains("verdict = \"accepted\"") {
            continue;
        }
        for line in text.lines() {
            if let Some(value) = line.trim().strip_prefix("formatted = ") {
                let relative = value.trim_matches('"');
                found.push(manifest.parent().expect("case directory").join(relative));
            }
        }
    }
}

#[test]
fn canonical_sources_are_reported_unchanged() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    files_below(&repository.join("stdlib"), "vib", &mut files);
    let stdlib = files.len();
    canonical_corpus_texts(&repository.join("conformance/cases"), &mut files);
    assert!(stdlib >= 6, "the standard library has its modules");
    assert!(
        files.len() > stdlib + 50,
        "the corpus has its canonical texts"
    );
    for file in &files {
        let directory = file.parent().expect("a directory");
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .expect("name");
        let (code, document) =
            envelope_in(directory, &["--format", "json", "fmt", name]);
        assert_eq!(document["payload"]["changed"], false, "{}", file.display());
        // A standard-library module uses forms only the embedded library
        // admits, so the ordinary reader keeps its bytes and reports them.
        if !file.starts_with(repository.join("stdlib")) {
            assert_eq!(code, 0, "{}: {document}", file.display());
        }
    }
}
