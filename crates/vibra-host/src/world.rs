//! The `@hostWorld.v2` document and the driver over it.
//!
//! `docs/spec/06-runtime.md`, "Host world", fixes the document and the
//! behaviour of a fake world, and "Closed operation semantics" the meaning of
//! each of the 17 operations, which [`WorldDriver`] performs over a copy of a
//! [`HostWorld`].
//!
//! # The document
//!
//! [`decode_world`] reads a document under the closed schema of the chapter,
//! fields in its order, every field but `format` optional with its default,
//! in the reader's order of `docs/spec/04-programs-and-packages.md`,
//! "Schemas". On top of the schema it holds the world's own rules: every
//! integer literal carries its exact width suffix, a content is a string or a
//! bytes record, every path is a canonical rooted virtual path, the root is
//! no file, no directory is listed twice, and no path is both a file and a
//! directory. A decoded world is data: decoding never makes a grant.
//!
//! # The driver
//!
//! A [`WorldDriver`] copies a world when it is made, so every driver is
//! independent and its effects change only its copy. It consults the gate
//! before it produces any content and before it changes its copy, measures
//! every byte it moves, and answers every ordinary failure as a
//! [`Completion`]. It names no filesystem, environment, clock, or random
//! source of the process: the ambient world is the system driver's.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::authority::{HostErrorCase, HostFsKind, HostOk, HostReply};
use vibra_ir::external::host::HostOp;
use vibra_syntax::{
    DataNode, DataValue, FieldSchema, IntegerSuffix, RecordSchema, Schema, Slot,
    TypedNode, TypedValue, decode, parse_data,
};

use crate::SplitMix64;
use crate::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Found, FoundAction, Gate,
    Measured, Operand, Progress, Resolved, ResourceId,
};
use crate::recipe::Provider;
use vibra_ir::names::{NameError, check_name};

/// The format atom of a host world document.
pub const WORLD_FORMAT: &str = "@hostWorld.v2";

/// The version every driver of a world reports: the toolchain release
/// (`docs/roadmap/v0.0.2/m2/spec-questions.md`, Q24).
pub const DRIVER_VERSION: &str = "0.0.2";

/// The longest scalar in UTF-8: a text read of a smaller count is
/// `unsupported`.
const LONGEST_SCALAR: u64 = 4;

/// The length of `bytes`, as the counters count it.
fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// The virtual filesystem.
// ---------------------------------------------------------------------------

/// One entry of a [`VirtualFs`].
#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    File(Vec<u8>),
}

/// A path of a fixture that is a file and a directory at once, a file given
/// twice, or a file that is the root.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FixtureConflict {
    /// The names of the path from the root.
    pub names: Vec<String>,
}

/// Why a primitive of [`WorldDriver`] made nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorldError {
    /// The fixture's paths conflict.
    Conflict(FixtureConflict),
    /// The driver has no resource identity left.
    Exhausted,
}

/// One virtual filesystem: its entries by their names from its root, every
/// ancestor of an entry a directory. The root is always a directory, and
/// the filesystem has no links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualFs {
    entries: BTreeMap<Vec<String>, Entry>,
}

impl Default for VirtualFs {
    fn default() -> Self {
        Self::new()
    }
}

/// What a path names in a [`VirtualFs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lookup<'f> {
    /// A file, with its contents.
    File(&'f [u8]),
    /// A directory.
    Directory,
    /// Nothing, and every ancestor that exists is a directory.
    Absent,
    /// Nothing, because an ancestor is a file.
    BelowFile,
}

impl VirtualFs {
    /// A filesystem holding its root directory and nothing else.
    #[must_use]
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(Vec::new(), Entry::Directory);
        Self { entries }
    }

    /// A filesystem holding `files`, each with the directories its path
    /// implies.
    ///
    /// # Errors
    ///
    /// The first path that is the root, a file and a directory at once, or
    /// a file given twice.
    pub fn with_files<I>(files: I) -> Result<Self, FixtureConflict>
    where
        I: IntoIterator<Item = (Vec<String>, Vec<u8>)>,
    {
        let mut filesystem = Self::new();
        for (names, contents) in files {
            filesystem.add_file(&names, contents)?;
        }
        Ok(filesystem)
    }

    /// Adds the directory `names` and every ancestor of it.
    fn add_directory(&mut self, names: &[String]) -> Result<(), FixtureConflict> {
        let mut path = Vec::with_capacity(names.len());
        for name in names {
            path.push(name.clone());
            match self.entries.get(&path) {
                Some(Entry::Directory) => {}
                Some(Entry::File(_)) => return Err(FixtureConflict { names: path }),
                None => {
                    self.entries.insert(path.clone(), Entry::Directory);
                }
            }
        }
        Ok(())
    }

    /// Adds the file `names` and every ancestor of it.
    fn add_file(
        &mut self,
        names: &[String],
        contents: Vec<u8>,
    ) -> Result<(), FixtureConflict> {
        let Some((_, parent)) = names.split_last() else {
            return Err(FixtureConflict { names: Vec::new() });
        };
        self.add_directory(parent)?;
        if self.entries.contains_key(names) {
            return Err(FixtureConflict {
                names: names.to_vec(),
            });
        }
        self.entries.insert(names.to_vec(), Entry::File(contents));
        Ok(())
    }

    /// What `names` names.
    fn lookup(&self, names: &[String]) -> Lookup<'_> {
        match self.entries.get(names) {
            Some(Entry::File(contents)) => return Lookup::File(contents),
            Some(Entry::Directory) => return Lookup::Directory,
            None => {}
        }
        // The deepest ancestor that exists decides why nothing is there.
        let mut depth = names.len();
        while depth > 0 {
            depth -= 1;
            match names.get(..depth).and_then(|path| self.entries.get(path)) {
                Some(Entry::File(_)) => return Lookup::BelowFile,
                Some(Entry::Directory) => return Lookup::Absent,
                None => {}
            }
        }
        Lookup::Absent
    }

    /// The contents of the file `names`, when it is one.
    #[must_use]
    pub fn file(&self, names: &[String]) -> Option<&[u8]> {
        match self.entries.get(names) {
            Some(Entry::File(contents)) => Some(contents),
            _ => None,
        }
    }

    /// Whether `names` is a directory; `[]` is the root.
    #[must_use]
    pub fn is_directory(&self, names: &[String]) -> bool {
        matches!(self.entries.get(names), Some(Entry::Directory))
    }

    /// The names of the entries of the directory `names`, sorted by Unicode
    /// scalar; `None` when `names` is no directory.
    #[must_use]
    pub fn list(&self, names: &[String]) -> Option<Vec<String>> {
        if !self.is_directory(names) {
            return None;
        }
        let children = self
            .entries
            .range::<[String], _>((Bound::Excluded(names), Bound::Unbounded))
            .map(|(path, _)| path)
            .take_while(|path| path.starts_with(names))
            .filter(|path| path.len() == names.len().saturating_add(1))
            .filter_map(|path| path.last().cloned())
            .collect();
        Some(children)
    }

    /// Every directory but the root, by its names, in path order.
    pub fn directories(&self) -> impl Iterator<Item = &[String]> {
        self.entries
            .iter()
            .filter(|(path, entry)| !path.is_empty() && **entry == Entry::Directory)
            .map(|(path, _)| path.as_slice())
    }

    /// Every file, by its names, with its contents, in path order.
    pub fn files(&self) -> impl Iterator<Item = (&[String], &[u8])> {
        self.entries.iter().filter_map(|(path, entry)| match entry {
            Entry::File(contents) => Some((path.as_slice(), contents.as_slice())),
            Entry::Directory => None,
        })
    }

    /// Replaces the contents of an existing file, or creates a file in an
    /// existing directory.
    fn write(&mut self, names: &[String], contents: Vec<u8>) {
        self.entries.insert(names.to_vec(), Entry::File(contents));
    }
}

// ---------------------------------------------------------------------------
// The document.
// ---------------------------------------------------------------------------

/// The clock record of a world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Clock {
    /// What every `WallClock.read` returns.
    pub wall: i64,
    /// What the first `MonotonicClock.read` returns.
    pub monotonic: u64,
    /// What each later `MonotonicClock.read` adds.
    pub step: u64,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            wall: 0,
            monotonic: 0,
            step: 1,
        }
    }
}

/// A decoded `@hostWorld.v2` document: the inputs of a fake world. It is
/// data, not authority.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostWorld {
    stdin: Vec<u8>,
    filesystem: VirtualFs,
    env: BTreeMap<String, String>,
    args: Vec<String>,
    clock: Clock,
    seed: u64,
}

impl HostWorld {
    /// The bytes of standard input.
    #[must_use]
    pub fn stdin(&self) -> &[u8] {
        &self.stdin
    }

    /// The filesystem: the listed directories, the files, and the
    /// directories their paths imply.
    #[must_use]
    pub const fn filesystem(&self) -> &VirtualFs {
        &self.filesystem
    }

    /// The variables of the environment.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// The program's arguments, in order.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The clocks.
    #[must_use]
    pub const fn clock(&self) -> Clock {
        self.clock
    }

    /// The seed of the random source.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }
}

/// Why a string is not a canonical rooted virtual path.
fn path_fault(text: &str) -> Option<&'static str> {
    if text == "/" {
        return None;
    }
    let Some(rest) = text.strip_prefix('/') else {
        return Some("a virtual path is absolute: it starts with `/`");
    };
    for component in rest.split('/') {
        if component.is_empty() {
            return Some("a virtual path has no empty component");
        }
        if component == "." || component == ".." {
            return Some("a virtual path has no `.` or `..` component");
        }
        if component.contains(['\\', '\0']) {
            return Some("a virtual path has no backslash or NUL");
        }
        if component.contains(':') {
            return Some("a virtual path has no drive or alternate-stream spelling");
        }
        if matches!(
            check_name(component.trim_end_matches(['.', ' '])),
            Err(NameError::ReservedDevice)
        ) {
            return Some("a virtual path has no device spelling");
        }
    }
    None
}

/// The rule of a directory path.
fn directory_rule(text: &str) -> Option<&'static str> {
    path_fault(text)
}

/// The rule of a file path: a canonical path that is not the root.
fn file_rule(text: &str) -> Option<&'static str> {
    if text == "/" {
        return Some("the root cannot be a file");
    }
    path_fault(text)
}

/// The names of a canonical rooted virtual path below the root.
fn path_names(text: &str) -> Vec<String> {
    text.split('/')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn empty_list() -> TypedValue {
    TypedValue::List(Vec::new())
}

fn empty_dict() -> TypedValue {
    TypedValue::Dict(Vec::new())
}

fn zero_i64() -> TypedValue {
    TypedValue::Integer(0, IntegerSuffix::I64)
}

fn zero_u64() -> TypedValue {
    TypedValue::Integer(0, IntegerSuffix::U64)
}

fn one_u64() -> TypedValue {
    TypedValue::Integer(1, IntegerSuffix::U64)
}

/// The schema of `@hostWorld.v2`, in the chapter's field order. A content
/// (`stdin`, a file's) is a string or a bytes record, which no schema slot
/// spells: its slot is opaque, and [`content`] decodes it.
fn schema() -> Schema {
    Schema::new(
        WORLD_FORMAT,
        vec![
            FieldSchema::optional("stdin", Slot::Opaque),
            FieldSchema::with_default(
                "directories",
                Slot::list(Slot::Str(Some(directory_rule))),
                empty_list,
            ),
            FieldSchema::with_default(
                "files",
                Slot::dict(Slot::Str(Some(file_rule)), Slot::Opaque),
                empty_dict,
            ),
            FieldSchema::with_default(
                "env",
                Slot::dict(Slot::Str(None), Slot::Str(None)),
                empty_dict,
            ),
            FieldSchema::with_default("args", Slot::list(Slot::Str(None)), empty_list),
            FieldSchema::optional(
                "clock",
                Slot::Record(RecordSchema::at_least_one(vec![
                    FieldSchema::with_default(
                        "wall",
                        Slot::Integer(IntegerSuffix::I64),
                        zero_i64,
                    ),
                    FieldSchema::with_default(
                        "monotonic",
                        Slot::Integer(IntegerSuffix::U64),
                        zero_u64,
                    ),
                    FieldSchema::with_default(
                        "step",
                        Slot::Integer(IntegerSuffix::U64),
                        one_u64,
                    ),
                ])),
            ),
            FieldSchema::optional(
                "random",
                Slot::Record(RecordSchema::new(vec![FieldSchema::required(
                    "seed",
                    Slot::Integer(IntegerSuffix::U64),
                )])),
            ),
        ],
    )
}

/// The faults the world's own rules find, by the reader's step.
#[derive(Debug, Default)]
struct Faults {
    shape: Vec<Diagnostic>,
    value: Vec<Diagnostic>,
}

impl Faults {
    fn shape(&mut self, span: ByteSpan, message: impl Into<String>) {
        self.shape.push(Diagnostic::new(
            DiagnosticCode::DataInvalidShape,
            span,
            message,
        ));
    }

    fn value(&mut self, span: ByteSpan, message: impl Into<String>) {
        self.value.push(Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            span,
            message,
        ));
    }

    /// A value of the wrong form for `expected`: an unsuffixed number is a
    /// value fault, anything else a shape fault, as "Schemas" states.
    fn wrong_form(&mut self, node: &DataNode, expected: &str) {
        match node.value() {
            DataValue::Integer(literal) if literal.suffix().is_none() => self.value(
                node.span(),
                format!("an unsuffixed number in a slot of type {expected}"),
            ),
            DataValue::Float(literal) if literal.suffix().is_none() => self.value(
                node.span(),
                format!("an unsuffixed number in a slot of type {expected}"),
            ),
            _ => self.shape(node.span(), format!("the slot holds {expected}")),
        }
    }
}

const UNSUFFIXED: &str =
    "every integer literal in a world carries its exact width suffix";

/// One byte of a bytes record: a `U8` literal with its suffix.
fn byte(node: &DataNode, faults: &mut Faults) -> Option<u8> {
    match node.value() {
        DataValue::Integer(literal) => match literal.suffix() {
            None => {
                faults.value(node.span(), UNSUFFIXED);
                None
            }
            Some(IntegerSuffix::U8) => {
                let value = literal.body().parse::<u8>().ok();
                if value.is_none() {
                    faults.value(node.span(), "the value is out of range for U8");
                }
                value
            }
            Some(_) => {
                faults.value(node.span(), "the slot holds a U8 integer");
                None
            }
        },
        DataValue::Float(_) => {
            faults.value(node.span(), "a float in a slot of type U8");
            None
        }
        _ => {
            faults.shape(node.span(), "the slot holds a U8 integer");
            None
        }
    }
}

/// A content: a `Str` literal, whose UTF-8 bytes it holds, or a bytes record
/// `{kind: @bytes values: [...]}`, a closed record of exactly those fields.
fn content(node: &DataNode, faults: &mut Faults) -> Option<Vec<u8>> {
    let fields = match node.value() {
        DataValue::String(text) => return Some(text.as_bytes().to_vec()),
        DataValue::Record(fields) => fields,
        _ => {
            faults.wrong_form(node, "a string or a bytes record");
            return None;
        }
    };
    let mut ok = true;
    for field in fields {
        if !matches!(field.name(), "kind" | "values") {
            faults.shape(
                field.name_span(),
                format!("the record has no field `{}`", field.name()),
            );
            ok = false;
        }
    }
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.name() == name)
            .map(|field| field.value())
    };
    match field("kind") {
        None => {
            faults.shape(node.span(), "the required field `kind` is missing");
            ok = false;
        }
        Some(kind) => match kind.value() {
            DataValue::Atom(atom) if atom == "@bytes" => {}
            DataValue::Atom(_) => {
                faults.value(kind.span(), "the atom is not one of @bytes");
                ok = false;
            }
            _ => {
                faults.wrong_form(kind, "an atom");
                ok = false;
            }
        },
    }
    let mut bytes = Vec::new();
    match field("values") {
        None => {
            faults.shape(node.span(), "the required field `values` is missing");
            ok = false;
        }
        Some(values) => match values.value() {
            DataValue::List(elements) => {
                bytes.reserve(elements.len());
                for element in elements {
                    match byte(element, faults) {
                        Some(value) => bytes.push(value),
                        None => ok = false,
                    }
                }
            }
            _ => {
                faults.wrong_form(values, "a list");
                ok = false;
            }
        },
    }
    ok.then_some(bytes)
}

/// The field `name` of a raw record.
fn raw_field<'n>(node: &'n DataNode, name: &str) -> Option<&'n DataNode> {
    match node.value() {
        DataValue::Record(fields) => fields
            .iter()
            .find(|field| field.name() == name)
            .map(|field| field.value()),
        _ => None,
    }
}

/// An unsuffixed integer in an integer slot of `record`: the schema accepts
/// it, the world does not.
fn unsuffixed(record: Option<&DataNode>, members: &[&str], faults: &mut Faults) {
    let Some(record) = record else {
        return;
    };
    for member in members {
        if let Some(node) = raw_field(record, member)
            && let DataValue::Integer(literal) = node.value()
            && literal.suffix().is_none()
        {
            faults.value(node.span(), UNSUFFIXED);
        }
    }
}

/// The contents a document holds, read from its raw value.
#[derive(Debug, Default)]
struct Contents {
    stdin: Option<Vec<u8>>,
    files: Vec<(Vec<String>, Vec<u8>)>,
    /// The span of every content node, where an opaque fault of the schema
    /// reader is a fault [`content`] reports itself.
    spans: Vec<ByteSpan>,
}

fn contents(root: &DataNode, faults: &mut Faults) -> Contents {
    let mut found = Contents::default();
    if let Some(stdin) = raw_field(root, "stdin") {
        found.spans.push(stdin.span());
        found.stdin = content(stdin, faults);
    }
    if let Some(files) = raw_field(root, "files")
        && let DataValue::Dict(entries) = files.value()
    {
        for entry in entries {
            found.spans.push(entry.value().span());
            let value = content(entry.value(), faults);
            if let (DataValue::String(path), Some(value)) = (entry.key().value(), value)
            {
                found.files.push((path_names(path), value));
            }
        }
    }
    found
}

/// The faults of paths that relate wrongly: a directory listed twice, and a
/// file that is a directory too, listed or implied by another path.
fn relations(root: &DataNode, faults: &mut Faults) {
    let mut listed: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut directories: BTreeSet<Vec<String>> = BTreeSet::new();
    let ancestors = |names: &[String], into: &mut BTreeSet<Vec<String>>| {
        for depth in 0..names.len() {
            if let Some(ancestor) = names.get(..depth) {
                into.insert(ancestor.to_vec());
            }
        }
    };
    if let Some(list) = raw_field(root, "directories")
        && let DataValue::List(elements) = list.value()
    {
        for element in elements {
            let DataValue::String(path) = element.value() else {
                continue;
            };
            if directory_rule(path).is_some() {
                continue;
            }
            let names = path_names(path);
            if !listed.insert(names.clone()) {
                faults.value(element.span(), "no canonical entry repeats");
            }
            ancestors(&names, &mut directories);
            directories.insert(names);
        }
    }
    let mut files: Vec<(Vec<String>, ByteSpan)> = Vec::new();
    if let Some(dict) = raw_field(root, "files")
        && let DataValue::Dict(entries) = dict.value()
    {
        for entry in entries {
            let DataValue::String(path) = entry.key().value() else {
                continue;
            };
            if file_rule(path).is_some() {
                continue;
            }
            let names = path_names(path);
            ancestors(&names, &mut directories);
            files.push((names, entry.key().span()));
        }
    }
    let mut reported = BTreeSet::new();
    for (names, span) in files {
        if directories.contains(&names) && reported.insert(names) {
            faults.value(span, "the path is both a file and a directory");
        }
    }
}

/// Sorts faults as the schema reader does: by primary start and end, then
/// registry order.
fn sorted(mut faults: Vec<Diagnostic>, source_id: Option<&str>) -> Vec<Diagnostic> {
    if let Some(source_id) = source_id {
        faults = faults
            .into_iter()
            .map(|fault| match fault.source_id() {
                Some(_) => fault,
                None => fault.with_source_id(source_id),
            })
            .collect();
    }
    faults.sort_by_key(|fault| {
        (
            fault.primary_span().start(),
            fault.primary_span().end(),
            fault.code(),
        )
    });
    faults
}

fn integer(node: Option<&TypedNode>) -> Option<i128> {
    node.and_then(TypedNode::as_integer)
}

/// Builds the world from a document every rule accepted.
fn build(
    document: &TypedNode,
    contents: Contents,
) -> Result<HostWorld, FixtureConflict> {
    let mut filesystem = VirtualFs::new();
    for element in document
        .get("directories")
        .and_then(TypedNode::as_list)
        .unwrap_or_default()
    {
        if let Some(path) = element.as_str() {
            filesystem.add_directory(&path_names(path))?;
        }
    }
    for (names, value) in contents.files {
        filesystem.add_file(&names, value)?;
    }
    let env = document
        .get("env")
        .and_then(TypedNode::as_entries)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            Some((
                entry.key().as_str()?.to_owned(),
                entry.value().as_str()?.to_owned(),
            ))
        })
        .collect();
    let args = document
        .get("args")
        .and_then(TypedNode::as_list)
        .unwrap_or_default()
        .iter()
        .filter_map(|arg| arg.as_str().map(str::to_owned))
        .collect();
    let defaults = Clock::default();
    let clock = document.get("clock").map_or(defaults, |clock| Clock {
        wall: integer(clock.get("wall"))
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or(defaults.wall),
        monotonic: integer(clock.get("monotonic"))
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(defaults.monotonic),
        step: integer(clock.get("step"))
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(defaults.step),
    });
    let seed = integer(document.get("random").and_then(|random| random.get("seed")))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(0);
    Ok(HostWorld {
        stdin: contents.stdin.unwrap_or_default(),
        filesystem,
        env,
        args,
        clock,
        seed,
    })
}

/// Reads `bytes` as a `@hostWorld.v2` document, in the six-step order of
/// "Schemas", with the world's own rules in the shape and value steps.
///
/// The result is data: decoding a world never restores or makes a grant.
///
/// # Errors
///
/// The faults of the first step that found any: `@data.invalidShape` for an
/// unknown field, a missing one, or a value of the wrong form;
/// `@data.invalidValue` for an unsupported `format`, an unsuffixed or
/// out-of-range integer, a path that is not a canonical rooted virtual path,
/// the root as a file, a repeated directory, or a path that is both a file
/// and a directory; `@data.duplicateKey` for a repeated key.
pub fn decode_world(
    source_id: Option<&str>,
    bytes: &[u8],
) -> Result<HostWorld, Vec<Diagnostic>> {
    let schema = schema();
    let (document, reader) = decode(&schema, source_id, bytes).into_parts();
    let raw = parse_data(source_id, bytes);
    let selected = raw.value().filter(|root| {
        raw_field(root, "format")
            .is_some_and(|format| matches!(format.value(), DataValue::Atom(atom) if atom == WORLD_FORMAT))
    });
    // Lexical, grammatical, repeated-field, and format faults end the
    // reading before any schema typed a slot.
    let schema_step = reader.iter().all(|fault| {
        matches!(
            fault.code(),
            DiagnosticCode::DataInvalidShape
                | DiagnosticCode::DataInvalidValue
                | DiagnosticCode::DataDuplicateKey
        )
    });
    let Some(root) = selected.filter(|_| raw.diagnostics().is_empty() && schema_step)
    else {
        return Err(reader);
    };
    let mut faults = Faults::default();
    let contents = contents(root, &mut faults);
    unsuffixed(
        raw_field(root, "clock"),
        &["wall", "monotonic", "step"],
        &mut faults,
    );
    unsuffixed(raw_field(root, "random"), &["seed"], &mut faults);
    relations(root, &mut faults);
    // The schema reader typed every content as opaque data; the faults it
    // found inside one are the ones `content` reports.
    let reader = reader
        .into_iter()
        .filter(|fault| {
            !contents
                .spans
                .iter()
                .any(|span| span.contains_span(fault.primary_span()))
        })
        .collect::<Vec<_>>();
    let of = |code: DiagnosticCode| {
        reader
            .iter()
            .filter(|fault| fault.code() == code)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut shape = of(DiagnosticCode::DataInvalidShape);
    shape.append(&mut faults.shape);
    if !shape.is_empty() {
        return Err(sorted(shape, source_id));
    }
    let mut value = of(DiagnosticCode::DataInvalidValue);
    value.append(&mut faults.value);
    if !value.is_empty() {
        return Err(sorted(value, source_id));
    }
    let keys = of(DiagnosticCode::DataDuplicateKey);
    if !keys.is_empty() {
        return Err(sorted(keys, source_id));
    }
    let Some(document) = document else {
        // Every fault the reader found is one `content` reports; a document
        // it did not type is still rejected with one.
        let fault = Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            root.span(),
            "the document is not a valid host world",
        );
        return Err(sorted(vec![fault], source_id));
    };
    build(document.root(), contents).map_err(|conflict| {
        let fault = Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            root.span(),
            format!(
                "the path /{} is both a file and a directory",
                conflict.names.join("/")
            ),
        );
        sorted(vec![fault], source_id)
    })
}

// ---------------------------------------------------------------------------
// The driver.
// ---------------------------------------------------------------------------

/// One standard input: its bytes and the position of the next read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Input {
    bytes: Vec<u8>,
    position: usize,
}

impl Input {
    const fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, position: 0 }
    }

    fn remaining(&self) -> &[u8] {
        self.bytes.get(self.position..).unwrap_or_default()
    }
}

/// What a resource of the driver holds.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Held {
    /// A directory of one of the driver's filesystems, by its names from
    /// that filesystem's root.
    Directory {
        filesystem: usize,
        anchor: Vec<String>,
    },
    /// One of the driver's standard inputs.
    Stdin(usize),
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
    /// The environment's variables.
    Variables,
    /// The program's arguments.
    Arguments,
    /// The wall clock.
    Wall,
    /// The monotonic clock.
    Monotonic,
    /// The random source.
    Random,
}

/// The standard streams `WorldDriver::make_streams` made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Streams {
    /// The new standard input.
    pub stdin: ResourceId,
    /// Standard output, captured with every other stream of the world.
    pub stdout: ResourceId,
    /// Standard error, captured with every other stream of the world.
    pub stderr: ResourceId,
}

/// The environment resources `WorldDriver::set_environment` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Environment {
    /// The variables.
    pub variables: ResourceId,
    /// The arguments.
    pub arguments: ResourceId,
}

/// What the streams of a world have received, per stream, in write order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Output<'w> {
    /// Standard output.
    pub stdout: &'w [u8],
    /// Standard error.
    pub stderr: &'w [u8],
}

/// How one call ends, before it is a [`Completion`].
enum Answer {
    /// The gate refused.
    Refused,
    /// Nothing was attempted: the case named.
    NotSubmitted(&'static str),
    /// The call completed with a success.
    Ok(HostOk, Progress),
    /// The call completed with the case named.
    Err(&'static str, Progress),
}

/// Progress of `input` bytes read and `output` bytes sent, both known.
const fn moved(input: u64, output: u64) -> Progress {
    Progress {
        input: Measured::Known(input),
        output: Measured::Known(output),
    }
}

/// The deterministic driver of a fake world: the 17 operations over a copy
/// of a [`HostWorld`], and the resources recipes resolve under it.
#[derive(Clone, Debug)]
pub struct WorldDriver {
    /// The world's filesystem first, then each one `make_directory` made.
    filesystems: Vec<VirtualFs>,
    /// The world's standard input first, then each one `make_streams` made.
    inputs: Vec<Input>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    env: BTreeMap<String, String>,
    args: Vec<String>,
    wall: i64,
    /// What the next monotonic read returns; `None` once it would overflow.
    monotonic: Option<u64>,
    step: u64,
    random: SplitMix64,
    held: BTreeMap<ResourceId, Held>,
    next: u64,
}

impl WorldDriver {
    /// A driver over a copy of `world`: what it does changes the copy only.
    #[must_use]
    pub fn new(world: &HostWorld) -> Self {
        Self {
            filesystems: vec![world.filesystem.clone()],
            inputs: vec![Input::new(world.stdin.clone())],
            stdout: Vec::new(),
            stderr: Vec::new(),
            env: world.env.clone(),
            args: world.args.clone(),
            wall: world.clock.wall,
            monotonic: Some(world.clock.monotonic),
            step: world.clock.step,
            random: SplitMix64::new(world.seed),
            held: BTreeMap::new(),
            next: 0,
        }
    }

    /// Holds `held` under a new resource; `None` when the identities are
    /// exhausted.
    fn hold(&mut self, held: Held) -> Option<ResourceId> {
        self.next = self.next.checked_add(1)?;
        let resource = ResourceId(self.next);
        self.held.insert(resource, held);
        Some(resource)
    }

    /// A new filesystem holding `files`, each with the directories its path
    /// implies, and a resource on its root.
    ///
    /// # Errors
    ///
    /// [`WorldError::Conflict`]: a path that is the root, a file and a
    /// directory at once, or a file given twice. [`WorldError::Exhausted`]:
    /// no resource identity is left. Nothing is made.
    pub fn make_directory<I>(&mut self, files: I) -> Result<ResourceId, WorldError>
    where
        I: IntoIterator<Item = (Vec<String>, Vec<u8>)>,
    {
        let filesystem = VirtualFs::with_files(files).map_err(WorldError::Conflict)?;
        let index = self.filesystems.len();
        let resource = self
            .hold(Held::Directory {
                filesystem: index,
                anchor: Vec::new(),
            })
            .ok_or(WorldError::Exhausted)?;
        self.filesystems.push(filesystem);
        Ok(resource)
    }

    /// New standard streams: an input holding `stdin`, and the world's
    /// output and error, which capture every stream's writes in order.
    ///
    /// # Errors
    ///
    /// [`WorldError::Exhausted`]: no resource identity is left.
    pub fn make_streams(&mut self, stdin: Vec<u8>) -> Result<Streams, WorldError> {
        let index = self.inputs.len();
        let streams = Streams {
            stdin: self.hold(Held::Stdin(index)).ok_or(WorldError::Exhausted)?,
            stdout: self.hold(Held::Stdout).ok_or(WorldError::Exhausted)?,
            stderr: self.hold(Held::Stderr).ok_or(WorldError::Exhausted)?,
        };
        self.inputs.push(Input::new(stdin));
        Ok(streams)
    }

    /// Sets the world's one environment, and holds resources on its
    /// variables and its arguments. Every resource on the environment reads
    /// what the latest call set.
    ///
    /// # Errors
    ///
    /// [`WorldError::Exhausted`]: no resource identity is left; the
    /// environment is unchanged.
    pub fn set_environment(
        &mut self,
        variables: BTreeMap<String, String>,
        arguments: Vec<String>,
    ) -> Result<Environment, WorldError> {
        let environment = Environment {
            variables: self.hold(Held::Variables).ok_or(WorldError::Exhausted)?,
            arguments: self.hold(Held::Arguments).ok_or(WorldError::Exhausted)?,
        };
        self.env = variables;
        self.args = arguments;
        Ok(environment)
    }

    /// Everything written to the world's streams so far.
    #[must_use]
    pub fn output(&self) -> Output<'_> {
        Output {
            stdout: &self.stdout,
            stderr: &self.stderr,
        }
    }

    /// The names of the virtual directory a recipe's path selects: a
    /// canonical rooted virtual path, or a relative one taken under `/`;
    /// `None` when it selects no directory.
    fn directory_at(&self, path: &str) -> Option<Vec<String>> {
        let rooted = if path.starts_with('/') {
            path.to_owned()
        } else if path.is_empty() {
            "/".to_owned()
        } else {
            format!("/{path}")
        };
        if path_fault(&rooted).is_some() {
            return None;
        }
        let names = path_names(&rooted);
        self.filesystems
            .first()
            .is_some_and(|filesystem| filesystem.is_directory(&names))
            .then_some(names)
    }

    fn answer(
        &mut self,
        held: &Held,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Answer {
        match (held, action.op) {
            (
                Held::Directory { filesystem, anchor },
                HostOp::FsReadFile
                | HostOp::FsReadText
                | HostOp::FsReadEntries
                | HostOp::FsMetadataStat
                | HostOp::FsWriteFile
                | HostOp::FsWriteText,
            ) => {
                let mut path = anchor.clone();
                path.extend(action.names.iter().cloned());
                let Some(filesystem) = self.filesystems.get_mut(*filesystem) else {
                    return Answer::NotSubmitted("unsupported");
                };
                filesystem_call(filesystem, &path, action, gate)
            }
            (Held::Stdin(index), HostOp::IoStdinText | HostOp::IoStdinBytes) => {
                let Some(input) = self.inputs.get_mut(*index) else {
                    return Answer::NotSubmitted("unsupported");
                };
                stdin_read(input, action, gate)
            }
            (Held::Stdout, HostOp::IoStdoutText | HostOp::IoStdoutBytes) => {
                stream_write(&mut self.stdout, action.operand)
            }
            (Held::Stderr, HostOp::IoStderrText | HostOp::IoStderrBytes) => {
                stream_write(&mut self.stderr, action.operand)
            }
            (Held::Variables, HostOp::EnvReadVar) => {
                let Operand::Text(name) = action.operand else {
                    return Answer::NotSubmitted("unsupported");
                };
                // No environment can hold an empty name, or one with `=` or
                // NUL.
                if name.is_empty() || name.contains(['=', '\0']) {
                    return Answer::Err("notPresent", Progress::NONE);
                }
                let Some(value) = self.env.get(name) else {
                    return Answer::Err("notPresent", Progress::NONE);
                };
                let size = length(value.as_bytes());
                match admit(gate, FoundAction::Read, &[], Some(size), None) {
                    None => Answer::Refused,
                    Some(reserved) if reserved < size => {
                        Answer::NotSubmitted("unsupported")
                    }
                    Some(_) => Answer::Ok(HostOk::Text(value.clone()), moved(size, 0)),
                }
            }
            (Held::Arguments, HostOp::EnvReadArgs) => {
                let size = self
                    .args
                    .iter()
                    .map(|arg| length(arg.as_bytes()))
                    .fold(0, u64::saturating_add);
                match admit(gate, FoundAction::Read, &[], Some(size), None) {
                    None => Answer::Refused,
                    Some(reserved) if reserved < size => {
                        Answer::NotSubmitted("unsupported")
                    }
                    Some(_) => {
                        Answer::Ok(HostOk::Names(self.args.clone()), moved(size, 0))
                    }
                }
            }
            (Held::Wall, HostOp::TimeNowWall) => {
                Answer::Ok(HostOk::I64(self.wall), Progress::NONE)
            }
            (Held::Monotonic, HostOp::TimeNowMonotonic) => match self.monotonic {
                Some(now) => {
                    self.monotonic = now.checked_add(self.step);
                    Answer::Ok(HostOk::U64(now), Progress::NONE)
                }
                None => Answer::Err("unavailable", Progress::NONE),
            },
            (Held::Random, HostOp::RandomGenerateBytes) => {
                let Operand::Count(count) = action.operand else {
                    return Answer::NotSubmitted("unsupported");
                };
                match self.random.bytes(count) {
                    Some(bytes) => Answer::Ok(HostOk::Bytes(bytes), moved(count, 0)),
                    // Nothing was drawn: the host could not hold the bytes.
                    // `unavailable`, a case of the row's own type, would be
                    // a completed call.
                    None => Answer::NotSubmitted("unsupported"),
                }
            }
            _ => Answer::NotSubmitted("unsupported"),
        }
    }
}

/// Asks the gate; the input bytes it reserved, or `None` when it refused.
fn admit(
    gate: &mut dyn Gate,
    action: FoundAction,
    real_names: &[String],
    size: Option<u64>,
    links: Option<u32>,
) -> Option<u64> {
    gate.admit(&Found {
        real_names,
        action,
        size,
        links,
    })
    .ok()
    .map(|reserved| reserved.input)
}

/// The contents a write carries.
fn written(operand: Operand<'_>) -> Option<&[u8]> {
    match operand {
        Operand::Text(text) => Some(text.as_bytes()),
        Operand::Bytes(bytes) => Some(bytes),
        Operand::None | Operand::Count(_) => None,
    }
}

/// One filesystem operation on `path`, the request's names below the
/// filesystem's root. The gate sees the request's names, which are the
/// names the world holds: it has no links and no aliases.
fn filesystem_call(
    filesystem: &mut VirtualFs,
    path: &[String],
    action: &Action<'_>,
    gate: &mut dyn Gate,
) -> Answer {
    // Every entry of a world reports one link.
    const LINKS: Option<u32> = Some(1);
    let names = action.names;
    let missing = |lookup: Lookup<'_>| match lookup {
        Lookup::BelowFile => Answer::Err("notADirectory", Progress::NONE),
        _ => Answer::Err("notFound", Progress::NONE),
    };
    match action.op {
        HostOp::FsReadFile | HostOp::FsReadText => match filesystem.lookup(path) {
            Lookup::File(contents) => {
                let size = length(contents);
                match admit(gate, FoundAction::Read, names, Some(size), LINKS) {
                    None => Answer::Refused,
                    Some(reserved) if reserved < size => {
                        Answer::NotSubmitted("unsupported")
                    }
                    Some(_) if action.op == HostOp::FsReadFile => {
                        Answer::Ok(HostOk::Bytes(contents.to_vec()), moved(size, 0))
                    }
                    Some(_) => match std::str::from_utf8(contents) {
                        Ok(text) => {
                            Answer::Ok(HostOk::Text(text.to_owned()), moved(size, 0))
                        }
                        Err(_) => Answer::Err("invalidText", moved(size, 0)),
                    },
                }
            }
            Lookup::Directory => Answer::Err("notAFile", Progress::NONE),
            lookup => missing(lookup),
        },
        HostOp::FsReadEntries => match filesystem.lookup(path) {
            Lookup::Directory => {
                match admit(gate, FoundAction::List, names, None, LINKS) {
                    None => Answer::Refused,
                    Some(_) => Answer::Ok(
                        HostOk::Names(filesystem.list(path).unwrap_or_default()),
                        Progress::NONE,
                    ),
                }
            }
            Lookup::File(_) => Answer::Err("notADirectory", Progress::NONE),
            lookup => missing(lookup),
        },
        HostOp::FsMetadataStat => match filesystem.lookup(path) {
            Lookup::File(contents) => {
                let size = length(contents);
                match admit(gate, FoundAction::StatFile, names, None, LINKS) {
                    None => Answer::Refused,
                    Some(_) => Answer::Ok(
                        HostOk::Metadata {
                            kind: HostFsKind::File,
                            size,
                        },
                        Progress::NONE,
                    ),
                }
            }
            Lookup::Directory => {
                match admit(gate, FoundAction::StatDirectory, names, None, LINKS) {
                    None => Answer::Refused,
                    Some(_) => Answer::Ok(
                        HostOk::Metadata {
                            kind: HostFsKind::Directory,
                            size: 0,
                        },
                        Progress::NONE,
                    ),
                }
            }
            lookup => missing(lookup),
        },
        HostOp::FsWriteFile | HostOp::FsWriteText => {
            let Some(contents) = written(action.operand) else {
                return Answer::NotSubmitted("unsupported");
            };
            let size = length(contents);
            let found = match filesystem.lookup(path) {
                Lookup::File(_) => FoundAction::Replace,
                Lookup::Directory => return Answer::Err("notAFile", Progress::NONE),
                Lookup::BelowFile => {
                    return Answer::Err("notADirectory", Progress::NONE);
                }
                Lookup::Absent => {
                    // A create needs its parent; it never makes one.
                    let parent =
                        path.split_last().map_or(&[][..], |(_, parent)| parent);
                    match filesystem.lookup(parent) {
                        Lookup::Directory => FoundAction::Create,
                        lookup => return missing(lookup),
                    }
                }
            };
            // The gate decides before the copy changes.
            if admit(gate, found, names, None, LINKS).is_none() {
                return Answer::Refused;
            }
            filesystem.write(path, contents.to_vec());
            Answer::Ok(HostOk::Void, moved(0, size))
        }
        _ => Answer::NotSubmitted("unsupported"),
    }
}

/// A read of standard input: the remaining size goes to the gate before a
/// byte is taken, and the read takes at most what the gate reserved.
fn stdin_read(input: &mut Input, action: &Action<'_>, gate: &mut dyn Gate) -> Answer {
    let Operand::Count(max) = action.operand else {
        return Answer::NotSubmitted("unsupported");
    };
    let text = action.op == HostOp::IoStdinText;
    // A text count below the longest scalar is refused before the stream is
    // touched (Q30).
    if text && max < LONGEST_SCALAR {
        return Answer::NotSubmitted("unsupported");
    }
    let remaining = input.remaining();
    let size = length(remaining);
    let Some(reserved) = admit(gate, FoundAction::Read, &[], Some(size), None) else {
        return Answer::Refused;
    };
    let take = usize::try_from(reserved.min(max).min(size))
        .unwrap_or(usize::MAX)
        .min(remaining.len());
    let window = remaining.get(..take).unwrap_or_default();
    let at_end = take == remaining.len();
    if !text {
        let (bytes, taken) = (window.to_vec(), length(window));
        input.position = input.position.saturating_add(take);
        return Answer::Ok(HostOk::Bytes(bytes), moved(taken, 0));
    }
    match std::str::from_utf8(window) {
        Ok(decoded) => {
            let (decoded, taken) = (decoded.to_owned(), length(window));
            input.position = input.position.saturating_add(take);
            Answer::Ok(HostOk::Text(decoded), moved(taken, 0))
        }
        // The window ends inside a scalar of valid text: the read returns the
        // whole scalars before it and takes exactly those bytes.
        Err(error)
            if error.error_len().is_none() && !at_end && error.valid_up_to() > 0 =>
        {
            let whole = window.get(..error.valid_up_to()).unwrap_or_default();
            let decoded = String::from_utf8_lossy(whole).into_owned();
            let (count, taken) = (whole.len(), length(whole));
            input.position = input.position.saturating_add(count);
            Answer::Ok(HostOk::Text(decoded), moved(taken, 0))
        }
        // A reservation below one scalar, which a known size never gives.
        Err(error) if error.error_len().is_none() && !at_end => {
            Answer::NotSubmitted("limitExceeded")
        }
        // Text that cannot be decoded: the bytes the adapter took for it
        // are consumed.
        Err(_) => {
            let taken = length(window);
            input.position = input.position.saturating_add(take);
            Answer::Err("invalidText", moved(taken, 0))
        }
    }
}

/// A write to a standard stream: the whole value, appended in order.
fn stream_write(stream: &mut Vec<u8>, operand: Operand<'_>) -> Answer {
    let Some(contents) = written(operand) else {
        return Answer::NotSubmitted("unsupported");
    };
    stream.extend_from_slice(contents);
    Answer::Ok(HostOk::Void, moved(0, length(contents)))
}

impl Driver for WorldDriver {
    fn identity(&self, _provider: Provider) -> Option<DriverIdentity> {
        Some(DriverIdentity {
            provider: WORLD_FORMAT.to_owned(),
            version: DRIVER_VERSION.to_owned(),
        })
    }

    fn acquire(&mut self, provider: Provider, resource: &Resolved) -> Acquire {
        let held = match (provider, resource) {
            (Provider::FsDirectory, Resolved::Directory { path }) => {
                match self.directory_at(path) {
                    Some(anchor) => Held::Directory {
                        filesystem: 0,
                        anchor,
                    },
                    None => return Acquire::Refused,
                }
            }
            (Provider::IoStdin, Resolved::ProcessStream) => Held::Stdin(0),
            (Provider::IoStdout, Resolved::ProcessStream) => Held::Stdout,
            (Provider::IoStderr, Resolved::ProcessStream) => Held::Stderr,
            (Provider::EnvVars, Resolved::VariableSet { .. }) => Held::Variables,
            (Provider::EnvArgs, Resolved::ProcessArguments) => Held::Arguments,
            (Provider::TimeWall, Resolved::SystemClock) => Held::Wall,
            (Provider::TimeMonotonic, Resolved::SystemClock) => Held::Monotonic,
            (Provider::Random, Resolved::SystemRandom) => Held::Random,
            (provider, _) => {
                return Acquire::Failed(format!(
                    "`{}` does not issue this resource",
                    provider.atom()
                ));
            }
        };
        match self.hold(held) {
            Some(resource) => Acquire::Acquired(resource),
            None => {
                Acquire::Failed("the world has no resource identity left".to_owned())
            }
        }
    }

    fn release(&mut self, resource: ResourceId) {
        self.held.remove(&resource);
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        let answer = match self.held.get(&resource).cloned() {
            Some(held) => self.answer(&held, action, gate),
            None => Answer::NotSubmitted("unsupported"),
        };
        let error = action.op.row().error;
        // Every case named here is a case of every host error type, or of
        // the row's own; `unsupported` is a case of all five.
        let case = |name: &str| {
            HostErrorCase::new(error, name)
                .or_else(|| HostErrorCase::new(error, "unsupported"))
        };
        let completion = match answer {
            Answer::Refused => Some(Completion::Refused),
            Answer::NotSubmitted(name) => case(name).map(Completion::NotSubmitted),
            Answer::Ok(value, progress) => Some(Completion::Completed {
                reply: HostReply::Ok(value),
                progress,
            }),
            Answer::Err(name, progress) => {
                case(name).map(|case| Completion::Completed {
                    reply: HostReply::Err(case),
                    progress,
                })
            }
        };
        // Unreachable: every error type has `unsupported`. The broker reports
        // a refusal its gate did not make as a defect of the driver.
        completion.unwrap_or(Completion::Refused)
    }
}
