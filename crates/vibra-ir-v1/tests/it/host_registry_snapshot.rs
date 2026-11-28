//! The `vibra_v1` host registry (Milestone 4, Step 16a; ledger D17, D19).
//!
//! `docs/spec/06-runtime.md`, "Registry versioning": a changed symbol,
//! signature, owner, or audit-event shape needs `vibra_v2`.

use vibra_ir_v1::external::host::{
    HOST_REGISTRY_MINOR, HOST_ROWS, HostErrorType, HostType, host_row,
};

#[test]
fn the_registry_has_the_seventeen_rows_of_the_runtime_chapter() {
    let symbols = HOST_ROWS.iter().map(|row| row.symbol).collect::<Vec<_>>();
    assert_eq!(
        symbols,
        [
            "io.stdin-text",
            "io.stdin-bytes",
            "io.stdout-text",
            "io.stdout-bytes",
            "io.stderr-text",
            "io.stderr-bytes",
            "fs.read-file",
            "fs.read-text",
            "fs.read-entries",
            "fs.write-file",
            "fs.write-text",
            "fs.metadata-stat",
            "env.read-var",
            "env.read-args",
            "time.now-wall",
            "time.now-monotonic",
            "random.generate-bytes",
        ]
    );
    assert!(HOST_ROWS.iter().all(|row| row.since == 0));
    assert_eq!(HOST_REGISTRY_MINOR, 0);
}

#[test]
fn every_row_has_the_signature_the_chapter_writes() {
    let signature = |symbol: &str| host_row(symbol).expect("a row").signature();
    assert_eq!(signature("io.stdin-text"), "() -> (result str io-error)");
    assert_eq!(
        signature("io.stdout-bytes"),
        "(value bytes) -> (result void io-error)"
    );
    assert_eq!(
        signature("fs.write-text"),
        "(at path contents str) -> (result void fs-error)"
    );
    assert_eq!(
        signature("fs.read-entries"),
        "(at path) -> (result (array str) fs-error)"
    );
    assert_eq!(
        signature("fs.metadata-stat"),
        "(at path) -> (result fs-metadata fs-error)"
    );
    assert_eq!(
        signature("env.read-var"),
        "(name str) -> (result str env-error)"
    );
    assert_eq!(signature("time.now-wall"), "() -> (result i64 time-error)");
    assert_eq!(
        signature("random.generate-bytes"),
        "(count u64) -> (result bytes random-error)"
    );
}

#[test]
fn owners_are_the_module_and_root_that_declare_the_operation() {
    let owner = |symbol: &str| host_row(symbol).expect("a row").owner;
    assert_eq!(owner("io.stdin-bytes"), "@std.io.stdin");
    assert_eq!(owner("io.stderr-text"), "@std.io.stderr");
    assert_eq!(owner("fs.read-file"), "@std.fs.read");
    assert_eq!(owner("fs.write-file"), "@std.fs.write");
    assert_eq!(owner("fs.metadata-stat"), "@std.fs.metadata");
    assert_eq!(owner("env.read-args"), "@std.env.read");
    assert_eq!(owner("time.now-monotonic"), "@std.time.now");
    assert_eq!(owner("random.generate-bytes"), "@std.random.generate");
}

#[test]
fn no_row_returns_never_and_every_filesystem_path_parameter_is_spelled_at() {
    for row in &HOST_ROWS {
        assert!(
            !row.signature().contains("never"),
            "{} returns never",
            row.symbol
        );
        for parameter in row.parameters {
            if parameter.ty == HostType::Path {
                assert_eq!(parameter.name, "at", "{}", row.symbol);
            }
        }
    }
}

#[test]
fn the_error_enums_are_closed_and_have_the_declared_variants() {
    assert_eq!(
        HostErrorType::Io.variants(),
        ["invalid-text", "closed", "other"]
    );
    assert_eq!(
        HostErrorType::Fs.variants(),
        [
            "not-found",
            "permission-denied",
            "not-a-file",
            "not-a-directory",
            "invalid-path",
            "invalid-text",
            "other"
        ]
    );
    assert_eq!(
        HostErrorType::Env.variants(),
        ["not-present", "not-unicode"]
    );
    assert_eq!(HostErrorType::Time.variants(), ["unavailable"]);
    assert_eq!(HostErrorType::Random.variants(), ["unavailable"]);
}
