//! M4 Step 19: the command result and exit code of an entry, through the real
//! binary in both output modes. An entry that returns `err` ends `run` with
//! `@command.program-failed` and exit code 6: `err` is a value the program
//! returned, so the run has no trap and no diagnostic, `programResult` is the
//! canonical encoding of the result, and human mode prints that encoding to
//! standard error. An entry that returns `void` or `ok` is `@command.ok`, a
//! trap stays `@command.trap` (5), and memory exhaustion stays the host event
//! `@command.operational-failure` (3), whatever the entry held.

use std::process::Output;

use serde_json::Value;

use crate::support::{
    TempDir, app_target, assert_envelope_schema, manifest, run_workspace, text,
};

/// A fresh project under the system temporary directory, removed when dropped.
struct Scratch(TempDir);

impl Scratch {
    fn new(label: &str, effects: &str, main: &str) -> Self {
        let manifest = manifest("outcome-demo", &app_target("main", effects), "(dict)");
        Self(TempDir::project(
            label,
            &manifest,
            &[("src/app/main.vib", main)],
        ))
    }

    /// Runs the binary with `input` on its standard input.
    fn vibra(&self, arguments: &[&str], input: &[u8]) -> Output {
        run_workspace(self.0.path(), arguments, Some(input))
    }

    /// The JSON envelope of `run`, validated against the command-result schema.
    fn json(&self, input: &[u8], code: i32, result: &str) -> Value {
        let output = self.vibra(&["--format", "json", "run", "src/app"], input);
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let envelope: Value = serde_json::from_slice(&output.stdout).expect("json");
        assert_envelope_schema(&envelope);
        assert_eq!(envelope["result"], result, "{envelope}");
        envelope
    }

    /// Human `run`.
    fn human(&self, input: &[u8], code: i32) -> Output {
        let output = self.vibra(&["run", "src/app"], input);
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        output
    }
}

const STDOUT: &str = "@std.io.stdout";
const STDIN: &str = "@std.io.stdin";

/// Writes a line, then returns an error of its own.
const OWN_ERR: &str = r#"(import io @std.io)

(deftype failure (record code i32))

(defn main () (result void failure)
  effects: (io.stdout)
  (let - (io.stdout.text "before\n"))
  (result.err (failure code: 7i32)))
"#;

#[test]
fn an_entry_returning_its_own_err_is_program_failed_in_json() {
    let scratch = Scratch::new("own-err-json", STDOUT, OWN_ERR);
    let envelope = scratch.json(b"", 6, "@command.program-failed");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    let result = envelope["payload"]["programResult"]
        .as_str()
        .expect("result");
    assert!(result.contains("variant: @err"), "{result}");
    assert!(result.contains("code: 7i32"), "{result}");
    // The program's own output stays in the envelope.
    assert_eq!(envelope["payload"]["stdout"], "before\n");
}

#[test]
fn an_entry_returning_its_own_err_prints_the_encoding_to_stderr_in_human_mode() {
    let scratch = Scratch::new("own-err-human", STDOUT, OWN_ERR);
    let json = scratch.json(b"", 6, "@command.program-failed");
    let encoding = json["payload"]["programResult"].as_str().expect("result");
    let human = scratch.human(b"", 6);
    // Standard output keeps the program's own bytes and nothing else.
    assert_eq!(text(&human.stdout), "before\n");
    // The encoding is a line of its own: it ends with its newline.
    assert_eq!(text(&human.stderr), encoding);
}

/// Reads standard input as text and returns the error of a failed read.
const PROPAGATES: &str = r#"(import io @std.io)

(defn main () (result void io.io-error)
  effects: (io.stdin io.stdout)
  (let text (try (io.stdin.text)))
  (io.stdout.text text))
"#;

#[test]
fn a_host_error_propagated_with_try_ends_the_run_program_failed() {
    let scratch = Scratch::new("propagates", &format!("{STDIN} {STDOUT}"), PROPAGATES);
    // Bytes that are not UTF-8 are the typed error of the read.
    let envelope = scratch.json(&[0xff, 0xfe], 6, "@command.program-failed");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    let result = envelope["payload"]["programResult"]
        .as_str()
        .expect("result");
    assert!(result.contains("variant: @err"), "{result}");
    assert!(result.contains("variant: @invalid-text"), "{result}");
    let human = scratch.human(&[0xff, 0xfe], 6);
    assert_eq!(text(&human.stderr), result);
    assert!(human.stdout.is_empty(), "{human:?}");

    // The same program on valid input is a success.
    let ok = scratch.json(b"fine", 0, "@command.ok");
    assert_eq!(ok["payload"]["stdout"], "fine");
    let human = scratch.human(b"fine", 0);
    assert_eq!(text(&human.stdout), "fine");
    assert!(human.stderr.is_empty(), "{human:?}");
}

/// Handles the typed error and then returns `ok`.
const HANDLED: &str = r#"(import io @std.io)

(defn main () (result void io.io-error)
  effects: (io.stdin io.stdout)
  (match (io.stdin.text)
    (result.ok text) (io.stdout.text text)
    (result.err -) (io.stdout.text "recovered")))
"#;

#[test]
fn a_host_error_the_program_handles_is_not_a_failure() {
    let scratch = Scratch::new("handled", &format!("{STDIN} {STDOUT}"), HANDLED);
    let envelope = scratch.json(&[0xff], 0, "@command.ok");
    assert_eq!(envelope["payload"]["stdout"], "recovered");
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    let result = envelope["payload"]["programResult"]
        .as_str()
        .expect("result");
    assert!(result.contains("variant: @ok"), "{result}");
    let human = scratch.human(&[0xff], 0);
    assert_eq!(text(&human.stdout), "recovered");
    assert!(human.stderr.is_empty(), "{human:?}");
}

#[test]
fn a_void_entry_stays_ok() {
    let scratch = Scratch::new("void", "", "(defn main () void (do))\n");
    let envelope = scratch.json(b"", 0, "@command.ok");
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    let human = scratch.human(b"", 0);
    assert!(
        human.stdout.is_empty() && human.stderr.is_empty(),
        "{human:?}"
    );
}

/// A function behind `any` in the err the entry holds has no encoding.
const TRAP_HOLDING_ERR: &str = r#"(defn inc (x i32) i32 x)

(defn main () (result void any) (result.err inc))
"#;

#[test]
fn a_trap_wins_over_the_err_the_entry_held() {
    let scratch = Scratch::new("trap", "", TRAP_HOLDING_ERR);
    let envelope = scratch.json(b"", 5, "@command.trap");
    assert_eq!(envelope["payload"]["programResult"], Value::Null);
    assert_eq!(
        envelope["payload"]["trap"]["trapCode"],
        "@runtime.unobservable-function"
    );
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@runtime.unobservable-function"
    );
    let human = scratch.human(b"", 5);
    assert!(human.stdout.is_empty(), "{human:?}");
    assert!(
        text(&human.stderr).contains("@runtime.unobservable-function"),
        "{human:?}"
    );
}

/// Holds an `err` and then recurses with no base case.
const EXHAUSTION_HOLDING_ERR: &str = r#"(deftype failure (record code i32))

(defn forever () i32 (let v (forever)) v)

(defn make () (result void failure) (result.err (failure code: 1i32)))

(defn main () (result void failure)
  (let held (make))
  (let - (forever))
  held)
"#;

#[test]
fn memory_exhaustion_stays_the_host_event_whatever_the_entry_held() {
    let scratch = Scratch::new("memory", "", EXHAUSTION_HOLDING_ERR);
    let envelope = scratch.json(b"", 3, "@command.operational-failure");
    assert_eq!(envelope["payload"]["programResult"], Value::Null);
    assert_eq!(envelope["payload"]["trap"], Value::Null);
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@runtime.memory-exhausted"
    );
    let human = scratch.human(b"", 3);
    assert!(human.stdout.is_empty(), "{human:?}");
    assert!(
        text(&human.stderr).contains("@runtime.memory-exhausted"),
        "{human:?}"
    );
}

/// An error nested five thousand deep.
const DEEP_ERR: &str = r#"(deftype nat (record prev (array nat)))

(deftype failure (record n nat))

(defn lower (count i32) i32
  (match (i32.sub-checked count 1i32)
    (result.ok value) value
    (result.err -) 0i32))

(defn build (count i32 inner nat) nat
  (if (i32.equal count 0i32)
    inner
    (build (lower count) (nat prev: (array.of inner)))))

(defn main () (result void failure)
  (result.err (failure n: (build 5000i32 (nat prev: (array.of))))))
"#;

#[test]
fn an_error_nested_five_thousand_deep_is_reported_whole() {
    let scratch = Scratch::new("deep", "", DEEP_ERR);
    let envelope = scratch.json(b"", 6, "@command.program-failed");
    let result = envelope["payload"]["programResult"]
        .as_str()
        .expect("result");
    assert!(result.contains("variant: @err"), "{result}");
    assert_eq!(result.matches("kind: @array").count(), 5001);
    let human = scratch.human(b"", 6);
    assert_eq!(text(&human.stderr), result);
}

/// An error whose payload is a union, with a text of one mebibyte.
const BIG_UNION_ERR: &str = r#"(import text @std.text)

(deftype detail (union i32 str))

(deftype failure (record detail detail))

(defn lower (count i32) i32
  (match (i32.sub-checked count 1i32)
    (result.ok value) value
    (result.err -) 0i32))

(defn grow (value str count i32) str
  (if (i32.equal count 0i32)
    value
    (grow (text.concat value value) (lower count))))

(defn main () (result void failure)
  (result.err (failure detail: (detail (grow "a" 20i32)))))
"#;

#[test]
fn a_union_payload_and_a_result_of_a_million_bytes_are_reported_whole() {
    let scratch = Scratch::new("big", "", BIG_UNION_ERR);
    let envelope = scratch.json(b"", 6, "@command.program-failed");
    let result = envelope["payload"]["programResult"]
        .as_str()
        .expect("result");
    assert!(result.contains("kind: @union"), "{result:.300}");
    assert!(result.len() > 1_000_000, "{}", result.len());
    let human = scratch.human(b"", 6);
    assert_eq!(human.stderr.len(), result.len());
    assert!(human.stdout.is_empty(), "{:?}", human.stdout.len());
}
