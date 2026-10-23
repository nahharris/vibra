//! The in-memory host world: the `@host-world.v1` document and its provider.
//!
//! `docs/spec/06-runtime.md`, "The host world". A world is a value. The decoder
//! turns the VIBON document into a [`HostWorld`], applying the defaults of the
//! chapter and reporting every violation as a `@data.*` diagnostic at the
//! document, and [`WorldProvider::new`] makes a fresh copy of it for one test or
//! one backend, so no two runs share anything.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::{DataNode, DataValue, IntegerSuffix, Literal, parse_data};

use crate::provider::{
    Done, EnvError, Failure, FsError, FsKind, HostProvider, IoError, Metadata,
    RandomError, TimeError, try_copy,
};
use crate::random::SplitMix64;

/// The fields of the document, in the order of the runtime chapter.
const WORLD_FIELDS: &[&str] = &[
    "format",
    "stdin",
    "directories",
    "files",
    "env",
    "args",
    "clock",
    "random",
];
const CLOCK_FIELDS: &[&str] = &["wall", "monotonic", "step"];
const RANDOM_FIELDS: &[&str] = &["seed"];

/// A path as its resolved components from the root.
type Components = Vec<String>;

/// A decoded `@host-world.v1` document.
///
/// The default world has the root directory and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostWorld {
    stdin: Vec<u8>,
    /// Every directory, the root and the ancestors of every listed path included.
    directories: BTreeSet<Components>,
    files: BTreeMap<Components, Vec<u8>>,
    env: BTreeMap<String, String>,
    args: Vec<String>,
    wall: i64,
    monotonic: u64,
    step: u64,
    seed: u64,
}

impl Default for HostWorld {
    fn default() -> Self {
        Self {
            stdin: Vec::new(),
            directories: BTreeSet::from([Vec::new()]),
            files: BTreeMap::new(),
            env: BTreeMap::new(),
            args: Vec::new(),
            wall: 0,
            monotonic: 0,
            step: 1,
            seed: 0,
        }
    }
}

impl HostWorld {
    /// Decodes the text of a `@host-world.v1` document.
    ///
    /// `source_id` is the document's identity in diagnostics; it names a
    /// `.vibon` file. Every diagnostic is reported once, at the document.
    pub fn decode(source_id: &str, text: &str) -> Result<Self, Vec<Diagnostic>> {
        let document = match parse_data(Path::new(source_id), text) {
            Ok(document) => document,
            Err(error) => {
                return Err(vec![
                    Diagnostic::new(
                        DiagnosticCode::DataInvalidExtension,
                        ByteSpan::new(0, 0),
                        error.to_string(),
                    )
                    .with_source_id(source_id),
                ]);
            }
        };
        if !document.accepted() || document.recovered() {
            let mut diagnostics = document
                .diagnostics()
                .iter()
                .cloned()
                .map(|diagnostic| diagnostic.with_source_id(source_id))
                .collect::<Vec<_>>();
            if diagnostics.is_empty() {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::DataInvalidShape,
                        ByteSpan::new(0, text.len()),
                        "the host world is not one valid VIBON value",
                    )
                    .with_source_id(source_id),
                );
            }
            return Err(diagnostics);
        }
        let Some(root) = document.data() else {
            return Err(vec![
                Diagnostic::new(
                    DiagnosticCode::DataInvalidShape,
                    ByteSpan::new(0, text.len()),
                    "the host world has no data value",
                )
                .with_source_id(source_id),
            ]);
        };
        let mut decoder = Decoder {
            source_id,
            diagnostics: Vec::new(),
        };
        let world = decoder.world(root);
        match world {
            Some(world) if decoder.diagnostics.is_empty() => Ok(world),
            _ => Err(decoder.diagnostics),
        }
    }
}

struct Decoder<'a> {
    source_id: &'a str,
    diagnostics: Vec<Diagnostic>,
}

type Fields<'n> = BTreeMap<&'n str, &'n DataNode>;

impl Decoder<'_> {
    fn error(&mut self, code: DiagnosticCode, node: &DataNode, message: &str) {
        self.diagnostics.push(
            Diagnostic::new(code, node.span(), message).with_source_id(self.source_id),
        );
    }

    fn shape(&mut self, node: &DataNode, message: &str) {
        self.error(DiagnosticCode::DataInvalidShape, node, message);
    }

    fn value(&mut self, node: &DataNode, message: &str) {
        self.error(DiagnosticCode::DataInvalidValue, node, message);
    }

    /// The fields of a closed record: unknown and repeated fields are reported.
    fn record<'n>(
        &mut self,
        node: &'n DataNode,
        allowed: &[&str],
        what: &str,
    ) -> Option<Fields<'n>> {
        let DataValue::Record(fields) = node.value() else {
            self.shape(node, &format!("{what} must be a record"));
            return None;
        };
        let mut found = Fields::new();
        for field in fields {
            let label = field.label().value();
            if !allowed.contains(&label) {
                self.shape(field.value(), &format!("unknown {what} field `{label}`"));
            } else if found.insert(label, field.value()).is_some() {
                self.shape(field.value(), &format!("repeated {what} field `{label}`"));
            }
        }
        Some(found)
    }

    fn world(&mut self, root: &DataNode) -> Option<HostWorld> {
        let fields = self.record(root, WORLD_FIELDS, "host world")?;
        let mut world = HostWorld::default();
        let mut valid = true;

        match fields.get("format") {
            None => {
                self.shape(root, "the host world needs its `format` field");
                valid = false;
            }
            Some(node) => match node.value() {
                DataValue::Atom(atom) if atom.value() == "host-world.v1" => {}
                DataValue::Atom(_) => {
                    self.value(node, "the host world format must be @host-world.v1");
                    valid = false;
                }
                _ => {
                    self.shape(node, "the host world format must be an atom");
                    valid = false;
                }
            },
        }
        if let Some(node) = fields.get("stdin") {
            match self.contents(node, "standard input") {
                Some(bytes) => world.stdin = bytes,
                None => valid = false,
            }
        }
        let directories = fields
            .get("directories")
            .map_or_else(|| Some(Vec::new()), |node| self.directories(node));
        let files = fields
            .get("files")
            .map_or_else(|| Some(Vec::new()), |node| self.files(node));
        match (directories, files) {
            (Some(directories), Some(files)) => {
                valid &= self.tree(&mut world, directories, files);
            }
            _ => valid = false,
        }
        if let Some(node) = fields.get("env") {
            match self.environment(node) {
                Some(env) => world.env = env,
                None => valid = false,
            }
        }
        if let Some(node) = fields.get("args") {
            match self.strings(node, "the arguments") {
                Some(args) => world.args = args,
                None => valid = false,
            }
        }
        if let Some(node) = fields.get("clock") {
            valid &= self.clock(node, &mut world);
        }
        if let Some(node) = fields.get("random") {
            valid &= self.random(node, &mut world);
        }
        valid.then_some(world)
    }

    fn string(&mut self, node: &DataNode, what: &str) -> Option<String> {
        if let DataValue::Literal(Literal::String(literal)) = node.value() {
            Some(literal.value().to_owned())
        } else {
            self.shape(node, &format!("{what} must be a string"));
            None
        }
    }

    fn strings(&mut self, node: &DataNode, what: &str) -> Option<Vec<String>> {
        let DataValue::Array(items) = node.value() else {
            self.shape(node, &format!("{what} must be an array"));
            return None;
        };
        let mut strings = Vec::with_capacity(items.len());
        let mut valid = true;
        for item in items {
            match self.string(item, &format!("an entry of {what}")) {
                Some(string) => strings.push(string),
                None => valid = false,
            }
        }
        valid.then_some(strings)
    }

    /// An integer literal of exactly the suffix `suffix`, within `min..=max`.
    fn integer(
        &mut self,
        node: &DataNode,
        suffix: IntegerSuffix,
        range: (i128, i128),
        what: &str,
    ) -> Option<i128> {
        let DataValue::Literal(Literal::Integer(literal)) = node.value() else {
            self.shape(
                node,
                &format!("{what} must be a `{}` literal", suffix.as_str()),
            );
            return None;
        };
        if literal.suffix() != Some(suffix) {
            self.value(
                node,
                &format!("{what} must carry the suffix `{}`", suffix.as_str()),
            );
            return None;
        }
        let magnitude = literal.digits().parse::<i128>().ok();
        let value = magnitude.map(|magnitude| {
            if literal.is_negative() {
                -magnitude
            } else {
                magnitude
            }
        });
        match value {
            Some(value) if (range.0..=range.1).contains(&value) => Some(value),
            _ => {
                self.value(
                    node,
                    &format!("{what} is outside the range of `{}`", suffix.as_str()),
                );
                None
            }
        }
    }

    /// A `str` literal, or the `bytes` record, as the bytes it holds.
    fn contents(&mut self, node: &DataNode, what: &str) -> Option<Vec<u8>> {
        match node.value() {
            DataValue::Literal(Literal::String(literal)) => {
                Some(literal.value().as_bytes().to_vec())
            }
            DataValue::Record(_) => {
                let fields = self.record(node, &["kind", "values"], what)?;
                let (Some(kind), Some(values)) =
                    (fields.get("kind"), fields.get("values"))
                else {
                    self.shape(
                        node,
                        &format!("{what} as bytes needs `kind` and `values`"),
                    );
                    return None;
                };
                match kind.value() {
                    DataValue::Atom(atom) if atom.value() == "bytes" => {}
                    _ => {
                        self.value(
                            kind,
                            &format!("{what} as bytes needs `kind: @bytes`"),
                        );
                        return None;
                    }
                }
                let DataValue::Array(items) = values.value() else {
                    self.shape(
                        values,
                        &format!("the bytes of {what} must be an array"),
                    );
                    return None;
                };
                let mut bytes = Vec::with_capacity(items.len());
                let mut valid = true;
                for item in items {
                    match self.integer(item, IntegerSuffix::U8, (0, 255), "a byte") {
                        Some(value) => bytes.push(u8::try_from(value).unwrap_or(0)),
                        None => valid = false,
                    }
                }
                valid.then_some(bytes)
            }
            _ => {
                self.shape(
                    node,
                    &format!("{what} must be a string or a `bytes` record"),
                );
                None
            }
        }
    }

    /// A path of the document, as its components from the root.
    fn path(&mut self, node: &DataNode) -> Option<Components> {
        let text = self.string(node, "a path")?;
        let components = resolve(&text);
        if components.is_none() {
            self.value(node, "a path cannot contain a NUL character");
        }
        components
    }

    fn directories(&mut self, node: &DataNode) -> Option<Vec<(Components, DataNode)>> {
        let DataValue::Array(items) = node.value() else {
            self.shape(node, "the directories must be an array");
            return None;
        };
        let mut found = Vec::new();
        let mut valid = true;
        for item in items {
            match self.path(item) {
                Some(components) => found.push((components, item.clone())),
                None => valid = false,
            }
        }
        valid.then_some(found)
    }

    #[allow(clippy::type_complexity)]
    fn files(
        &mut self,
        node: &DataNode,
    ) -> Option<Vec<(Components, Vec<u8>, DataNode)>> {
        let DataValue::Dict(entries) = node.value() else {
            self.shape(node, "the files must be a dict");
            return None;
        };
        let mut found = Vec::new();
        let mut valid = true;
        for (key, value) in entries {
            let path = self.path(key);
            let contents = self.contents(value, "the contents of a file");
            match (path, contents) {
                (Some(path), Some(contents)) => {
                    found.push((path, contents, key.clone()))
                }
                _ => valid = false,
            }
        }
        valid.then_some(found)
    }

    /// Joins the listed directories and files into the world's tree: every
    /// ancestor exists, and no path is both a file and a directory.
    fn tree(
        &mut self,
        world: &mut HostWorld,
        directories: Vec<(Components, DataNode)>,
        files: Vec<(Components, Vec<u8>, DataNode)>,
    ) -> bool {
        let mut valid = true;
        for (components, _) in &directories {
            for length in 0..=components.len() {
                world
                    .directories
                    .insert(components.get(..length).unwrap_or_default().to_vec());
            }
        }
        for (components, _, _) in &files {
            for length in 0..components.len() {
                world
                    .directories
                    .insert(components.get(..length).unwrap_or_default().to_vec());
            }
        }
        for (components, contents, key) in files {
            if world.directories.contains(&components) {
                self.value(&key, "a path cannot be both a file and a directory");
                valid = false;
            } else if world.files.insert(components, contents).is_some() {
                self.value(&key, "a file is listed twice under one resolved path");
                valid = false;
            }
        }
        valid
    }

    fn environment(&mut self, node: &DataNode) -> Option<BTreeMap<String, String>> {
        let DataValue::Dict(entries) = node.value() else {
            self.shape(node, "the environment must be a dict");
            return None;
        };
        let mut env = BTreeMap::new();
        let mut valid = true;
        for (key, value) in entries {
            let name = self.string(key, "a variable name");
            let text = self.string(value, "a variable value");
            match (name, text) {
                (Some(name), Some(text)) => {
                    env.insert(name, text);
                }
                _ => valid = false,
            }
        }
        valid.then_some(env)
    }

    fn clock(&mut self, node: &DataNode, world: &mut HostWorld) -> bool {
        let Some(fields) = self.record(node, CLOCK_FIELDS, "clock") else {
            return false;
        };
        let mut valid = true;
        if let Some(field) = fields.get("wall") {
            match self.integer(
                field,
                IntegerSuffix::I64,
                (i128::from(i64::MIN), i128::from(i64::MAX)),
                "the wall time",
            ) {
                Some(value) => world.wall = i64::try_from(value).unwrap_or(0),
                None => valid = false,
            }
        }
        for (name, slot) in [
            ("monotonic", &mut world.monotonic),
            ("step", &mut world.step),
        ] {
            if let Some(field) = fields.get(name) {
                match self.integer(
                    field,
                    IntegerSuffix::U64,
                    (0, i128::from(u64::MAX)),
                    &format!("the clock `{name}`"),
                ) {
                    Some(value) => *slot = u64::try_from(value).unwrap_or(0),
                    None => valid = false,
                }
            }
        }
        valid
    }

    fn random(&mut self, node: &DataNode, world: &mut HostWorld) -> bool {
        let Some(fields) = self.record(node, RANDOM_FIELDS, "random") else {
            return false;
        };
        let Some(field) = fields.get("seed") else {
            return true;
        };
        match self.integer(
            field,
            IntegerSuffix::U64,
            (0, i128::from(u64::MAX)),
            "the seed",
        ) {
            Some(value) => {
                world.seed = u64::try_from(value).unwrap_or(0);
                true
            }
            None => false,
        }
    }
}

/// Resolves the text of a path from the root, as the system provider would from
/// a working directory at the root: components are separated by `/`, empty and
/// `.` components are skipped, `..` names the parent, the parent of the root is
/// the root, and there are no links. A NUL character cannot be interpreted.
fn resolve(text: &str) -> Option<Components> {
    if text.contains('\0') {
        return None;
    }
    let mut components = Components::new();
    for component in text.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            name => components.push(name.to_owned()),
        }
    }
    Some(components)
}

/// What a resolved path names.
enum Entry {
    Directory,
    File,
}

/// The provider of one fresh copy of a [`HostWorld`].
///
/// Standard output and error deliver nothing anywhere: in a world, what a
/// program writes is the argument of its audit event and nothing else.
#[derive(Clone, Debug)]
pub struct WorldProvider {
    stdin: Vec<u8>,
    stdin_position: usize,
    directories: BTreeSet<Components>,
    files: BTreeMap<Components, Vec<u8>>,
    env: BTreeMap<String, String>,
    args: Vec<String>,
    wall: i64,
    /// The next monotonic reading, or `None` once the `u64` range is spent.
    next_monotonic: Option<u64>,
    step: u64,
    random: SplitMix64,
}

impl WorldProvider {
    /// A provider over a fresh copy of `world`.
    #[must_use]
    pub fn new(world: &HostWorld) -> Self {
        Self {
            stdin: world.stdin.clone(),
            stdin_position: 0,
            directories: world.directories.clone(),
            files: world.files.clone(),
            env: world.env.clone(),
            args: world.args.clone(),
            wall: world.wall,
            next_monotonic: Some(world.monotonic),
            step: world.step,
            random: SplitMix64::new(world.seed),
        }
    }

    fn rest_of_stdin(&mut self) -> Option<Vec<u8>> {
        let rest = self.stdin.get(self.stdin_position..).unwrap_or_default();
        let copy = try_copy(rest);
        self.stdin_position = self.stdin.len();
        copy
    }

    /// What `path` names, checking every ancestor: a file where a directory is
    /// needed is `not-a-directory`, and a missing ancestor is `not-found`.
    fn lookup(&self, path: &[String]) -> Result<Entry, FsError> {
        for length in 0..path.len() {
            let ancestor = path.get(..length).unwrap_or_default();
            if self.files.contains_key(ancestor) {
                return Err(FsError::NotADirectory);
            }
            if !self.directories.contains(ancestor) {
                return Err(FsError::NotFound);
            }
        }
        if self.files.contains_key(path) {
            Ok(Entry::File)
        } else if self.directories.contains(path) {
            Ok(Entry::Directory)
        } else {
            Err(FsError::NotFound)
        }
    }

    fn file(&self, at: &str) -> Result<&Vec<u8>, FsError> {
        let path = resolve(at).ok_or(FsError::InvalidPath)?;
        match self.lookup(&path)? {
            Entry::Directory => Err(FsError::NotAFile),
            Entry::File => self.files.get(&path).ok_or(FsError::NotFound),
        }
    }

    fn replace(&mut self, at: &str, contents: Vec<u8>) -> Result<(), FsError> {
        let path = resolve(at).ok_or(FsError::InvalidPath)?;
        let Some((_, parent)) = path.split_last() else {
            return Err(FsError::NotAFile);
        };
        match self.lookup(parent)? {
            Entry::File => return Err(FsError::NotADirectory),
            Entry::Directory => {}
        }
        if self.directories.contains(&path) {
            return Err(FsError::NotAFile);
        }
        self.files.insert(path, contents);
        Ok(())
    }
}

impl HostProvider for WorldProvider {
    fn stdin_text(&mut self) -> Done<String, IoError> {
        let bytes = self.rest_of_stdin().ok_or(Failure::MemoryExhausted)?;
        String::from_utf8(bytes).map_err(|_| Failure::Error(IoError::InvalidText))
    }

    fn stdin_bytes(&mut self) -> Done<Vec<u8>, IoError> {
        self.rest_of_stdin().ok_or(Failure::MemoryExhausted)
    }

    fn stdout_text(&mut self, _value: &str) -> Done<(), IoError> {
        Ok(())
    }

    fn stdout_bytes(&mut self, _value: &[u8]) -> Done<(), IoError> {
        Ok(())
    }

    fn stderr_text(&mut self, _value: &str) -> Done<(), IoError> {
        Ok(())
    }

    fn stderr_bytes(&mut self, _value: &[u8]) -> Done<(), IoError> {
        Ok(())
    }

    fn read_file(&mut self, at: &str) -> Done<Vec<u8>, FsError> {
        let contents = self.file(at)?;
        try_copy(contents).ok_or(Failure::MemoryExhausted)
    }

    fn read_text(&mut self, at: &str) -> Done<String, FsError> {
        let contents = self.file(at)?;
        let bytes = try_copy(contents).ok_or(Failure::MemoryExhausted)?;
        String::from_utf8(bytes).map_err(|_| Failure::Error(FsError::InvalidText))
    }

    fn read_entries(&mut self, at: &str) -> Done<Vec<String>, FsError> {
        let path = resolve(at).ok_or(FsError::InvalidPath)?;
        match self.lookup(&path)? {
            Entry::File => return Err(Failure::Error(FsError::NotADirectory)),
            Entry::Directory => {}
        }
        let depth = path.len();
        let mut names = BTreeSet::new();
        for entry in self.directories.iter().chain(self.files.keys()) {
            if entry.len() == depth + 1
                && entry.starts_with(&path)
                && let Some(name) = entry.last()
            {
                names.insert(name.clone());
            }
        }
        Ok(names.into_iter().collect())
    }

    fn write_file(&mut self, at: &str, contents: &[u8]) -> Done<(), FsError> {
        let copy = try_copy(contents).ok_or(Failure::MemoryExhausted)?;
        Ok(self.replace(at, copy)?)
    }

    fn write_text(&mut self, at: &str, contents: &str) -> Done<(), FsError> {
        self.write_file(at, contents.as_bytes())
    }

    fn stat(&mut self, at: &str) -> Done<Metadata, FsError> {
        let path = resolve(at).ok_or(FsError::InvalidPath)?;
        Ok(match self.lookup(&path)? {
            Entry::Directory => Metadata {
                kind: FsKind::Directory,
                size: 0,
            },
            Entry::File => Metadata {
                kind: FsKind::File,
                size: self.files.get(&path).map_or(0, |bytes| bytes.len() as u64),
            },
        })
    }

    fn env_var(&mut self, name: &str) -> Done<String, EnvError> {
        // A name that no environment can hold is not present.
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(Failure::Error(EnvError::NotPresent));
        }
        let value = self.env.get(name).ok_or(EnvError::NotPresent)?;
        Ok(value.clone())
    }

    fn env_args(&mut self) -> Done<Vec<String>, EnvError> {
        Ok(self.args.clone())
    }

    fn now_wall(&mut self) -> Done<i64, TimeError> {
        Ok(self.wall)
    }

    fn now_monotonic(&mut self) -> Done<u64, TimeError> {
        let reading = self.next_monotonic.ok_or(TimeError::Unavailable)?;
        self.next_monotonic = reading.checked_add(self.step);
        Ok(reading)
    }

    fn random_bytes(&mut self, count: u64) -> Done<Vec<u8>, RandomError> {
        self.random.bytes(count).ok_or(Failure::MemoryExhausted)
    }
}
