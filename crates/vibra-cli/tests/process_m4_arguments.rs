//! M4 Step 18: `vibra run TARGET [-- ARG...]` through the real binary. The
//! arguments after the literal `--` are exactly what `env.read.args` returns
//! under the system provider, verbatim and in order; `--` is accepted only by
//! `run`, only after `TARGET`, and only as the first `--` after it.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const PROJECT: &str = r#"(record format: @project.v1 package: (record name: "args-demo" version: "0.1.0") targets: (array (record name: @app kind: @bin root: "src/app" entry: @app.main.main effects: (array @std.env.read @std.io.stdout))) dependencies: (dict))
"#;

/// Writes each argument on its own line, so the output shows the list.
const MAIN: &str = r#"(import env @std.env)
(import io @std.io)
(import text @std.text)

(defn line (acc str item str) str
  (text.concat acc (text.concat item "\n")))

(defn main () (result void io.io-error)
  effects: (env.read io.stdout)
  (match (env.read.args)
    (result.ok found) (io.stdout.text (array.fold found "" line))
    (result.err -) (io.stdout.text "not-unicode")))
"#;

/// A fresh project under the system temporary directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "vibra-args18-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("after the epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(path.join("src/app")).expect("project");
        std::fs::write(path.join("project.vibon"), PROJECT).expect("manifest");
        std::fs::write(path.join("src/app/main.vib"), MAIN).expect("source");
        Self(path)
    }

    fn vibra(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vibra"))
            .args(["--workspace", self.0.to_str().expect("utf-8")])
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("vibra binary")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8")
}

#[test]
fn arguments_after_the_separator_are_what_env_read_args_returns() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["run", "src/app", "--", "a", "b", "c"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(stdout(&run), "a\nb\nc\n");
}

#[test]
fn a_separator_with_no_argument_gives_none_and_so_does_its_absence() {
    let scratch = Scratch::new();
    for arguments in [&["run", "src/app", "--"][..], &["run", "src/app"][..]] {
        let run = scratch.vibra(arguments);
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(stdout(&run), "");
    }
}

#[test]
fn everything_after_the_separator_is_an_argument_even_an_option_or_another_separator() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&[
        "run", "src/app", "--", "--flag", "--", "-h", "--format", "json", "",
    ]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(stdout(&run), "--flag\n--\n-h\n--format\njson\n\n");
}

#[test]
fn json_run_reports_the_read_in_the_trace() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["--format", "json", "run", "src/app", "--", "x y"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let envelope: serde_json::Value =
        serde_json::from_slice(&run.stdout).expect("json");
    assert_eq!(envelope["payload"]["stdout"], "x y\n");
    assert_eq!(envelope["payload"]["auditTrace"][0]["op"], "env.read-args");
    assert_eq!(
        envelope["payload"]["auditTrace"][0]["result"],
        "(record kind: @enum type: @std.result.result variant: @ok payload: (record kind: @array values: (array \"x y\")))"
    );
}

#[test]
fn ten_thousand_arguments_and_a_one_mebibyte_argument_arrive_verbatim() {
    // Too long for a Windows command line, so the library entry point stands in
    // for the process: it parses the same tokens and runs the same code.
    let scratch = Scratch::new();
    let in_process = |tail: Vec<String>| -> String {
        let mut arguments = ["--format", "json", "--workspace"]
            .map(std::ffi::OsString::from)
            .to_vec();
        arguments.push(scratch.0.clone().into_os_string());
        arguments.extend(["run", "src/app", "--"].map(std::ffi::OsString::from));
        arguments.extend(tail.into_iter().map(std::ffi::OsString::from));
        let mut out = Vec::new();
        let code = vibra_cli::run(arguments, &mut out, std::io::sink());
        assert_eq!(code, 0);
        let envelope: serde_json::Value = serde_json::from_slice(&out).expect("json");
        envelope["payload"]["stdout"]
            .as_str()
            .expect("stdout")
            .to_owned()
    };
    let many = (0..10_000).map(|n| n.to_string()).collect::<Vec<_>>();
    let expected = many.iter().fold(String::new(), |acc, n| acc + n + "\n");
    assert_eq!(in_process(many), expected);
    let big = "x".repeat(1024 * 1024);
    assert_eq!(in_process(vec![big.clone()]), format!("{big}\n"));
}

#[test]
fn the_separator_is_invalid_before_the_target_and_on_other_commands() {
    let scratch = Scratch::new();
    for arguments in [
        &["run", "--", "src/app"][..],
        &["--", "run", "src/app"][..],
        &["test", "--", "x"][..],
        &["test", "--"][..],
        &["check", "--", "a"][..],
        &["check", "src/app", "--"][..],
        &["fmt", "--"][..],
        &["fmt", "src/app/main.vib", "--", "a"][..],
        &["help", "--"][..],
        &["project", "init", "--", "a"][..],
    ] {
        let output = scratch.vibra(&[&["--format", "json"][..], arguments].concat());
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {output:?}");
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("json");
        assert_eq!(
            envelope["result"], "@command.invalid-input",
            "{arguments:?}"
        );
    }
}

#[test]
fn an_error_in_the_command_line_runs_nothing() {
    let scratch = Scratch::new();
    let output =
        scratch.vibra(&["--format", "json", "run", "--", "src/app", "--", "a"]);
    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(envelope["result"], "@command.invalid-input");
    assert_eq!(envelope["payload"]["auditTrace"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["stdout"], "");
}

#[test]
fn the_help_text_is_the_closed_grammar_with_the_separator() {
    let scratch = Scratch::new();
    let output = scratch.vibra(&["help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout(&output).contains("run TARGET [-- ARG...]"),
        "{}",
        stdout(&output)
    );
}

#[cfg(unix)]
#[test]
fn a_non_unicode_argument_is_not_unicode_at_the_operation_not_a_command_error() {
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new();
    let bad = std::ffi::OsStr::from_bytes(&[b'a', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--workspace", scratch.0.to_str().expect("utf-8")])
        .args(["run", "src/app", "--"])
        .arg(bad)
        .stdin(Stdio::null())
        .output()
        .expect("vibra binary");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout(&output), "not-unicode");
}
