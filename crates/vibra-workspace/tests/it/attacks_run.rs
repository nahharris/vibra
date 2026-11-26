//! The attack rows that need a real filesystem, each run through a whole
//! target under `Launch::System` only (07 Required coverage 11). Each row
//! writes its world with [`materialize`], builds its objects (links,
//! junctions, hard links, aliases) in the same temporary directory, and runs
//! the target. A refusal is judged by the directory, never by the reply
//! alone: the test lists the temporary directory recursively, without
//! following a link, and reads every file's bytes, before and after the run,
//! and compares the two.
//!
//! Windows only: the directory driver exists there. Elsewhere every row
//! prints `NOT RUN`. A row that needs an object the machine cannot make (a
//! file symbolic link without Developer Mode, a volume with no 8.3 names)
//! prints `NOT RUN` with the reason instead of passing.

#[cfg(windows)]
mod windows {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::windows::process::CommandExt as _;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use vibra_workspace::run::RunOutcome;

    use crate::dual_run::{
        ANY_NAME, Materialized, Observed, STDOUT, TXT, copy_default, copy_program,
        copy_project, file_rule, materialize, project, read_code, run_system, uses,
        world,
    };
    use crate::support::{TempDir, link_directory};

    /// One entry of a directory listing.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Entry {
        Directory,
        File(Vec<u8>),
        /// A symbolic link or a junction, by its target; never followed.
        Link(PathBuf),
    }

    /// The recursive listing of `top`, every file's bytes included, links
    /// recorded and not followed. A loop over a stack, not recursion.
    fn snapshot(top: &Path) -> BTreeMap<PathBuf, Entry> {
        let mut entries = BTreeMap::new();
        let mut pending = vec![top.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for child in fs::read_dir(&directory).expect("list a directory") {
                let path = child.expect("an entry").path();
                let kind = fs::symlink_metadata(&path).expect("metadata").file_type();
                let entry = if kind.is_symlink() {
                    Entry::Link(fs::read_link(&path).expect("a link's target"))
                } else if kind.is_dir() {
                    pending.push(path.clone());
                    Entry::Directory
                } else {
                    Entry::File(fs::read(&path).expect("a file's bytes"))
                };
                let relative =
                    path.strip_prefix(top).expect("below the top").to_path_buf();
                entries.insert(relative, entry);
            }
        }
        entries
    }

    /// A row's temporary directory: the world under `world/`, the objects
    /// outside the grant under `outside/`, the tree under `tree/`.
    struct Attack {
        directory: TempDir,
        materialized: Materialized,
        root: PathBuf,
        outside: PathBuf,
    }

    impl Attack {
        fn new(world_text: &str) -> Self {
            let directory = TempDir::new("attack");
            let materialized =
                materialize(&world(world_text), &directory.join("world"))
                    .expect("the world materializes");
            let root = materialized.root.clone();
            let outside = directory.join("outside");
            fs::create_dir(&outside).expect("the outside directory");
            Self {
                directory,
                materialized,
                root,
                outside,
            }
        }

        /// What the run can touch: the world and the outside objects.
        fn state(&self) -> (BTreeMap<PathBuf, Entry>, BTreeMap<PathBuf, Entry>) {
            (snapshot(&self.root), snapshot(&self.outside))
        }

        /// Runs the target and returns its outcome with the directory state
        /// before and after.
        fn run(self, document: &str, main: &str) -> Ran {
            self.run_during(document, main, |_| {})
        }

        /// [`Self::run`], with `race` running on another thread for the whole
        /// run and only then: it starts after the state before is taken, and
        /// it is told to stop, and has stopped, before the state after is.
        fn run_during(
            self,
            document: &str,
            main: &str,
            race: impl FnOnce(&AtomicBool) + Send,
        ) -> Ran {
            /// Tells the racing thread to stop, even when the run panics.
            struct Stop<'s>(&'s AtomicBool);
            impl Drop for Stop<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Relaxed);
                }
            }
            let before = self.state();
            let tree = self.directory.join("tree");
            let root = self.root.clone();
            let outside = self.outside.clone();
            let stop = AtomicBool::new(false);
            let (outcome, observed) = std::thread::scope(|scope| {
                let racing = scope.spawn(|| race(&stop));
                let _stop = Stop(&stop);
                let ran = run_system(&tree, document, main, self.materialized);
                drop(_stop);
                racing.join().expect("the racing thread");
                ran
            });
            let after = (snapshot(&root), snapshot(&outside));
            Ran {
                outcome,
                observed,
                before,
                after,
                root,
                _directory: self.directory,
            }
        }
    }

    struct Ran {
        outcome: RunOutcome,
        observed: Observed,
        before: (BTreeMap<PathBuf, Entry>, BTreeMap<PathBuf, Entry>),
        after: (BTreeMap<PathBuf, Entry>, BTreeMap<PathBuf, Entry>),
        root: PathBuf,
        _directory: TempDir,
    }

    impl Ran {
        fn code(&self) -> Option<u8> {
            self.outcome.application_code
        }

        fn stdout(&self) -> String {
            String::from_utf8_lossy(&self.observed.stdout).into_owned()
        }

        /// The directory is as it was: the world and the outside objects,
        /// every entry and every byte.
        fn assert_unchanged(&self, row: &str) {
            assert_eq!(self.before.0, self.after.0, "{row}: the world changed");
            assert_eq!(self.before.1, self.after.1, "{row}: the outside changed");
        }

        /// The number of completed host calls of `symbol` whose result is
        /// the error `case`.
        fn completed(&self, symbol: &str, case: &str) -> usize {
            self.outcome
                .trace
                .events
                .iter()
                .filter(|event| match event {
                    vibra_host::trace::Event::Host(event) => {
                        event.op.symbol() == symbol && event.result.contains(case)
                    }
                    _ => false,
                })
                .count()
        }

        fn passed(&self, row: &str, what: &str) {
            println!("ROW {row}: windows, passed: {what}");
        }
    }

    /// A file symbolic link at `link` to `target`, or why the machine cannot
    /// make one.
    fn symlink_file(target: &Path, link: &Path) -> Result<(), String> {
        std::os::windows::fs::symlink_file(target, link).map_err(|error| {
            format!(
                "no file symbolic link (Developer Mode or elevation is needed): {error}"
            )
        })
    }

    /// The 8.3 short name of the file at `path`, when the volume made one.
    fn short_name(path: &Path) -> Option<String> {
        let plain = path.display().to_string();
        let plain = plain.strip_prefix(r"\\?\").unwrap_or(&plain).to_owned();
        // cmd reads its command line itself: pass it unquoted.
        let output = std::process::Command::new("cmd")
            .raw_arg(format!("/C for %I in (\"{plain}\") do @echo %~snxI"))
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let long = path.file_name()?.to_string_lossy().into_owned();
        let plain_name = name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "~._-".contains(c));
        (output.status.success() && plain_name && !name.is_empty() && name != long)
            .then_some(name)
    }

    const SECRET: &str = "OUTSIDE-SECRET";

    /// The world of the copy rows: `/in` and `/out`, and `extra` files.
    fn copy_world(files: &str, args: &str) -> String {
        format!(
            "{{format: @hostWorld.v2 directories: [\"/in\" \"/out\"] files: [{files}] \
             args: [{args}]}}"
        )
    }

    #[test]
    fn atk_run_symlink_denied() {
        let row = "ATK-run-symlink-denied";
        let attack = Attack::new(&copy_world(":", "\"/in\" \"l.txt\""));
        let secret = attack.outside.join("secret.txt");
        fs::write(&secret, SECRET).expect("the outside file");
        if let Err(reason) =
            symlink_file(&secret, &attack.root.join("in").join("l.txt"))
        {
            println!("NOT RUN {row}: {reason}");
            return;
        }
        let ran = attack.run(&copy_default(), &copy_program());
        assert_eq!(ran.code(), Some(32), "{row}: the entry sees denied");
        assert!(
            !ran.stdout().contains(SECRET),
            "{row}: stdout {}",
            ran.stdout()
        );
        assert!(
            !ran.root.join("out").join("l.txt").exists(),
            "{row}: no copy"
        );
        assert_eq!(
            ran.completed("fs.readText", "denied"),
            1,
            "{row}: a completed event"
        );
        ran.assert_unchanged(row);
        ran.passed(
            row,
            "denied, a completed event; the outside text appears nowhere",
        );
    }

    #[test]
    fn atk_run_junction_denied() {
        let row = "ATK-run-junction-denied";
        let attack = Attack::new(&copy_world(":", "\"/in\" \"sub\" \"a.txt\""));
        let target = attack.outside.join("sub");
        fs::create_dir(&target).expect("the outside directory");
        fs::write(target.join("a.txt"), SECRET).expect("the outside file");
        link_directory(&target, &attack.root.join("in").join("sub"));
        let ran = attack.run(&copy_default(), &copy_program());
        assert_eq!(ran.code(), Some(32), "{row}: denied");
        assert!(
            !ran.stdout().contains(SECRET),
            "{row}: stdout {}",
            ran.stdout()
        );
        assert!(!ran.root.join("out").join("sub").exists(), "{row}: no copy");
        ran.assert_unchanged(row);
        ran.passed(row, "denied through a policy with depth @tree");
    }

    #[test]
    fn atk_run_alias_denied() {
        let row = "ATK-run-alias-denied";
        let mut attack = Attack::new(&copy_world(
            &format!("\"/in/longsecretname.dat\": \"{SECRET}\""),
            "\"/in\"",
        ));
        let Some(short) =
            short_name(&attack.root.join("in").join("longsecretname.dat"))
        else {
            println!("NOT RUN {row}: the volume made no 8.3 name");
            return;
        };
        attack.materialized.arguments.push(short.clone().into());
        let document = copy_project(
            "",
            &format!("{{kind: @exact names: [\"{short}\"]}}"),
            "",
            "out",
            "@create @replace",
        );
        let ran = attack.run(&document, &copy_program());
        assert_eq!(ran.code(), Some(32), "{row}: denied");
        assert_eq!(
            ran.completed("fs.readText", "denied"),
            1,
            "{row}: a completed event"
        );
        assert!(
            !ran.stdout().contains(SECRET),
            "{row}: stdout {}",
            ran.stdout()
        );
        ran.assert_unchanged(row);
        ran.passed(row, &format!("{short} denied, a completed event"));
    }

    #[test]
    fn atk_run_hard_link_read_allowed() {
        let row = "ATK-run-hard-link-read-allowed";
        let attack = Attack::new(&copy_world(":", "\"/in\" \"a.txt\""));
        let secret = attack.outside.join("data.txt");
        fs::write(&secret, "linked data").expect("the outside file");
        fs::hard_link(&secret, attack.root.join("in").join("a.txt"))
            .expect("a hard link");
        let ran = attack.run(&copy_default(), &copy_program());
        assert_eq!(ran.code(), Some(0), "{row}: the read succeeds");
        assert_eq!(ran.stdout(), "linked data");
        let copied = fs::read(ran.root.join("out").join("a.txt")).expect("the copy");
        assert_eq!(copied, b"linked data");
        ran.passed(row, "the read succeeds and the copy holds the linked data");
    }

    #[test]
    fn atk_run_hard_link_replace_denied() {
        let row = "ATK-run-hard-link-replace-denied";
        let attack = Attack::new(&copy_world(
            "\"/in/a.txt\": \"new text\"",
            "\"/in\" \"a.txt\"",
        ));
        let target = attack.outside.join("target.txt");
        fs::write(&target, "outside original").expect("the outside file");
        fs::hard_link(&target, attack.root.join("out").join("a.txt"))
            .expect("a hard link");
        let ran = attack.run(&copy_default(), &copy_program());
        assert_eq!(ran.code(), Some(42), "{row}: the write is denied");
        assert_eq!(
            ran.completed("fs.writeText", "denied"),
            1,
            "{row}: a completed event"
        );
        assert_eq!(fs::read(&target).expect("the target"), b"outside original");
        ran.assert_unchanged(row);
        ran.passed(
            row,
            "denied; the outside file and the directory are unchanged",
        );
    }

    #[test]
    fn atk_run_denied_leaves_file() {
        let row = "ATK-run-denied-leaves-file";
        let attack = Attack::new(&copy_world(
            "\"/in/a.txt\": \"new text\" \"/out/a.txt\": \"old text\"",
            "\"/in\" \"a.txt\"",
        ));
        let document = copy_project("", TXT, "", "out", "@create");
        let ran = attack.run(&document, &copy_program());
        assert_eq!(ran.code(), Some(42), "{row}: the replace is denied");
        assert_eq!(
            fs::read(ran.root.join("out").join("a.txt")).expect("the file"),
            b"old text"
        );
        ran.assert_unchanged(row);
        ran.passed(row, "denied; the existing file is untouched");
    }

    #[test]
    fn atk_run_create_denied_leaves_directory() {
        let row = "ATK-run-create-denied-leaves-directory";
        let attack =
            Attack::new("{format: @hostWorld.v2 directories: [\"/dest/out\"]}");
        let document = project(
            "@std.fs.DirWriteText",
            &file_rule(
                "@dest",
                "{kind: @fixedPath path: \"dest\"}",
                "\"OUT\"",
                "@create",
                ANY_NAME,
                "after: []",
            ),
        );
        let main = format!(
            "{}{}
proc main
  takes {{dest: DirWriteText}}
  returns code: ExitCode
  performs {{DirWriteText}}
  do
    let Result.ok(value: path) be RelativePath([\"OUT\" \"n.txt\"]) endlet
    match await DirWriteText.write(dest path \"x\")
      case Result.err(error: error)
        return ExitCode(U8.add(10u8 readCode(error)))
      case _
        return ExitCode(0u8)
    endmatch
endproc
",
            uses(&[
                "@std.fs.DirWriteText",
                "@std.fs.FsError",
                "@std.fs.RelativePath"
            ]),
            read_code()
        );
        let ran = attack.run(&document, &main);
        assert_eq!(ran.code(), Some(42), "{row}: the create is denied");
        assert_eq!(
            ran.completed("fs.writeText", "denied"),
            1,
            "{row}: a completed event"
        );
        let out = fs::read_dir(ran.root.join("dest").join("out"))
            .expect("out")
            .count();
        assert_eq!(out, 0, "{row}: `out` holds no entry");
        ran.assert_unchanged(row);
        ran.passed(row, "denied, a completed event; `out` holds no entry");
    }

    /// `R`: once startup holds `in`, writes `ctl/ready` and polls `ctl/go`
    /// (at most 20,000 reads); then reads `a.txt` through `source` and writes
    /// it. Codes: 0 read; 3 no `go`; 4 `ready` not written; a read error its
    /// `readCode`.
    fn read_after_handshake() -> (String, String) {
        let ctl = "{kind: @fixedPath path: \"ctl\"}";
        let document = project(
            "@std.fs.DirReadText @std.fs.DirWriteText @std.io.StdoutText",
            &format!(
                "{} {} @out: {STDOUT} {}",
                file_rule("@ctlRead", ctl, "", "@read", ANY_NAME, "after: []"),
                file_rule("@ctlWrite", ctl, "", "@create", ANY_NAME, "after: []"),
                file_rule(
                    "@source",
                    "{kind: @fixedPath path: \"in\"}",
                    "",
                    "@read",
                    ANY_NAME,
                    "after: []"
                )
            ),
        );
        let main = format!(
            "{}{}
proc wait
  takes {{ctlRead: DirReadText go: RelativePath left: U32}}
  returns seen: Bool
  performs {{DirReadText}}
  do
    match await DirReadText.read(ctlRead go)
      case Result.ok(value: _)
        return true
      case _
        match left
          case 0u32
            return false
          case _
            return await wait(ctlRead go U32.sub(left 1u32))
        endmatch
    endmatch
endproc

proc main
  takes {{ctlRead: DirReadText ctlWrite: DirWriteText out: StdoutText source: DirReadText}}
  returns code: ExitCode
  performs {{DirReadText DirWriteText StdoutText}}
  do
    let Result.ok(value: ready) be RelativePath([\"ready\"]) endlet
    let Result.ok(value: go) be RelativePath([\"go\"]) endlet
    let Result.ok(value: path) be RelativePath([\"a.txt\"]) endlet
    let Result.ok(value: _) be await DirWriteText.write(ctlWrite ready \"1\") else
      return ExitCode(4u8)
    endlet
    match await wait(ctlRead go 20000u32)
      case false
        return ExitCode(3u8)
      case _
        match await DirReadText.read(source path)
          case Result.ok(value: text)
            let _ be await StdoutText.write(out text) endlet
            return ExitCode(0u8)
          case Result.err(error: error)
            return ExitCode(readCode(error))
        endmatch
    endmatch
endproc
",
            uses(&[
                "@std.fs.DirReadText",
                "@std.fs.DirWriteText",
                "@std.fs.FsError",
                "@std.fs.RelativePath",
                "@std.io.StdoutText",
            ]),
            read_code()
        );
        (document, main)
    }

    #[test]
    fn atk_run_root_replaced() {
        let row = "ATK-run-root-replaced";
        let attack = Attack::new(
            "{format: @hostWorld.v2 directories: [\"/ctl\"] files: [\"/in/a.txt\": \"original\"]}",
        );
        let root = attack.root.clone();
        let replaced = std::sync::Mutex::new(None);
        let (document, main) = read_after_handshake();
        // Between startup, which holds `in`, and the entry's read: rename `in`
        // and put another directory at its path, then let the entry go on.
        let ran = attack.run_during(&document, &main, |stop| {
            let ctl = root.join("ctl");
            while !ctl.join("ready").exists() {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::yield_now();
            }
            let replace = || -> std::io::Result<()> {
                fs::rename(root.join("in"), root.join("inOld"))?;
                fs::create_dir(root.join("in"))?;
                fs::write(root.join("in").join("a.txt"), "replacement")
            };
            let result = replace().map_err(|error| error.to_string());
            *replaced.lock().expect("the result") = Some(result);
            fs::write(ctl.join("go"), "1").expect("the go file");
        });
        let replaced = replaced.into_inner().expect("the result");
        assert_eq!(replaced, Some(Ok(())), "{row}: the root was replaced");
        assert_eq!(ran.code(), Some(0), "{row}");
        assert_eq!(ran.stdout(), "original", "{row}: the renamed directory");
        assert_eq!(
            fs::read(ran.root.join("in").join("a.txt")).expect("the new file"),
            b"replacement"
        );
        ran.passed(row, "the entry read from the renamed directory");
    }

    #[test]
    fn atk_run_prefix_by_names() {
        let row = "ATK-run-prefix-by-names";
        let attack = Attack::new(&copy_world(
            &format!(
                "\"/in/reports/a.txt\": \"report\" \"/in/reportsOld/a.txt\": \"{SECRET}\""
            ),
            "\"/in\" \"reportsOld\" \"a.txt\"",
        ));
        let document = copy_project("\"reports\"", TXT, "", "out", "@create @replace");
        let ran = attack.run(&document, &copy_program());
        assert_eq!(ran.code(), Some(32), "{row}: denied");
        assert_eq!(
            ran.observed.calls("fs.readText"),
            0,
            "{row}: before the driver"
        );
        assert!(!ran.stdout().contains(SECRET));
        ran.assert_unchanged(row);
        ran.passed(row, "`reports` does not cover `reportsOld`");
    }

    /// `S`: reads `sub/a.txt` 1,000 times in one entry and writes one mark per
    /// read: `i` the inside text, `x` any other text, `d` denied, `n` not
    /// found, `e` another error.
    fn many_reads() -> (String, String) {
        let document = project(
            "@std.fs.DirReadText @std.io.StdoutText",
            &format!(
                "@out: {STDOUT} {}",
                file_rule(
                    "@source",
                    "{kind: @fixedPath path: \"in\"}",
                    "",
                    "@read",
                    ANY_NAME,
                    "after: []"
                )
            ),
        );
        let main = format!(
            "{}proc reads
  takes {{source: DirReadText path: RelativePath left: U32 seen: Str}}
  returns all: Str
  performs {{DirReadText}}
  do
    match left
      case 0u32
        return seen
      case _
        let mark be match await DirReadText.read(source path)
          case Result.ok(value: \"inside\")
            \"i\"
          case Result.ok(value: _)
            \"x\"
          case Result.err(error: FsError.denied())
            \"d\"
          case Result.err(error: FsError.notFound())
            \"n\"
          case _
            \"e\"
        endmatch endlet
        return await reads(source path U32.sub(left 1u32) Str.concat(seen mark))
    endmatch
endproc

proc main
  takes {{out: StdoutText source: DirReadText}}
  returns code: ExitCode
  performs {{DirReadText StdoutText}}
  do
    let Result.ok(value: path) be RelativePath([\"sub\" \"a.txt\"]) endlet
    let marks be await reads(source path 1000u32 \"\") endlet
    let _ be await StdoutText.write(out marks) endlet
    return ExitCode(0u8)
endproc
",
            uses(&[
                "@std.fs.DirReadText",
                "@std.fs.FsError",
                "@std.fs.RelativePath",
                "@std.io.StdoutText",
            ])
        );
        (document, main)
    }

    /// Renames `from` to `to`, retrying while the reader holds the entry, at
    /// most 100,000 times.
    fn rename(from: &Path, to: &Path) {
        for _ in 0..100_000 {
            if fs::rename(from, to).is_ok() {
                return;
            }
            std::thread::yield_now();
        }
        panic!("{} cannot be renamed", from.display());
    }

    #[test]
    fn atk_run_swapped_component() {
        let row = "ATK-run-swapped-component";
        let attack = Attack::new(
            "{format: @hostWorld.v2 files: [\"/in/sub/a.txt\": \"inside\"]}",
        );
        let target = attack.outside.join("sub");
        fs::create_dir(&target).expect("the outside directory");
        fs::write(target.join("a.txt"), "outside").expect("the outside file");
        let inside = attack.root.join("in");
        link_directory(&target, &inside.join("junction"));
        let swaps = AtomicU64::new(0);
        let (document, main) = many_reads();
        let ran = attack.run_during(&document, &main, |stop| {
            let (sub, real, link) = (
                inside.join("sub"),
                inside.join("real"),
                inside.join("junction"),
            );
            // `sub` is the directory or the junction in turn; between two
            // renames it is absent for a moment. Each cycle ends as it began.
            while !stop.load(Ordering::Relaxed) {
                rename(&sub, &real);
                rename(&link, &sub);
                rename(&sub, &link);
                rename(&real, &sub);
                swaps.fetch_add(1, Ordering::Relaxed);
            }
        });
        assert_eq!(ran.code(), Some(0), "{row}");
        let marks = ran.stdout();
        assert_eq!(marks.len(), 1000, "{row}: one mark per read");
        let count = |mark: char| marks.chars().filter(|seen| *seen == mark).count();
        assert_eq!(count('x'), 0, "{row}: the outside text was read: {marks}");
        assert_eq!(count('e'), 0, "{row}: another error: {marks}");
        assert!(swaps.load(Ordering::Relaxed) > 0, "{row}: the swapper ran");
        ran.assert_unchanged(row);
        ran.passed(
            row,
            &format!(
                "1000 reads during {} swaps: {} inside, {} denied, {} not found, 0 outside",
                swaps.load(Ordering::Relaxed),
                count('i'),
                count('d'),
                count('n')
            ),
        );
    }

    #[test]
    fn atk_run_ambient_path() {
        let row = "ATK-run-ambient-path";
        for names in ["\"..\" \"x\"", "\"C:\\\\x\"", "\"\\\\\\\\?\\\\C:\\\\x\""] {
            let attack = Attack::new(&copy_world(
                &format!("\"/in/x\": \"{SECRET}\""),
                &format!("\"/in\" {names}"),
            ));
            let ran = attack.run(&copy_default(), &copy_program());
            assert_eq!(ran.code(), Some(21), "{row} {names}: no RelativePath");
            for symbol in ["fs.readText", "fs.writeText"] {
                assert_eq!(
                    ran.observed.calls(symbol),
                    0,
                    "{row} {names}: no host call"
                );
            }
            ran.assert_unchanged(row);
        }
        println!(
            "ROW {row}: windows, passed: `..`, `C:\\x` and `\\\\?\\C:\\x` are no \
             RelativePath; no host call"
        );
    }
}

#[cfg(not(windows))]
#[test]
fn attack_rows_not_run() {
    for row in [
        "ATK-run-symlink-denied",
        "ATK-run-junction-denied",
        "ATK-run-alias-denied",
        "ATK-run-hard-link-read-allowed",
        "ATK-run-hard-link-replace-denied",
        "ATK-run-denied-leaves-file",
        "ATK-run-create-denied-leaves-directory",
        "ATK-run-root-replaced",
        "ATK-run-prefix-by-names",
        "ATK-run-swapped-component",
        "ATK-run-ambient-path",
    ] {
        println!("NOT RUN {row}: no directory driver");
    }
}
