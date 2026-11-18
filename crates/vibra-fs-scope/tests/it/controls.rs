//! Positive controls: the operations the contract permits.

use std::fs;

use vibra_fs_scope::{ScopeError, ScopedDir};

use crate::Sandbox;

#[test]
fn reads_an_existing_file() {
    let sandbox = Sandbox::new("read");
    fs::write(sandbox.inside().join("notes.txt"), b"inside notes").unwrap();
    let root = sandbox.root();
    let mut file = root.open_read(&["notes.txt"]).unwrap();
    assert_eq!(file.read_all().unwrap(), b"inside notes");
}

#[test]
fn creates_a_new_file() {
    let sandbox = Sandbox::new("create");
    let root = sandbox.root();
    let mut file = root.create_new(&["fresh.txt"]).unwrap();
    file.write_all(b"new bytes").unwrap();
    drop(file);
    assert_eq!(
        fs::read(sandbox.inside().join("fresh.txt")).unwrap(),
        b"new bytes"
    );
}

#[test]
fn opens_nested_directories_to_depth_three() {
    let sandbox = Sandbox::new("nested");
    let deep = sandbox.inside().join("a").join("b").join("c");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("deep.txt"), b"deep").unwrap();
    let root = sandbox.root();

    let mut read = root.open_read(&["a", "b", "c", "deep.txt"]).unwrap();
    assert_eq!(read.read_all().unwrap(), b"deep");

    let mut created = root.create_new(&["a", "b", "c", "made.txt"]).unwrap();
    created.write_all(b"made").unwrap();
    drop(created);
    assert_eq!(fs::read(deep.join("made.txt")).unwrap(), b"made");

    let mut replaced = root.open_for_replace(&["a", "b", "c", "deep.txt"]).unwrap();
    replaced.replace_contents(b"replaced").unwrap();
    drop(replaced);
    assert_eq!(fs::read(deep.join("deep.txt")).unwrap(), b"replaced");
}

#[test]
fn replace_does_not_truncate_at_open() {
    let sandbox = Sandbox::new("replace");
    let path = sandbox.inside().join("data.txt");
    fs::write(&path, b"the old bytes").unwrap();
    let root = sandbox.root();

    let mut file = root.open_for_replace(&["data.txt"]).unwrap();
    // A second, independent handle still reads the old bytes after the open
    // returned and before the deliberate replacement.
    assert_eq!(fs::read(&path).unwrap(), b"the old bytes");
    assert_eq!(fs::metadata(&path).unwrap().len(), 13);

    file.replace_contents(b"new").unwrap();
    drop(file);
    assert_eq!(fs::read(&path).unwrap(), b"new");
}

#[test]
fn a_missing_file_is_not_found() {
    let sandbox = Sandbox::new("missing");
    let root = sandbox.root();
    assert!(matches!(
        root.open_read(&["absent.txt"]),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        root.open_for_replace(&["absent", "x.txt"]),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        root.create_new(&["absent", "x.txt"]),
        Err(ScopeError::NotFound)
    ));
}

#[test]
fn the_root_follows_the_directory_it_opened() {
    let sandbox = Sandbox::new("rename");
    fs::write(sandbox.inside().join("notes.txt"), b"held").unwrap();
    let root = sandbox.root();

    // Rename the held directory and put another directory at its old path.
    let moved = sandbox.base().join("moved");
    fs::rename(sandbox.inside(), &moved).unwrap();
    fs::create_dir(sandbox.inside()).unwrap();
    fs::write(sandbox.inside().join("notes.txt"), b"impostor").unwrap();

    let mut file = root.open_read(&["notes.txt"]).unwrap();
    assert_eq!(file.read_all().unwrap(), b"held");
    drop(file);
    let mut created = root.create_new(&["after.txt"]).unwrap();
    created.write_all(b"x").unwrap();
    drop(created);
    assert!(moved.join("after.txt").exists());
    assert!(!sandbox.inside().join("after.txt").exists());
}

#[test]
fn a_root_that_is_a_file_is_refused() {
    let sandbox = Sandbox::new("rootfile");
    assert!(matches!(
        ScopedDir::open_root(&sandbox.secret()),
        Err(ScopeError::Io(error)) if error.kind() == std::io::ErrorKind::NotADirectory
    ));
}
