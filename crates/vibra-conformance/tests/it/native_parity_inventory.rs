//! The native parity inventory, `conformance/native-parity.tsv`, held against
//! the corpus (package X3-04; `docs/roadmap/nexus/decisions.md`, N6 and N14).
//!
//! The table has one row for each executable case and one cell for each native
//! executor. Only the `stepper` column is executable today, so it is checked
//! against what the reference stepper does with the real corpus, in both
//! directions; the platform columns are checked for form only. The checking
//! logic is also run on deliberately wrong tables, and through the runner
//! binary, which must exit non-zero.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    ConformanceRunner, Corpus, Inventory, InventoryCell, NativeStatus, RunReport,
    corpus_root, owning_package, standard_dispatcher, workspace_root,
};

fn table_path() -> PathBuf {
    workspace_root()
        .join("conformance")
        .join("native-parity.tsv")
}

fn table() -> Inventory {
    let text = std::fs::read_to_string(table_path()).expect("the committed inventory");
    Inventory::parse(&text).expect("the inventory parses")
}

/// The corpus run once for every test of this module.
fn report() -> &'static (Vec<String>, RunReport) {
    static REPORT: OnceLock<(Vec<String>, RunReport)> = OnceLock::new();
    REPORT.get_or_init(|| {
        let corpus = Corpus::discover(corpus_root()).expect("the corpus");
        let mut executable = corpus
            .cases()
            .iter()
            .filter(|case| case.manifest().is_executable())
            .map(|case| case.manifest().id.clone())
            .collect::<Vec<_>>();
        executable.sort();
        let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
        (executable, report)
    })
}

#[test]
fn the_table_names_every_executable_case_once() {
    let (executable, report) = report();
    assert_eq!(
        executable.len(),
        174,
        "the executable cases of the baseline"
    );
    let table = table();
    assert_eq!(table.problems(executable), Vec::<String>::new());

    // The stepper column is `matched` or `pending <package>`, and the
    // executor agrees with it in both directions.
    let statuses = report.native_statuses();
    assert_eq!(
        statuses
            .iter()
            .map(|(case, _)| case)
            .collect::<BTreeSet<_>>(),
        executable.iter().collect::<BTreeSet<_>>(),
        "the stepper has a status for exactly the executable cases"
    );
    let failed = statuses
        .iter()
        .filter(|(_, status)| matches!(status, NativeStatus::Failed { .. }))
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(table.stepper_disagreements(&statuses), Vec::<String>::new());

    // The counts the runner prints are the table's.
    let counts = report.native_counts();
    let matched = table
        .rows()
        .iter()
        .filter(|(_, cells)| cells[0] == InventoryCell::Matched)
        .count();
    assert_eq!(counts.matched, matched);
    assert_eq!(counts.matched + counts.pending, executable.len());
    assert_eq!(counts.failed, 0);
}

/// The identifiers of the packages the plan names as a heading or a table row.
fn plan_packages() -> BTreeSet<String> {
    let plan = std::fs::read_to_string(
        workspace_root()
            .join("docs")
            .join("roadmap")
            .join("nexus")
            .join("plan-v0.0.3.md"),
    )
    .expect("the plan");
    let mut found = BTreeSet::new();
    for line in plan.lines() {
        let scope = if line.starts_with("## ") {
            line
        } else if let Some(row) = line.strip_prefix('|') {
            row.split('|').next().unwrap_or("")
        } else {
            continue;
        };
        found.extend(package_ids(scope));
    }
    found
}

/// The package identifiers in `text`: `X`, a digit, `-`, and letters or digits.
fn package_ids(text: &str) -> Vec<String> {
    let characters = text.char_indices().collect::<Vec<_>>();
    let mut ids = Vec::new();
    for (position, (start, character)) in characters.iter().enumerate() {
        let digit = characters.get(position + 1).map(|(_, c)| *c);
        let dash = characters.get(position + 2).map(|(_, c)| *c);
        if *character != 'X'
            || !digit.is_some_and(|c| c.is_ascii_digit())
            || dash != Some('-')
        {
            continue;
        }
        let tail = text.get(*start..).unwrap_or("");
        let id = tail
            .chars()
            .enumerate()
            .take_while(|(index, c)| *index < 3 || c.is_ascii_alphanumeric())
            .map(|(_, c)| c)
            .collect::<String>();
        ids.push(id);
    }
    ids
}

#[test]
fn a_pending_cell_names_a_package_of_the_plan() {
    let packages = plan_packages();
    for expected in ["X3-04", "X3-10", "X3-18", "X3-19", "X3-09", "X3-S1"] {
        assert!(packages.contains(expected), "{expected} in {packages:?}");
    }
    let mut pending = 0;
    for (case, cells) in table().rows() {
        for cell in cells {
            if let InventoryCell::Pending(package) = cell {
                pending += 1;
                assert!(
                    packages.contains(package),
                    "{case}: `{package}` is not a package of the plan"
                );
            }
        }
    }
    assert!(pending > 0);
    // A pending cell that names nothing, or something that is not in the
    // plan, does not parse or is refused here.
    assert!(Inventory::parse("case\tstepper\twindows\tlinux\tmacos\nx\tpending\tmatched\tmatched\tmatched\n").is_err());
    assert!(!packages.contains("X3-99"));
}

#[test]
fn every_form_the_lowering_refuses_has_an_owning_package() {
    let packages = plan_packages();
    for name in vibra_lower::form::ALL {
        let owner =
            owning_package(name).unwrap_or_else(|| panic!("no owner for {name}"));
        assert!(packages.contains(owner), "{name}: {owner}");
    }
    assert_eq!(owning_package("a form nothing refuses"), None);
}

/// The first case the stepper is pending on, and one it matches.
fn one_pending_and_one_matched(statuses: &[(String, NativeStatus)]) -> (&str, &str) {
    let pending = statuses
        .iter()
        .find(|(_, status)| matches!(status, NativeStatus::Pending(_)))
        .expect("a pending case");
    let matched = statuses
        .iter()
        .find(|(_, status)| *status == NativeStatus::Matched)
        .expect("a matched case");
    (&pending.0, &matched.0)
}

fn set_stepper(table: &mut Inventory, case: &str, cell: InventoryCell) {
    let row = table
        .rows_mut()
        .iter_mut()
        .find(|(row, _)| row == case)
        .expect("a row");
    row.1[0] = cell;
}

#[test]
fn the_checking_logic_reports_a_wrong_cell_and_its_reason() {
    let (_, report) = report();
    let statuses = report.native_statuses();
    let (pending, matched) = one_pending_and_one_matched(&statuses);

    let mut wrong = table();
    set_stepper(&mut wrong, pending, InventoryCell::Matched);
    set_stepper(
        &mut wrong,
        matched,
        InventoryCell::Pending("X3-12".to_owned()),
    );
    let found = wrong.stepper_disagreements(&statuses);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().any(|line| line.starts_with(pending)
        && line.contains("marked matched, but the stepper is pending")));
    assert!(found.iter().any(|line| line.starts_with(matched)
        && line.contains("marked pending X3-12, but the stepper matches")));

    // A pending cell held by another package is also a disagreement.
    let mut moved = table();
    set_stepper(
        &mut moved,
        pending,
        InventoryCell::Pending("X3-19".to_owned()),
    );
    let other = moved.stepper_disagreements(&statuses);
    assert!(
        other.is_empty()
            == statuses.iter().any(|(case, status)| case == pending
                && *status == NativeStatus::Pending("X3-19".to_owned())),
        "{other:?}"
    );

    // A stepper that fails a case is a disagreement whatever the cell says.
    let mut failing = statuses.clone();
    for (case, status) in &mut failing {
        if case == matched {
            *status = NativeStatus::Failed {
                reason: "result snapshot mismatch".to_owned(),
            };
        }
    }
    let found = table().stepper_disagreements(&failing);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains(
            "marked matched, but the stepper fails: result snapshot mismatch"
        )
    );

    // The shape of the table: a missing row, an extra row, a duplicate, a
    // disorder.
    let (executable, _) = self::report();
    let mut shaped = table();
    shaped.rows_mut().remove(0);
    let first = executable[0].clone();
    assert!(
        shaped
            .problems(executable)
            .iter()
            .any(|line| line.contains(&first) && line.contains("no row"))
    );
    let mut extra = table();
    let template = extra.rows()[0].1.clone();
    extra
        .rows_mut()
        .push(("V1-ZZZ-no-such-case".to_owned(), template));
    assert!(
        extra
            .problems(executable)
            .iter()
            .any(|line| line.contains("not an executable case"))
    );
    let mut twice = table();
    let copy = twice.rows()[3].clone();
    twice.rows_mut().insert(3, copy);
    assert!(
        twice
            .problems(executable)
            .iter()
            .any(|line| line.contains("two rows"))
    );
    let mut unsorted = table();
    unsorted.rows_mut().swap(1, 2);
    assert!(
        unsorted
            .problems(executable)
            .iter()
            .any(|line| line.contains("out of order"))
    );
}

#[test]
fn the_cells_have_two_forms_and_no_third() {
    for bad in [
        "",
        "Matched",
        "pending",
        "pending ",
        "pending X3-04 extra",
        "skipped",
        "n/a",
    ] {
        let text = format!(
            "case\tstepper\twindows\tlinux\tmacos\nc\t{bad}\tmatched\tmatched\tmatched\n"
        );
        assert!(Inventory::parse(&text).is_err(), "`{bad}` parsed");
    }
    let good = "case\tstepper\twindows\tlinux\tmacos\nc\tmatched\tpending X3-09\tpending X3-09\tpending X3-09\n";
    let parsed = Inventory::parse(good).expect("two forms");
    assert_eq!(parsed.render(), good);
    assert!(Inventory::parse("case\tstepper\nc\tmatched\n").is_err());
    // The committed file is in the canonical form.
    let text = std::fs::read_to_string(table_path()).expect("the inventory");
    assert_eq!(table().render(), text);
    // The `linux` and `macos` columns have a package that moves them; the
    // `windows` column is executable and is held to the executables.
    for (case, cells) in table().rows() {
        assert!(
            cells[2..]
                .iter()
                .all(|cell| matches!(cell, InventoryCell::Pending(_))),
            "{case}"
        );
    }
}

// -- the windows column ------------------------------------------------------------

#[test]
fn the_windows_checking_logic_reports_a_wrong_cell_and_a_row_that_should_move() {
    let (_, report) = report();
    let statuses = report.native_statuses();
    let table = table();

    // A `pending` cell whose executable matches: the row should move.
    let pending_row = table
        .rows()
        .iter()
        .find(|(_, cells)| matches!(cells[1], InventoryCell::Pending(_)))
        .map(|(case, _)| case.clone())
        .expect("a row pending in windows");
    let found =
        table.windows_disagreements(&[(pending_row.clone(), NativeStatus::Matched)]);
    assert!(
        found.iter().any(|line| line.starts_with(&pending_row)
            && line.contains("but the windows executable matches")),
        "{found:?}"
    );

    // A `matched` cell whose executable fails, or is pending.
    let matched = statuses
        .iter()
        .find(|(_, status)| *status == NativeStatus::Matched)
        .map(|(case, _)| case.clone())
        .expect("a matched case");
    let mut wrong = self::table();
    wrong
        .rows_mut()
        .iter_mut()
        .find(|(row, _)| *row == matched)
        .expect("a row")
        .1[1] = InventoryCell::Matched;
    let failing = [(
        matched.clone(),
        NativeStatus::Failed {
            reason: "audit mismatch".to_owned(),
        },
    )];
    let found = wrong.windows_disagreements(&failing);
    assert!(
        found.iter().any(|line| line.contains(
            "marked matched, but the windows executable fails: audit mismatch"
        )),
        "{found:?}"
    );
    let found = wrong.windows_disagreements(&[(
        matched.clone(),
        NativeStatus::Pending("X3-09".to_owned()),
    )]);
    assert!(
        found.iter().any(|line| line
            .contains("marked matched, but the windows executable is pending X3-09")),
        "{found:?}"
    );

    // A row may be `matched` in `windows` only if it is `matched` in `stepper`.
    let pending = statuses
        .iter()
        .find(|(_, status)| matches!(status, NativeStatus::Pending(_)))
        .map(|(case, _)| case.clone())
        .expect("a pending case");
    let mut forged = self::table();
    forged
        .rows_mut()
        .iter_mut()
        .find(|(row, _)| *row == pending)
        .expect("a row")
        .1[1] = InventoryCell::Matched;
    assert!(
        forged
            .windows_disagreements(&[])
            .contains(&format!("{pending}: matched in windows but not in stepper"))
    );
}

#[cfg(windows)]
#[test]
fn every_windows_row_matches_its_linked_executable() {
    use vibra_conformance::WindowsExecutor;

    use crate::linked_exe_support::{Profile, archive};

    let work = workspace_root().join("target").join("x3-09-windows-test");
    let executor = WindowsExecutor::new(archive(Profile::Harness).to_path_buf(), work)
        .unwrap_or_else(|error| panic!("{error}"));
    let corpus = Corpus::discover(corpus_root()).expect("the corpus");
    let started = std::time::Instant::now();
    let report = ConformanceRunner::new(standard_dispatcher())
        .with_windows(executor)
        .run(&corpus);
    eprintln!("windows column: {:?}", started.elapsed());
    let statuses = report.windows_statuses();
    let failed = statuses
        .iter()
        .filter(|(_, status)| matches!(status, NativeStatus::Failed { .. }))
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(report.failed(), 0);
    // Both directions: a `matched` cell must match, and a `pending` cell whose
    // case matches is a row that should move. A row is `matched` in `windows`
    // only if it is `matched` in `stepper`.
    let table = table();
    assert_eq!(table.windows_disagreements(&statuses), Vec::<String>::new());
    let counts = report.windows_counts();
    let matched = table
        .rows()
        .iter()
        .filter(|(_, cells)| cells[1] == InventoryCell::Matched)
        .count();
    assert_eq!(counts.matched, matched);
    assert!(matched > 0);
    assert_eq!(counts.failed, 0);
}

// -- the runner binary ---------------------------------------------------------------

fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("a case directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// A corpus of one reader case and the named cases, with `rows` as its
/// inventory, and the output of the runner binary over it.
fn run_binary(cases: &[&str], rows: &[(&str, &str)]) -> (bool, String) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "vibra-native-parity-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("cases");
    for case in ["V1-SRC-READER-data-loader-extension"]
        .into_iter()
        .chain(cases.iter().copied())
    {
        copy_directory(&corpus_root().join(case), &root.join(case));
    }
    let mut text = String::from("case\tstepper\twindows\tlinux\tmacos\n");
    for (case, stepper) in rows {
        text.push_str(&format!(
            "{case}\t{stepper}\tpending X3-09\tpending X3-09\tpending X3-09\n"
        ));
    }
    std::fs::write(base.join("native-parity.tsv"), text).expect("the table");
    let output = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .arg("--root")
        .arg(&root)
        .output()
        .expect("the binary runs");
    let _ = std::fs::remove_dir_all(&base);
    (
        output.status.success(),
        String::from_utf8(output.stdout).expect("utf-8"),
    )
}

#[test]
fn a_matched_cell_that_fails_fails_the_run() {
    let (_, report) = report();
    let statuses = report.native_statuses();
    let (pending, matched) = one_pending_and_one_matched(&statuses);

    // The right table: the run passes, and prints the native line.
    let (ok, stdout) = run_binary(&[matched], &[(matched, "matched")]);
    assert!(ok, "{stdout}");
    assert!(
        stdout.contains("native backend: stepper 1 matched, 0 pending, 0 failed\n"),
        "{stdout}"
    );

    // A case the stepper cannot run, marked matched: the run fails and names
    // the case and the reason.
    let (ok, stdout) = run_binary(&[pending], &[(pending, "matched")]);
    assert!(!ok, "{stdout}");
    assert!(
        stdout.contains(&format!(
            "NATIVE {pending}: marked matched, but the stepper is pending"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("native backend: stepper 0 matched, 1 pending, 1 failed\n"),
        "{stdout}"
    );
    // The interpreter and Wasm lines are untouched by it.
    assert!(
        stdout.contains("interpreter backend: 1 passed, 0 failed, 0 unavailable\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("wasm backend: 1 matched, 0 failed\n"),
        "{stdout}"
    );
}

#[test]
fn a_cell_that_matches_but_is_marked_pending_fails() {
    let (_, report) = report();
    let statuses = report.native_statuses();
    let (pending, matched) = one_pending_and_one_matched(&statuses);

    let (ok, stdout) = run_binary(&[matched], &[(matched, "pending X3-12")]);
    assert!(!ok, "{stdout}");
    assert!(
        stdout.contains(&format!(
            "NATIVE {matched}: marked pending X3-12, but the stepper matches"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("native backend: stepper 1 matched, 0 pending, 1 failed\n"),
        "{stdout}"
    );

    // A pending cell the stepper agrees with does not fail the run: a gap is
    // not a pass, and it fails the Milestone 2 gate, not this one.
    let package = match statuses.iter().find(|(case, _)| case == pending) {
        Some((_, NativeStatus::Pending(package))) => package.clone(),
        other => panic!("{other:?}"),
    };
    let (ok, stdout) =
        run_binary(&[pending], &[(pending, &format!("pending {package}"))]);
    assert!(ok, "{stdout}");
    assert!(
        stdout.contains("native backend: stepper 0 matched, 1 pending, 0 failed\n"),
        "{stdout}"
    );
}
