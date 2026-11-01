//! The environment, clock, and random rows through both providers (Milestone 4,
//! Step 18). The system provider reads the real process; the world is
//! deterministic. These tests cover what the earlier provider tests do not: the
//! rows through `HostCall`, the boundaries of the matrix, and the platform cases.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::ffi::OsString;

use vibra_host::{
    EnvError, Failure, HostProvider, HostWorld, SystemProvider, TimeError,
    WorldProvider,
};

fn done<T, E: std::fmt::Debug>(result: Result<T, Failure<E>>) -> Result<T, E> {
    match result {
        Ok(value) => Ok(value),
        Err(Failure::Error(error)) => Err(error),
        Err(Failure::MemoryExhausted) => panic!("memory exhausted"),
    }
}

fn world(text: &str) -> WorldProvider {
    WorldProvider::new(&HostWorld::decode("world.vibon", text).expect("a world"))
}

#[test]
fn the_system_environment_is_the_process_environment() {
    let mut system = SystemProvider::new();
    // `PATH` exists on every platform the toolchain runs on.
    let expected = std::env::var("PATH").expect("a Unicode PATH");
    assert_eq!(done(system.env_var("PATH")), Ok(expected));
    assert_eq!(
        done(system.env_var("VIBRA_SURELY_NOT_SET_18")),
        Err(EnvError::NotPresent)
    );
    for name in ["", "A=B", "A\0B"] {
        assert_eq!(
            done(system.env_var(name)),
            Err(EnvError::NotPresent),
            "{name:?}"
        );
        assert_eq!(
            done(world("(record format: @host-world.v1)").env_var(name)),
            Err(EnvError::NotPresent),
            "{name:?}"
        );
    }
}

#[test]
fn the_system_arguments_are_verbatim_ordered_and_may_be_large() {
    let big = "x".repeat(1024 * 1024);
    let many = (0..10_000).map(|n| n.to_string()).collect::<Vec<_>>();
    let mut arguments = vec![
        OsString::from("--flag"),
        OsString::from(""),
        OsString::from("é 😀"),
        OsString::from(big.clone()),
    ];
    arguments.extend(many.iter().map(OsString::from));
    let mut expected = vec!["--flag".to_owned(), String::new(), "é 😀".to_owned(), big];
    expected.extend(many);
    let mut system = SystemProvider::with_arguments(arguments);
    assert_eq!(done(system.env_args()), Ok(expected));
    assert_eq!(done(SystemProvider::new().env_args()), Ok(Vec::new()));
}

#[cfg(unix)]
#[test]
fn a_non_unicode_argument_is_not_unicode() {
    use std::os::unix::ffi::OsStringExt;
    let mut system = SystemProvider::with_arguments(vec![
        OsString::from("ok"),
        OsString::from_vec(vec![b'a', 0xff]),
    ]);
    assert_eq!(done(system.env_args()), Err(EnvError::NotUnicode));
}

#[test]
fn the_world_clock_is_constant_wall_and_stepped_monotonic() {
    let mut p = world(
        "(record format: @host-world.v1 clock: (record wall: -5i64 monotonic: 7u64 step: 0u64))",
    );
    assert_eq!(done(p.now_wall()), Ok(-5));
    assert_eq!(done(p.now_wall()), Ok(-5));
    // A step of zero keeps the clock where it is, and it never goes back.
    assert_eq!(done(p.now_monotonic()), Ok(7));
    assert_eq!(done(p.now_monotonic()), Ok(7));
    let mut p = world("(record format: @host-world.v1)");
    assert_eq!(done(p.now_wall()), Ok(0));
    assert_eq!(
        [
            done(p.now_monotonic()),
            done(p.now_monotonic()),
            done(p.now_monotonic())
        ],
        [Ok(0), Ok(1), Ok(2)]
    );
}

#[test]
fn a_world_monotonic_read_past_the_range_is_unavailable_and_stays_so() {
    let mut p = world(
        "(record format: @host-world.v1 clock: (record monotonic: 18446744073709551614u64 step: 1u64))",
    );
    assert_eq!(done(p.now_monotonic()), Ok(u64::MAX - 1));
    assert_eq!(done(p.now_monotonic()), Ok(u64::MAX));
    assert_eq!(done(p.now_monotonic()), Err(TimeError::Unavailable));
    assert_eq!(done(p.now_monotonic()), Err(TimeError::Unavailable));
}

#[test]
fn the_system_clocks_are_sane_and_the_monotonic_clock_never_decreases() {
    let mut system = SystemProvider::new();
    let wall = done(system.now_wall()).expect("a wall clock");
    // After 2020-01-01 in nanoseconds since the epoch.
    assert!(wall > 1_577_836_800_000_000_000);
    let mut last = done(system.now_monotonic()).expect("a monotonic clock");
    // The monotonic clock counts from the provider's creation.
    assert!(last < 60_000_000_000, "{last}");
    for _ in 0..1000 {
        let next = done(system.now_monotonic()).expect("a monotonic clock");
        assert!(next >= last);
        last = next;
    }
}

#[test]
fn random_counts_of_zero_and_one_mebibyte_and_the_system_generator() {
    let mut p = world("(record format: @host-world.v1)");
    assert_eq!(done(p.random_bytes(0)), Ok(Vec::new()));
    let big = done(p.random_bytes(1024 * 1024)).expect("bytes");
    assert_eq!(big.len(), 1024 * 1024);
    // The same seed gives the same bytes in a fresh copy.
    let mut again = world("(record format: @host-world.v1)");
    assert_eq!(done(again.random_bytes(1024 * 1024)), Ok(big));
    let mut system = SystemProvider::new();
    assert_eq!(done(system.random_bytes(0)), Ok(Vec::new()));
    let real = done(system.random_bytes(1024 * 1024)).expect("bytes");
    assert_eq!(real.len(), 1024 * 1024);
    assert!(real.iter().any(|byte| *byte != 0));
}
