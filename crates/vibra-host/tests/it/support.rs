//! Helpers shared by the `vibra-host` integration tests: the reference
//! implementation of the value-access trait, the way a provider's failure is
//! unwrapped, the world and its provider, and a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_host::{
    Failure, HostEnum, HostRecord, HostValues, HostWorld, ReadError, WorldProvider,
};

/// The reference implementation of the value-access trait: a plain tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tree {
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

#[derive(Default)]
pub(crate) struct Plain;

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

/// The error of a provider call, with memory exhaustion a test failure.
pub(crate) fn done<T, E: std::fmt::Debug>(
    result: Result<T, Failure<E>>,
) -> Result<T, E> {
    match result {
        Ok(value) => Ok(value),
        Err(Failure::Error(error)) => Err(error),
        Err(Failure::MemoryExhausted) => panic!("memory exhausted"),
    }
}

/// The world `text` decodes to.
pub(crate) fn world(text: &str) -> HostWorld {
    match HostWorld::decode("world.vibon", text) {
        Ok(world) => world,
        Err(diagnostics) => panic!("{diagnostics:?}"),
    }
}

/// The provider over the world `text` decodes to.
pub(crate) fn provider(text: &str) -> WorldProvider {
    WorldProvider::new(&world(text))
}

/// A counter, not the clock, makes the name unique within the process: the
/// clock of some platforms is too coarse to tell two parallel tests apart.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// A fresh directory under the system temporary directory, removed on drop.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "vibra-host-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// The path of `relative` below the directory, as the platform spells it.
    pub(crate) fn at(&self, relative: &str) -> String {
        self.0
            .join(relative)
            .to_str()
            .expect("utf-8 path")
            .to_owned()
    }

    /// The directory's own spelling, with `/` separators.
    pub(crate) fn root(&self) -> String {
        self.0.to_str().expect("utf-8 path").replace('\\', "/")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
