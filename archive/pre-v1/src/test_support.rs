//! Deterministic building blocks for table- and property-style test tooling.
//!
//! These helpers deliberately avoid ambient randomness. A failing property
//! reports both the original generated value and a deterministic smallest
//! counterexample found by shrinking toward zero.

use std::fmt;
use std::ops::RangeInclusive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCase<I, E> {
    pub name: String,
    pub input: I,
    pub expected: E,
}

pub fn check_table<I, E, A>(
    cases: impl IntoIterator<Item = TableCase<I, E>>,
    evaluate: impl Fn(&I) -> A,
) -> Result<(), TableFailure<E, A>>
where
    E: PartialEq<A>,
{
    for case in cases {
        let actual = evaluate(&case.input);
        if case.expected != actual {
            return Err(TableFailure {
                name: case.name,
                expected: case.expected,
                actual,
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableFailure<E, A> {
    pub name: String,
    pub expected: E,
    pub actual: A,
}

impl<E: fmt::Debug, A: fmt::Debug> fmt::Display for TableFailure<E, A> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "table case `{}` failed: expected {:?}, got {:?}",
            self.name, self.expected, self.actual
        )
    }
}

#[derive(Debug, Clone)]
pub struct SeededGenerator {
    state: u64,
}

impl SeededGenerator {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    pub fn int_inclusive(&mut self, range: RangeInclusive<i64>) -> i64 {
        let start = *range.start();
        let end = *range.end();
        assert!(start <= end, "generator range must not be empty");
        let width = (i128::from(end) - i128::from(start) + 1) as u128;
        (i128::from(start) + (u128::from(self.next_u64()) % width) as i128) as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyFailure {
    pub seed: u64,
    pub case_index: usize,
    pub original: i64,
    pub minimized: i64,
}

pub fn check_int_property(
    seed: u64,
    cases: usize,
    range: RangeInclusive<i64>,
    property: impl Fn(i64) -> bool,
) -> Result<(), PropertyFailure> {
    let mut generator = SeededGenerator::new(seed);
    for case_index in 0..cases {
        let original = generator.int_inclusive(range.clone());
        if !property(original) {
            let minimized = shrink_int(original, &property);
            return Err(PropertyFailure {
                seed,
                case_index,
                original,
                minimized,
            });
        }
    }
    Ok(())
}

fn shrink_int(mut value: i64, property: &impl Fn(i64) -> bool) -> i64 {
    loop {
        let candidate = value / 2;
        if candidate == value || property(candidate) {
            return value;
        }
        value = candidate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_failure_preserves_case_identity() {
        let cases = [
            TableCase {
                name: "zero".into(),
                input: 0,
                expected: 0,
            },
            TableCase {
                name: "negative".into(),
                input: -2,
                expected: 2,
            },
        ];
        let failure = check_table(cases, |value| *value).unwrap_err();
        assert_eq!(failure.name, "negative");
        assert_eq!(failure.expected, 2);
        assert_eq!(failure.actual, -2);
    }

    #[test]
    fn generation_is_seeded_and_shrinking_is_stable() {
        let generated = (0..2)
            .map(|_| {
                let mut generator = SeededGenerator::new(42);
                (0..8).map(|_| generator.next_u64()).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(generated[0], generated[1]);

        let failure = check_int_property(7, 100, -100..=100, |value| value <= 10).unwrap_err();
        assert!(failure.original > 10);
        assert_eq!(failure.minimized, 11);
    }
}
