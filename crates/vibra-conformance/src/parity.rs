//! The parity inventory of the differential harness.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Differential execution": a
//! checked-in table has one row for every executable case, giving its
//! disposition, which is `matched`, or `not-lowered` with the implementation
//! step that owns the form the case uses. A host test fails when an executable
//! case has no row, when a row names no case, when a not-lowered row names no
//! step, and when the runner's result for a case disagrees with its
//! disposition. This module reads the table and states those rules; the test
//! that applies them to the real corpus is
//! `tests/parity_inventory_m4_step4.rs`.

use std::collections::BTreeMap;
use std::fmt;

use crate::runner::WasmStatus;

/// The steps of Stage 4A that can own a not-lowered case, in delivery order.
pub const OWNING_STEPS: &[&str] =
    &["5a", "5b", "6", "7", "8a", "8b", "8c", "9", "10", "11"];

/// One case's disposition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// The WebAssembly backend reproduces the case's one expectation, or no
    /// backend had a program to run because checking rejected it.
    Matched,
    /// The program uses a form the backend does not lower yet.
    NotLowered {
        /// The step whose delivery lets the case match.
        step: String,
    },
}

impl fmt::Display for Disposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Matched => formatter.write_str("matched"),
            Self::NotLowered { step } => write!(formatter, "not-lowered (step {step})"),
        }
    }
}

/// A table that is not a well-formed inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParityError {
    line: usize,
    message: String,
}

impl ParityError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "parity inventory line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for ParityError {}

/// What the runner observed for an executable case that has no row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observed {
    /// The case matched.
    Matched,
    /// The case did not lower, because of these forms.
    NotLowered(Vec<String>),
}

/// An executable case that has no row, with what the runner observed, so that
/// the row to add can be written exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingRow {
    /// The case that needs a row.
    pub case_id: String,
    /// What the runner observed.
    pub observed: Observed,
}

/// Every way the inventory disagrees with a run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Findings {
    /// Rows that disagree with the run, rows that name no executable case, and
    /// cases on which the Wasm backend failed.
    pub problems: Vec<String>,
    /// Executable cases that have no row.
    pub missing: Vec<MissingRow>,
}

impl Findings {
    /// Whether the inventory agrees with the run.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.problems.is_empty() && self.missing.is_empty()
    }
}

/// The checked-in table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParityInventory {
    rows: BTreeMap<String, Disposition>,
}

impl ParityInventory {
    /// Reads a table: `#` starts a comment line, blank lines are ignored, and a
    /// row is `case-id`, a tab, and `matched`, or `case-id`, a tab,
    /// `not-lowered`, a tab, and an owning step.
    ///
    /// # Errors
    ///
    /// The first malformed row: a column count that does not fit, a
    /// disposition that is neither spelling, a not-lowered row that names no
    /// step or a step outside [`OWNING_STEPS`], a matched row that names a
    /// step, or a case that has two rows.
    pub fn parse(text: &str) -> Result<Self, ParityError> {
        let mut rows = BTreeMap::new();
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let columns = line.split('\t').collect::<Vec<_>>();
            let (case_id, disposition) = match columns.as_slice() {
                [case_id, "matched"] => ((*case_id).to_owned(), Disposition::Matched),
                [_, "matched", step, ..] => {
                    return Err(ParityError::new(
                        number,
                        format!("a matched row names no step, found `{step}`"),
                    ));
                }
                [_, "not-lowered"] => {
                    return Err(ParityError::new(
                        number,
                        format!(
                            "a not-lowered row must name its owning step, one of {OWNING_STEPS:?}"
                        ),
                    ));
                }
                [case_id, "not-lowered", step] if OWNING_STEPS.contains(step) => (
                    (*case_id).to_owned(),
                    Disposition::NotLowered {
                        step: (*step).to_owned(),
                    },
                ),
                [_, "not-lowered", step] => {
                    return Err(ParityError::new(
                        number,
                        format!(
                            "`{step}` is not an owning step; expected one of {OWNING_STEPS:?}"
                        ),
                    ));
                }
                [_, other, ..] => {
                    return Err(ParityError::new(
                        number,
                        format!(
                            "disposition `{other}` is neither `matched` nor `not-lowered`, or the row has too many columns"
                        ),
                    ));
                }
                _ => {
                    return Err(ParityError::new(
                        number,
                        "a row is a case id, a tab, and a disposition",
                    ));
                }
            };
            if case_id.is_empty() {
                return Err(ParityError::new(number, "a row names no case"));
            }
            if rows.insert(case_id.clone(), disposition).is_some() {
                return Err(ParityError::new(
                    number,
                    format!("case `{case_id}` has two rows"),
                ));
            }
        }
        Ok(Self { rows })
    }

    /// The disposition of a case, if it has a row.
    #[must_use]
    pub fn disposition(&self, case_id: &str) -> Option<&Disposition> {
        self.rows.get(case_id)
    }

    /// Every row in case-id order.
    pub fn rows(&self) -> impl Iterator<Item = (&str, &Disposition)> {
        self.rows
            .iter()
            .map(|(case_id, row)| (case_id.as_str(), row))
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Holds the table to a run: `executable` is every executable case of the
    /// corpus with the Wasm status the runner reported for it, or `None` when
    /// the case never reached the backend.
    #[must_use]
    pub fn check<'a>(
        &self,
        executable: impl IntoIterator<Item = (&'a str, Option<&'a WasmStatus>)>,
    ) -> Findings {
        let mut findings = Findings::default();
        let mut seen = std::collections::BTreeSet::new();
        for (case_id, status) in executable {
            seen.insert(case_id);
            let Some(status) = status else {
                findings.problems.push(format!(
                    "executable case `{case_id}` never reached the Wasm backend"
                ));
                continue;
            };
            if let WasmStatus::Failed { reason } = status {
                findings.problems.push(format!(
                    "executable case `{case_id}`: the Wasm backend disagrees: {reason}"
                ));
                continue;
            }
            let observed = match status {
                WasmStatus::Matched => Observed::Matched,
                WasmStatus::NotLowered { forms } => Observed::NotLowered(forms.clone()),
                WasmStatus::Failed { .. } => continue,
            };
            match (self.rows.get(case_id), &observed) {
                (None, _) => findings.missing.push(MissingRow {
                    case_id: case_id.to_owned(),
                    observed,
                }),
                (Some(Disposition::Matched), Observed::Matched)
                | (Some(Disposition::NotLowered { .. }), Observed::NotLowered(_)) => {}
                (Some(row @ Disposition::Matched), Observed::NotLowered(forms)) => {
                    findings.problems.push(format!(
                        "case `{case_id}` is {row} but no longer lowers: {}; a step moves rows toward matched and never back",
                        forms.join(", ")
                    ));
                }
                (Some(row @ Disposition::NotLowered { .. }), Observed::Matched) => {
                    findings.problems.push(format!(
                        "case `{case_id}` is {row} but now matches; change its row to `matched`"
                    ));
                }
            }
        }
        for case_id in self.rows.keys() {
            if !seen.contains(case_id.as_str()) {
                findings.problems.push(format!(
                    "row `{case_id}` names no executable case of the corpus"
                ));
            }
        }
        findings
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    fn not_lowered(forms: &[&str]) -> WasmStatus {
        WasmStatus::NotLowered {
            forms: forms.iter().map(|form| (*form).to_owned()).collect(),
        }
    }

    #[test]
    fn a_table_parses_comments_blank_lines_and_both_dispositions() {
        let inventory = ParityInventory::parse(
            "# header\n\nA\tmatched\nB\tnot-lowered\t5b\nC\tnot-lowered\t8a\n",
        )
        .expect("a well-formed table");
        assert_eq!(inventory.len(), 3);
        assert_eq!(inventory.disposition("A"), Some(&Disposition::Matched));
        assert_eq!(
            inventory.disposition("B"),
            Some(&Disposition::NotLowered {
                step: "5b".to_owned()
            })
        );
        assert_eq!(inventory.disposition("D"), None);
    }

    #[test]
    fn a_not_lowered_row_must_name_a_known_step() {
        let missing = ParityInventory::parse("A\tnot-lowered\n").expect_err("no step");
        assert!(missing.to_string().contains("owning step"), "{missing}");
        let unknown =
            ParityInventory::parse("A\tnot-lowered\t99\n").expect_err("unknown step");
        assert!(
            unknown.to_string().contains("not an owning step"),
            "{unknown}"
        );
        let blank =
            ParityInventory::parse("A\tnot-lowered\t\n").expect_err("blank step");
        assert!(blank.to_string().contains("not an owning step"), "{blank}");
    }

    #[test]
    fn malformed_rows_are_reported_with_their_line() {
        for (text, needle) in [
            ("A\tmatched\t5b\n", "names no step"),
            ("A\tmaybe\n", "neither"),
            ("A\n", "case id, a tab"),
            ("\tmatched\n", "names no case"),
            ("A\tmatched\nA\tmatched\n", "two rows"),
        ] {
            let error = ParityInventory::parse(text).expect_err(text);
            assert!(error.to_string().contains(needle), "{text:?}: {error}");
            assert!(error.to_string().contains("line"), "{error}");
        }
    }

    #[test]
    fn a_case_with_no_row_is_missing_and_says_what_to_add() {
        let inventory = ParityInventory::parse("").expect("empty");
        let matched = WasmStatus::Matched;
        let lowering = not_lowered(&["call:direct", "literal"]);
        let findings = inventory.check([("A", Some(&matched)), ("B", Some(&lowering))]);
        assert!(findings.problems.is_empty());
        assert_eq!(
            findings.missing,
            vec![
                MissingRow {
                    case_id: "A".to_owned(),
                    observed: Observed::Matched
                },
                MissingRow {
                    case_id: "B".to_owned(),
                    observed: Observed::NotLowered(vec![
                        "call:direct".to_owned(),
                        "literal".to_owned()
                    ])
                },
            ]
        );
    }

    #[test]
    fn a_row_that_names_no_case_is_reported() {
        let inventory =
            ParityInventory::parse("A\tmatched\nGone\tmatched\n").expect("table");
        let matched = WasmStatus::Matched;
        let findings = inventory.check([("A", Some(&matched))]);
        assert_eq!(findings.problems.len(), 1, "{findings:?}");
        assert!(findings.problems[0].contains("`Gone` names no executable case"));
    }

    #[test]
    fn a_matched_row_on_a_case_that_no_longer_lowers_is_reported() {
        let inventory = ParityInventory::parse("A\tmatched\n").expect("table");
        let regressed = not_lowered(&["match"]);
        let findings = inventory.check([("A", Some(&regressed))]);
        assert!(findings.problems[0].contains("never back"), "{findings:?}");
    }

    #[test]
    fn a_not_lowered_row_on_a_case_that_now_matches_is_reported() {
        let inventory = ParityInventory::parse("A\tnot-lowered\t5b\n").expect("table");
        let matched = WasmStatus::Matched;
        let findings = inventory.check([("A", Some(&matched))]);
        assert!(findings.problems[0].contains("now matches"), "{findings:?}");
    }

    #[test]
    fn a_failed_wasm_backend_is_reported_whatever_the_row_says() {
        let inventory =
            ParityInventory::parse("A\tmatched\nB\tnot-lowered\t6\n").expect("table");
        let failed = WasmStatus::Failed {
            reason: "result snapshot mismatch".to_owned(),
        };
        let findings = inventory.check([("A", Some(&failed)), ("B", Some(&failed))]);
        assert_eq!(findings.problems.len(), 2, "{findings:?}");
        assert!(findings.problems.iter().all(|p| p.contains("disagrees")));
    }

    #[test]
    fn a_case_that_never_reached_the_backend_is_reported() {
        let inventory = ParityInventory::parse("A\tmatched\n").expect("table");
        let findings = inventory.check([("A", None)]);
        assert!(
            findings.problems[0].contains("never reached"),
            "{findings:?}"
        );
    }

    #[test]
    fn an_agreeing_table_has_no_findings() {
        let inventory =
            ParityInventory::parse("A\tmatched\nB\tnot-lowered\t6\n").expect("table");
        let matched = WasmStatus::Matched;
        let lowering = not_lowered(&["closure"]);
        assert!(
            inventory
                .check([("A", Some(&matched)), ("B", Some(&lowering))])
                .is_empty()
        );
    }
}
