//! The directory driver: `@host.fs.directory` through `Context` over a
//! `SystemDriver`, on the real filesystem (M2 step 31).
//!
//! On Windows each row makes its objects in a new temporary directory whose
//! `r` is the held root and whose `outside` is a sibling the attacks aim at.
//! `P(…)` in a row's name is the root grant's policy. After a refused write
//! the row reads the directory with `std::fs`: "unchanged" means the listing
//! of `r`, recursively, and the bytes of every file equal what they were
//! before the call. A row that needs an object the machine cannot make (a
//! symbolic link, a junction, an 8.3 short name, a case-sensitive directory,
//! a file identity from `fsutil`) prints `NOT RUN <row>: <reason>` and
//! returns; run with `--nocapture` to see such lines.
//!
//! The rows "names agree with the adapter" and "no remove and no rename" run
//! on every platform, and "not Windows" on Linux and macOS.

use std::path::{Path, PathBuf};

use vibra_syntax::decode_string;

/// Row "names agree with the adapter": the name profile of `vibra-host`
/// (`path::Name`, over `vibra_ir::names`) and the adapter's own
/// `vibra_fs_scope::check_name` give the verdict of every vector of
/// `names.tsv`, on every platform.
#[test]
fn names_agree_with_the_adapter() {
    let vectors = include_str!("names.tsv");
    let mut lines = vectors.lines();
    assert_eq!(lines.next(), Some("name\tverdict"));
    let mut seen = 0;
    for line in lines {
        let (literal, verdict) = line.split_once('\t').expect("two columns");
        let name = decode_string(literal).expect("a string literal");
        let accepted = match verdict {
            "accepted" => true,
            "rejected" => false,
            other => panic!("unknown verdict {other:?}"),
        };
        let host = vibra_host::path::Name::new(name.clone()).is_ok();
        let adapter = vibra_fs_scope::check_name(&name).is_ok();
        assert_eq!(host, accepted, "the host's profile on {literal}");
        assert_eq!(adapter, accepted, "the adapter's profile on {literal}");
        seen += 1;
    }
    assert!(seen > 80, "{seen} vectors");
}

// --- No remove and no rename -----------------------------------------------

/// The identifiers of a delete disposition, a delete access, a rename, and
/// a removal, which neither the adapter nor the driver names.
const MUTATIONS: &[&str] = &[
    "DELETE",
    "FILE_DELETE_ON_CLOSE",
    "FileDispositionInformation",
    "FileDispositionInformationEx",
    "FILE_DISPOSITION_INFORMATION",
    "FileRenameInformation",
    "FileRenameInformationEx",
    "FILE_RENAME_INFORMATION",
    "NtDeleteFile",
    "DeleteFileW",
    "RemoveDirectoryW",
    "MoveFileW",
    "MoveFileExW",
    "SetFileInformationByHandle",
    "rename",
    "remove_file",
    "remove_dir",
    "remove_dir_all",
];

/// The identifiers of `source` outside comments and `#[cfg(test)]`
/// modules, with the line each is on.
fn identifiers(source: &str) -> Vec<(usize, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        if line.trim_start().starts_with("#[cfg(test)]") {
            // Skip the attributes and the module's block, brace by brace.
            let mut depth = 0_i64;
            let mut opened = false;
            index += 1;
            while let Some(text) = lines.get(index) {
                depth += text.matches('{').count() as i64;
                depth -= text.matches('}').count() as i64;
                opened |= text.contains('{');
                index += 1;
                if opened && depth <= 0 {
                    break;
                }
            }
            continue;
        }
        let code = line.split("//").next().unwrap_or_default();
        for word in code
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        {
            if !word.is_empty() {
                found.push((index + 1, word.to_owned()));
            }
        }
        index += 1;
    }
    found
}

/// Each line of `source` that names one of [`MUTATIONS`].
fn mutations(source: &str) -> Vec<String> {
    identifiers(source)
        .into_iter()
        .filter(|(_, word)| MUTATIONS.contains(&word.as_str()))
        .map(|(line, word)| format!("{line}: `{word}`"))
        .collect()
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// Row "no remove and no rename": `vibra-fs-scope/src` and the directory
/// driver name no delete disposition or delete access, no rename, and no
/// removal (03 Actions: "Append, remove, and rename do not exist yet").
#[test]
fn no_remove_and_no_rename() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![manifest.join("src").join("system").join("fs.rs")];
    rust_sources(
        &manifest.join("..").join("vibra-fs-scope").join("src"),
        &mut files,
    );
    assert!(
        files.iter().any(|path| path.ends_with("windows.rs")),
        "the scan reads the adapter"
    );
    let found = files
        .iter()
        .flat_map(|path| {
            let source = std::fs::read_to_string(path).expect("a readable source");
            mutations(&source)
                .into_iter()
                .map(move |line| format!("{}:{line}", path.display()))
        })
        .collect::<Vec<_>>();
    assert!(
        found.is_empty(),
        "the adapter or the driver can remove or rename:\n{}",
        found.join("\n")
    );
}

/// The scan of `no_remove_and_no_rename` finds each kind of mutation, and
/// skips comments, other identifiers, and test modules.
#[test]
fn the_mutation_scan_finds_each_form() {
    assert_eq!(
        mutations("let x = std::fs::remove_file(p);\n"),
        ["1: `remove_file`"]
    );
    assert_eq!(mutations("    std::fs::rename(a, b)?;\n"), ["1: `rename`"]);
    assert_eq!(
        mutations("access: FILE_WRITE_DATA | DELETE | SYNCHRONIZE,\n"),
        ["1: `DELETE`"]
    );
    assert_eq!(
        mutations("    FileDispositionInformation,\n"),
        ["1: `FileDispositionInformation`"]
    );
    assert!(mutations("share: FILE_SHARE_DELETE,\n").is_empty());
    assert!(mutations("// a created entry is never removed or renamed\n").is_empty());
    assert!(
        mutations(
            "#[cfg(test)]\nmod tests {\n    fn t() { std::fs::remove_file(p); }\n}\n"
        )
        .is_empty()
    );
    assert_eq!(
        mutations("#[cfg(test)]\nmod tests {\n}\nfn f() { remove_dir(p) }\n"),
        ["4: `remove_dir`"]
    );
}

// --- Not Windows -----------------------------------------------------------

/// Row "not Windows": the platform has no directory driver, so admission
/// refuses a target that binds one.
#[cfg(not(windows))]
#[test]
fn not_windows() {
    use vibra_host::driver::Driver;
    use vibra_host::recipe::Provider;
    use vibra_host::system::{SystemDriver, SystemInputs};

    let driver = SystemDriver::new(SystemInputs::process(Vec::new()));
    assert_eq!(driver.identity(Provider::FsDirectory), None);
}

#[cfg(windows)]
mod on_windows {
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use vibra_host::broker::{Context, RootGrant};
    use vibra_host::driver::{
        Acquire, Action, Completion, Driver, DriverIdentity, Found, Gate, Refused,
        Reserved, Resolved, ResourceId,
    };
    use vibra_host::policy::{self, Policy};
    use vibra_host::recipe::{LeafName, Limits, Provider};
    use vibra_host::system::{Input, Output, SystemDriver, SystemInputs, Variables};
    use vibra_host::trace::GrantOrigin;
    use vibra_ir::authority::{
        GrantToken, HostArgument, HostContext, HostErrorCase, HostFsKind, HostOk,
        HostReply, HostRequest, NarrowRequest, Narrowed,
    };
    use vibra_ir::external::host::{HostInterface, HostOp};

    use crate::trace::header;

    // --- The sandbox -------------------------------------------------------

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A fresh directory holding the root `r` and a sibling `outside`. It is
    /// removed on drop; removal does not follow links.
    struct Sandbox {
        base: PathBuf,
    }

    impl Sandbox {
        fn new(row: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "vibra-host-fs-{row}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(base.join("r")).unwrap();
            fs::create_dir_all(base.join("outside")).unwrap();
            Self { base }
        }

        /// The held root `r`.
        fn r(&self) -> PathBuf {
            self.base.join("r")
        }

        /// The entry `names` below `r`.
        fn at(&self, names: &[&str]) -> PathBuf {
            names.iter().fold(self.r(), |path, name| path.join(name))
        }

        fn outside(&self) -> PathBuf {
            self.base.join("outside")
        }

        /// Every entry below `r`, with the bytes of each file.
        fn snapshot(&self) -> BTreeMap<PathBuf, Node> {
            snapshot(&self.r())
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// One entry of a snapshot.
    #[derive(Debug, PartialEq, Eq)]
    enum Node {
        Directory,
        File(Vec<u8>),
        /// A symbolic link or junction, recorded and not followed.
        Link,
    }

    /// The recursive listing of `directory` and the bytes of every file,
    /// read with `std::fs`, following no link.
    fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Node> {
        let mut found = BTreeMap::new();
        let mut pending = vec![directory.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(&next).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let kind = entry.file_type().unwrap();
                if kind.is_symlink() {
                    found.insert(path, Node::Link);
                } else if kind.is_dir() {
                    pending.push(path.clone());
                    found.insert(path, Node::Directory);
                } else {
                    found.insert(path.clone(), Node::File(fs::read(&path).unwrap()));
                }
            }
        }
        found
    }

    fn not_run(row: &str, reason: &str) {
        eprintln!("NOT RUN {row}: {reason}");
    }

    /// Holds `path` open with no sharing: while the lock lives, any open of
    /// its data by anyone else fails, so a typed denial cannot have opened
    /// it.
    fn lock(path: &Path) -> File {
        OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .expect("the file can be locked")
    }

    fn symlink_file(link: &Path, target: &Path) -> Result<(), String> {
        std::os::windows::fs::symlink_file(target, link)
            .map_err(|error| format!("cannot create a file symbolic link: {error}"))
    }

    /// Creates a junction with `cmd /c mklink /J`, or says why it failed.
    fn junction(link: &Path, target: &Path) -> Result<(), String> {
        let status = Command::new("cmd")
            .raw_arg(format!(
                "/c mklink /J \"{}\" \"{}\"",
                link.display(),
                target.display()
            ))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("cannot run cmd: {error}"))?;
        if status.success() && link.exists() {
            Ok(())
        } else {
            Err(format!("mklink /J failed with {status}"))
        }
    }

    /// The 8.3 short name of `path`, or why it has none.
    fn short_name(path: &Path) -> Result<String, String> {
        let output = Command::new("cmd")
            .raw_arg(format!(
                "/c for %I in (\"{}\") do @echo %~snxI",
                path.display()
            ))
            .output()
            .map_err(|error| format!("cannot run cmd: {error}"))?;
        let short = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let long = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !output.status.success() || short.is_empty() {
            Err("cmd did not report a short name".to_owned())
        } else if short.eq_ignore_ascii_case(long) {
            Err(format!(
                "the volume generated no 8.3 short name for {long:?} (8.3 creation is \
                 disabled)"
            ))
        } else {
            Ok(short)
        }
    }

    /// Makes the empty directory `directory` case-sensitive, or says why
    /// the machine cannot.
    fn case_sensitive(directory: &Path) -> Result<(), String> {
        let status = Command::new("fsutil")
            .args(["file", "setCaseSensitiveInfo"])
            .arg(directory)
            .arg("enable")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("cannot run fsutil: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "fsutil file setCaseSensitiveInfo failed with {status}"
            ))
        }
    }

    /// The file identity of `path` (its NTFS file ID), or why it cannot be
    /// read.
    fn file_id(path: &Path) -> Result<String, String> {
        let output = Command::new("fsutil")
            .args(["file", "queryFileID"])
            .arg(path)
            .output()
            .map_err(|error| format!("cannot run fsutil: {error}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        match text
            .split_whitespace()
            .find(|token| token.starts_with("0x"))
        {
            Some(id) if output.status.success() => Ok(id.to_owned()),
            _ => Err(format!(
                "fsutil file queryFileID failed with {}",
                output.status
            )),
        }
    }

    // --- The driver --------------------------------------------------------

    fn driver() -> SystemDriver {
        SystemDriver::new(SystemInputs {
            arguments: Vec::new(),
            stdin: Input::Absent,
            stdout: Output::Absent,
            stderr: Output::Absent,
            variables: Variables::Fixed(Vec::new()),
        })
    }

    fn acquire_at(driver: &mut dyn Driver, path: &Path) -> Acquire {
        driver.acquire(
            Provider::FsDirectory,
            &Resolved::Directory {
                path: path.to_str().expect("a Unicode path").to_owned(),
            },
        )
    }

    fn held(driver: &mut dyn Driver, path: &Path) -> ResourceId {
        match acquire_at(driver, path) {
            Acquire::Acquired(resource) => resource,
            other => panic!("the root is not acquired: {other:?}"),
        }
    }

    /// The system driver with a hook after the gate and a count of the
    /// calls that reach it.
    struct Hooked<'d> {
        driver: &'d mut SystemDriver,
        calls: usize,
        /// Runs after the gate admitted, before the driver goes on.
        after_admit: Option<Box<dyn FnMut()>>,
    }

    impl<'d> Hooked<'d> {
        fn new(driver: &'d mut SystemDriver) -> Self {
            Self {
                driver,
                calls: 0,
                after_admit: None,
            }
        }
    }

    struct AfterAdmit<'g, 'h> {
        gate: &'g mut dyn Gate,
        hook: Option<&'h mut Box<dyn FnMut()>>,
    }

    impl Gate for AfterAdmit<'_, '_> {
        fn admit(&mut self, found: &Found<'_>) -> Result<Reserved, Refused> {
            let answer = self.gate.admit(found);
            if answer.is_ok()
                && let Some(hook) = self.hook.as_mut()
            {
                hook();
            }
            answer
        }
    }

    impl Driver for Hooked<'_> {
        fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
            self.driver.identity(provider)
        }

        fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
            self.driver.acquire(provider, resource)
        }

        fn release(&mut self, resource: ResourceId) {
            self.driver.release(resource);
        }

        fn call(
            &mut self,
            resource: ResourceId,
            action: &Action<'_>,
            gate: &mut dyn Gate,
        ) -> Completion {
            self.calls += 1;
            let mut gate = AfterAdmit {
                gate,
                hook: self.after_admit.as_mut(),
            };
            self.driver.call(resource, action, &mut gate)
        }
    }

    // --- Policies ----------------------------------------------------------

    const ANY: &str = "{kind: @anyName}";

    fn suffix(value: &str) -> String {
        format!("{{kind: @suffix value: \"{value}\"}}")
    }

    fn quoted(names: &[&str]) -> String {
        names
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A file rule of depth `@tree` below `prefix`.
    fn files(prefix: &[&str], actions: &str, names: &str) -> String {
        format!(
            "{{kind: @fileRule root: @notes prefix: [{}] depth: @tree actions: \
             [{actions}] names: {names}}}",
            quoted(prefix)
        )
    }

    /// A directory rule of depth `@tree` below `prefix`.
    fn directories(prefix: &[&str], actions: &str) -> String {
        format!(
            "{{kind: @directoryRule root: @notes prefix: [{}] depth: @tree actions: \
             [{actions}]}}",
            quoted(prefix)
        )
    }

    fn any_of(items: &[String]) -> String {
        format!("{{kind: @anyOf items: [{}]}}", items.join(" "))
    }

    fn policy(expression: &str) -> Policy {
        policy::decode(
            format!(
                "{{format: @scopePolicy.v1 family: @filesystem expression: \
                 {expression}}}"
            )
            .as_bytes(),
        )
        .expect("a policy")
    }

    fn input_bytes(bound: u64) -> Limits {
        Limits {
            input_bytes: Some(bound),
            ..Limits::default()
        }
    }

    // --- Calls -------------------------------------------------------------

    fn grant(
        cx: &mut Context<'_>,
        interface: HostInterface,
        resource: ResourceId,
        expression: &str,
        limits: Limits,
    ) -> GrantToken {
        cx.root_for_tests(RootGrant {
            interface,
            resource,
            origin: GrantOrigin::Binding {
                leaf: LeafName::new("@notes"),
            },
            policy: Some(policy(expression)),
            variables: None,
            limits,
        })
        .expect("a root grant")
    }

    /// Runs `body` on a context over `driver` with one root grant of
    /// `interface` and `expression` on `resource`.
    fn with_grant<R>(
        driver: &mut dyn Driver,
        resource: ResourceId,
        interface: HostInterface,
        expression: &str,
        limits: Limits,
        body: impl FnOnce(&mut Context<'_>, GrantToken) -> R,
    ) -> R {
        let mut cx = Context::new(driver, header());
        let token = grant(&mut cx, interface, resource, expression, limits);
        body(&mut cx, token)
    }

    /// Calls and commits: the reply, and whether an event was due.
    fn call(
        cx: &mut Context<'_>,
        op: HostOp,
        grant: GrantToken,
        arguments: &[HostArgument<'_>],
    ) -> (HostReply, bool) {
        let completion = cx
            .host(HostRequest {
                op,
                grant,
                arguments,
                invocation: 1,
                parent: None,
                origin: None,
                room: u64::MAX,
            })
            .expect("no fault");
        let due = completion.receipt.is_some();
        if let Some(receipt) = completion.receipt {
            cx.commit(receipt).expect("the pending receipt");
        }
        (completion.reply, due)
    }

    fn names(at: &[&str]) -> Vec<String> {
        at.iter().map(|name| (*name).to_owned()).collect()
    }

    fn on_path(
        cx: &mut Context<'_>,
        op: HostOp,
        grant: GrantToken,
        at: &[&str],
    ) -> (HostReply, bool) {
        let at = names(at);
        call(cx, op, grant, &[HostArgument::Path(&at)])
    }

    fn read_text(
        cx: &mut Context<'_>,
        grant: GrantToken,
        at: &[&str],
    ) -> (HostReply, bool) {
        on_path(cx, HostOp::FsReadText, grant, at)
    }

    fn write_text(
        cx: &mut Context<'_>,
        grant: GrantToken,
        at: &[&str],
        contents: &str,
    ) -> (HostReply, bool) {
        let at = names(at);
        call(
            cx,
            HostOp::FsWriteText,
            grant,
            &[HostArgument::Path(&at), HostArgument::Text(contents)],
        )
    }

    fn err(op: HostOp, case: &str) -> HostReply {
        HostReply::Err(HostErrorCase::new(op.row().error, case).expect("a case"))
    }

    fn text(value: &str) -> HostReply {
        HostReply::Ok(HostOk::Text(value.to_owned()))
    }

    const VOID: HostReply = HostReply::Ok(HostOk::Void);

    /// A completed `denied`, with its event.
    fn denied(op: HostOp) -> (HostReply, bool) {
        (err(op, "denied"), true)
    }

    // --- Reads -------------------------------------------------------------

    /// Row "read text".
    #[test]
    fn read_text_row() {
        let sandbox = Sandbox::new("read-text");
        fs::write(sandbox.at(&["a.txt"]), "hello").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let events = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| {
                assert_eq!(read_text(cx, grant, &["a.txt"]), (text("hello"), true));
                cx.trace().events.len()
            },
        );
        assert_eq!(events, 1, "one event");
    }

    /// Row "each operation": read bytes, list, stat, write text of a new
    /// file, write bytes over an existing one; each `Ok`, and the directory
    /// holds the results.
    #[test]
    fn each_operation() {
        let sandbox = Sandbox::new("each");
        fs::write(sandbox.at(&["a.bin"]), [1_u8, 2, 3]).unwrap();
        fs::create_dir(sandbox.at(&["d"])).unwrap();
        fs::write(sandbox.at(&["old.txt"]), "old contents").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let mut cx = Context::new(&mut driver, header());
        let read = grant(
            &mut cx,
            HostInterface::DirReadBytes,
            root,
            &files(&[], "@read", ANY),
            Limits::default(),
        );
        assert_eq!(
            on_path(&mut cx, HostOp::FsReadFile, read, &["a.bin"]),
            (HostReply::Ok(HostOk::Bytes(vec![1, 2, 3])), true)
        );
        let list = grant(
            &mut cx,
            HostInterface::DirList,
            root,
            &directories(&[], "@list"),
            Limits::default(),
        );
        assert_eq!(
            on_path(&mut cx, HostOp::FsReadEntries, list, &[]),
            (
                HostReply::Ok(HostOk::Names(names(&["a.bin", "d", "old.txt"]))),
                true
            )
        );
        let stat = grant(
            &mut cx,
            HostInterface::DirStat,
            root,
            &any_of(&[files(&[], "@stat", ANY), directories(&[], "@stat")]),
            Limits::default(),
        );
        assert_eq!(
            on_path(&mut cx, HostOp::FsMetadataStat, stat, &["a.bin"]),
            (
                HostReply::Ok(HostOk::Metadata {
                    kind: HostFsKind::File,
                    size: 3
                }),
                true
            )
        );
        assert_eq!(
            on_path(&mut cx, HostOp::FsMetadataStat, stat, &["d"]),
            (
                HostReply::Ok(HostOk::Metadata {
                    kind: HostFsKind::Directory,
                    size: 0
                }),
                true
            )
        );
        let write = grant(
            &mut cx,
            HostInterface::DirWriteText,
            root,
            &files(&[], "@create @replace", ANY),
            Limits::default(),
        );
        assert_eq!(
            write_text(&mut cx, write, &["new.txt"], "new text"),
            (VOID, true)
        );
        let bytes = grant(
            &mut cx,
            HostInterface::DirWriteBytes,
            root,
            &files(&[], "@create @replace", ANY),
            Limits::default(),
        );
        let at = names(&["old.txt"]);
        assert_eq!(
            call(
                &mut cx,
                HostOp::FsWriteFile,
                bytes,
                &[HostArgument::Path(&at), HostArgument::Bytes(&[9, 8])],
            ),
            (VOID, true)
        );
        drop(cx);
        assert_eq!(fs::read(sandbox.at(&["new.txt"])).unwrap(), b"new text");
        assert_eq!(fs::read(sandbox.at(&["old.txt"])).unwrap(), [9, 8]);
        assert_eq!(fs::read(sandbox.at(&["a.bin"])).unwrap(), [1, 2, 3]);
    }

    /// Row `ATK-fs-alias-name`: the request's case alias passes the name
    /// test of step 3, the real name fails it at the gate; a completed
    /// `denied`, and the file is not read.
    #[test]
    fn atk_fs_alias_name() {
        let sandbox = Sandbox::new("alias-name");
        fs::write(sandbox.at(&["NOTES.TXT"]), "secret").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", &suffix(".txt")),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &["notes.txt"]),
        );
        assert_eq!(reply, denied(HostOp::FsReadText));
        assert_eq!(driver.file_reads_for_tests(), 0, "the file is not read");
    }

    /// Row "alias that passes": a case alias whose real name passes the
    /// name test is read. The guide's input (`r/Notes.txt`, a read of
    /// `["NOTES.TXT"]`, `P(suffix ".txt")`) is `denied` in step 3, which
    /// checks the requested name against the name test (06 "Host calls",
    /// step 3) before the driver is asked; that is asserted first. The alias
    /// is then a request that passes step 3 and differs from the real name
    /// in case.
    #[test]
    fn alias_that_passes() {
        let sandbox = Sandbox::new("alias-passes");
        fs::write(sandbox.at(&["Notes.txt"]), "first").unwrap();
        fs::write(sandbox.at(&["NOTES.md.txt"]), "second").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let mut hooked = Hooked::new(&mut driver);
        let (literal, alias) = with_grant(
            &mut hooked,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", &suffix(".txt")),
            Limits::default(),
            |cx, grant| {
                let literal = read_text(cx, grant, &["NOTES.TXT"]);
                let alias = read_text(cx, grant, &["notes.md.txt"]);
                (literal, alias)
            },
        );
        assert_eq!(literal, (err(HostOp::FsReadText, "denied"), false));
        assert_eq!(alias, (text("second"), true));
        assert_eq!(hooked.calls, 1, "only the alias reached the driver");
    }

    /// Row "short name": the 8.3 name passes the name test of step 3, the
    /// long name the filesystem reports fails it.
    #[test]
    fn short_name_row() {
        let sandbox = Sandbox::new("short-name");
        let long = sandbox.at(&["longfilename.text"]);
        fs::write(&long, "secret").unwrap();
        let short = match short_name(&long) {
            Ok(short) => short,
            Err(reason) => return not_run("short name", &reason),
        };
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", &suffix(".TEX")),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &[short.as_str()]),
        );
        assert_eq!(reply, denied(HostOp::FsReadText));
        assert_eq!(driver.file_reads_for_tests(), 0);
    }

    /// Row "symlink leaf": the link is opened as the link and refused; the
    /// outside file is held with no sharing, so nothing opened its data.
    #[test]
    fn symlink_leaf() {
        let sandbox = Sandbox::new("symlink-leaf");
        let secret = sandbox.outside().join("secret.txt");
        fs::write(&secret, "outside secret").unwrap();
        if let Err(reason) = symlink_file(&sandbox.at(&["l.txt"]), &secret) {
            return not_run("symlink leaf", &reason);
        }
        let _lock = lock(&secret);
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &["l.txt"]),
        );
        assert_eq!(reply, denied(HostOp::FsReadText));
        assert_eq!(driver.file_reads_for_tests(), 0);
    }

    /// Row "junction component": a junction on the way is refused; the
    /// outside file is held with no sharing, so it was not opened.
    #[test]
    fn junction_component() {
        let sandbox = Sandbox::new("junction");
        let target = sandbox.outside().join("a.txt");
        fs::write(&target, "outside").unwrap();
        if let Err(reason) = junction(&sandbox.at(&["j"]), &sandbox.outside()) {
            return not_run("junction component", &reason);
        }
        let _lock = lock(&target);
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &["j", "a.txt"]),
        );
        assert_eq!(reply, denied(HostOp::FsReadText));
        assert_eq!(driver.file_reads_for_tests(), 0);
    }

    // --- Hard links and replacement ----------------------------------------

    /// `r/a.txt`, a second name of `outside/a.txt`.
    fn hard_linked(sandbox: &Sandbox) {
        let original = sandbox.outside().join("a.txt");
        fs::write(&original, "shared").unwrap();
        fs::hard_link(&original, sandbox.at(&["a.txt"])).unwrap();
    }

    /// Row `ATK-fs-hard-link-replace`: the gate refuses the replace from
    /// the link count of the opened file; both names keep their content.
    #[test]
    fn atk_fs_hard_link_replace() {
        let sandbox = Sandbox::new("hard-link-replace");
        hard_linked(&sandbox);
        let before = sandbox.snapshot();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirWriteText,
            &files(&[], "@replace", ANY),
            Limits::default(),
            |cx, grant| write_text(cx, grant, &["a.txt"], "changed"),
        );
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert_eq!(sandbox.snapshot(), before, "r unchanged");
        let outside = sandbox.outside().join("a.txt");
        assert_eq!(fs::read(&outside).unwrap(), b"shared");
        assert_eq!(fs::metadata(&outside).unwrap().len(), 6);
        assert_eq!(fs::metadata(sandbox.at(&["a.txt"])).unwrap().len(), 6);
    }

    /// Row "hard link read": reading a file with a second name is allowed.
    #[test]
    fn hard_link_read() {
        let sandbox = Sandbox::new("hard-link-read");
        hard_linked(&sandbox);
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &["a.txt"]),
        );
        assert_eq!(reply, (text("shared"), true));
    }

    /// Row "replace in place": the file keeps its identity.
    #[test]
    fn replace_in_place() {
        let sandbox = Sandbox::new("replace-in-place");
        let file = sandbox.at(&["a.txt"]);
        fs::write(&file, "a longer old text").unwrap();
        let before = match file_id(&file) {
            Ok(id) => id,
            Err(reason) => return not_run("replace in place", &reason),
        };
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirWriteText,
            &files(&[], "@replace", ANY),
            Limits::default(),
            |cx, grant| write_text(cx, grant, &["a.txt"], "new"),
        );
        assert_eq!(reply, (VOID, true));
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(file_id(&file), Ok(before), "the file identity is kept");
    }

    // --- Creation ------------------------------------------------------------

    /// Runs one write of `contents` to `at` through a grant of
    /// `expression`; the reply, and whether `r` is unchanged.
    fn write_in(
        sandbox: &Sandbox,
        expression: &str,
        at: &[&str],
        contents: &str,
    ) -> ((HostReply, bool), bool) {
        let before = sandbox.snapshot();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirWriteText,
            expression,
            Limits::default(),
            |cx, grant| write_text(cx, grant, at, contents),
        );
        (reply, sandbox.snapshot() == before)
    }

    /// Row "create exclusive": a create-only grant finds an existing file,
    /// which the gate is shown as a replace and refuses.
    #[test]
    fn create_exclusive() {
        let sandbox = Sandbox::new("create-exclusive");
        fs::write(sandbox.at(&["a.txt"]), "keep").unwrap();
        let (reply, unchanged) =
            write_in(&sandbox, &files(&[], "@create", ANY), &["a.txt"], "new");
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert!(unchanged, "r unchanged");
    }

    /// Row "create over an alias": the existing entry under another case is
    /// found and not replaced.
    #[test]
    fn create_over_an_alias() {
        let sandbox = Sandbox::new("create-over-alias");
        fs::write(sandbox.at(&["A.TXT"]), "keep").unwrap();
        let (reply, unchanged) =
            write_in(&sandbox, &files(&[], "@create", ANY), &["a.txt"], "new");
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert!(unchanged, "r unchanged");
    }

    /// Row "create in the checked parent".
    #[test]
    fn create_in_the_checked_parent() {
        let sandbox = Sandbox::new("create-in-parent");
        fs::create_dir(sandbox.at(&["out"])).unwrap();
        let mut expected = sandbox.snapshot();
        expected.insert(sandbox.at(&["out", "n.txt"]), Node::File(b"text".to_vec()));
        let (reply, _) = write_in(
            &sandbox,
            &files(&["out"], "@create", ANY),
            &["out", "n.txt"],
            "text",
        );
        assert_eq!(reply, (VOID, true));
        assert_eq!(sandbox.snapshot(), expected, "r holds nothing else new");
    }

    /// Row `ATK-fs-create-denied-on-real-parent`: the request's parent
    /// `OUT` passes the policy of step 3; the gate is shown the real names
    /// of the held parent, `out`, and refuses before anything is created.
    #[test]
    fn atk_fs_create_denied_on_real_parent() {
        let sandbox = Sandbox::new("create-real-parent");
        fs::create_dir(sandbox.at(&["out"])).unwrap();
        let (reply, unchanged) = write_in(
            &sandbox,
            &files(&["OUT"], "@create", ANY),
            &["OUT", "n.txt"],
            "text",
        );
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert!(unchanged, "r unchanged");
        assert_eq!(fs::read_dir(sandbox.at(&["out"])).unwrap().count(), 0);
    }

    /// Row "create below a short-name parent".
    #[test]
    fn create_below_a_short_name_parent() {
        let sandbox = Sandbox::new("create-short-parent");
        let long = sandbox.at(&["longdirectoryname"]);
        fs::create_dir(&long).unwrap();
        let short = match short_name(&long) {
            Ok(short) => short,
            Err(reason) => return not_run("create below a short-name parent", &reason),
        };
        let (reply, unchanged) = write_in(
            &sandbox,
            &files(&[short.as_str()], "@create", ANY),
            &[short.as_str(), "n.txt"],
            "text",
        );
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert!(unchanged, "r unchanged");
    }

    /// Row "create loses a race": the entry appears after the gate admitted
    /// the create and before the adapter creates; the create is a completed
    /// `denied` and the entry keeps the hook's bytes.
    #[test]
    fn create_loses_a_race() {
        let sandbox = Sandbox::new("create-race");
        let target = sandbox.at(&["n.txt"]);
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let mut hooked = Hooked::new(&mut driver);
        let racer = target.clone();
        hooked.after_admit = Some(Box::new(move || {
            fs::write(&racer, "the hook's bytes").unwrap();
        }));
        let reply = with_grant(
            &mut hooked,
            root,
            HostInterface::DirWriteText,
            &files(&[], "@create", ANY),
            Limits::default(),
            |cx, grant| write_text(cx, grant, &["n.txt"], "the program's bytes"),
        );
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert_eq!(fs::read(&target).unwrap(), b"the hook's bytes");
    }

    /// Row "replace only, new file".
    #[test]
    fn replace_only_new_file() {
        let sandbox = Sandbox::new("replace-only");
        let (reply, unchanged) =
            write_in(&sandbox, &files(&[], "@replace", ANY), &["new.txt"], "new");
        assert_eq!(reply, denied(HostOp::FsWriteText));
        assert!(unchanged, "r unchanged");
    }

    /// Row "denied leaves no trace on disk": refused in step 3, before the
    /// adapter: no call reaches the driver and no event is due.
    #[test]
    fn denied_leaves_no_trace_on_disk() {
        let sandbox = Sandbox::new("denied-no-trace");
        let before = sandbox.snapshot();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let mut hooked = Hooked::new(&mut driver);
        let reply = with_grant(
            &mut hooked,
            root,
            HostInterface::DirWriteText,
            &files(&[], "@create @replace", &suffix(".txt")),
            Limits::default(),
            |cx, grant| write_text(cx, grant, &["a.bin"], "x"),
        );
        assert_eq!(reply, (err(HostOp::FsWriteText, "denied"), false));
        assert_eq!(hooked.calls, 0, "refused before the adapter");
        assert_eq!(sandbox.snapshot(), before, "r unchanged");
    }

    /// Row "no parent creation".
    #[test]
    fn no_parent_creation() {
        let sandbox = Sandbox::new("no-parent");
        let (reply, unchanged) = write_in(
            &sandbox,
            &files(&[], "@create @replace", ANY),
            &["no", "a.txt"],
            "x",
        );
        assert_eq!(reply, (err(HostOp::FsWriteText, "notFound"), true));
        assert!(unchanged, "r unchanged");
        assert!(!sandbox.at(&["no"]).exists());
    }

    /// A name every component of `RelativePath` allows and the filesystem
    /// cannot interpret is `invalidPath` (06 Error types: "a path the native
    /// adapter cannot interpret"), for a read and for a write.
    #[test]
    fn a_name_the_filesystem_cannot_interpret() {
        let sandbox = Sandbox::new("invalid-path");
        let before = sandbox.snapshot();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let mut cx = Context::new(&mut driver, header());
        let read = grant(
            &mut cx,
            HostInterface::DirReadText,
            root,
            &files(&[], "@read", ANY),
            Limits::default(),
        );
        assert_eq!(
            read_text(&mut cx, read, &["a*b.txt"]),
            (err(HostOp::FsReadText, "invalidPath"), true)
        );
        let write = grant(
            &mut cx,
            HostInterface::DirWriteText,
            root,
            &files(&[], "@create @replace", ANY),
            Limits::default(),
        );
        assert_eq!(
            write_text(&mut cx, write, &["a?b.txt"], "x"),
            (err(HostOp::FsWriteText, "invalidPath"), true)
        );
        drop(cx);
        assert_eq!(sandbox.snapshot(), before, "r unchanged");
    }

    // --- Listing, stat, and read errors --------------------------------------

    /// Row "list needs a directory rule".
    #[test]
    fn list_needs_a_directory_rule() {
        let sandbox = Sandbox::new("list-file-rule");
        fs::write(sandbox.at(&["a.txt"]), "x").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirList,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| on_path(cx, HostOp::FsReadEntries, grant, &[]),
        );
        assert_eq!(reply.0, err(HostOp::FsReadEntries, "denied"));
    }

    fn listed(sandbox: &Sandbox, at: &[&str]) -> (HostReply, bool) {
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        with_grant(
            &mut driver,
            root,
            HostInterface::DirList,
            &directories(&[], "@list"),
            Limits::default(),
            |cx, grant| on_path(cx, HostOp::FsReadEntries, grant, at),
        )
    }

    /// Row "list sorted": by Unicode scalar, so an uppercase name sorts
    /// before every lowercase one. `b`, `A`, and `a` together need a
    /// case-sensitive directory; `b`, `A`, and `c` are listed on any volume.
    #[test]
    fn list_sorted() {
        let sandbox = Sandbox::new("list-sorted");
        fs::create_dir(sandbox.at(&["any"])).unwrap();
        for name in ["b", "A", "c"] {
            fs::write(sandbox.at(&["any", name]), name).unwrap();
        }
        assert_eq!(
            listed(&sandbox, &["any"]),
            (HostReply::Ok(HostOk::Names(names(&["A", "b", "c"]))), true)
        );
        let sensitive = sandbox.at(&["sensitive"]);
        fs::create_dir(&sensitive).unwrap();
        if let Err(reason) = case_sensitive(&sensitive) {
            return not_run("list sorted (b, A, a)", &reason);
        }
        for name in ["b", "A", "a"] {
            fs::write(sensitive.join(name), name).unwrap();
        }
        if fs::read_dir(&sensitive).unwrap().count() != 3 {
            return not_run(
                "list sorted (b, A, a)",
                "the directory is not case-sensitive",
            );
        }
        assert_eq!(
            listed(&sandbox, &["sensitive"]),
            (HostReply::Ok(HostOk::Names(names(&["A", "a", "b"]))), true)
        );
    }

    /// Row "stat a link": the link is not followed, and refused.
    #[test]
    fn stat_a_link() {
        let sandbox = Sandbox::new("stat-link");
        let secret = sandbox.outside().join("secret.txt");
        fs::write(&secret, "outside secret").unwrap();
        if let Err(reason) = symlink_file(&sandbox.at(&["l.txt"]), &secret) {
            return not_run("stat a link", &reason);
        }
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirStat,
            &any_of(&[files(&[], "@stat", ANY), directories(&[], "@stat")]),
            Limits::default(),
            |cx, grant| on_path(cx, HostOp::FsMetadataStat, grant, &["l.txt"]),
        );
        assert_eq!(reply, denied(HostOp::FsMetadataStat));
    }

    fn read_one(sandbox: &Sandbox, at: &[&str], limits: Limits) -> (HostReply, bool) {
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            limits,
            |cx, grant| read_text(cx, grant, at),
        )
    }

    /// Row "read a directory".
    #[test]
    fn read_a_directory() {
        let sandbox = Sandbox::new("read-directory");
        fs::create_dir(sandbox.at(&["d"])).unwrap();
        assert_eq!(
            read_one(&sandbox, &["d"], Limits::default()),
            (err(HostOp::FsReadText, "notAFile"), true)
        );
    }

    /// Row "invalid text".
    #[test]
    fn invalid_text() {
        let sandbox = Sandbox::new("invalid-text");
        fs::write(sandbox.at(&["a.txt"]), [255_u8]).unwrap();
        assert_eq!(
            read_one(&sandbox, &["a.txt"], Limits::default()),
            (err(HostOp::FsReadText, "invalidText"), true)
        );
    }

    /// Row "read over the limit" (Q29): the gate refuses the size before any
    /// content is read; no event, and the call holds no invocation.
    #[test]
    fn read_over_the_limit() {
        let sandbox = Sandbox::new("read-over-limit");
        fs::write(sandbox.at(&["a.txt"]), "12345").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let (reply, invocations) = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            input_bytes(4),
            |cx, grant| {
                let reply = read_text(cx, grant, &["a.txt"]);
                let used = cx.counters_for_tests(grant).expect("counters");
                (reply, used.invocations().used())
            },
        );
        assert_eq!(reply, (err(HostOp::FsReadText, "limitExceeded"), false));
        assert_eq!(invocations, 0, "no invocation is held");
        assert_eq!(driver.file_reads_for_tests(), 0, "no read call");
    }

    // --- The root and `sub` --------------------------------------------------

    /// Row "`sub` walks from the root": the child names `d` below the root;
    /// after `d` is renamed and a new `d` made, the next read opens the new
    /// one.
    #[test]
    fn sub_walks_from_the_root() {
        let sandbox = Sandbox::new("sub-walks");
        fs::create_dir(sandbox.at(&["d"])).unwrap();
        fs::write(sandbox.at(&["d", "a.txt"]), "first").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        let (first, second) = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| {
                let prefix = names(&["d"]);
                let child = match cx
                    .narrow(NarrowRequest::Sub {
                        grant,
                        prefix: &prefix,
                    })
                    .expect("a valid sub")
                {
                    Narrowed::Grant(child) => child,
                    Narrowed::Refused(refusal) => panic!("refused: {refusal:?}"),
                };
                let first = read_text(cx, child, &["a.txt"]);
                fs::rename(sandbox.at(&["d"]), sandbox.at(&["e"])).unwrap();
                fs::create_dir(sandbox.at(&["d"])).unwrap();
                fs::write(sandbox.at(&["d", "a.txt"]), "second").unwrap();
                (first, read_text(cx, child, &["a.txt"]))
            },
        );
        assert_eq!(first, (text("first"), true));
        assert_eq!(second, (text("second"), true));
    }

    /// Row "root renamed": the grant follows the directory it opened, not
    /// its path.
    #[test]
    fn root_renamed() {
        let sandbox = Sandbox::new("root-renamed");
        fs::write(sandbox.at(&["a.txt"]), "the opened directory").unwrap();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        fs::rename(sandbox.r(), sandbox.base.join("renamed")).unwrap();
        fs::create_dir(sandbox.r()).unwrap();
        fs::write(sandbox.at(&["a.txt"]), "a new directory at the old path").unwrap();
        let reply = with_grant(
            &mut driver,
            root,
            HostInterface::DirReadText,
            &files(&[], "@read", ANY),
            Limits::default(),
            |cx, grant| read_text(cx, grant, &["a.txt"]),
        );
        assert_eq!(reply, (text("the opened directory"), true));
    }

    /// Row `ATK-fs-no-normalized-name` (Q29): with the adapter's name query
    /// failing, a read and a create are `unsupported` before acting: nothing
    /// read, `r` unchanged, no event.
    #[test]
    fn atk_fs_no_normalized_name() {
        let sandbox = Sandbox::new("no-normalized-name");
        fs::write(sandbox.at(&["a.txt"]), "x").unwrap();
        let before = sandbox.snapshot();
        let mut driver = driver();
        let root = held(&mut driver, &sandbox.r());
        driver.fail_name_queries_for_tests();
        let (read, write, events) = {
            let mut cx = Context::new(&mut driver, header());
            let reader = grant(
                &mut cx,
                HostInterface::DirReadText,
                root,
                &files(&[], "@read", ANY),
                Limits::default(),
            );
            let writer = grant(
                &mut cx,
                HostInterface::DirWriteText,
                root,
                &files(&[], "@create @replace", ANY),
                Limits::default(),
            );
            let read = read_text(&mut cx, reader, &["a.txt"]);
            let write = write_text(&mut cx, writer, &["new.txt"], "y");
            (read, write, cx.trace().events.len())
        };
        assert_eq!(read, (err(HostOp::FsReadText, "unsupported"), false));
        assert_eq!(write, (err(HostOp::FsWriteText, "unsupported"), false));
        assert_eq!(events, 0, "no event");
        assert_eq!(driver.file_reads_for_tests(), 0, "nothing read");
        assert_eq!(sandbox.snapshot(), before, "r unchanged");
    }

    // --- Acquisition ---------------------------------------------------------

    /// Row "acquisition refusals": a path that names nothing, and one that
    /// names a file.
    #[test]
    fn acquisition_refusals() {
        let sandbox = Sandbox::new("acquire-refused");
        fs::write(sandbox.at(&["a.txt"]), "x").unwrap();
        let mut driver = driver();
        assert_eq!(
            acquire_at(&mut driver, &sandbox.at(&["missing"])),
            Acquire::Refused
        );
        assert_eq!(
            acquire_at(&mut driver, &sandbox.at(&["a.txt"])),
            Acquire::Refused
        );
    }

    /// Row "acquisition failure": a volume that does not report normalized
    /// names cannot enforce the leaf's policy.
    #[test]
    fn acquisition_failure() {
        let sandbox = Sandbox::new("acquire-failed");
        let mut driver = driver();
        driver.fail_name_queries_for_tests();
        assert!(matches!(
            acquire_at(&mut driver, &sandbox.r()),
            Acquire::Failed(_)
        ));
    }

    /// A root that is itself a junction is no directory the driver can
    /// hold: a failure, not a refusal (04 lists no such refusal).
    #[test]
    fn a_root_that_is_a_junction_fails() {
        let sandbox = Sandbox::new("acquire-junction");
        let link = sandbox.base.join("j");
        if let Err(reason) = junction(&link, &sandbox.outside()) {
            return not_run("a root that is a junction", &reason);
        }
        let mut driver = driver();
        assert!(matches!(acquire_at(&mut driver, &link), Acquire::Failed(_)));
    }

    /// Opens the directory `path` with no sharing: this fails while any
    /// other handle is open on it.
    fn open_exclusively(path: &Path) -> std::io::Result<File> {
        // FILE_FLAG_BACKUP_SEMANTICS, which opening a directory needs.
        OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(0x0200_0000)
            .open(path)
    }

    /// The identity of the directory driver, and its release: the root is
    /// held while acquired, and its handle is closed by `release`.
    #[test]
    fn identity_and_release() {
        let sandbox = Sandbox::new("release");
        let mut driver = driver();
        assert_eq!(
            driver.identity(Provider::FsDirectory),
            Some(DriverIdentity {
                provider: "@host.fs.directory".to_owned(),
                version: "0.0.2".to_owned(),
            })
        );
        let root = held(&mut driver, &sandbox.r());
        assert!(
            open_exclusively(&sandbox.r()).is_err(),
            "the acquired root is held open"
        );
        driver.release(root);
        drop(
            open_exclusively(&sandbox.r()).expect("no handle holds the released root"),
        );
    }
}
