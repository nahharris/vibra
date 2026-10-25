//! The `@host-world.v1` decoder, the default world, and the world provider
//! (Milestone 4, Step 16a; ledger D20).
//!
//! `docs/spec/06-runtime.md`, "The host world".

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_diagnostics::DiagnosticCode;
use vibra_host::{
    EnvError, Failure, FsError, FsKind, HostProvider, HostWorld, IoError, Metadata,
    TimeError, WorldProvider,
};

const DEFAULT_WORLD: &str = "(record format: @host-world.v1)\n";

fn world(text: &str) -> HostWorld {
    match HostWorld::decode("world.vibon", text) {
        Ok(world) => world,
        Err(diagnostics) => panic!("{diagnostics:?}"),
    }
}

fn provider(text: &str) -> WorldProvider {
    WorldProvider::new(&world(text))
}

fn done<T, E: std::fmt::Debug>(result: Result<T, Failure<E>>) -> Result<T, E> {
    match result {
        Ok(value) => Ok(value),
        Err(Failure::Error(error)) => Err(error),
        Err(Failure::MemoryExhausted) => panic!("memory exhausted"),
    }
}

fn codes(text: &str) -> Vec<DiagnosticCode> {
    match HostWorld::decode("world.vibon", text) {
        Ok(_) => panic!("the world was accepted: {text}"),
        Err(diagnostics) => diagnostics.iter().map(|d| d.code()).collect(),
    }
}

// --- the default world ---------------------------------------------------

#[test]
fn the_default_world_is_the_record_with_only_a_format() {
    assert_eq!(world(DEFAULT_WORLD), HostWorld::default());
}

#[test]
fn the_default_world_matches_the_runtime_chapter_field_for_field() {
    let mut p = WorldProvider::new(&HostWorld::default());
    // stdin: empty text.
    assert_eq!(done(p.stdin_text()), Ok(String::new()));
    assert_eq!(done(p.stdin_bytes()), Ok(Vec::new()));
    // directories and files: only the root.
    assert_eq!(done(p.read_entries("/")), Ok(Vec::new()));
    assert_eq!(done(p.read_file("/x")), Err(FsError::NotFound));
    // env and args: none.
    assert_eq!(done(p.env_var("HOME")), Err(EnvError::NotPresent));
    assert_eq!(done(p.env_args()), Ok(Vec::new()));
    // clock: wall 0, monotonic 0 stepping by 1.
    assert_eq!(done(p.now_wall()), Ok(0));
    assert_eq!(done(p.now_wall()), Ok(0));
    assert_eq!(done(p.now_monotonic()), Ok(0));
    assert_eq!(done(p.now_monotonic()), Ok(1));
    assert_eq!(done(p.now_monotonic()), Ok(2));
    // random: SplitMix64 from seed 0.
    assert_eq!(
        done(p.random_bytes(8)),
        Ok(0xE220_A839_7B1D_CDAF_u64.to_le_bytes().to_vec())
    );
}

#[test]
fn a_world_with_every_field_decodes_and_serves_each_root() {
    let mut p = provider(
        r#"(record
  format: @host-world.v1
  stdin: "line one\nline two\n"
  directories: (array "/empty" "/a/b")
  files: (dict
    "/in.txt" "hi"
    "/data/raw.bin" (record kind: @bytes values: (array 0u8 255u8 16u8))
    "/data/text.txt" "caf\u{e9}")
  env: (dict "HOME" "/home/me" "EMPTY" "")
  args: (array "first" "second")
  clock: (record wall: 1700000000000000000i64 monotonic: 5u64 step: 10u64)
  random: (record seed: 1234567u64))
"#,
    );
    assert_eq!(done(p.stdin_text()), Ok("line one\nline two\n".to_owned()));
    assert_eq!(done(p.read_text("/in.txt")), Ok("hi".to_owned()));
    assert_eq!(done(p.read_file("/in.txt")), Ok(b"hi".to_vec()));
    assert_eq!(done(p.read_file("/data/raw.bin")), Ok(vec![0, 255, 16]));
    assert_eq!(
        done(p.read_text("/data/raw.bin")),
        Err(FsError::InvalidText)
    );
    assert_eq!(
        done(p.read_text("/data/text.txt")),
        Ok("caf\u{e9}".to_owned())
    );
    assert_eq!(
        done(p.read_entries("/")),
        Ok(vec![
            "a".to_owned(),
            "data".to_owned(),
            "empty".to_owned(),
            "in.txt".to_owned()
        ])
    );
    assert_eq!(done(p.read_entries("/a")), Ok(vec!["b".to_owned()]));
    assert_eq!(done(p.read_entries("/empty")), Ok(Vec::new()));
    assert_eq!(done(p.env_var("HOME")), Ok("/home/me".to_owned()));
    assert_eq!(done(p.env_var("EMPTY")), Ok(String::new()));
    assert_eq!(done(p.env_var("MISSING")), Err(EnvError::NotPresent));
    assert_eq!(
        done(p.env_args()),
        Ok(vec!["first".to_owned(), "second".to_owned()])
    );
    assert_eq!(done(p.now_wall()), Ok(1_700_000_000_000_000_000));
    assert_eq!(done(p.now_monotonic()), Ok(5));
    assert_eq!(done(p.now_monotonic()), Ok(15));
    assert_eq!(done(p.now_wall()), Ok(1_700_000_000_000_000_000));
}

#[test]
fn stdin_may_be_a_bytes_record_and_the_position_ends_after_any_call() {
    let mut p = provider(
        "(record format: @host-world.v1 stdin: (record kind: @bytes values: (array 104u8 105u8)))",
    );
    assert_eq!(done(p.stdin_bytes()), Ok(b"hi".to_vec()));
    // A second call returns the empty value.
    assert_eq!(done(p.stdin_bytes()), Ok(Vec::new()));
    assert_eq!(done(p.stdin_text()), Ok(String::new()));

    // Invalid UTF-8 as text: `invalid-text`, with the position at the end.
    let mut bad = provider(
        "(record format: @host-world.v1 stdin: (record kind: @bytes values: (array 255u8 254u8)))",
    );
    assert_eq!(done(bad.stdin_text()), Err(IoError::InvalidText));
    assert_eq!(done(bad.stdin_bytes()), Ok(Vec::new()));
}

#[test]
fn output_operations_deliver_and_never_fail_in_a_world() {
    let mut p = WorldProvider::new(&HostWorld::default());
    assert_eq!(done(p.stdout_text("caf\u{e9} \u{1d11e}")), Ok(()));
    assert_eq!(done(p.stdout_bytes(&[0, 255])), Ok(()));
    assert_eq!(done(p.stderr_text("")), Ok(()));
    assert_eq!(done(p.stderr_bytes(&[])), Ok(()));
}

// --- filesystem resolution -------------------------------------------------

fn fs_world() -> WorldProvider {
    provider(
        r#"(record
  format: @host-world.v1
  directories: (array "/dir/sub")
  files: (dict "/dir/file.txt" "abc" "/top" "t"))
"#,
    )
}

#[test]
fn paths_resolve_from_the_root_without_links() {
    let mut p = fs_world();
    // Empty and `.` components are skipped; `..` names the parent; the parent
    // of the root is the root; relative paths start at the root.
    for spelling in [
        "/dir/file.txt",
        "dir/file.txt",
        "//dir///file.txt",
        "/dir/./file.txt",
        "/dir/sub/../file.txt",
        "/../../dir/file.txt",
        "/dir/sub/../../dir/file.txt",
    ] {
        assert_eq!(
            done(p.read_text(spelling)),
            Ok("abc".to_owned()),
            "{spelling}"
        );
    }
    assert_eq!(done(p.read_entries("")), done(p.read_entries("/")));
    assert_eq!(done(p.read_entries("/..")), done(p.read_entries("/")));
}

#[test]
fn a_nul_in_a_path_is_invalid_path() {
    let mut p = fs_world();
    assert_eq!(done(p.read_file("/dir/\0")), Err(FsError::InvalidPath));
    assert_eq!(done(p.write_text("/a\0b", "x")), Err(FsError::InvalidPath));
    assert_eq!(done(p.stat("\0")), Err(FsError::InvalidPath));
    assert_eq!(done(p.read_entries("/\0")), Err(FsError::InvalidPath));
}

#[test]
fn every_ancestor_of_a_listed_path_exists() {
    let mut p = fs_world();
    assert_eq!(
        done(p.read_entries("/dir")),
        Ok(vec!["file.txt".to_owned(), "sub".to_owned()])
    );
    assert_eq!(
        done(p.read_entries("/")),
        Ok(vec!["dir".to_owned(), "top".to_owned()])
    );
}

#[test]
fn filesystem_errors_follow_the_registry_meanings() {
    let mut p = fs_world();
    // A missing path, and a missing parent for a write.
    assert_eq!(done(p.read_file("/nope")), Err(FsError::NotFound));
    assert_eq!(done(p.read_entries("/nope")), Err(FsError::NotFound));
    assert_eq!(done(p.stat("/nope/deeper")), Err(FsError::NotFound));
    assert_eq!(done(p.write_file("/nope/x", b"1")), Err(FsError::NotFound));
    // A directory where a file is read or replaced.
    assert_eq!(done(p.read_file("/dir")), Err(FsError::NotAFile));
    assert_eq!(done(p.read_text("/")), Err(FsError::NotAFile));
    assert_eq!(done(p.write_text("/dir", "x")), Err(FsError::NotAFile));
    assert_eq!(done(p.write_file("/", b"x")), Err(FsError::NotAFile));
    // A file where a directory is needed: the target of read-entries or a parent.
    assert_eq!(done(p.read_entries("/top")), Err(FsError::NotADirectory));
    assert_eq!(done(p.read_file("/top/x")), Err(FsError::NotADirectory));
    assert_eq!(
        done(p.write_file("/top/x", b"1")),
        Err(FsError::NotADirectory)
    );
    assert_eq!(done(p.stat("/top/x")), Err(FsError::NotADirectory));
    // The world has no permission failures.
    assert!(done(p.write_text("/dir/new", "x")).is_ok());
}

#[test]
fn writes_create_or_replace_the_whole_file_and_never_create_parents() {
    let mut p = fs_world();
    assert_eq!(done(p.write_text("/dir/new.txt", "first")), Ok(()));
    assert_eq!(done(p.read_text("/dir/new.txt")), Ok("first".to_owned()));
    assert_eq!(done(p.write_file("/dir/new.txt", &[1, 2, 3])), Ok(()));
    assert_eq!(done(p.read_file("/dir/new.txt")), Ok(vec![1, 2, 3]));
    assert_eq!(done(p.write_text("/dir/new.txt", "")), Ok(()));
    assert_eq!(done(p.read_file("/dir/new.txt")), Ok(Vec::new()));
    assert_eq!(
        done(p.write_text("/missing/new.txt", "x")),
        Err(FsError::NotFound)
    );
    assert_eq!(done(p.read_entries("/dir")).unwrap().len(), 3);
}

#[test]
fn metadata_reports_kind_and_a_size_that_is_zero_for_non_files() {
    let mut p = fs_world();
    assert_eq!(
        done(p.stat("/dir/file.txt")),
        Ok(Metadata {
            kind: FsKind::File,
            size: 3
        })
    );
    assert_eq!(
        done(p.stat("/dir")),
        Ok(Metadata {
            kind: FsKind::Directory,
            size: 0
        })
    );
    assert_eq!(
        done(p.stat("/")),
        Ok(Metadata {
            kind: FsKind::Directory,
            size: 0
        })
    );
}

#[test]
fn directory_entries_sort_by_unicode_scalar_order() {
    let mut p = provider(
        r#"(record format: @host-world.v1
  files: (dict "/b" "" "/B" "" "/\u{e9}" "" "/z" "" "/\u{1d11e}" "" "/a" ""))"#,
    );
    // Scalar order, not locale or UTF-16 order: U+1D11E sorts after U+00E9.
    assert_eq!(
        done(p.read_entries("/")),
        Ok(["B", "a", "b", "z", "\u{e9}", "\u{1d11e}"]
            .map(str::to_owned)
            .to_vec())
    );
}

#[test]
fn a_path_of_many_components_and_a_one_mebibyte_file() {
    let mut deep = String::new();
    for index in 0..2000 {
        deep.push_str(&format!("/d{index}"));
    }
    let big = "x".repeat(1 << 20);
    let text = format!(
        "(record format: @host-world.v1 directories: (array \"{deep}\") files: (dict \"/big\" \"{big}\"))"
    );
    let mut p = provider(&text);
    assert_eq!(done(p.read_entries(&deep)), Ok(Vec::new()));
    assert_eq!(done(p.write_text(&format!("{deep}/f"), "ok")), Ok(()));
    assert_eq!(done(p.read_text(&format!("{deep}/f"))), Ok("ok".to_owned()));
    assert_eq!(done(p.read_file("/big")).unwrap().len(), 1 << 20);
    assert_eq!(
        done(p.stat("/big")),
        Ok(Metadata {
            kind: FsKind::File,
            size: 1 << 20
        })
    );
}

// --- environment ------------------------------------------------------------

#[test]
fn names_no_environment_can_hold_are_not_present() {
    let mut p = provider("(record format: @host-world.v1 env: (dict \"A\" \"1\"))");
    assert_eq!(done(p.env_var("")), Err(EnvError::NotPresent));
    assert_eq!(done(p.env_var("A=B")), Err(EnvError::NotPresent));
    assert_eq!(done(p.env_var("A\0")), Err(EnvError::NotPresent));
    assert_eq!(done(p.env_var("A")), Ok("1".to_owned()));
}

// --- clock --------------------------------------------------------------------

#[test]
fn a_zero_step_keeps_the_monotonic_clock_constant() {
    let mut p = provider(
        "(record format: @host-world.v1 clock: (record wall: -5i64 monotonic: 7u64 step: 0u64))",
    );
    assert_eq!(done(p.now_wall()), Ok(-5));
    assert_eq!(done(p.now_monotonic()), Ok(7));
    assert_eq!(done(p.now_monotonic()), Ok(7));
}

#[test]
fn a_monotonic_read_past_the_u64_range_is_unavailable() {
    let mut p = provider(
        "(record format: @host-world.v1 clock: (record monotonic: 18446744073709551615u64))",
    );
    assert_eq!(done(p.now_monotonic()), Ok(u64::MAX));
    assert_eq!(done(p.now_monotonic()), Err(TimeError::Unavailable));
    assert_eq!(done(p.now_monotonic()), Err(TimeError::Unavailable));

    let mut stepped = provider(
        "(record format: @host-world.v1 clock: (record monotonic: 18446744073709551600u64 step: 10u64))",
    );
    assert_eq!(
        done(stepped.now_monotonic()),
        Ok(18_446_744_073_709_551_600)
    );
    // 18446744073709551610 is still within the range; the next would not be.
    assert_eq!(
        done(stepped.now_monotonic()),
        Ok(18_446_744_073_709_551_610)
    );
    assert_eq!(done(stepped.now_monotonic()), Err(TimeError::Unavailable));
    // The wall clock is unaffected.
    assert_eq!(done(stepped.now_wall()), Ok(0));
}

#[test]
fn the_clock_record_defaults_each_missing_field() {
    let mut p = provider("(record format: @host-world.v1 clock: (record step: 3u64))");
    assert_eq!(done(p.now_wall()), Ok(0));
    assert_eq!(done(p.now_monotonic()), Ok(0));
    assert_eq!(done(p.now_monotonic()), Ok(3));
}

// --- random -------------------------------------------------------------------

fn outputs(seed: u64, count: usize) -> Vec<u8> {
    let mut p = provider(&format!(
        "(record format: @host-world.v1 random: (record seed: {seed}u64))"
    ));
    done(p.random_bytes(count as u64)).unwrap()
}

fn le(values: &[u64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn splitmix64_vectors_for_seed_zero() {
    let expected = le(&[
        0xE220_A839_7B1D_CDAF,
        0x6E78_9E6A_A1B9_65F4,
        0x06C4_5D18_8009_454F,
        0xF88B_B8A8_724C_81EC,
    ]);
    assert_eq!(outputs(0, 0), Vec::<u8>::new());
    assert_eq!(outputs(0, 1), expected[..1]);
    assert_eq!(outputs(0, 8), expected[..8]);
    assert_eq!(outputs(0, 9), expected[..9]);
    assert_eq!(outputs(0, 32), expected);
}

#[test]
fn splitmix64_vectors_for_a_second_seed() {
    let expected = le(&[
        6_457_827_717_110_365_317,
        3_203_168_211_198_807_973,
        9_817_491_932_198_370_423,
        4_593_380_528_125_082_431,
        16_408_922_859_458_223_821,
    ]);
    assert_eq!(outputs(1_234_567, 40), expected);
    assert_eq!(outputs(1_234_567, 17), expected[..17]);
}

#[test]
fn the_generator_state_continues_across_calls_and_discards_unused_bytes() {
    let all = le(&[
        0xE220_A839_7B1D_CDAF,
        0x6E78_9E6A_A1B9_65F4,
        0x06C4_5D18_8009_454F,
        0xF88B_B8A8_724C_81EC,
    ]);
    let mut p = provider("(record format: @host-world.v1)");
    // 3 bytes take one output; its other 5 bytes are discarded.
    assert_eq!(done(p.random_bytes(3)), Ok(all[..3].to_vec()));
    // The next call starts at the second output.
    assert_eq!(done(p.random_bytes(9)), Ok(all[8..17].to_vec()));
    // A call of 0 takes no output.
    assert_eq!(done(p.random_bytes(0)), Ok(Vec::new()));
    assert_eq!(done(p.random_bytes(8)), Ok(all[24..32].to_vec()));
}

#[test]
fn the_world_has_a_generator_so_random_is_never_unavailable_for_small_counts() {
    let mut p = WorldProvider::new(&HostWorld::default());
    assert!(done(p.random_bytes(1000)).is_ok());
}

// --- a fresh copy ---------------------------------------------------------------

#[test]
fn each_provider_is_a_fresh_copy_and_shares_nothing() {
    let shared = world(
        "(record format: @host-world.v1 stdin: \"in\" files: (dict \"/f\" \"orig\") clock: (record step: 2u64))",
    );
    let mut first = WorldProvider::new(&shared);
    let mut second = WorldProvider::new(&shared);
    assert_eq!(done(first.write_text("/f", "changed")), Ok(()));
    assert_eq!(done(first.write_text("/g", "new")), Ok(()));
    assert_eq!(done(first.stdin_text()), Ok("in".to_owned()));
    assert_eq!(done(first.now_monotonic()), Ok(0));
    assert_eq!(done(first.now_monotonic()), Ok(2));
    assert_eq!(done(first.random_bytes(8)).unwrap().len(), 8);

    assert_eq!(done(second.read_text("/f")), Ok("orig".to_owned()));
    assert_eq!(done(second.read_text("/g")), Err(FsError::NotFound));
    assert_eq!(done(second.stdin_text()), Ok("in".to_owned()));
    assert_eq!(done(second.now_monotonic()), Ok(0));
    assert_eq!(done(second.random_bytes(8)).unwrap(), outputs(0, 8));
    // The decoded world itself is unchanged by either run.
    let mut third = WorldProvider::new(&shared);
    assert_eq!(done(third.read_text("/f")), Ok("orig".to_owned()));
}

// --- the decoder's diagnostics ---------------------------------------------------------

#[test]
fn a_world_that_is_not_a_record_or_has_another_format_is_rejected() {
    assert_eq!(codes("(array)"), [DiagnosticCode::DataInvalidShape]);
    assert_eq!(
        codes("(record format: @host-world.v2)"),
        [DiagnosticCode::DataInvalidValue]
    );
    assert_eq!(
        codes("(record format: \"host-world.v1\")"),
        [DiagnosticCode::DataInvalidShape]
    );
    assert_eq!(codes("(record)"), [DiagnosticCode::DataInvalidShape]);
}

#[test]
fn an_unknown_field_is_rejected() {
    assert_eq!(
        codes("(record format: @host-world.v1 network: (array))"),
        [DiagnosticCode::DataInvalidShape]
    );
    assert_eq!(
        codes("(record format: @host-world.v1 clock: (record wall: 0i64 sleep: 1u64))"),
        [DiagnosticCode::DataInvalidShape]
    );
    assert_eq!(
        codes(
            "(record format: @host-world.v1 random: (record seed: 0u64 algorithm: @x))"
        ),
        [DiagnosticCode::DataInvalidShape]
    );
}

#[test]
fn a_duplicate_key_or_field_is_rejected() {
    let duplicate_field =
        codes("(record format: @host-world.v1 stdin: \"a\" stdin: \"b\")");
    assert!(!duplicate_field.is_empty());
    let duplicate_key =
        codes("(record format: @host-world.v1 env: (dict \"A\" \"1\" \"A\" \"2\"))");
    assert!(!duplicate_key.is_empty());
    let duplicate_file = codes(
        "(record format: @host-world.v1 files: (dict \"/a\" \"1\" \"/a\" \"2\"))",
    );
    assert!(!duplicate_file.is_empty());
}

#[test]
fn a_wrong_value_kind_is_rejected() {
    let shapes = [
        "(record format: @host-world.v1 stdin: 7u8)",
        "(record format: @host-world.v1 stdin: (array))",
        "(record format: @host-world.v1 directories: \"/a\")",
        "(record format: @host-world.v1 directories: (array 1u8))",
        "(record format: @host-world.v1 files: (array))",
        "(record format: @host-world.v1 files: (dict \"/a\" 1u8))",
        "(record format: @host-world.v1 files: (dict 1u8 \"x\"))",
        "(record format: @host-world.v1 env: (dict \"A\" (array)))",
        "(record format: @host-world.v1 args: (dict))",
        "(record format: @host-world.v1 args: (array 3u8))",
        "(record format: @host-world.v1 clock: 0u64)",
        "(record format: @host-world.v1 clock: (record wall: \"0\"))",
        "(record format: @host-world.v1 random: (array))",
        "(record format: @host-world.v1 random: (record seed: \"0\"))",
    ];
    for text in shapes {
        assert_eq!(codes(text), [DiagnosticCode::DataInvalidShape], "{text}");
    }
}

#[test]
fn a_value_out_of_its_type_is_invalid_value() {
    let values = [
        // The suffix must be the field's type.
        "(record format: @host-world.v1 clock: (record wall: 0u64))",
        "(record format: @host-world.v1 clock: (record monotonic: 0i64))",
        "(record format: @host-world.v1 clock: (record step: 1))",
        "(record format: @host-world.v1 random: (record seed: 0i64))",
        // Out of range.
        "(record format: @host-world.v1 clock: (record wall: 9223372036854775808i64))",
        "(record format: @host-world.v1 clock: (record monotonic: 18446744073709551616u64))",
        "(record format: @host-world.v1 random: (record seed: -1u64))",
        // A bytes record needs `u8` literals and the right kind.
        "(record format: @host-world.v1 stdin: (record kind: @bytes values: (array 256u8)))",
        "(record format: @host-world.v1 stdin: (record kind: @bytes values: (array 1)))",
        "(record format: @host-world.v1 stdin: (record kind: @text values: (array)))",
        // A NUL in a path.
        "(record format: @host-world.v1 directories: (array \"/a\\u{0}b\"))",
        "(record format: @host-world.v1 files: (dict \"/a\\u{0}b\" \"x\"))",
    ];
    for text in values {
        let found = codes(text);
        assert!(
            found.iter().all(|code| matches!(
                code,
                DiagnosticCode::DataInvalidValue | DiagnosticCode::DataInvalidShape
            )) && !found.is_empty(),
            "{text}: {found:?}"
        );
    }
    assert_eq!(
        codes("(record format: @host-world.v1 clock: (record wall: 0u64))"),
        [DiagnosticCode::DataInvalidValue]
    );
}

#[test]
fn an_odd_dict_is_rejected() {
    assert!(
        !codes("(record format: @host-world.v1 env: (dict \"A\" \"1\" \"B\"))")
            .is_empty()
    );
    assert!(!codes("(record format: @host-world.v1 files: (dict \"/a\"))").is_empty());
}

#[test]
fn a_path_that_is_both_a_file_and_a_directory_is_invalid_value() {
    for text in [
        // A file that is also an explicit directory.
        "(record format: @host-world.v1 directories: (array \"/a\") files: (dict \"/a\" \"x\"))",
        // A file that needs another file as its ancestor.
        "(record format: @host-world.v1 files: (dict \"/a\" \"x\" \"/a/b\" \"y\"))",
        // The root is a directory.
        "(record format: @host-world.v1 files: (dict \"/\" \"x\"))",
        // Two spellings of one path.
        "(record format: @host-world.v1 files: (dict \"/a\" \"x\" \"/./a\" \"y\"))",
    ] {
        assert_eq!(codes(text), [DiagnosticCode::DataInvalidValue], "{text}");
    }
}

#[test]
fn every_diagnostic_names_the_world_document() {
    let diagnostics = HostWorld::decode(
        "cases/world.vibon",
        "(record format: @host-world.v1 clock: (record wall: 0u64) random: (record seed: \"x\"))",
    )
    .expect_err("rejected");
    assert!(diagnostics.len() >= 2, "{diagnostics:?}");
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.source_id(), Some("cases/world.vibon"));
    }
}

#[test]
fn text_that_is_not_data_is_rejected_with_the_reader_diagnostics() {
    assert!(
        HostWorld::decode("world.vibon", "(record format: @host-world.v1").is_err()
    );
    assert!(HostWorld::decode("world.vibon", "").is_err());
    assert!(
        HostWorld::decode("world.vibon", "(record format: @host-world.v1) (array)")
            .is_err()
    );
}
