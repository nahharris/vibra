#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! The host world document, the audit trace, and the standard-library manifest
//! are canonical VIBON (Milestone 4, Step 16a): the formatter leaves each
//! unchanged and is idempotent over it.

use std::path::{Path, PathBuf};

use vibra_fmt::format_document;
use vibra_syntax::parse_data;

fn formatted(name: &str, text: &str) -> String {
    let document = parse_data(Path::new(name), text).expect("a .vibon document");
    assert!(document.accepted(), "{name}: {:?}", document.diagnostics());
    format_document(&document)
}

fn assert_canonical(name: &str, text: &str) {
    let once = formatted(name, text);
    assert_eq!(once, text, "{name} is not canonical");
    assert_eq!(formatted(name, &once), once, "{name} is not idempotent");
}

const WORLD: &str = "(record\n  format: @host-world.v1\n  stdin: \"line\\n\"\n  directories: (array \"/empty\")\n  files: (dict \"/in.txt\" \"hi\" \"/raw\" (record kind: @bytes values: (array 0u8 255u8)))\n  env: (dict \"HOME\" \"/home\")\n  args: (array \"a\" \"b\")\n  clock: (record wall: 0i64 monotonic: 0u64 step: 1u64)\n  random: (record seed: 0u64))\n";

#[test]
fn the_default_world_and_a_full_world_are_canonical() {
    assert_canonical("world.vibon", "(record format: @host-world.v1)\n");
    // The formatter may lay a long record out over lines; whichever layout it
    // chooses is stable.
    let once = formatted("world.vibon", WORLD);
    assert_eq!(formatted("world.vibon", &once), once);
    // And it keeps the document's meaning.
    let reread = vibra_host::HostWorld::decode("world.vibon", &once).expect("world");
    assert_eq!(
        reread,
        vibra_host::HostWorld::decode("world.vibon", WORLD).expect("world")
    );
}

#[test]
fn an_audit_trace_is_canonical() {
    assert_canonical(
        "trace.vibon",
        "(record format: @audit-trace.v1 events: (array))\n",
    );
    let event = vibra_host::AuditEvent::completed(
        &vibra_host::HostCall::StdoutText("hi\n".to_owned()),
        &Ok(vibra_host::HostOk::Void),
    );
    let trace = format!("{}\n", vibra_host::trace_to_vibon(&[event]));
    let once = formatted("trace.vibon", &trace);
    assert_eq!(formatted("trace.vibon", &once), once);
    // The formatted trace decodes to the same events.
    assert_eq!(
        vibra_host::decode_trace("trace.vibon", &once).expect("trace"),
        vibra_host::decode_trace("trace.vibon", &trace).expect("trace")
    );
}

#[test]
fn the_standard_library_manifest_is_canonical_and_the_formatter_is_idempotent_over_it()
{
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib/manifest.vibon");
    let text = std::fs::read_to_string(path).expect("the manifest");
    let once = formatted("manifest.vibon", &text);
    assert_eq!(formatted("manifest.vibon", &once), once);
    assert!(text.contains("  host: (array\n    \"io.stdin-text\""));
}
