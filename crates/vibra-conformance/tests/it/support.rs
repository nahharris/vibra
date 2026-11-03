//! Helpers shared by the test modules.

use std::process::{Command, Output};

/// Runs the `#[ignore]`d test `name` of the module `module_path` in a child
/// process of this test binary, with its output captured.
///
/// `module_path` is the caller's `module_path!()`. A test is selected by its
/// path from the crate root, so the crate's own name is dropped from it. That is
/// what `cargo test` selects by, and nextest runs the same binary through the
/// same harness, so one name serves both.
pub(crate) fn run_ignored_child(module_path: &str, name: &str) -> Output {
    let in_crate = module_path.split_once("::").map_or("", |(_, path)| path);
    Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--ignored",
            "--exact",
            &format!("{in_crate}::{name}"),
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("the child process runs")
}
