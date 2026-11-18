//! `help`: the usage text, the support inventory, and its spellings.

use serde_json::json;

use crate::support::{envelope, run};

#[test]
fn help_json() {
    let (code, document) = envelope(&["--format", "json", "help"]);
    assert_eq!(code, 0);
    assert_eq!(document["schemaVersion"], 2);
    assert_eq!(document["command"], "help");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["result"], "@command.ok");
    assert_eq!(document["workspaceRevision"], json!(null));
    assert_eq!(document["diagnostics"], json!([]));
    assert_eq!(
        document["limits"],
        json!({"profile": "m1Default", "reached": [], "complete": true})
    );
    let capabilities = document["payload"]["capabilities"]
        .as_array()
        .expect("capabilities");
    assert_eq!(capabilities.len(), 18);
    let supported = capabilities
        .iter()
        .filter(|capability| capability["supported"] == true)
        .map(|capability| capability["command"].as_str().expect("command"))
        .collect::<Vec<_>>();
    assert_eq!(supported, ["help"]);
    assert!(
        document["payload"]["usage"]
            .as_str()
            .expect("usage")
            .contains("vibra [GLOBAL...] fmt PATH [--write]")
    );
}

#[test]
fn help_names_what_it_supports_and_nothing_else() {
    let (_, document) = envelope(&["--format", "json", "help"]);
    for capability in document["payload"]["capabilities"]
        .as_array()
        .expect("array")
    {
        if capability["supported"] == true {
            assert_eq!(capability["command"], "help");
            assert_eq!(
                capability["schemaIds"],
                json!(["urn:vibra:schema:v2:command-result"])
            );
        } else {
            assert_eq!(capability["operations"], json!([]));
            assert_eq!(capability["profiles"], json!([]));
            assert_eq!(capability["schemaIds"], json!([]));
        }
    }
}

#[test]
fn help_spellings() {
    for format in [&[][..], &["--format", "json"][..]] {
        let outputs = ["help", "--help", "-h"]
            .into_iter()
            .map(|spelling| {
                let mut arguments = format.to_vec();
                arguments.push(spelling);
                let outcome = run(&arguments);
                (outcome.code, outcome.stdout, outcome.stderr)
            })
            .collect::<Vec<_>>();
        assert_eq!(outputs[0].0, 0);
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(outputs[1], outputs[2]);
        assert!(!outputs[0].1.is_empty());
    }
}

#[test]
fn help_takes_no_arguments() {
    for arguments in [
        &["--format", "json", "help", "fmt"][..],
        &["--format", "json", "--help", "fmt"][..],
        &["--format", "json", "-h", "--write"][..],
    ] {
        let (code, document) = envelope(arguments);
        assert_eq!(code, 2, "{arguments:?}");
        assert_eq!(document["result"], "@command.invalidInput");
        assert_eq!(document["command"], "help");
        assert_eq!(document["payload"], json!({}));
        assert_eq!(document["diagnostics"], json!([]));
        assert_eq!(document["workspaceRevision"], json!(null));
    }
}

#[test]
fn help_marks_unavailable() {
    let outcome = run(&["help"]);
    assert_eq!(outcome.code, 0);
    let commands = outcome
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  vibra [GLOBAL...] "))
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), 18);
    for line in &commands {
        if *line == "help" {
            assert!(!line.contains("unavailable"), "{line}");
        } else {
            assert!(line.ends_with(" (unavailable)"), "{line}");
        }
    }
    assert_eq!(
        commands
            .iter()
            .filter(|line| !line.contains("unavailable"))
            .count(),
        1,
        "exactly one command is shown as working"
    );
    assert!(outcome.stderr.is_empty());
}
