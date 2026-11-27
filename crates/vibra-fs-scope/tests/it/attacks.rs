//! The attack and negative cases. Each asserts the specific typed result and
//! reads the directories with `std::fs` afterwards: the outside tree, and `r`
//! where the row says it is unchanged, are compared with a snapshot taken
//! before. Where the secret file is the target it is held open with no
//! sharing during the attack, so a denial cannot come after an open of its
//! data.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use vibra_fs_scope::{ScopeError, ScopedDir, check_name};

use crate::{
    SECRET, Sandbox, assert_denied, create, is_already_exists, is_denied_name,
    is_denied_reparse, is_not_a_directory, is_not_a_file, is_not_found, junction,
    not_run, read_opened, short_name, symlink_dir, symlink_file,
};

/// Runs read, replace, and create on `names` and asserts `opened` for a read
/// and a replace and `created` for a create, with the secret locked and the
/// outside tree unchanged.
fn deny(
    sandbox: &Sandbox,
    names: &[&str],
    opened: fn(&ScopeError) -> bool,
    created: fn(&ScopeError) -> bool,
    case: &str,
) {
    let before = sandbox.outside_snapshot();
    let root = sandbox.root();
    let lock = sandbox.lock_secret();
    assert_denied(root.open_read(names), opened, &format!("{case}, read"));
    assert_denied(
        root.open_for_replace(names),
        opened,
        &format!("{case}, replace"),
    );
    assert_denied(create(&root, names), created, &format!("{case}, create"));
    drop(lock);
    assert_eq!(
        sandbox.outside_snapshot(),
        before,
        "{case}: the outside tree is unchanged"
    );
    assert_eq!(fs::read(sandbox.secret()).unwrap(), SECRET);
}

/// As [`deny`], with one expected denial for every operation.
fn deny_all(
    sandbox: &Sandbox,
    names: &[&str],
    expected: fn(&ScopeError) -> bool,
    case: &str,
) {
    deny(sandbox, names, expected, expected, case);
}

// Row `ATK-fs-file-symlink`.
#[test]
fn atk_fs_file_symlink() {
    let sandbox = Sandbox::new("filelink");
    if let Err(reason) =
        symlink_file(&sandbox.inside().join("link.txt"), &sandbox.secret())
    {
        return not_run("ATK-fs-file-symlink", &reason);
    }
    // A create of the link's own name finds an existing entry.
    deny(
        &sandbox,
        &["link.txt"],
        is_denied_reparse,
        is_already_exists,
        "ATK-fs-file-symlink",
    );
}

// Row "stat a link".
#[test]
fn stat_a_link() {
    let sandbox = Sandbox::new("statlink");
    if let Err(reason) =
        symlink_file(&sandbox.inside().join("link.txt"), &sandbox.secret())
    {
        return not_run("stat a link", &reason);
    }
    let root = sandbox.root();
    let lock = sandbox.lock_secret();
    assert_denied(root.stat(&["link.txt"]), is_denied_reparse, "stat a link");
    drop(lock);
}

// Row `ATK-fs-junction`.
#[test]
fn atk_fs_junction() {
    let sandbox = Sandbox::new("junction");
    fs::write(sandbox.outside().join("a.txt"), SECRET).unwrap();
    if let Err(reason) = junction(&sandbox.inside().join("j"), &sandbox.outside()) {
        return not_run("ATK-fs-junction", &reason);
    }
    deny_all(
        &sandbox,
        &["j", "a.txt"],
        is_denied_reparse,
        "ATK-fs-junction",
    );
    deny_all(
        &sandbox,
        &["j", "secret.txt"],
        is_denied_reparse,
        "ATK-fs-junction, the locked secret",
    );
    deny(
        &sandbox,
        &["j"],
        is_denied_reparse,
        is_already_exists,
        "ATK-fs-junction as the leaf",
    );
}

// Row "list through a junction".
#[test]
fn list_through_a_junction() {
    let sandbox = Sandbox::new("listjunction");
    if let Err(reason) = junction(&sandbox.inside().join("j"), &sandbox.outside()) {
        return not_run("list through a junction", &reason);
    }
    let root = sandbox.root();
    assert_denied(
        root.list(&["j"]),
        is_denied_reparse,
        "list through a junction",
    );
    assert_denied(root.stat(&["j"]), is_denied_reparse, "stat of a junction");
}

// Row "create below a link".
#[test]
fn create_below_a_link() {
    let sandbox = Sandbox::new("createlink");
    if let Err(reason) = junction(&sandbox.inside().join("j"), &sandbox.outside()) {
        return not_run("create below a link", &reason);
    }
    let before = sandbox.outside_snapshot();
    let root = sandbox.root();
    assert_denied(
        root.open_parent(&["j"]),
        is_denied_reparse,
        "create below a link",
    );
    assert_eq!(
        sandbox.outside_snapshot(),
        before,
        "the outside is unchanged"
    );
}

// Row "intermediate symlink".
#[test]
fn intermediate_symlink() {
    let sandbox = Sandbox::new("midlink");
    fs::write(sandbox.outside().join("a.txt"), SECRET).unwrap();
    if let Err(reason) = symlink_dir(&sandbox.inside().join("d"), &sandbox.outside()) {
        return not_run("intermediate symlink", &reason);
    }
    deny_all(
        &sandbox,
        &["d", "a.txt"],
        is_denied_reparse,
        "intermediate symlink",
    );
    deny(
        &sandbox,
        &["d"],
        is_denied_reparse,
        is_already_exists,
        "directory symlink as the leaf",
    );
}

// Row `ATK-fs-swapped-junction`. A second thread swaps `r/d` with a junction
// to the outside directory, by renames, while the reads run. A read can also
// fall in the moment between two renames, when no `r/d` exists: that is
// `NotFound`.
#[test]
fn atk_fs_swapped_junction() {
    let sandbox = Sandbox::new("swap");
    let inside = sandbox.inside();
    fs::create_dir(inside.join("d")).unwrap();
    fs::write(inside.join("d").join("a.txt"), b"inside").unwrap();
    fs::write(sandbox.outside().join("a.txt"), SECRET).unwrap();
    if let Err(reason) = junction(&inside.join("j"), &sandbox.outside()) {
        return not_run("ATK-fs-swapped-junction", &reason);
    }
    let root = sandbox.root();
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let stop = Arc::clone(&stop);
        let inside = inside.clone();
        thread::spawn(move || {
            let rename = |from: &str, to: &str| {
                while fs::rename(inside.join(from), inside.join(to)).is_err() {
                    if stop.load(Ordering::Relaxed) {
                        return false;
                    }
                }
                true
            };
            let mut swaps = 0_u64;
            while !stop.load(Ordering::Relaxed) {
                let done = rename("d", "real")
                    && rename("j", "d")
                    && rename("d", "j")
                    && rename("real", "d");
                if done {
                    swaps += 1;
                }
            }
            swaps
        })
    };
    let (mut inside_reads, mut denied, mut missing) = (0_u32, 0_u32, 0_u32);
    for _ in 0..10_000 {
        match root.open_read(&["d", "a.txt"]).map_err(ScopeError::from) {
            Ok(mut file) => {
                let bytes = read_opened(&mut file);
                assert_ne!(bytes, SECRET, "a read returned the outside content");
                assert_eq!(bytes, b"inside");
                inside_reads += 1;
            }
            Err(ScopeError::DeniedReparse) => denied += 1,
            Err(ScopeError::NotFound) => missing += 1,
            Err(other) => panic!("ATK-fs-swapped-junction: unexpected {other:?}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    let swaps = swapper.join().unwrap();
    eprintln!(
        "ATK-fs-swapped-junction: {inside_reads} inside, {denied} deniedReparse, \
         {missing} notFound, {swaps} swaps"
    );
    assert!(
        denied > 0 && inside_reads > 0,
        "the race was not exercised: {inside_reads} inside, {denied} denied"
    );
}

// Row `ATK-fs-case-alias`: the alias opens, and the caller sees the real name.
#[test]
fn atk_fs_case_alias() {
    let sandbox = Sandbox::new("case");
    fs::write(sandbox.inside().join("Notes.TXT"), b"notes").unwrap();
    fs::create_dir(sandbox.inside().join("Sub")).unwrap();
    fs::write(sandbox.inside().join("Sub").join("x.txt"), b"x").unwrap();
    let root = sandbox.root();
    let file = root.open_read(&["notes.txt"]).unwrap();
    assert_eq!(file.real_names(), ["Notes.TXT"]);
    drop(file);
    let replace = root.open_for_replace(&["NOTES.txt"]).unwrap();
    assert_eq!(replace.real_names(), ["Notes.TXT"]);
    drop(replace);
    assert_eq!(
        root.open_read(&["SUB", "X.TXT"]).unwrap().real_names(),
        ["Sub", "x.txt"]
    );
    assert_eq!(root.stat(&["sub"]).unwrap().real_names, ["Sub"]);
    assert_eq!(root.list(&["sUB"]).unwrap().real_names(), ["Sub"]);
    assert_eq!(
        fs::read(sandbox.inside().join("Notes.TXT")).unwrap(),
        b"notes"
    );
}

// Row `ATK-fs-short-name`: the 8.3 name opens, and the caller sees the long
// name.
#[test]
fn atk_fs_short_name() {
    let sandbox = Sandbox::new("shortname");
    let long_dir = sandbox.inside().join("LongDirectoryName");
    fs::create_dir(&long_dir).unwrap();
    let long = sandbox.inside().join("longfilename.text");
    fs::write(&long, b"long name").unwrap();
    let short_file = match short_name(&long) {
        Ok(short) => short,
        Err(reason) => return not_run("ATK-fs-short-name", &reason),
    };
    let short_dir = match short_name(&long_dir) {
        Ok(short) => short,
        Err(reason) => return not_run("ATK-fs-short-name", &reason),
    };
    assert_ne!(short_file, "longfilename.text");
    let root = sandbox.root();
    let mut file = root.open_read(&[&short_file]).unwrap();
    assert_eq!(file.real_names(), ["longfilename.text"]);
    assert_eq!(read_opened(&mut file), b"long name");
    drop(file);
    let parent = root.open_parent(&[&short_dir]).unwrap();
    assert_eq!(parent.real_names(), ["LongDirectoryName"]);
    drop(parent);
    // A create over the short name finds the long-named file.
    let before = sandbox.inside_snapshot();
    assert_denied(
        create(&root, &[&short_file]),
        is_already_exists,
        "create over a short name",
    );
    assert_eq!(sandbox.inside_snapshot(), before);
}

// Row `ATK-fs-replace-no-truncate`.
#[test]
fn atk_fs_replace_no_truncate() {
    let sandbox = Sandbox::new("notruncate");
    let path = sandbox.inside().join("a.txt");
    fs::write(&path, b"hello").unwrap();
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    let file = root.open_for_replace(&["a.txt"]).unwrap();
    // A second, independent handle still reads the old bytes while the
    // replace handle is open.
    assert_eq!(fs::read(&path).unwrap(), b"hello");
    drop(file);
    assert_eq!(fs::read(&path).unwrap(), b"hello");
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Row `ATK-fs-create-over-alias`.
#[test]
fn atk_fs_create_over_alias() {
    let sandbox = Sandbox::new("createalias");
    fs::write(sandbox.inside().join("Notes.txt"), b"keep me").unwrap();
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    let parent = root.open_parent(&[]).unwrap();
    assert_denied(
        parent.create_new("NOTES.TXT"),
        is_already_exists,
        "ATK-fs-create-over-alias",
    );
    assert_denied(
        create(&root, &["Notes.txt"]),
        is_already_exists,
        "create over the same name",
    );
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Row `ATK-fs-create-parent-swapped`.
#[test]
fn atk_fs_create_parent_swapped() {
    let sandbox = Sandbox::new("parentswap");
    let inside = sandbox.inside();
    fs::create_dir(inside.join("d")).unwrap();
    let root = sandbox.root();
    let parent = root.open_parent(&["d"]).unwrap();
    assert_eq!(parent.real_names(), ["d"]);
    fs::rename(inside.join("d"), inside.join("e")).unwrap();
    if let Err(reason) = junction(&inside.join("d"), &sandbox.outside()) {
        return not_run("ATK-fs-create-parent-swapped", &reason);
    }
    let before = sandbox.outside_snapshot();
    let file = parent.create_new("n.txt").unwrap();
    drop(file);
    assert!(
        inside.join("e").join("n.txt").is_file(),
        "the file is r/e/n.txt"
    );
    assert_eq!(
        sandbox.outside_snapshot(),
        before,
        "the outside is unchanged"
    );
}

// Row "create with a refused leaf".
#[test]
fn create_with_a_refused_leaf() {
    let sandbox = Sandbox::new("refusedleaf");
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    let parent = root.open_parent(&[]).unwrap();
    assert_denied(parent.create_new("nul.txt"), is_denied_name, "nul.txt");
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Row `ATK-fs-root-renamed`.
#[test]
fn atk_fs_root_renamed() {
    let sandbox = Sandbox::new("rename");
    fs::write(sandbox.inside().join("a.txt"), b"held").unwrap();
    let root = sandbox.root();

    let moved = sandbox.base().join("moved");
    fs::rename(sandbox.inside(), &moved).unwrap();
    fs::create_dir(sandbox.inside()).unwrap();
    fs::write(sandbox.inside().join("a.txt"), b"impostor").unwrap();

    let mut file = root.open_read(&["a.txt"]).unwrap();
    assert_eq!(read_opened(&mut file), b"held");
    drop(file);
    drop(create(&root, &["after.txt"]).unwrap());
    assert!(moved.join("after.txt").exists());
    assert!(!sandbox.inside().join("after.txt").exists());
}

// Row "list a file".
#[test]
fn list_a_file() {
    let sandbox = Sandbox::new("listfile");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    assert_denied(root.list(&["a.txt"]), is_not_a_directory, "list a file");
    assert_denied(
        root.open_read(&["a.txt", "b.txt"]),
        is_not_a_directory,
        "a file as a parent",
    );
    assert_denied(
        root.open_parent(&["a.txt"]),
        is_not_a_directory,
        "a file as a parent of a create",
    );
}

// Row "read a directory".
#[test]
fn read_a_directory() {
    let sandbox = Sandbox::new("readdir");
    fs::create_dir(sandbox.inside().join("d")).unwrap();
    let root = sandbox.root();
    assert_denied(root.open_read(&["d"]), is_not_a_file, "read a directory");
    assert_denied(
        root.open_for_replace(&["d"]),
        is_not_a_file,
        "replace a directory",
    );
    assert_denied(
        create(&root, &["d"]),
        is_already_exists,
        "create over a directory",
    );
}

// Row "missing parent".
#[test]
fn missing_parent() {
    let sandbox = Sandbox::new("missing");
    let root = sandbox.root();
    assert_denied(
        root.open_read(&["no", "a.txt"]),
        is_not_found,
        "missing parent",
    );
    assert_denied(root.open_read(&["a.txt"]), is_not_found, "missing file");
    assert_denied(
        root.open_for_replace(&["no", "a.txt"]),
        is_not_found,
        "missing parent, replace",
    );
    assert_denied(
        root.open_parent(&["no"]),
        is_not_found,
        "missing parent, create",
    );
    assert_denied(root.list(&["no"]), is_not_found, "missing directory, list");
    assert_denied(root.stat(&["no"]), is_not_found, "missing entry, stat");
    assert_eq!(fs::read_dir(sandbox.inside()).unwrap().count(), 0);
}

// Row "each reserved stem".
#[test]
fn each_reserved_stem() {
    for name in [
        "nul",
        "NUL.txt",
        "con .log",
        "COM0",
        "LPT9.x",
        "CONIN$",
        "COM\u{b9}",
        "aux.tar.gz",
    ] {
        assert_denied(check_name(name), is_denied_name, name);
    }
}

// Row "a stem in simple uppercase": U+0131 maps to `I`, so the stem is
// `CONIN$`. Refused by the name check, and nothing is created.
#[test]
fn a_stem_in_simple_uppercase() {
    let sandbox = Sandbox::new("dotlessi");
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    for name in ["CON\u{131}N$", "con\u{131}n$.txt"] {
        assert_denied(check_name(name), is_denied_name, name);
        let parent = root.open_parent(&[]).unwrap();
        assert_denied(parent.create_new(name), is_denied_name, name);
        assert_denied(root.open_read(&[name]), is_denied_name, name);
    }
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Rows "read the empty path" and "replace the empty path": the empty path
// names the root, a directory, where a file is required.
#[test]
fn read_the_empty_path() {
    let sandbox = Sandbox::new("emptyread");
    let root = sandbox.root();
    assert_denied(root.open_read(&[]), is_not_a_file, "read the empty path");
}

#[test]
fn replace_the_empty_path() {
    let sandbox = Sandbox::new("emptyreplace");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    assert_denied(
        root.open_for_replace(&[]),
        is_not_a_file,
        "replace the empty path",
    );
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Row "create at the empty path": there is no leaf, so no name to create.
#[test]
fn create_at_the_empty_path() {
    let sandbox = Sandbox::new("emptycreate");
    let before = sandbox.inside_snapshot();
    let root = sandbox.root();
    let parent = root.open_parent(&[]).unwrap();
    assert_denied(
        parent.create_new(""),
        is_denied_name,
        "create at the empty path",
    );
    assert_eq!(sandbox.inside_snapshot(), before, "r is unchanged");
}

// Row "name profile".
#[test]
fn name_profile() {
    for name in [".", "..", "a/b", "a\\b", "a:b", "a.", "a ", "a\u{7}", ""] {
        assert_denied(check_name(name), is_denied_name, &format!("{name:?}"));
    }
}

// Row "alternate stream".
#[test]
fn alternate_stream() {
    let sandbox = Sandbox::new("datastream");
    fs::write(sandbox.inside().join("a.txt"), b"hello").unwrap();
    let root = sandbox.root();
    assert_denied(check_name("a.txt::$DATA"), is_denied_name, "check_name");
    assert_denied(
        root.open_read(&["a.txt::$DATA"]),
        is_denied_name,
        "alternate stream",
    );
}

// Every operation applies the profile to every name before anything is
// opened.
#[test]
fn every_operation_checks_every_name() {
    let sandbox = Sandbox::new("everyname");
    fs::create_dir(sandbox.inside().join("d")).unwrap();
    let root = sandbox.root();
    for names in [&["d", ".."][..], &["..", "d"][..], &["d", "nul"][..]] {
        assert_denied(root.list(names), is_denied_name, "list");
        assert_denied(root.stat(names), is_denied_name, "stat");
        assert_denied(root.open_parent(names), is_denied_name, "open_parent");
    }
}

// The feasibility record's path-shape attacks, kept as regressions.

#[test]
fn dot_dot_escape_is_denied() {
    let sandbox = Sandbox::new("dotdot");
    deny_all(
        &sandbox,
        &["..", "outside", "secret.txt"],
        is_denied_name,
        "..",
    );
    deny_all(
        &sandbox,
        &["..\\outside\\secret.txt"],
        is_denied_name,
        "..\\ in one name",
    );
    deny_all(&sandbox, &[".", "secret.txt"], is_denied_name, ".");
    deny_all(
        &sandbox,
        &["", "secret.txt"],
        is_denied_name,
        "empty component",
    );
}

#[test]
fn absolute_path_is_denied() {
    let sandbox = Sandbox::new("absolute");
    let secret = sandbox.secret().display().to_string();
    deny_all(&sandbox, &[&secret], is_denied_name, "absolute path");
    let rooted = secret.get(2..).unwrap().to_owned();
    deny_all(&sandbox, &[&rooted], is_denied_name, "drive-rooted path");
}

#[test]
fn drive_relative_path_is_denied() {
    let sandbox = Sandbox::new("driverel");
    let drive = sandbox
        .secret()
        .display()
        .to_string()
        .get(..2)
        .unwrap()
        .to_owned();
    let relative = format!("{drive}secret.txt");
    deny_all(
        &sandbox,
        &[&relative],
        is_denied_name,
        "drive-relative C:foo",
    );
    deny_all(
        &sandbox,
        &[&drive, "secret.txt"],
        is_denied_name,
        "drive as a component",
    );
}

#[test]
fn unc_path_is_denied() {
    let sandbox = Sandbox::new("unc");
    let secret = sandbox.secret().display().to_string();
    let drive_letter = secret.get(..1).unwrap();
    let unc = format!(
        "\\\\localhost\\{drive_letter}$\\{}",
        secret.get(3..).unwrap()
    );
    deny_all(&sandbox, &[&unc], is_denied_name, "UNC path");
    deny_all(
        &sandbox,
        &["", "", "localhost", &format!("{drive_letter}$")],
        is_denied_name,
        "UNC as components",
    );
}

#[test]
fn verbatim_prefix_is_denied() {
    let sandbox = Sandbox::new("verbatim");
    let secret = sandbox.secret().display().to_string();
    deny_all(
        &sandbox,
        &[&format!("\\\\?\\{secret}")],
        is_denied_name,
        "\\\\?\\ prefix",
    );
    deny_all(
        &sandbox,
        &[&format!("\\??\\{secret}")],
        is_denied_name,
        "\\??\\ prefix",
    );
    deny_all(
        &sandbox,
        &["\\\\.\\NUL"],
        is_denied_name,
        "\\\\.\\ device prefix",
    );
}

#[test]
fn a_separator_inside_a_component_is_denied() {
    let sandbox = Sandbox::new("separator");
    fs::create_dir(sandbox.inside().join("sub")).unwrap();
    fs::write(sandbox.inside().join("sub").join("notes.txt"), b"x").unwrap();
    deny_all(
        &sandbox,
        &["sub/notes.txt"],
        is_denied_name,
        "slash inside a name",
    );
    deny_all(
        &sandbox,
        &["sub\\notes.txt"],
        is_denied_name,
        "backslash inside a name",
    );
}

#[test]
fn a_trailing_dot_or_space_is_denied() {
    let sandbox = Sandbox::new("trailing");
    fs::write(sandbox.inside().join("notes.txt"), b"x").unwrap();
    deny_all(&sandbox, &["notes.txt."], is_denied_name, "trailing dot");
    deny_all(
        &sandbox,
        &["sub.", "notes.txt"],
        is_denied_name,
        "trailing dot, directory",
    );
    deny_all(&sandbox, &["notes.txt "], is_denied_name, "trailing space");
    assert_eq!(fs::read_dir(sandbox.inside()).unwrap().count(), 1);
}

#[test]
fn a_named_stream_is_denied() {
    let sandbox = Sandbox::new("namedstream");
    let file = sandbox.inside().join("file.txt");
    fs::write(&file, b"x").unwrap();
    deny_all(
        &sandbox,
        &["file.txt:stream"],
        is_denied_name,
        "named stream",
    );
    let stream = sandbox.inside().join("file.txt:stream");
    assert!(fs::metadata(&stream).is_err(), "no stream was created");
    assert_eq!(fs::read(&file).unwrap(), b"x");
}

#[test]
fn a_device_name_is_denied_and_nothing_is_created() {
    for device in [
        "NUL",
        "nul",
        "CON",
        "COM1",
        "aux.txt",
        "CONOUT$",
        "lpt\u{b2}",
    ] {
        let sandbox = Sandbox::new("device");
        deny_all(&sandbox, &[device], is_denied_name, device);
        deny_all(&sandbox, &[device, "x.txt"], is_denied_name, device);
        assert_eq!(
            fs::read_dir(sandbox.inside()).unwrap().count(),
            0,
            "{device}: nothing was created inside the root"
        );
    }
}

#[test]
fn create_through_a_dangling_symlink_is_refused() {
    let sandbox = Sandbox::new("dangling");
    let missing = sandbox.outside().join("missing.txt");
    if let Err(reason) = symlink_file(&sandbox.inside().join("dangling.txt"), &missing)
    {
        return not_run("create through a dangling symlink", &reason);
    }
    let missing_dir = sandbox.outside().join("missing");
    if let Err(reason) =
        symlink_dir(&sandbox.inside().join("danglingdir"), &missing_dir)
    {
        return not_run("create through a dangling symlink", &reason);
    }
    let before = sandbox.outside_snapshot();
    let root = sandbox.root();
    assert_denied(
        create(&root, &["dangling.txt"]),
        is_already_exists,
        "create on a dangling file symlink",
    );
    assert_denied(
        create(&root, &["danglingdir", "x.txt"]),
        is_denied_reparse,
        "create below a dangling directory symlink",
    );
    assert_denied(
        root.open_for_replace(&["dangling.txt"]),
        is_denied_reparse,
        "replace through a dangling symlink",
    );
    assert!(!missing.exists(), "the dangling target was not created");
    assert!(!missing_dir.exists());
    assert_eq!(sandbox.outside_snapshot(), before);
}

#[test]
fn a_root_that_is_a_junction_is_denied() {
    let sandbox = Sandbox::new("rootjunction");
    let link = sandbox.base().join("rootlink");
    if let Err(reason) = junction(&link, &sandbox.outside()) {
        return not_run("a root that is a junction", &reason);
    }
    assert_denied(
        ScopedDir::open_root(&link),
        is_denied_reparse,
        "root junction",
    );
}

// A hard link to an outside file is an ordinary entry: it reads, and its
// link count tells the caller that a replace would be seen outside.
#[test]
fn a_hard_link_to_an_outside_file_reports_its_count() {
    let sandbox = Sandbox::new("hardlink");
    let alias = sandbox.inside().join("alias.txt");
    if let Err(error) = fs::hard_link(sandbox.secret(), &alias) {
        return not_run("hard link to an outside file", &error.to_string());
    }
    let root = sandbox.root();
    let mut read = root.open_read(&["alias.txt"]).unwrap();
    assert_eq!(read.links(), 2);
    assert_eq!(read_opened(&mut read), SECRET);
    drop(read);
    assert_denied(
        create(&root, &["alias.txt"]),
        is_already_exists,
        "create over a hard link",
    );
    let replace = root.open_for_replace(&["alias.txt"]).unwrap();
    assert_eq!(replace.links(), 2);
    drop(replace);
    assert_eq!(
        fs::read(sandbox.secret()).unwrap(),
        SECRET,
        "no change at open"
    );
}
