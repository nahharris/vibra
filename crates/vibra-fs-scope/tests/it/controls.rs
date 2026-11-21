//! Positive and boundary controls: the operations the contract permits, and
//! what each reports before the caller's data step.

use std::fs;
use std::process::Command;

use vibra_fs_scope::{EntryKind, Metadata, ScopeError, ScopedDir, check_name};

use crate::{Sandbox, create, file_id, not_run, read_opened};

// Row "read a file".
#[test]
fn read_a_file() {
    let sandbox = Sandbox::new("read");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    let mut file = root.open_read(&["a.txt"]).unwrap();
    assert_eq!(file.real_names(), ["a.txt"]);
    assert_eq!(file.kind(), EntryKind::File);
    assert_eq!(file.size(), 5);
    assert_eq!(read_opened(&mut file), b"hello");
}

// A read takes at most the count it is given, whatever the file holds.
#[test]
fn a_read_stops_at_its_count() {
    let sandbox = Sandbox::new("readmax");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    let mut file = root.open_read(&["a.txt"]).unwrap();
    assert_eq!(file.read_at_most(3).unwrap(), b"hel");
    assert_eq!(file.read_at_most(0).unwrap(), b"");
}

// Row "replace writes in place".
#[test]
fn replace_writes_in_place() {
    let sandbox = Sandbox::new("inplace");
    let path = sandbox.inside().join("a.txt");
    let other = sandbox.outside().join("other-name.txt");
    fs::write(&path, b"hello").unwrap();
    if let Err(error) = fs::hard_link(&path, &other) {
        return not_run("replace writes in place", &error.to_string());
    }
    let before = match file_id(&path) {
        Ok(id) => id,
        Err(reason) => return not_run("replace writes in place", &reason),
    };
    let root = sandbox.root();
    let mut file = root.open_for_replace(&["a.txt"]).unwrap();
    assert_eq!(file.links(), 2);
    file.replace_contents(b"x").unwrap();
    drop(file);
    assert_eq!(fs::read(&path).unwrap(), b"x");
    assert_eq!(fs::read(&other).unwrap(), b"x");
    assert_eq!(file_id(&path).unwrap(), before, "the file identity is kept");
    assert_eq!(file_id(&other).unwrap(), before);
    assert_eq!(root.open_read(&["a.txt"]).unwrap().links(), 2);
}

// Row "the parent is reported before creation".
#[test]
fn the_parent_is_reported_before_creation() {
    let sandbox = Sandbox::new("parentnames");
    fs::create_dir(sandbox.inside().join("Out")).unwrap();
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    let parent = root.open_parent(&["out"]).unwrap();
    assert_eq!(parent.real_names(), ["Out"]);
    drop(parent);
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");

    let root_parent = root.open_parent(&[]).unwrap();
    assert!(root_parent.real_names().is_empty(), "the root has no names");
}

// Row "create a new file".
#[test]
fn create_a_new_file() {
    let sandbox = Sandbox::new("create");
    let root = sandbox.root();
    let file = root
        .open_parent(&[])
        .unwrap()
        .create_new("new.txt")
        .unwrap();
    assert_eq!(file.real_names(), ["new.txt"]);
    assert_eq!(file.size(), 0);
    drop(file);
    let entries = fs::read_dir(sandbox.inside())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, ["new.txt"], "r holds one new entry");
    assert_eq!(fs::read(sandbox.inside().join("new.txt")).unwrap(), b"");
}

// A created file takes the bytes its caller writes, in the held parent.
#[test]
fn a_created_file_is_written_in_its_parent() {
    let sandbox = Sandbox::new("createwrite");
    fs::create_dir(sandbox.inside().join("Out")).unwrap();
    let root = sandbox.root();
    let parent = root.open_parent(&["Out"]).unwrap();
    let mut file = parent.create_new("n.txt").unwrap();
    assert_eq!(file.real_names(), ["Out", "n.txt"]);
    file.write_all(b"new bytes").unwrap();
    drop(file);
    assert_eq!(
        fs::read(sandbox.inside().join("Out").join("n.txt")).unwrap(),
        b"new bytes"
    );
}

// Rows "link count one" and "link count two".
#[test]
fn link_count_one() {
    let sandbox = Sandbox::new("linksone");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    assert_eq!(root.open_read(&["a.txt"]).unwrap().links(), 1);
    assert_eq!(root.open_for_replace(&["a.txt"]).unwrap().links(), 1);
}

#[test]
fn link_count_two() {
    let sandbox = Sandbox::new("linkstwo");
    let path = sandbox.inside().join("a.txt");
    fs::write(&path, b"hello").unwrap();
    if let Err(error) = fs::hard_link(&path, sandbox.outside().join("a.txt")) {
        return not_run("link count two", &error.to_string());
    }
    let root = sandbox.root();
    assert_eq!(root.open_read(&["a.txt"]).unwrap().links(), 2);
    assert_eq!(root.open_for_replace(&["a.txt"]).unwrap().links(), 2);
}

// Row "list sorted". `A` and `a` can only both exist in a case-sensitive
// directory.
#[test]
fn list_sorted() {
    let sandbox = Sandbox::new("listsorted");
    if let Err(reason) = crate::case_sensitive(&sandbox.inside()) {
        return not_run("list sorted", &reason);
    }
    for name in ["b", "A", "\u{e9}", "a"] {
        fs::write(sandbox.inside().join(name), name).unwrap();
    }
    let root = sandbox.root();
    let listing = root.list(&[]).unwrap();
    assert!(listing.real_names().is_empty());
    assert_eq!(listing.entries().unwrap(), ["A", "a", "b", "\u{e9}"]);
}

// A listing below the root reports the directory's real names first, and a
// directory's entries include its subdirectories.
#[test]
fn list_a_subdirectory() {
    let sandbox = Sandbox::new("listsub");
    let sub = sandbox.inside().join("Sub");
    fs::create_dir_all(sub.join("inner")).unwrap();
    fs::write(sub.join("z.txt"), b"z").unwrap();
    let root = sandbox.root();
    let listing = root.list(&["sub"]).unwrap();
    assert_eq!(listing.real_names(), ["Sub"]);
    assert_eq!(listing.entries().unwrap(), ["inner", "z.txt"]);
    assert!(
        root.list(&["Sub", "inner"])
            .unwrap()
            .entries()
            .unwrap()
            .is_empty()
    );
}

// Rows "stat a file" and "stat a directory".
#[test]
fn stat_a_file() {
    let sandbox = Sandbox::new("statfile");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    assert_eq!(
        root.stat(&["a.txt"]).unwrap(),
        Metadata {
            real_names: vec!["a.txt".to_owned()],
            kind: EntryKind::File,
            size: 5,
        }
    );
}

#[test]
fn stat_a_directory() {
    let sandbox = Sandbox::new("statdir");
    fs::create_dir(sandbox.inside().join("d")).unwrap();
    fs::write(sandbox.inside().join("d").join("x"), b"x").unwrap();
    let root = sandbox.root();
    assert_eq!(
        root.stat(&["d"]).unwrap(),
        Metadata {
            real_names: vec!["d".to_owned()],
            kind: EntryKind::Directory,
            size: 0,
        }
    );
    assert_eq!(
        root.stat(&[]).unwrap(),
        Metadata {
            real_names: Vec::new(),
            kind: EntryKind::Directory,
            size: 0,
        }
    );
}

// Row "the empty path names the root": list, stat, and open_parent accept
// it, and a creation through it lands in the root.
#[test]
fn the_empty_path_names_the_root() {
    let sandbox = Sandbox::new("emptypath");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    let listing = root.list(&[]).unwrap();
    assert!(listing.real_names().is_empty());
    assert_eq!(listing.entries().unwrap(), ["a.txt"]);
    assert_eq!(
        root.stat(&[]).unwrap(),
        Metadata {
            real_names: Vec::new(),
            kind: EntryKind::Directory,
            size: 0,
        }
    );
    let parent = root.open_parent(&[]).unwrap();
    assert!(parent.real_names().is_empty());
    drop(parent.create_new("b.txt").unwrap());
    assert!(sandbox.inside().join("b.txt").is_file());
}

// Row "near a reserved stem".
#[test]
fn near_a_reserved_stem() {
    for name in ["nul2", "com10", "console", "lpt"] {
        assert!(check_name(name).is_ok(), "{name:?} is accepted");
    }
}

// Opens through nested directories report every component's real name.
#[test]
fn opens_nested_directories_to_depth_three() {
    let sandbox = Sandbox::new("nested");
    let deep = sandbox.inside().join("a").join("b").join("C");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("deep.txt"), b"deep").unwrap();
    let root = sandbox.root();

    let mut read = root.open_read(&["a", "b", "c", "deep.txt"]).unwrap();
    assert_eq!(read.real_names(), ["a", "b", "C", "deep.txt"]);
    assert_eq!(read_opened(&mut read), b"deep");
    drop(read);

    let mut created = create(&root, &["a", "b", "C", "made.txt"]).unwrap();
    created.write_all(b"made").unwrap();
    drop(created);
    assert_eq!(fs::read(deep.join("made.txt")).unwrap(), b"made");

    let mut replaced = root.open_for_replace(&["a", "b", "C", "deep.txt"]).unwrap();
    replaced.replace_contents(b"replaced").unwrap();
    drop(replaced);
    assert_eq!(fs::read(deep.join("deep.txt")).unwrap(), b"replaced");
}

#[test]
fn a_root_that_is_a_file_is_refused() {
    let sandbox = Sandbox::new("rootfile");
    assert!(matches!(
        ScopedDir::open_root(&sandbox.secret()),
        Err(ScopeError::NotADirectory)
    ));
}

#[test]
fn a_missing_root_is_not_found() {
    let sandbox = Sandbox::new("rootmissing");
    assert!(matches!(
        ScopedDir::open_root(&sandbox.base().join("absent")),
        Err(ScopeError::NotFound)
    ));
}

// Row "no handle kept". The count is the whole process's, so it is measured
// in a child process that runs only `no_handle_kept_child`, with no other
// test thread opening handles meanwhile.
#[test]
fn no_handle_kept() {
    let exe = std::env::current_exe().unwrap();
    let output = Command::new(exe)
        .args([
            "controls::no_handle_kept_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = stderr.lines().find(|line| line.starts_with("NOT RUN")) {
        return eprintln!("{line}");
    }
    assert!(
        output.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("HANDLES"),
        "the child measured nothing:\n{stdout}\n{stderr}"
    );
    eprintln!(
        "{}",
        stderr
            .lines()
            .find(|line| line.contains("HANDLES"))
            .unwrap_or_default()
    );
}

const CHILD: &str = "VIBRA_FS_SCOPE_HANDLE_CHILD";

/// The handle count of this process, read by another process.
fn handle_count() -> Result<u64, String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("(Get-Process -Id {}).HandleCount", std::process::id()),
        ])
        .output()
        .map_err(|error| format!("cannot run powershell: {error}"))?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "powershell reported no handle count".to_owned())
}

#[test]
#[ignore = "run by `no_handle_kept` in a child process"]
fn no_handle_kept_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let sandbox = Sandbox::new("handles");
    let d = sandbox.inside().join("d").join("e");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    let operations = |round: usize| {
        let mut file = root.open_read(&["d", "e", "a.txt"]).unwrap();
        assert_eq!(read_opened(&mut file), b"hello");
        drop(file);
        let mut replace = root.open_for_replace(&["d", "e", "a.txt"]).unwrap();
        replace.replace_contents(b"hello").unwrap();
        drop(replace);
        assert_eq!(
            root.list(&["d", "e"]).unwrap().entries().unwrap().len(),
            1 + round.min(1)
        );
        assert_eq!(root.stat(&["d", "e", "a.txt"]).unwrap().size, 5);
        let parent = root.open_parent(&["d", "e"]).unwrap();
        drop(parent);
        assert!(matches!(
            root.open_read(&["d", "missing", "a.txt"]),
            Err(ScopeError::NotFound)
        ));
        assert!(matches!(
            create(&root, &["d", "e", "a.txt"]),
            Err(ScopeError::AlreadyExists)
        ));
    };
    // The first round creates `made.txt` and warms every lazy initialization.
    operations(0);
    drop(create(&root, &["d", "e", "made.txt"]).unwrap());
    operations(1);
    let measure = || match handle_count() {
        Ok(count) => count,
        Err(reason) => {
            not_run("no handle kept", &reason);
            std::process::exit(0);
        }
    };
    let _ = measure();
    let before = measure();
    let mut count = 0;
    while count < 1_000 {
        operations(1);
        count += 7;
    }
    let after = measure();
    eprintln!(
        "HANDLES with the root alone: {before}; after {count} operations: {after}"
    );
    assert_eq!(after, before, "no handle outlives an operation");
}
