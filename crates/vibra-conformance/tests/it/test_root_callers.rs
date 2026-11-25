//! Who may issue a test world's root grant.
//!
//! `vibra_host::broker::Context::test_root` is public: the API of
//! `vibra-host` is the trusted embedding boundary, and checked source cannot
//! reach it (ruling F5a on the review of grants). Among the crates of the
//! workspace, the code that calls it outside tests is the test runner, which
//! gives each test its world, and the conformance runner beside it. A call
//! anywhere else would let a target's path issue a `TestWorld`, which
//! `docs/spec/06-runtime.md`, "Host world", gives only to a test.

use crate::support::{rust_sources, workspace_root};

/// The sources that may call `test_root`, by their paths below the
/// workspace root: one file, or every file below a directory ending in `/`.
const ALLOWED: [&str; 2] = [
    "crates/vibra-workspace/src/test_runner.rs",
    "crates/vibra-conformance/src/",
];

/// Whether `path`, below the workspace root with `/` separators, may call
/// `test_root`.
fn allowed(path: &str) -> bool {
    ALLOWED.iter().any(|allowed| {
        if allowed.ends_with('/') {
            path.starts_with(allowed)
        } else {
            path == *allowed
        }
    })
}

/// The lines of `source`, numbered from one, that call `test_root`: outside
/// comments and outside a `#[cfg(test)] mod` block, which is test code.
fn calls(source: &str) -> Vec<usize> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]")
            && lines
                .get(index + 1)
                .is_some_and(|next| next.trim_start().starts_with("mod "))
        {
            // Skip the module: through `mod name;` or its block.
            let mut depth: i64 = 0;
            let mut opened = false;
            index += 1;
            while let Some(text) = lines.get(index) {
                let code = text.split("//").next().unwrap_or(text);
                depth += i64::try_from(code.matches('{').count()).unwrap_or(0);
                depth -= i64::try_from(code.matches('}').count()).unwrap_or(0);
                opened |= code.contains('{');
                index += 1;
                if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';'))
                {
                    break;
                }
            }
            continue;
        }
        let code = line.split("//").next().unwrap_or(line);
        if !trimmed.starts_with("//")
            && (code.contains(".test_root(") || code.contains("::test_root("))
        {
            found.push(index + 1);
        }
        index += 1;
    }
    found
}

/// The calls of `test_root` outside the allowed sources, over `files` given
/// as `(path below the workspace root, source)`.
fn unexpected(files: &[(String, String)]) -> Vec<String> {
    files
        .iter()
        .filter(|(path, _)| !allowed(path))
        .flat_map(|(path, source)| {
            calls(source)
                .into_iter()
                .map(move |line| format!("{path}:{line}"))
        })
        .collect()
}

/// Every source below a `src` directory of a crate of the workspace, by its
/// path below the root with `/` separators.
fn workspace_sources() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("a crates directory") {
        let source_root = entry.expect("a readable entry").path().join("src");
        if !source_root.is_dir() {
            continue;
        }
        let mut sources = Vec::new();
        rust_sources(&source_root, &mut sources);
        for path in sources {
            let relative = path
                .strip_prefix(&root)
                .expect("a source below the root")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let source = std::fs::read_to_string(&path).expect("readable source");
            files.push((relative, source));
        }
    }
    files
}

#[test]
fn only_the_test_runner_and_the_conformance_runner_issue_a_test_root() {
    let files = workspace_sources();
    assert!(files.len() > 100, "the scan reads the workspace's sources");
    assert!(
        files
            .iter()
            .any(|(path, source)| allowed(path) && !calls(source).is_empty()),
        "the audit is not vacuous: the test runner issues the test root"
    );
    let found = unexpected(&files);
    assert!(
        found.is_empty(),
        "`Context::test_root` is called outside the test runner and the \
         conformance runner:\n{}",
        found.join("\n")
    );
}

#[test]
fn atk_test_root_called_by_a_target_path() {
    let file = |path: &str, source: &str| (path.to_owned(), source.to_owned());
    let files = [
        file(
            "crates/vibra-workspace/src/test_runner.rs",
            "let root = context.test_root();\n",
        ),
        file(
            "crates/vibra-conformance/src/workspace_test.rs",
            "let root = cx.test_root();\n",
        ),
        file(
            "crates/vibra-workspace/src/run.rs",
            "// context.test_root() is not ours\nlet root = context.test_root();\n",
        ),
        file(
            "crates/vibra-cli/src/run.rs",
            "let root = Context::test_root(&mut context);\n",
        ),
        file(
            "crates/vibra-host/src/startup.rs",
            "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() { cx.test_root(); }\n}\n\
             fn h() { cx.test_root(); }\n",
        ),
    ];
    assert_eq!(
        unexpected(&files),
        [
            "crates/vibra-workspace/src/run.rs:2",
            "crates/vibra-cli/src/run.rs:1",
            "crates/vibra-host/src/startup.rs:6",
        ]
    );
}
