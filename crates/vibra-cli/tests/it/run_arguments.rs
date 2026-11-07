//! M4 Step 18: `vibra run [TARGET] [-- ARG...]` through the real binary. The
//! arguments after the literal `--` are exactly what `env.read.args` returns
//! under the system provider, verbatim and in order; `--` is accepted only by
//! `run`, after the optional `TARGET`, and only as the first `--` after it.

use std::process::Output;

use crate::support::{TempDir, app_target, manifest, run_workspace, text};

/// Writes each argument on its own line, so the output shows the list.
const MAIN: &str = r#"(import env @std.env)
(import io @std.io)
(import text @std.text)

(defn line (acc str item str) str
  (str.concat acc (str.concat item "\n")))

(defn main () (result void io.io-error)
  effects: (env.read io.stdout)
  (match (env.read.args)
    (result.ok found) (io.stdout.text (array.fold found "" line))
    (result.err -) (io.stdout.text "not-unicode")))
"#;

/// A fresh project under the system temporary directory, removed when dropped.
struct Scratch(TempDir);

impl Scratch {
    fn new() -> Self {
        let manifest = manifest(
            "args-demo",
            &app_target("main", "@std.env.read @std.io.stdout"),
            "(dict)",
        );
        Self(TempDir::project(
            "args18",
            &manifest,
            &[("src/app/main.vib", MAIN)],
        ))
    }

    fn path(&self) -> &std::path::Path {
        self.0.path()
    }

    fn vibra(&self, arguments: &[&str]) -> Output {
        run_workspace(self.path(), arguments, None)
    }
}

fn stdout(output: &Output) -> String {
    text(&output.stdout)
}

#[test]
fn arguments_after_the_separator_are_what_env_read_args_returns() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["run", "@app", "--", "a", "b", "c"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(stdout(&run), "a\nb\nc\n");
}

#[test]
fn an_omitted_target_runs_the_only_binary_target() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["run", "--", "a", "@app"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(stdout(&run), "a\n@app\n");
    let bare = scratch.vibra(&["run"]);
    assert_eq!(bare.status.code(), Some(0), "{bare:?}");
    assert_eq!(stdout(&bare), "");
}

#[test]
fn an_omitted_target_needs_exactly_one_binary_target() {
    let two = manifest(
        "args-demo",
        &format!(
            "{} {}",
            app_target("main", "@std.env.read @std.io.stdout"),
            "(record name: @tool kind: @bin root: \"src/tool\" entry: @tool.main.main effects: (array @std.env.read @std.io.stdout))"
        ),
        "(dict)",
    );
    let project = TempDir::project(
        "args18-two",
        &two,
        &[("src/app/main.vib", MAIN), ("src/tool/main.vib", MAIN)],
    );
    let output = run_workspace(project.path(), &["--format", "json", "run"], None);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let named = run_workspace(project.path(), &["run", "@tool", "--", "z"], None);
    assert_eq!(named.status.code(), Some(0), "{named:?}");
    assert_eq!(stdout(&named), "z\n");
}

#[test]
fn a_separator_with_no_argument_gives_none_and_so_does_its_absence() {
    let scratch = Scratch::new();
    for arguments in [&["run", "@app", "--"][..], &["run", "@app"][..]] {
        let run = scratch.vibra(arguments);
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(stdout(&run), "");
    }
}

#[test]
fn everything_after_the_separator_is_an_argument_even_an_option_or_another_separator() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&[
        "run", "@app", "--", "--flag", "--", "-h", "--format", "json", "",
    ]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(stdout(&run), "--flag\n--\n-h\n--format\njson\n\n");
}

#[test]
fn json_run_reports_the_read_in_the_trace() {
    let scratch = Scratch::new();
    let run = scratch.vibra(&["--format", "json", "run", "@app", "--", "x y"]);
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
        arguments.push(scratch.path().to_path_buf().into_os_string());
        arguments.extend(["run", "@app", "--"].map(std::ffi::OsString::from));
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
fn the_separator_is_invalid_before_the_command_and_on_other_commands() {
    let scratch = Scratch::new();
    for arguments in [
        &["--", "run", "@app"][..],
        &["test", "--", "x"][..],
        &["test", "--"][..],
        &["check", "--", "a"][..],
        &["check", "@app", "--"][..],
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
    let output = scratch.vibra(&["--format", "json", "run", "@app", "@app", "--", "a"]);
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
        stdout(&output).contains("run [TARGET] [-- ARG...]"),
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
    let output = crate::support::vibra()
        .args(["--workspace", scratch.path().to_str().expect("utf-8")])
        .args(["run", "@app", "--"])
        .arg(bad)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("vibra binary");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout(&output), "not-unicode");
}
