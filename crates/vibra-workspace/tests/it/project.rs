//! The `@project.v2` decoder (`docs/spec/04-programs-and-packages.md`,
//! "Project file").

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_workspace::{Availability, DependencyKind, TargetKind, decode_project};

use crate::support::LIBRARY;

/// The chapter's example project.
pub(crate) const BINARY: &str = r#"{
  format: @project.v2
  package: {name: "hello" version: "0.1.0"}
  targets: [
    {
      name: @hello
      kind: @bin
      root: "src/hello"
      entry: @hello.main.main
      effects: [@std.env.EnvReadArgs @std.fs.DirReadText @std.io.StdoutText]
      grants: [
        @args: {
          provider: @host.env.args
          resource: {kind: @processArguments}
          after: []
        }
        @notes: {
          provider: @host.fs.directory
          resource: {kind: @argumentPath index: 0u64 source: @args}
          policy: {
            format: @scopePolicy.v1
            family: @filesystem
            expression: {
              kind: @fileRule
              root: @notes
              prefix: []
              depth: @tree
              actions: [@read]
              names: {kind: @suffix value: ".txt"}
            }
          }
          after: [@args]
        }
        @out: {
          provider: @host.io.stdout
          resource: {kind: @processStream}
          after: []
        }
      ]
    }
  ]
  dependencies: [:]
}
"#;

fn faults(text: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    decode_project("project.vibon", text.as_bytes())
        .expect_err("the document is rejected")
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

pub(crate) fn at(text: &str, needle: &str) -> ByteSpan {
    let start = text.find(needle).expect("the needle occurs");
    ByteSpan::new(start, start + needle.len())
}

#[test]
fn a_library_project_decodes_to_one_available_library_target() {
    let project =
        decode_project("project.vibon", LIBRARY.as_bytes()).expect("accepted");
    assert_eq!(project.package().name(), "hello");
    assert_eq!(project.package().version(), "0.1.0");
    let [target] = project.targets() else {
        panic!("one target");
    };
    assert_eq!(target.name(), "@app");
    assert_eq!(target.kind(), TargetKind::Lib);
    assert_eq!(target.root(), "src/app");
    assert_eq!(target.root_span(), at(LIBRARY, "\"src/app\""));
    assert_eq!(target.entry(), None);
    assert!(project.dependencies().is_empty());
}

#[test]
fn the_chapters_binary_project_keeps_its_entry_effects_and_grants() {
    let project = decode_project("project.vibon", BINARY.as_bytes()).expect("accepted");
    let [target] = project.targets() else {
        panic!("one target");
    };
    assert_eq!(target.kind(), TargetKind::Bin);
    assert_eq!(
        target.entry(),
        Some(("@hello.main.main", at(BINARY, "@hello.main.main")))
    );
    let effects = "[@std.env.EnvReadArgs @std.fs.DirReadText @std.io.StdoutText]";
    assert_eq!(target.effects_span(), Some(at(BINARY, effects)));
    assert_eq!(
        target
            .effects()
            .iter()
            .map(|(path, span)| (path.as_str(), *span))
            .collect::<Vec<_>>(),
        [
            ("@std.env.EnvReadArgs", at(BINARY, "@std.env.EnvReadArgs")),
            ("@std.fs.DirReadText", at(BINARY, "@std.fs.DirReadText")),
            ("@std.io.StdoutText", at(BINARY, "@std.io.StdoutText")),
        ]
    );
    let grants = target.grants_span().expect("the grants");
    assert_eq!(BINARY.get(grants.start()..=grants.start()), Some("["));
    assert_eq!(BINARY.get(grants.end() - 1..grants.end()), Some("]"));
    assert_eq!(
        target
            .grant_keys()
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["@args", "@notes", "@out"]
    );
}

#[test]
fn canonical_bytes_suffix_an_unsuffixed_bound_and_keep_one_line() {
    let text = BINARY.replace(
        "          after: [@args]\n",
        "          limits: {\n            // one call\n            invocations: 5\n          }\n          after: [@args]\n",
    );
    let project = decode_project("project.vibon", text.as_bytes()).expect("accepted");
    let bytes = project.document().canonical_bytes();
    assert!(bytes.contains("limits: {invocations: 5u64}"), "{bytes}");
    assert!(!bytes.contains('\n') && !bytes.contains("//"));
    assert!(bytes.starts_with("{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @hello kind: @bin root: \"src/hello\" entry: @hello.main.main"));
    assert!(bytes.ends_with("] dependencies: [:]}"));
    // The policy is opaque: its fields are in name order.
    assert!(bytes.contains("policy: {expression: {actions: [@read] depth: @tree kind: @fileRule names: {kind: @suffix value: \".txt\"} prefix: [] root: @notes} family: @filesystem format: @scopePolicy.v1}"), "{bytes}");
}

#[test]
fn a_limits_record_holds_at_least_one_bound() {
    let text = BINARY.replace(
        "          after: [@args]\n",
        "          limits: {invocations: 5u64}\n          after: [@args]\n",
    );
    decode_project("project.vibon", text.as_bytes()).expect("one bound is accepted");
    let text = BINARY.replace(
        "          after: [@args]\n",
        "          limits: {}\n          after: [@args]\n",
    );
    let found = faults(&text);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn every_other_format_is_rejected_at_the_format_value_only() {
    let text = LIBRARY
        .replace("@project.v2", "@project.v1")
        .replace("[:]}", "[:] author: 1}");
    assert_eq!(
        faults(&text),
        [(DiagnosticCode::DataInvalidValue, at(&text, "@project.v1"))]
    );
}

#[test]
fn the_value_constraints_are_value_faults_at_the_value() {
    let cases = [
        ("\"hello\"", "\"My-App\""),
        ("\"0.1.0\"", "\"^1.0\""),
        ("\"0.1.0\"", "1"),
        ("@app kind", "@app.main kind"),
    ];
    for (from, to) in cases {
        let text = LIBRARY.replacen(from, to, 1);
        let needle = to.trim_end_matches(" kind");
        assert_eq!(
            faults(&text),
            [(DiagnosticCode::DataInvalidValue, at(&text, needle))],
            "{to}"
        );
    }
}

#[test]
fn a_library_that_writes_a_binary_field_is_a_shape_fault_at_the_field() {
    let text = LIBRARY.replace("root: \"src/app\"", "root: \"src/app\" entry: @x.main");
    assert_eq!(
        faults(&text),
        [(DiagnosticCode::DataInvalidShape, at(&text, "entry"))]
    );
}

#[test]
fn dependencies_are_decoded_and_constrained() {
    let rev = "0123456789abcdef0123456789abcdef01234567";
    let text = LIBRARY.replace(
        "dependencies: [:]",
        &format!(
            "dependencies: [\"money\": {{kind: @git git: \"https://example.com/money.git\" rev: \"{rev}\"}} \"local\": {{kind: @path path: \"lib/local\"}}]"
        ),
    );
    let project = decode_project("project.vibon", text.as_bytes()).expect("accepted");
    let [local, money] = project.dependencies() else {
        panic!("two dependencies");
    };
    assert_eq!(local.name(), "local");
    assert_eq!(local.kind(), DependencyKind::Path);
    assert_eq!(local.source(), "lib/local");
    assert_eq!(local.availability(), Availability::Available);
    assert_eq!(money.kind(), DependencyKind::Git);
    assert_eq!(money.rev(), Some(rev));
    assert_eq!(money.availability(), Availability::Unavailable);
    let start = text.find("\"money\"").expect("key");
    let end = text.find(&format!("{rev}\"}}")).expect("value") + rev.len() + 2;
    assert_eq!(money.span(), ByteSpan::new(start, end));

    let bad = text.replace(rev, "abc");
    assert_eq!(
        faults(&bad),
        [(DiagnosticCode::DataInvalidValue, at(&bad, "\"abc\""))]
    );
    let bad = text.replace("https://example.com", "https://user@example.com");
    assert_eq!(
        faults(&bad).first().map(|fault| fault.0),
        Some(DiagnosticCode::DataInvalidValue)
    );
    let bad = text.replace("path: \"lib/local\"", "path: \"lib/local\" target: @local");
    let field = bad.find("target: @local").expect("the field");
    assert_eq!(
        faults(&bad),
        [(
            DiagnosticCode::DataInvalidShape,
            ByteSpan::new(field, field + 6)
        )]
    );
}

#[test]
fn a_repeated_grant_is_a_duplicate_key() {
    let text = BINARY.replace(
        "        @out: {",
        "        @args: {\n          provider: @host.env.args\n          resource: {kind: @processArguments}\n          after: []\n        }\n        @out: {",
    );
    let second = text
        .match_indices("@args:")
        .nth(1)
        .expect("the second key")
        .0;
    assert_eq!(
        faults(&text),
        [(
            DiagnosticCode::DataDuplicateKey,
            ByteSpan::new(second, second + "@args".len())
        )]
    );
}

#[test]
fn the_validation_order_reports_only_the_repeated_field() {
    let text = LIBRARY.replace(
        "dependencies: [:]",
        "package: {name: \"x\" version: \"1.0.0\"} author: \"x\" dependencies: [:]",
    );
    let second = text.match_indices("package").nth(1).expect("second").0;
    assert_eq!(
        faults(&text),
        [(
            DiagnosticCode::DataDuplicateField,
            ByteSpan::new(second, second + "package".len())
        )]
    );
}
