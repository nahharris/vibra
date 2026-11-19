//! The attack cases of the effects chapter. Each asserts the specific typed
//! denial and that the outside target was not read, created, or modified:
//! the outside tree is snapshotted before and compared after, and where the
//! secret file is the target it is held open with no sharing during the
//! attack, so a denial cannot come after an open of its data.

use std::fs;

use vibra_fs_scope::{ScopeError, ScopedDir};

use crate::{
    SECRET, Sandbox, assert_denied, is_already_exists, is_denied_name,
    is_denied_reparse, junction, not_run, short_name, symlink_dir, symlink_file,
};

/// Runs every operation on `names` and asserts `expected` for each, with the
/// secret locked and the outside tree unchanged.
fn deny_all(
    sandbox: &Sandbox,
    names: &[&str],
    expected: fn(&ScopeError) -> bool,
    case: &str,
) {
    deny(sandbox, names, expected, expected, case);
}

/// As [`deny_all`], with the denial of a read and a replace in `opened` and
/// that of a create in `created`. A create whose leaf is an existing link
/// finds an existing entry, so it is `alreadyExists`, not `deniedReparse`.
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
    assert_denied(root.create_new(names), created, &format!("{case}, create"));
    drop(lock);
    assert_eq!(
        sandbox.outside_snapshot(),
        before,
        "{case}: the outside tree is unchanged"
    );
    assert_eq!(fs::read(sandbox.secret()).unwrap(), SECRET);
}

// Links and reparse points.

#[test]
fn file_symlink_to_outside_is_denied() {
    let sandbox = Sandbox::new("filelink");
    if let Err(reason) =
        symlink_file(&sandbox.inside().join("link.txt"), &sandbox.secret())
    {
        return not_run("file symlink to outside", &reason);
    }
    deny(
        &sandbox,
        &["link.txt"],
        is_denied_reparse,
        is_already_exists,
        "file symlink",
    );
}

#[test]
fn directory_symlink_to_outside_is_denied() {
    let sandbox = Sandbox::new("dirlink");
    if let Err(reason) = symlink_dir(&sandbox.inside().join("link"), &sandbox.outside())
    {
        return not_run("directory symlink to outside", &reason);
    }
    deny_all(
        &sandbox,
        &["link", "secret.txt"],
        is_denied_reparse,
        "directory symlink",
    );
    deny_all(
        &sandbox,
        &["link", "new.txt"],
        is_denied_reparse,
        "directory symlink, new name",
    );
    deny(
        &sandbox,
        &["link"],
        is_denied_reparse,
        is_already_exists,
        "directory symlink as the leaf",
    );
}

#[test]
fn junction_to_outside_is_denied() {
    let sandbox = Sandbox::new("junction");
    if let Err(reason) = junction(&sandbox.inside().join("link"), &sandbox.outside()) {
        return not_run("junction to outside", &reason);
    }
    deny_all(
        &sandbox,
        &["link", "secret.txt"],
        is_denied_reparse,
        "junction",
    );
    deny_all(
        &sandbox,
        &["link", "new.txt"],
        is_denied_reparse,
        "junction, new name",
    );
    deny(
        &sandbox,
        &["link"],
        is_denied_reparse,
        is_already_exists,
        "junction as the leaf",
    );
}

#[test]
fn symlink_as_an_intermediate_component_is_denied() {
    let sandbox = Sandbox::new("midlink");
    fs::create_dir(sandbox.inside().join("real")).unwrap();
    if let Err(reason) =
        symlink_dir(&sandbox.inside().join("real").join("hop"), sandbox.base())
    {
        return not_run("symlink as an intermediate component", &reason);
    }
    deny_all(
        &sandbox,
        &["real", "hop", "outside", "secret.txt"],
        is_denied_reparse,
        "intermediate symlink",
    );
}

#[test]
fn a_directory_swapped_for_a_junction_between_opens_is_denied() {
    let sandbox = Sandbox::new("swap");
    let data = sandbox.inside().join("data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("secret.txt"), b"inside data").unwrap();
    let root = sandbox.root();
    let mut first = root.open_read(&["data", "secret.txt"]).unwrap();
    assert_eq!(first.read_all().unwrap(), b"inside data");
    drop(first);

    fs::rename(&data, sandbox.inside().join("data-old")).unwrap();
    if let Err(reason) = junction(&data, &sandbox.outside()) {
        return not_run("directory swapped for a junction", &reason);
    }
    let before = sandbox.outside_snapshot();
    let lock = sandbox.lock_secret();
    assert_denied(
        root.open_read(&["data", "secret.txt"]),
        is_denied_reparse,
        "swapped junction, read through the same held root",
    );
    assert_denied(
        root.open_for_replace(&["data", "secret.txt"]),
        is_denied_reparse,
        "swapped junction, replace",
    );
    assert_denied(
        root.create_new(&["data", "new.txt"]),
        is_denied_reparse,
        "swapped junction, create",
    );
    drop(lock);
    assert_eq!(sandbox.outside_snapshot(), before);
    assert_eq!(fs::read(sandbox.secret()).unwrap(), SECRET);
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

// Path shapes that are not one plain name per component.

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
    deny_all(&sandbox, &[], is_denied_name, "empty request");
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

// Aliases of a real name.

#[test]
fn a_case_different_name_is_denied() {
    let sandbox = Sandbox::new("case");
    let sub = sandbox.inside().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("notes.txt"), b"inside notes").unwrap();
    let root = sandbox.root();
    for names in [
        &["sub", "NOTES.TXT"][..],
        &["sub", "Notes.txt"][..],
        &["SUB", "notes.txt"][..],
    ] {
        assert_denied(root.open_read(names), is_denied_name, "case alias, read");
        assert_denied(
            root.open_for_replace(names),
            is_denied_name,
            "case alias, replace",
        );
        assert_denied(
            root.create_new(names),
            if names[0] == "SUB" {
                is_denied_name
            } else {
                is_already_exists
            },
            "case alias, create",
        );
    }
    assert_eq!(fs::read(sub.join("notes.txt")).unwrap(), b"inside notes");
    assert_eq!(
        fs::read_dir(&sub).unwrap().count(),
        1,
        "nothing was created"
    );
}

#[test]
fn an_eight_dot_three_short_name_is_denied() {
    let sandbox = Sandbox::new("shortname");
    let long_dir = sandbox.inside().join("LongDirectoryName");
    fs::create_dir(&long_dir).unwrap();
    let long = long_dir.join("LongFileNameExample.txt");
    fs::write(&long, b"long name").unwrap();
    let short_file = match short_name(&long) {
        Ok(short) => short,
        Err(reason) => return not_run("8.3 short name", &reason),
    };
    let short_dir = match short_name(&long_dir) {
        Ok(short) => short,
        Err(reason) => return not_run("8.3 short name", &reason),
    };
    assert_ne!(short_file, "LongFileNameExample.txt");
    // The short name really opens the file through the ordinary path rules.
    assert_eq!(fs::read(long_dir.join(&short_file)).unwrap(), b"long name");

    let root = sandbox.root();
    for names in [
        [short_dir.as_str(), "LongFileNameExample.txt"],
        ["LongDirectoryName", short_file.as_str()],
    ] {
        assert_denied(root.open_read(&names), is_denied_name, "short name, read");
        assert_denied(
            root.open_for_replace(&names),
            is_denied_name,
            "short name, replace",
        );
    }
    assert_denied(
        root.create_new(&["LongDirectoryName", &short_file]),
        is_already_exists,
        "short name, create",
    );
    assert_eq!(fs::read(&long).unwrap(), b"long name");
    assert_eq!(fs::read_dir(&long_dir).unwrap().count(), 1);
}

#[test]
fn a_trailing_dot_is_denied() {
    let sandbox = Sandbox::new("trailingdot");
    fs::write(sandbox.inside().join("notes.txt"), b"x").unwrap();
    deny_all(&sandbox, &["notes.txt."], is_denied_name, "trailing dot");
    deny_all(
        &sandbox,
        &["sub.", "notes.txt"],
        is_denied_name,
        "trailing dot, directory",
    );
    assert_eq!(fs::read_dir(sandbox.inside()).unwrap().count(), 1);
}

#[test]
fn a_trailing_space_is_denied() {
    let sandbox = Sandbox::new("trailingspace");
    fs::write(sandbox.inside().join("notes.txt"), b"x").unwrap();
    deny_all(&sandbox, &["notes.txt "], is_denied_name, "trailing space");
    assert_eq!(fs::read_dir(sandbox.inside()).unwrap().count(), 1);
}

#[test]
fn the_default_data_stream_syntax_is_denied() {
    let sandbox = Sandbox::new("datastream");
    fs::write(sandbox.inside().join("file.txt"), b"x").unwrap();
    deny_all(&sandbox, &["file.txt::$DATA"], is_denied_name, "::$DATA");
    assert_eq!(fs::read(sandbox.inside().join("file.txt")).unwrap(), b"x");
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

fn deny_device(case: &str, device: &str) {
    let sandbox = Sandbox::new(case);
    deny_all(&sandbox, &[device], is_denied_name, device);
    deny_all(&sandbox, &[device, "x.txt"], is_denied_name, device);
    assert_eq!(
        fs::read_dir(sandbox.inside()).unwrap().count(),
        0,
        "{device}: nothing was created inside the root"
    );
}

#[test]
fn the_nul_device_is_denied() {
    deny_device("nul", "NUL");
    deny_device("nullower", "nul");
}

#[test]
fn the_con_device_is_denied() {
    deny_device("con", "CON");
}

#[test]
fn the_com1_device_is_denied() {
    deny_device("com1", "COM1");
}

#[test]
fn a_device_name_with_an_extension_is_denied() {
    deny_device("auxtxt", "aux.txt");
}

// Creation.

#[test]
fn create_over_an_existing_file_is_refused() {
    let sandbox = Sandbox::new("createover");
    let path = sandbox.inside().join("notes.txt");
    fs::write(&path, b"keep me").unwrap();
    let root = sandbox.root();
    assert_denied(
        root.create_new(&["notes.txt"]),
        is_already_exists,
        "create over",
    );
    assert_denied(
        root.create_new(&["NOTES.txt"]),
        is_already_exists,
        "create over a case alias",
    );
    assert_eq!(fs::read(&path).unwrap(), b"keep me");
    assert_eq!(fs::read_dir(sandbox.inside()).unwrap().count(), 1);
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
        root.create_new(&["dangling.txt"]),
        is_already_exists,
        "create on a dangling file symlink",
    );
    assert_denied(
        root.create_new(&["danglingdir", "x.txt"]),
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

// Hard links: recorded, not denied. The specification states that a policy
// does not establish where data came from; the adapter cannot tell an
// ordinary file from one of several names of the same file by name alone.

#[test]
fn a_hard_link_to_an_outside_file_is_followed_as_the_specification_states() {
    let sandbox = Sandbox::new("hardlink");
    let alias = sandbox.inside().join("alias.txt");
    if let Err(error) = fs::hard_link(sandbox.secret(), &alias) {
        return not_run("hard link to an outside file", &error.to_string());
    }
    let root = sandbox.root();

    // A read through the permitted name reads the shared data.
    let mut read = root.open_read(&["alias.txt"]).expect("the hard link opens");
    assert_eq!(read.read_all().unwrap(), SECRET);
    drop(read);

    // Create-new does not replace it.
    assert_denied(
        root.create_new(&["alias.txt"]),
        is_already_exists,
        "create over a hard link",
    );

    // A replace through the permitted name changes the outside name's data.
    let mut replace = root
        .open_for_replace(&["alias.txt"])
        .expect("the hard link opens for replace");
    assert_eq!(
        fs::read(sandbox.secret()).unwrap(),
        SECRET,
        "no change at open"
    );
    replace.replace_contents(b"written inside").unwrap();
    drop(replace);
    assert_eq!(fs::read(sandbox.secret()).unwrap(), b"written inside");
}
