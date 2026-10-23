//! The ambient-read audit as a test (Milestone 4, Step 16a; ledger D31.1).
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Required implementation
//! suites": no crate that checks, lowers, or executes a program uses the ambient
//! filesystem, environment, clock, or random source. The few tooling crates that
//! read a project, a command line, or a corpus are named below, and the provider
//! crate `vibra-host` is where the ambient reads of a running program live, so
//! that every one of them passes through the host provider and a registry row.

// A test asserts by panicking, and indexes fixtures it just built.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The crate that reads ambient state on behalf of a running program: the system
/// provider.
const PROVIDER_CRATE: &str = "vibra-host";

/// The tooling crates that may read ambient state, each for the reason given.
/// They check, lower, or execute nothing by themselves: they hand explicit
/// inputs to the crates that do.
const TOOLING_EXCEPTIONS: &[(&str, &str)] = &[
    (
        "vibra-cli",
        "reads the command line and the process environment of the `vibra` binary",
    ),
    (
        "vibra-workspace",
        "acquires a project's files through the confined filesystem",
    ),
    (
        "vibra-conformance",
        "reads the conformance corpus and its fixtures",
    ),
];

/// The ambient APIs: the filesystem, the environment, the clocks, the operating
/// system's random source, and the process's standard streams.
const AMBIENT_PATTERNS: &[&str] = &[
    "std::fs",
    "fs::",
    "File::open",
    "File::create",
    "read_dir(",
    "std::env",
    "env::var",
    "env::args",
    "env::current_dir",
    "SystemTime",
    "Instant::now",
    "std::time::Instant",
    "UNIX_EPOCH",
    "getrandom",
    "RandomState",
    "std::io::stdin",
    "io::stdin(",
    "io::stdout(",
    "io::stderr(",
    "std::process",
    "println!",
    "eprintln!",
    "print!(",
    "eprint!(",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn crate_names(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root.join("crates"))
        .expect("the workspace has a crates directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// Every crate of the workspace except the provider crate and the named tooling
/// crates: the crates that check, lower, or execute a program, and the wire and
/// formatting crates beside them.
fn scanned_crates(root: &Path) -> BTreeSet<String> {
    let exempt = TOOLING_EXCEPTIONS
        .iter()
        .map(|(name, _)| *name)
        .chain([PROVIDER_CRATE])
        .collect::<BTreeSet<_>>();
    crate_names(root)
        .into_iter()
        .filter(|name| !exempt.contains(name.as_str()))
        .collect()
}

/// The ambient APIs `source` names outside of comments and `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
fn ambient_references(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        // Everything from the unit-test module on is test code.
        if trimmed.starts_with("#[cfg(test)]") {
            break;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        let code = line.split("//").next().unwrap_or(line);
        for pattern in AMBIENT_PATTERNS {
            if code.contains(pattern) {
                found.push((index + 1, *pattern));
            }
        }
    }
    found
}

fn scan(root: &Path, crate_name: &str) -> Vec<String> {
    let mut sources = Vec::new();
    rust_sources(
        &root.join("crates").join(crate_name).join("src"),
        &mut sources,
    );
    assert!(!sources.is_empty(), "{crate_name} has no sources");
    let mut found = Vec::new();
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("readable source");
        for (line, pattern) in ambient_references(&source) {
            found.push(format!("{}:{line} names `{pattern}`", path.display()));
        }
    }
    found
}

#[test]
fn no_crate_that_checks_lowers_or_executes_a_program_reads_ambient_state() {
    let root = workspace_root();
    let scanned = scanned_crates(&root);
    assert!(
        scanned.len() >= 10,
        "the scan covers the language crates ({scanned:?})"
    );
    let found = scanned
        .iter()
        .flat_map(|crate_name| scan(&root, crate_name))
        .collect::<Vec<_>>();
    assert!(
        found.is_empty(),
        "ambient reads outside the host provider and the named tooling crates:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_tooling_exceptions_and_the_provider_crate_exist_and_the_provider_does_read() {
    let root = workspace_root();
    let crates = crate_names(&root);
    for (name, reason) in TOOLING_EXCEPTIONS {
        assert!(
            crates.contains(*name),
            "the exception `{name}` names no crate"
        );
        assert!(!reason.is_empty());
    }
    assert!(crates.contains(PROVIDER_CRATE));
    // The audit is not vacuous: the provider is where the reads are.
    let provider = scan(&root, PROVIDER_CRATE);
    for api in [
        "std::fs",
        "std::env",
        "SystemTime",
        "Instant::now",
        "getrandom",
        "io::stdin(",
    ] {
        assert!(
            provider.iter().any(|line| line.contains(api)),
            "the provider crate no longer reads through `{api}`: {provider:?}"
        );
    }
}

#[test]
fn the_emitter_the_checker_and_the_interpreter_are_scanned() {
    let scanned_crates = scanned_crates(&workspace_root());
    for scanned in [
        "vibra-syntax",
        "vibra-resolve",
        "vibra-types",
        "vibra-ir",
        "vibra-interp",
        "vibra-native",
        "vibra-wasm",
        "vibra-wasm-run",
        "vibra-fmt",
        "vibra-schema",
    ] {
        assert!(
            scanned_crates.contains(scanned),
            "`{scanned}` must be scanned"
        );
    }
    for exempt in TOOLING_EXCEPTIONS
        .iter()
        .map(|(name, _)| *name)
        .chain([PROVIDER_CRATE])
    {
        assert!(!scanned_crates.contains(exempt), "`{exempt}` is exempt");
    }
}

#[test]
fn an_ambient_read_is_reported_and_comments_and_test_modules_are_not() {
    assert_eq!(
        ambient_references("let text = std::fs::read_to_string(path);"),
        [(1, "std::fs"), (1, "fs::")]
    );
    assert_eq!(
        ambient_references("let now = std::time::SystemTime::now();"),
        [(1, "SystemTime")]
    );
    assert!(
        ambient_references("// std::env::var is read by the provider\n").is_empty()
    );
    assert!(
        ambient_references("fn f() {}\n#[cfg(test)]\nmod tests { use std::fs; }\n")
            .is_empty()
    );
    assert!(
        ambient_references("let x = 1; // std::fs in a trailing comment").is_empty()
    );
}
