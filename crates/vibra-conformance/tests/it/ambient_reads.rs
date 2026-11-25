//! The ambient-read audit as a test (Milestone 4, Step 16a; ledger D31.1).
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Required implementation
//! suites": no crate that checks, lowers, or executes a program uses the ambient
//! filesystem, environment, clock, or random source. The few tooling crates that
//! read a project, a command line, or a corpus are named below, and the provider
//! crates `vibra-host` and `vibra-host-v1` are where the ambient reads of a
//! running program live, so that every one of them passes through the host
//! provider and a registry row. Inside `vibra-host` they live in one place,
//! the system drivers under `src/system/`: the broker, the host world, the
//! trace, and startup reach the process only through a driver.

// A test asserts by panicking, and indexes fixtures it just built.
use crate::support::{rust_sources, workspace_root};

use std::collections::BTreeSet;
use std::path::Path;

/// The crates that read ambient state on behalf of a running program: the
/// system provider.
const PROVIDER_CRATES: &[&str] = &["vibra-host", "vibra-host-v1"];

/// The tooling crates that may read ambient state, each for the reason given.
/// They check, lower, or execute nothing by themselves: they hand explicit
/// inputs to the crates that do.
const TOOLING_EXCEPTIONS: &[(&str, &str)] = &[
    (
        "vibra-cli",
        "reads the command line, the working directory, and the standard streams of the `vibra` binary, in `main.rs`",
    ),
    (
        "vibra-workspace",
        "acquires a project's files: discovery and the source snapshot",
    ),
    (
        "vibra-conformance",
        "reads the conformance corpus, its rule index, and its fixtures, and its binary reads the command line",
    ),
    (
        "vibra-build",
        "reads the environment, the registry through `reg.exe`, and the filesystem to find the linker, and starts it, all in its module `system` (`the_build_driver_reads_the_machine_in_one_module`)",
    ),
    (
        "vibra-fs-scope",
        "is the filesystem adapter itself: it opens entries relative to a held directory handle and does nothing else",
    ),
    (
        "vibra-nexus-bridge",
        "its launcher `main` reads the process arguments and standard streams of the executable, and its harness profile reads standard input, then hands them to the host provider",
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

/// The ambient APIs `vibra-host` names only under `src/system/`: those of
/// every other crate, and the clocks of `std::time` whatever their spelling.
const HOST_SYSTEM_PATTERNS: &[&str] = &["std::time"];

/// The directory of `vibra-host/src` where its system drivers live.
const HOST_SYSTEM_DIRECTORY: &str = "system/";

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
        .chain(PROVIDER_CRATES.iter().copied())
        .collect::<BTreeSet<_>>();
    crate_names(root)
        .into_iter()
        .filter(|name| !exempt.contains(name.as_str()))
        .collect()
}

/// The attribute that gates host-language test code.
const TEST_GATE: &str = "#[cfg(test)]";

/// Whether `text`, trimmed, opens a module: `mod name`, with or without a
/// visibility.
fn opens_module(text: &str) -> bool {
    let text = text.trim_start();
    let text = ["pub(crate) ", "pub(super) ", "pub "]
        .iter()
        .find_map(|visibility| text.strip_prefix(visibility))
        .unwrap_or(text);
    text.starts_with("mod ")
}

/// The lines of `source`, numbered from one, that are not in a
/// `#[cfg(test)]` module: the attribute, the `mod` it gates, and that
/// module's block, matched brace by brace, are left out. A `#[cfg(test)]`
/// field, function, or other item is kept, and so is every line after the
/// module.
fn outside_test_modules(source: &str) -> Vec<(usize, &str)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(TEST_GATE) else {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        };
        // The gated item: on the attribute's line, or on the next line that
        // is neither an attribute nor a comment.
        let mut item = index;
        if rest.trim().is_empty() {
            item += 1;
            while lines.get(item).is_some_and(|text| {
                let text = text.trim_start();
                text.starts_with("#[") || text.starts_with("//")
            }) {
                item += 1;
            }
        }
        let opens = lines.get(item).is_some_and(|text| {
            let text = if item == index { rest } else { text };
            opens_module(text)
        });
        if !opens {
            kept.push((index + 1, *line));
            index += 1;
            continue;
        }
        // Skip through the line that closes the module's block, or through
        // `mod name;`.
        let mut depth: i64 = 0;
        let mut opened = false;
        let mut end = item;
        while let Some(text) = lines.get(end) {
            let text = if end == index { rest } else { text };
            let code = text.split("//").next().unwrap_or(text);
            for character in code.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            end += 1;
            if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';')) {
                break;
            }
        }
        index = end;
    }
    kept
}

/// The `patterns` `source` names outside of comments and `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
fn references<'p>(
    source: &str,
    patterns: impl Iterator<Item = &'p &'static str> + Clone,
) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (number, line) in outside_test_modules(source) {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let code = line.split("//").next().unwrap_or(line);
        for pattern in patterns.clone() {
            if code.contains(pattern) {
                found.push((number, *pattern));
            }
        }
    }
    found
}

/// The ambient APIs `source` names outside of comments and `#[cfg(test)]`
/// modules, which are host-language test code and run no program.
fn ambient_references(source: &str) -> Vec<(usize, &'static str)> {
    references(source, AMBIENT_PATTERNS.iter())
}

/// The ambient references of `vibra-host` outside its system drivers: each
/// file is given by its path below the crate's `src`, with `/` separators.
fn host_reads_outside_system(files: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, source) in files {
        if path.starts_with(HOST_SYSTEM_DIRECTORY) {
            continue;
        }
        let patterns = AMBIENT_PATTERNS.iter().chain(HOST_SYSTEM_PATTERNS);
        for (line, pattern) in references(source, patterns) {
            found.push(format!("src/{path}:{line} names `{pattern}`"));
        }
    }
    found
}

/// The sources of `vibra-host/src`, by their paths below it.
fn host_sources(root: &Path) -> Vec<(String, String)> {
    let source_root = root.join("crates").join("vibra-host").join("src");
    let mut sources = Vec::new();
    rust_sources(&source_root, &mut sources);
    assert!(!sources.is_empty(), "vibra-host has no sources");
    sources
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&source_root)
                .expect("a source below src")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let source = std::fs::read_to_string(&path).expect("readable source");
            (relative, source)
        })
        .collect()
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
    for provider_crate in PROVIDER_CRATES {
        assert!(crates.contains(*provider_crate));
    }
    for provider_crate in ["vibra-host-v1"] {
        // The audit is not vacuous: the provider is where the reads are.
        let provider = scan(&root, provider_crate);
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
                "`{provider_crate}` no longer reads through `{api}`: {provider:?}"
            );
        }
    }
}

/// The host provider is exempt from the scan above as a crate, and held to a
/// narrower rule here: every ambient read of `vibra-host` is in its system
/// drivers, under `src/system/`.
#[test]
fn the_host_reads_ambient_state_only_in_its_system_drivers() {
    let outside = host_reads_outside_system(&host_sources(&workspace_root()));
    assert!(
        outside.is_empty(),
        "`vibra-host` reads ambient state outside `src/system/`:\n{}",
        outside.join("\n")
    );
}

#[test]
fn atk_ambient_read_outside_system() {
    let file = |path: &str, source: &str| (path.to_owned(), source.to_owned());
    let broker = [file("broker.rs", "let bytes = std::fs::read(path)?;\n")];
    assert_eq!(
        host_reads_outside_system(&broker),
        [
            "src/broker.rs:1 names `std::fs`",
            "src/broker.rs:1 names `fs::`"
        ]
    );
    let clock = [file("world.rs", "use std::time::Duration;\n")];
    assert_eq!(
        host_reads_outside_system(&clock),
        ["src/world.rs:1 names `std::time`"]
    );
    let system = [
        file("system/fs.rs", "let bytes = std::fs::read(path)?;\n"),
        file(
            "system/clock.rs",
            "let now = std::time::SystemTime::now();\n",
        ),
    ];
    assert!(host_reads_outside_system(&system).is_empty());
}

/// The build driver is exempt from the scan above as a crate, and held to a
/// narrower rule here: every ambient read, process, and filesystem call of the
/// crate is in its module `system`.
#[test]
fn the_build_driver_reads_the_machine_in_one_module() {
    let found = scan(&workspace_root(), "vibra-build");
    assert!(
        found.iter().any(|line| line.contains("system.rs:")),
        "the audit is not vacuous: the module `system` reads the machine"
    );
    let outside = found
        .iter()
        .filter(|line| !line.contains("system.rs:"))
        .collect::<Vec<_>>();
    assert!(
        outside.is_empty(),
        "ambient reads outside the module `system`:\n{outside:?}"
    );
}

#[test]
fn the_emitter_the_checker_and_the_interpreter_are_scanned() {
    let scanned_crates = scanned_crates(&workspace_root());
    for scanned in [
        "vibra-syntax",
        "vibra-fmt",
        "vibra-resolve",
        "vibra-types",
        "vibra-ir",
        "vibra-interp",
        "vibra-native",
        "vibra-schema",
        "vibra-syntax-v1",
        "vibra-ir-v1",
        "vibra-native-v1",
    ] {
        assert!(
            scanned_crates.contains(scanned),
            "`{scanned}` must be scanned"
        );
    }
    for exempt in TOOLING_EXCEPTIONS
        .iter()
        .map(|(name, _)| *name)
        .chain(PROVIDER_CRATES.iter().copied())
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

/// F3 of the review of grants: only a `#[cfg(test)]` module is skipped. A
/// read after a `#[cfg(test)]` field, function, or other item is found, and
/// so is a read after a `#[cfg(test)]` module that is not the last item.
#[test]
fn atk_read_after_a_test_only_item() {
    let field = "struct Slot {\n    #[cfg(test)]\n    delivered: u8,\n}\n\
                 fn read() { let _ = std::fs::read(\"x\"); }\n";
    assert_eq!(ambient_references(field), [(5, "std::fs"), (5, "fs::")]);
    let item =
        "#[cfg(test)]\nfn hook() {}\n\nfn now() { let _ = std::env::var(\"X\"); }\n";
    assert_eq!(ambient_references(item), [(4, "std::env"), (4, "env::var")]);
    let module = "#[cfg(test)]\nmod tests {\n    fn f() { std::env::args(); }\n}\n\n\
                  fn after() { let _ = std::env::var(\"X\"); }\n";
    assert_eq!(
        ambient_references(module),
        [(6, "std::env"), (6, "env::var")]
    );
    let inline =
        "#[cfg(test)] mod tests { use std::fs; }\nfn after() { getrandom(); }\n";
    assert_eq!(ambient_references(inline), [(2, "getrandom")]);
    let out_of_line = "#[cfg(test)]\nmod tests;\nfn after() { getrandom(); }\n";
    assert_eq!(ambient_references(out_of_line), [(3, "getrandom")]);
    let files = [(
        "broker.rs".to_owned(),
        "struct Slot {\n    #[cfg(test)]\n    seen: u8,\n}\nuse std::time::Duration;\n"
            .to_owned(),
    )];
    assert_eq!(
        host_reads_outside_system(&files),
        ["src/broker.rs:5 names `std::time`"]
    );
}

/// The runner judges every case through an in-process handler and starts no
/// process.
#[test]
fn the_runner_launches_no_process() {
    let mut users = Vec::new();
    let mut sources = Vec::new();
    rust_sources(
        &workspace_root()
            .join("crates")
            .join("vibra-conformance")
            .join("src"),
        &mut sources,
    );
    assert!(!sources.is_empty(), "vibra-conformance has no sources");
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("readable source");
        if source.contains("Command::new") {
            users.push(path.display().to_string());
        }
    }
    assert!(users.is_empty(), "the runner starts a process in {users:?}");
}
