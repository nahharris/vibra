//! The six filesystem operations through both providers (Milestone 4, Step 17).
//!
//! `docs/spec/06-runtime.md`, "The host registry". Each operation is written once
//! in `vibra-host`: a call is read from the arguments by [`HostCall::read`], run
//! by [`perform`], and built back by [`build_reply`]. These tests drive that path
//! with the reference tree implementation of [`HostValues`], once over the
//! system provider against a real temporary directory under the system temporary
//! directory (removed afterwards), and once over the world provider, and require
//! the same reply from both for every call whose meaning is the same on every
//! host.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::PathBuf;

use vibra_host::{
    HostCall, HostEnum, HostProvider, HostRecord, HostValues, HostWorld, ReadError,
    SystemProvider, WorldProvider, build_reply, perform,
};

/// The reference implementation of the value-access trait: a plain tree.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tree {
    Void,
    Text(String),
    Path(String),
    Bytes(Vec<u8>),
    I64(i64),
    U64(u64),
    Array(Vec<Tree>),
    Enum(HostEnum, u32, Option<Box<Tree>>),
    Record(HostRecord, Vec<Tree>),
}

struct Plain;

impl HostValues for Plain {
    type Value = Tree;

    fn read_text(&mut self, value: &Tree) -> Result<String, ReadError> {
        match value {
            Tree::Text(text) => Ok(text.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_bytes(&mut self, value: &Tree) -> Result<Vec<u8>, ReadError> {
        match value {
            Tree::Bytes(bytes) => Ok(bytes.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_path(&mut self, value: &Tree) -> Result<String, ReadError> {
        match value {
            Tree::Path(text) => Ok(text.clone()),
            _ => Err(ReadError::Value),
        }
    }
    fn read_u64(&mut self, value: &Tree) -> Result<u64, ReadError> {
        match value {
            Tree::U64(number) => Ok(*number),
            _ => Err(ReadError::Value),
        }
    }
    fn build_void(&mut self) -> Tree {
        Tree::Void
    }
    fn build_text(&mut self, text: &str) -> Tree {
        Tree::Text(text.to_owned())
    }
    fn build_bytes(&mut self, bytes: &[u8]) -> Tree {
        Tree::Bytes(bytes.to_vec())
    }
    fn build_i64(&mut self, value: i64) -> Tree {
        Tree::I64(value)
    }
    fn build_u64(&mut self, value: u64) -> Tree {
        Tree::U64(value)
    }
    fn build_array(&mut self, elements: Vec<Tree>) -> Tree {
        Tree::Array(elements)
    }
    fn build_enum(
        &mut self,
        ty: HostEnum,
        variant: u32,
        payload: Option<Tree>,
    ) -> Tree {
        Tree::Enum(ty, variant, payload.map(Box::new))
    }
    fn build_record(&mut self, ty: HostRecord, fields: Vec<Tree>) -> Tree {
        Tree::Record(ty, fields)
    }
}

const OK: u32 = 0;
const ERR: u32 = 1;

fn ok(payload: Option<Tree>) -> Tree {
    Tree::Enum(HostEnum::Result, OK, payload.map(Box::new))
}

fn fs_err(name: &str) -> Tree {
    let index = vibra_host::FsError::ALL
        .iter()
        .position(|error| error.name() == name)
        .expect("a variant of fs-error");
    Tree::Enum(
        HostEnum::Result,
        ERR,
        Some(Box::new(Tree::Enum(
            HostEnum::FsError,
            u32::try_from(index).unwrap(),
            None,
        ))),
    )
}

fn metadata(kind: &str, size: u64) -> Tree {
    let index = vibra_host::FsKind::ALL
        .iter()
        .position(|candidate| candidate.name() == kind)
        .expect("a variant of fs-kind");
    ok(Some(Tree::Record(
        HostRecord::FsMetadata,
        vec![
            Tree::Enum(HostEnum::FsKind, u32::try_from(index).unwrap(), None),
            Tree::U64(size),
        ],
    )))
}

fn names(entries: &[&str]) -> Tree {
    ok(Some(Tree::Array(
        entries
            .iter()
            .map(|name| Tree::Text((*name).to_owned()))
            .collect(),
    )))
}

fn bytes(contents: &[u8]) -> Tree {
    ok(Some(Tree::Bytes(contents.to_vec())))
}

fn text(contents: &str) -> Tree {
    ok(Some(Tree::Text(contents.to_owned())))
}

/// Reads the call of `symbol`, performs it on `provider`, and builds the reply.
fn call(provider: &mut dyn HostProvider, symbol: &str, arguments: &[Tree]) -> Tree {
    let mut values = Plain;
    let call = HostCall::read(symbol, arguments, &mut values).expect("a readable call");
    let reply = perform(&call, provider).expect("no memory exhaustion");
    build_reply(&reply, &mut values)
}

fn path(at: &str) -> Tree {
    Tree::Path(at.to_owned())
}

/// A fresh directory that is removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        // A counter, not the clock, makes the name unique within the process: the
        // clock of some platforms is too coarse to tell two parallel tests apart.
        static NEXT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vibra-fs17-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }

    /// The directory's own spelling, with `/` separators.
    fn root(&self) -> String {
        self.0.to_str().expect("utf-8 path").replace('\\', "/")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The two providers over the same tree: the real temporary directory, and a
/// world whose root holds the same entries. `at` takes a path below the tree.
struct Pair {
    scratch: Scratch,
    system: SystemProvider,
    world: WorldProvider,
}

impl Pair {
    fn new(name: &str, world: &str) -> Self {
        let world = HostWorld::decode("world.vibon", world).expect("a world");
        Self {
            scratch: Scratch::new(name),
            system: SystemProvider::new(),
            world: WorldProvider::new(&world),
        }
    }

    /// Seeds the real directory with `files` (`None` for a directory).
    fn seed(&self, entries: &[(&str, Option<&[u8]>)]) {
        for (relative, contents) in entries {
            let target = self.scratch.0.join(relative);
            match contents {
                Some(contents) => {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).expect("parent");
                    }
                    std::fs::write(&target, contents).expect("seed file");
                }
                None => std::fs::create_dir_all(&target).expect("seed directory"),
            }
        }
    }

    /// The same call on both providers, which must agree; answers the reply.
    fn both(&mut self, symbol: &str, relative: &str, rest: &[Tree]) -> Tree {
        let mut system_arguments =
            vec![path(&format!("{}/{relative}", self.scratch.root()))];
        let mut world_arguments = vec![path(&format!("/{relative}"))];
        system_arguments.extend_from_slice(rest);
        world_arguments.extend_from_slice(rest);
        let real = call(&mut self.system, symbol, &system_arguments);
        let model = call(&mut self.world, symbol, &world_arguments);
        assert_eq!(real, model, "{symbol} {relative}: system against world");
        real
    }
}

#[test]
fn reads_of_files_in_both_providers_agree_including_non_ascii_and_the_empty_file() {
    let mut pair = Pair::new(
        "reads",
        r#"(record format: @host-world.v1 files: (dict "/h.txt" "héllo 😀\n" "/raw.bin" (record kind: @bytes values: (array 0u8 255u8 104u8)) "/empty" ""))"#,
    );
    pair.seed(&[
        ("h.txt", Some("héllo 😀\n".as_bytes())),
        ("raw.bin", Some(&[0, 255, 104])),
        ("empty", Some(&[])),
    ]);
    assert_eq!(pair.both("fs.read-text", "h.txt", &[]), text("héllo 😀\n"));
    assert_eq!(
        pair.both("fs.read-file", "h.txt", &[]),
        bytes("héllo 😀\n".as_bytes())
    );
    assert_eq!(
        pair.both("fs.read-file", "raw.bin", &[]),
        bytes(&[0, 255, 104])
    );
    assert_eq!(
        pair.both("fs.read-text", "raw.bin", &[]),
        fs_err("invalid-text")
    );
    assert_eq!(pair.both("fs.read-file", "empty", &[]), bytes(&[]));
    assert_eq!(pair.both("fs.read-text", "empty", &[]), text(""));
    // The host resolves the text of a path: `.` components and doubled
    // separators name the same file.
    assert_eq!(
        pair.both("fs.read-text", "./h.txt", &[]),
        text("héllo 😀\n")
    );
    assert_eq!(
        pair.both("fs.read-text", "/h.txt", &[]).clone(),
        pair.both("fs.read-text", "h.txt", &[])
    );
}

#[test]
fn entries_are_sorted_by_scalar_order_and_a_stat_has_a_size_only_for_a_file() {
    let mut pair = Pair::new(
        "entries",
        r#"(record format: @host-world.v1 directories: (array "/box/empty" "/box/Zeta") files: (dict "/box/b" "" "/box/é" "" "/box/a" "" "/box/😀" "" "/box/10" "" "/box/9" "abc"))"#,
    );
    pair.seed(&[
        ("box/empty", None),
        ("box/Zeta", None),
        ("box/b", Some(&[])),
        ("box/é", Some(&[])),
        ("box/a", Some(&[])),
        ("box/😀", Some(&[])),
        ("box/10", Some(&[])),
        ("box/9", Some(b"abc")),
    ]);
    assert_eq!(
        pair.both("fs.read-entries", "box", &[]),
        names(&["10", "9", "Zeta", "a", "b", "empty", "é", "😀"])
    );
    assert_eq!(pair.both("fs.read-entries", "box/empty", &[]), names(&[]));
    assert_eq!(
        pair.both("fs.metadata-stat", "box/9", &[]),
        metadata("file", 3)
    );
    assert_eq!(
        pair.both("fs.metadata-stat", "box", &[]),
        metadata("directory", 0)
    );
    assert_eq!(
        pair.both("fs.metadata-stat", "box/empty", &[]),
        metadata("directory", 0)
    );
    assert_eq!(
        pair.both("fs.metadata-stat", "box/b", &[]),
        metadata("file", 0)
    );
}

#[test]
fn every_error_variant_of_every_row_is_the_same_in_both_providers() {
    let mut pair = Pair::new(
        "errors",
        r#"(record format: @host-world.v1 files: (dict "/dir/file.txt" "x"))"#,
    );
    pair.seed(&[("dir/file.txt", Some(b"x"))]);
    for symbol in ["fs.read-file", "fs.read-text", "fs.metadata-stat"] {
        assert_eq!(
            pair.both(symbol, "missing", &[]),
            fs_err("not-found"),
            "{symbol}"
        );
        assert_eq!(
            pair.both(symbol, "missing/inner", &[]),
            fs_err("not-found"),
            "{symbol}"
        );
        // A file where a directory is needed, as a parent.
        assert_eq!(
            pair.both(symbol, "dir/file.txt/below", &[]),
            fs_err("not-a-directory"),
            "{symbol}"
        );
    }
    assert_eq!(
        pair.both("fs.read-entries", "missing", &[]),
        fs_err("not-found")
    );
    assert_eq!(
        pair.both("fs.read-entries", "dir/file.txt", &[]),
        fs_err("not-a-directory")
    );
    assert_eq!(
        pair.both("fs.read-entries", "dir/file.txt/below", &[]),
        fs_err("not-a-directory")
    );
    // A directory where a file is read or replaced.
    assert_eq!(pair.both("fs.read-file", "dir", &[]), fs_err("not-a-file"));
    assert_eq!(pair.both("fs.read-text", "dir", &[]), fs_err("not-a-file"));
    assert_eq!(
        pair.both("fs.write-file", "dir", &[Tree::Bytes(vec![1])]),
        fs_err("not-a-file")
    );
    assert_eq!(
        pair.both("fs.write-text", "dir", &[Tree::Text("x".to_owned())]),
        fs_err("not-a-file")
    );
    // A missing parent for a write, and a file as the parent of a write.
    assert_eq!(
        pair.both(
            "fs.write-text",
            "no/such/dir/f",
            &[Tree::Text("x".to_owned())]
        ),
        fs_err("not-found")
    );
    assert_eq!(
        pair.both(
            "fs.write-file",
            "dir/file.txt/below",
            &[Tree::Bytes(vec![1])]
        ),
        fs_err("not-a-directory")
    );
    // A NUL is `invalid-path` for every row, and no row traps.
    for symbol in [
        "fs.read-file",
        "fs.read-text",
        "fs.read-entries",
        "fs.metadata-stat",
    ] {
        assert_eq!(
            call(&mut pair.system, symbol, &[path("a\0b")]),
            fs_err("invalid-path"),
            "{symbol}"
        );
        assert_eq!(
            call(&mut pair.world, symbol, &[path("a\0b")]),
            fs_err("invalid-path"),
            "{symbol}"
        );
    }
    for symbol in ["fs.write-file", "fs.write-text"] {
        let contents = if symbol == "fs.write-file" {
            Tree::Bytes(vec![])
        } else {
            Tree::Text(String::new())
        };
        for provider in [
            &mut pair.system as &mut dyn HostProvider,
            &mut pair.world as &mut dyn HostProvider,
        ] {
            assert_eq!(
                call(provider, symbol, &[path("\0"), contents.clone()]),
                fs_err("invalid-path"),
                "{symbol}"
            );
        }
    }
}

#[test]
fn a_write_creates_or_replaces_the_whole_file_and_a_failed_write_leaves_the_prior_contents()
 {
    let mut pair = Pair::new(
        "writes",
        r#"(record format: @host-world.v1 files: (dict "/log.txt" "zero" "/ro/keep" "kept"))"#,
    );
    pair.seed(&[("log.txt", Some(b"zero")), ("ro/keep", Some(b"kept"))]);
    let void = ok(None);
    assert_eq!(
        pair.both("fs.write-text", "log.txt", &[Tree::Text("one".to_owned())]),
        void
    );
    assert_eq!(pair.both("fs.read-text", "log.txt", &[]), text("one"));
    // Replacing is not appending, and a shorter value leaves nothing of the old.
    assert_eq!(
        pair.both("fs.write-text", "log.txt", &[Tree::Text("é".to_owned())]),
        void
    );
    assert_eq!(
        pair.both("fs.read-file", "log.txt", &[]),
        bytes("é".as_bytes())
    );
    assert_eq!(
        pair.both("fs.write-file", "log.txt", &[Tree::Bytes(vec![0, 255])]),
        void
    );
    assert_eq!(pair.both("fs.read-file", "log.txt", &[]), bytes(&[0, 255]));
    // A new file is created in an existing directory, and the listing sees it.
    assert_eq!(
        pair.both("fs.write-text", "ro/new", &[Tree::Text(String::new())]),
        void
    );
    assert_eq!(
        pair.both("fs.read-entries", "ro", &[]),
        names(&["keep", "new"])
    );
    // A failed write returns `err`, the next call runs, and the file is intact.
    // The operation is not atomic, but a refused write changes nothing here.
    assert_eq!(
        pair.both("fs.write-text", "ro", &[Tree::Text("x".to_owned())]),
        fs_err("not-a-file")
    );
    assert_eq!(
        pair.both("fs.write-text", "absent/f", &[Tree::Text("x".to_owned())]),
        fs_err("not-found")
    );
    assert_eq!(pair.both("fs.read-text", "ro/keep", &[]), text("kept"));
    // The system provider wrote to the real directory, and the world did not:
    // the two trees are separate, and the world never reaches the real files.
    assert_eq!(
        std::fs::read(pair.scratch.0.join("log.txt")).expect("real file"),
        [0, 255]
    );
}

#[test]
fn a_four_mebibyte_file_and_a_path_of_a_thousand_components() {
    let scratch = Scratch::new("bounds");
    let root = scratch.root();
    let mut system = SystemProvider::new();
    let mut world = WorldProvider::new(&HostWorld::default());
    let big = vec![0x5a_u8; 4 * 1024 * 1024];
    // Components of `.` are skipped by the host, so a path of a thousand
    // components names the same file as the plain one.
    // The world has no limit, so it takes a thousand. The system provider hands
    // the text to the operating system, whose path limit is a host condition
    // (1,024 bytes on macOS, 4,096 on Linux), so its path stays under 1,000 bytes.
    let system_components = 990_usize.saturating_sub(root.len()) / 2;
    for (provider, base, components) in [
        (
            &mut system as &mut dyn HostProvider,
            root.as_str(),
            system_components,
        ),
        (&mut world as &mut dyn HostProvider, "", 1000),
    ] {
        let dots = "/.".repeat(components);
        let file = format!("{base}/big.bin");
        assert_eq!(
            call(
                provider,
                "fs.write-file",
                &[path(&file), Tree::Bytes(big.clone())]
            ),
            ok(None)
        );
        assert_eq!(call(provider, "fs.read-file", &[path(&file)]), bytes(&big));
        assert_eq!(
            call(provider, "fs.metadata-stat", &[path(&file)]),
            metadata("file", 4 * 1024 * 1024)
        );
        let long = format!("{base}{dots}/big.bin");
        assert_eq!(call(provider, "fs.read-file", &[path(&long)]), bytes(&big));
    }
    // A path past every operating system's limit is an `err`, never a trap. Unix
    // answers `ENAMETOOLONG`, which is `invalid-path`; Windows reports the path
    // as not found, so only the closed `err` is common to the hosts.
    let too_long = format!("{root}{}", "/x".repeat(5000));
    let reply = call(&mut system, "fs.read-file", &[path(&too_long)]);
    #[cfg(unix)]
    assert_eq!(reply, fs_err("invalid-path"));
    assert!(
        reply == fs_err("invalid-path") || reply == fs_err("not-found"),
        "{reply:?}"
    );
}

#[test]
fn a_directory_of_ten_thousand_entries_lists_sorted_in_both_providers() {
    let scratch = Scratch::new("many");
    let root = scratch.root();
    let mut system = SystemProvider::new();
    std::fs::create_dir_all(scratch.0.join("many")).expect("directory");
    let mut world_text = String::from("(record format: @host-world.v1 files: (dict");
    let mut expected = Vec::new();
    for number in 0..10_000_u32 {
        let name = format!("f{number:05}");
        std::fs::write(scratch.0.join("many").join(&name), []).expect("entry");
        world_text.push_str(&format!(" \"/many/{name}\" \"\""));
        expected.push(name);
    }
    world_text.push_str("))");
    let mut world = WorldProvider::new(
        &HostWorld::decode("world.vibon", &world_text).expect("a world"),
    );
    let expected = ok(Some(Tree::Array(
        expected.into_iter().map(Tree::Text).collect(),
    )));
    assert_eq!(
        call(
            &mut system,
            "fs.read-entries",
            &[path(&format!("{root}/many"))]
        ),
        expected
    );
    assert_eq!(
        call(&mut world, "fs.read-entries", &[path("/many")]),
        expected
    );
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_unicode_is_invalid_text() {
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new("names");
    let name = std::ffi::OsStr::from_bytes(&[b'f', 0xff, b'g']);
    // Whether a name that is not Unicode can exist is a property of the volume:
    // macOS APFS refuses it, and so may a Linux filesystem. When the volume
    // cannot hold the name the condition cannot arise, so there is nothing to
    // assert; gating on the operating system would skip volumes that accept it.
    if std::fs::write(scratch.0.join(name), b"x").is_err() {
        return;
    }
    let mut system = SystemProvider::new();
    assert_eq!(
        call(&mut system, "fs.read-entries", &[path(&scratch.root())]),
        fs_err("invalid-text")
    );
}

#[cfg(unix)]
#[test]
fn a_device_is_the_other_kind_with_size_zero() {
    let mut system = SystemProvider::new();
    assert_eq!(
        call(&mut system, "fs.metadata-stat", &[path("/dev/null")]),
        metadata("other", 0)
    );
}

#[cfg(unix)]
#[test]
fn a_refusal_by_the_host_is_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("denied");
    let file = scratch.0.join("secret");
    std::fs::write(&file, b"x").expect("file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000))
        .expect("chmod");
    let mut system = SystemProvider::new();
    let at = path(&format!("{}/secret", scratch.root()));
    let reply = call(&mut system, "fs.read-file", std::slice::from_ref(&at));
    // A process that may read any file (root) cannot be refused.
    if std::fs::read(&file).is_err() {
        assert_eq!(reply, fs_err("permission-denied"));
        assert_eq!(
            call(
                &mut system,
                "fs.write-text",
                &[at, Tree::Text("y".to_owned())]
            ),
            fs_err("permission-denied")
        );
    }
}
