//! The system provider over the real host (Milestone 4, Step 16a).
//!
//! These are host-language tests of the one module that reads ambient state.
//! The directory they use is a fresh one under the system temporary directory.
//! Standard input is the test harness's and is not read here.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::ffi::OsString;
use std::path::PathBuf;

use vibra_host::{
    EnvError, Failure, FsError, FsKind, HostProvider, Metadata, SystemProvider,
};

fn done<T, E: std::fmt::Debug>(result: Result<T, Failure<E>>) -> Result<T, E> {
    match result {
        Ok(value) => Ok(value),
        Err(Failure::Error(error)) => Err(error),
        Err(Failure::MemoryExhausted) => panic!("memory exhausted"),
    }
}

/// A fresh directory that is removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        // A counter, not the clock, makes the name unique within the process: the
        // clock of some platforms is too coarse to tell two parallel tests apart.
        static NEXT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-host-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }

    fn at(&self, relative: &str) -> String {
        self.0
            .join(relative)
            .to_str()
            .expect("utf-8 path")
            .to_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn files_round_trip_and_directories_list_sorted() {
    let scratch = Scratch::new("files");
    let mut provider = SystemProvider::new();
    assert_eq!(
        done(provider.write_text(&scratch.at("b.txt"), "caf\u{e9}")),
        Ok(())
    );
    assert_eq!(
        done(provider.write_file(&scratch.at("B.bin"), &[0, 255])),
        Ok(())
    );
    assert_eq!(done(provider.write_text(&scratch.at("a"), "")), Ok(()));
    assert_eq!(
        done(provider.read_text(&scratch.at("b.txt"))),
        Ok("caf\u{e9}".to_owned())
    );
    assert_eq!(
        done(provider.read_file(&scratch.at("B.bin"))),
        Ok(vec![0, 255])
    );
    assert_eq!(
        done(provider.read_text(&scratch.at("B.bin"))),
        Err(FsError::InvalidText)
    );
    assert_eq!(
        done(provider.read_entries(scratch.0.to_str().unwrap())),
        Ok(vec!["B.bin".to_owned(), "a".to_owned(), "b.txt".to_owned()])
    );
    // A write replaces the whole contents.
    assert_eq!(done(provider.write_text(&scratch.at("b.txt"), "x")), Ok(()));
    assert_eq!(
        done(provider.read_text(&scratch.at("b.txt"))),
        Ok("x".to_owned())
    );
}

#[test]
fn metadata_has_a_size_that_is_zero_for_non_files() {
    let scratch = Scratch::new("stat");
    let mut provider = SystemProvider::new();
    done(provider.write_text(&scratch.at("f"), "abc")).unwrap();
    assert_eq!(
        done(provider.stat(&scratch.at("f"))),
        Ok(Metadata {
            kind: FsKind::File,
            size: 3
        })
    );
    assert_eq!(
        done(provider.stat(scratch.0.to_str().unwrap())),
        Ok(Metadata {
            kind: FsKind::Directory,
            size: 0
        })
    );
}

#[test]
fn filesystem_errors_are_the_registry_errors() {
    let scratch = Scratch::new("errors");
    let mut provider = SystemProvider::new();
    done(provider.write_text(&scratch.at("f"), "abc")).unwrap();
    let directory = scratch.0.to_str().unwrap().to_owned();
    assert_eq!(
        done(provider.read_file(&scratch.at("missing"))),
        Err(FsError::NotFound)
    );
    assert_eq!(
        done(provider.stat(&scratch.at("missing"))),
        Err(FsError::NotFound)
    );
    assert_eq!(done(provider.read_file(&directory)), Err(FsError::NotAFile));
    assert_eq!(
        done(provider.write_text(&directory, "x")),
        Err(FsError::NotAFile)
    );
    assert_eq!(
        done(provider.write_text(&scratch.at("no/such/dir/f"), "x")),
        Err(FsError::NotFound)
    );
    assert_eq!(
        done(provider.read_entries(&scratch.at("f"))),
        Err(FsError::NotADirectory)
    );
    assert_eq!(done(provider.read_file("a\0b")), Err(FsError::InvalidPath));
    assert_eq!(
        done(provider.write_file("\0", &[])),
        Err(FsError::InvalidPath)
    );
}

#[test]
fn environment_names_that_no_environment_can_hold_are_not_present() {
    let mut provider = SystemProvider::new();
    for name in ["", "A=B", "A\0B"] {
        assert_eq!(
            done(provider.env_var(name)),
            Err(EnvError::NotPresent),
            "{name:?}"
        );
    }
    assert_eq!(
        done(provider.env_var("VIBRA_HOST_TEST_NO_SUCH_VARIABLE")),
        Err(EnvError::NotPresent)
    );
    // The test harness always sets this one.
    assert!(done(provider.env_var("CARGO_MANIFEST_DIR")).is_ok());
}

#[test]
fn arguments_are_the_ones_the_provider_was_given_in_order() {
    let mut none = SystemProvider::new();
    assert_eq!(done(none.env_args()), Ok(Vec::new()));
    let mut some = SystemProvider::with_arguments(vec!["a".into(), "b c".into()]);
    assert_eq!(
        done(some.env_args()),
        Ok(vec!["a".to_owned(), "b c".to_owned()])
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bad =
            SystemProvider::with_arguments(vec![OsString::from_vec(vec![0xff])]);
        assert_eq!(done(bad.env_args()), Err(EnvError::NotUnicode));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut bad =
            SystemProvider::with_arguments(vec![OsString::from_wide(&[0xD800])]);
        assert_eq!(done(bad.env_args()), Err(EnvError::NotUnicode));
    }
}

#[test]
fn the_wall_clock_is_after_2020_and_the_monotonic_clock_never_goes_back() {
    let mut provider = SystemProvider::new();
    let wall = done(provider.now_wall()).unwrap();
    assert!(wall > 1_577_836_800_000_000_000, "{wall}");
    let mut last = done(provider.now_monotonic()).unwrap();
    for _ in 0..100 {
        let now = done(provider.now_monotonic()).unwrap();
        assert!(now >= last);
        last = now;
    }
}

#[test]
fn random_bytes_have_the_requested_length_and_differ_between_calls() {
    let mut provider = SystemProvider::new();
    assert_eq!(done(provider.random_bytes(0)), Ok(Vec::new()));
    for count in [1, 7, 8, 9, 4096] {
        assert_eq!(
            done(provider.random_bytes(count)).unwrap().len(),
            count as usize
        );
    }
    let first = done(provider.random_bytes(32)).unwrap();
    let second = done(provider.random_bytes(32)).unwrap();
    assert_ne!(first, second);
}

#[test]
fn an_allocation_the_host_cannot_make_is_memory_exhaustion_not_an_error() {
    let mut provider = SystemProvider::new();
    assert!(matches!(
        provider.random_bytes(u64::MAX),
        Err(Failure::MemoryExhausted)
    ));
}

// -- the output sink (Step 16b-1) ------------------------------------------------

#[test]
fn a_capturing_provider_keeps_standard_output_and_error_apart_in_call_order() {
    let mut provider = SystemProvider::new().capturing();
    assert_eq!(done(provider.stdout_text("h\u{e9}")), Ok(()));
    assert_eq!(done(provider.stderr_bytes(&[0, 255])), Ok(()));
    assert_eq!(done(provider.stdout_bytes(&[33])), Ok(()));
    assert_eq!(provider.captured_stdout(), "h\u{e9}!".as_bytes());
    assert_eq!(provider.captured_stderr(), &[0, 255]);
}

#[test]
fn an_inheriting_provider_captures_nothing() {
    let provider = SystemProvider::new();
    assert!(provider.captured_stdout().is_empty());
    assert!(provider.captured_stderr().is_empty());
}
