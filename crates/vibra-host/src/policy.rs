//! The `@scopePolicy.v1` document, its normalization, and its evaluation.
//!
//! `docs/spec/03-effects.md`, "Scope policies", fixes all of it except the
//! HTTP rules, which are reserved:
//!
//! - **The wire form.** A document is a record of `format`, `family`, and
//!   `expression`. An expression is `{kind: @none}`, `@allOf` or `@anyOf`
//!   with `items`, a `@fileRule` (`root`, `prefix`, `depth`, `actions`,
//!   `names`), or a `@directoryRule` (`root`, `prefix`, `depth`, `actions`).
//!   A name test is `@anyName`, `@exact` with `names`, `@suffix` with
//!   `value`, or `@allOf` or `@anyOf` with `items`. Unknown fields, kinds,
//!   and versions are rejected; a missing field is an error, never a
//!   wildcard; an empty suffix is invalid; the family `@http` is reserved
//!   ([`decode`]).
//! - **The normal form.** Nested `allOf` or `anyOf` of the same kind are
//!   flattened, duplicate items removed, items sorted by their canonical
//!   bytes, single-item wrappers removed, and sets sorted; path components
//!   keep their order ([`normalize`]). The hash identifies the normal form
//!   ([`hash`]).
//! - **The limits.** At most [`MAX_SIZE`] nodes and entries and a depth of at
//!   most [`MAX_DEPTH`], counted after normalization ([`count`]). A node is
//!   one expression or name-test record; an entry is one element of a
//!   `prefix`, `actions`, or `names` list; the depth is the nesting of those
//!   records, plus one for each `restrict` and `sub` in a grant's ancestry.
//! - **The meaning.** `allOf` is intersection and `anyOf` union; an empty
//!   `allOf` permits everything the parent permits, and an empty `anyOf`,
//!   like `none`, permits nothing. A rule is one whole alternative, and its
//!   fields are never separated ([`permits`]).
//!
//! # No recursion
//!
//! A decoded policy can be as deep as its input, so no function of this
//! module calls itself on a sub-expression. Every walk keeps an explicit
//! stack, and [`Expr`] and [`NameTest`] drop, clone, compare, and print
//! without recursion. Normalization stops as soon as one normalized subtree
//! proves the whole normal form over the limits, so the normal forms it holds
//! are at most one level deeper than [`MAX_DEPTH`] allows.

use std::convert::Infallible;
use std::fmt::{self, Write as _};
use std::slice;

use sha2::{Digest, Sha256};
use vibra_diagnostics::ByteSpan;
use vibra_syntax::{DataNode, DataValue, canonical_string, parse_data};

use crate::path::RelativeNames;

/// The format atom of a policy document.
pub const FORMAT: &str = "@scopePolicy.v1";

/// The family atom of a filesystem policy.
pub const FILESYSTEM: &str = "@filesystem";

/// The family atom of an HTTP policy, which is reserved.
pub const HTTP: &str = "@http";

/// The most nodes and entries a policy holds, in total.
pub const MAX_SIZE: usize = 4_096;

/// The greatest depth of a policy, counting the narrowings of its ancestry.
pub const MAX_DEPTH: usize = 64;

/// The kind of a wrapper: `allOf` or `anyOf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Join {
    /// `@allOf`: intersection.
    AllOf,
    /// `@anyOf`: union.
    AnyOf,
}

impl Join {
    /// The kind's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::AllOf => "@allOf",
            Self::AnyOf => "@anyOf",
        }
    }
}

/// The `depth` of a file rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileDepth {
    /// `@children`: exactly one further name.
    Children,
    /// `@tree`: any positive number of further names.
    Tree,
}

impl FileDepth {
    /// The depth's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Children => "@children",
            Self::Tree => "@tree",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        match atom {
            "@children" => Some(Self::Children),
            "@tree" => Some(Self::Tree),
            _ => None,
        }
    }
}

/// The `depth` of a directory rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirDepth {
    /// `@self`: the prefix itself.
    Itself,
    /// `@children`: one name further.
    Children,
    /// `@tree`: the prefix and everything below.
    Tree,
}

impl DirDepth {
    /// The depth's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Itself => "@self",
            Self::Children => "@children",
            Self::Tree => "@tree",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        match atom {
            "@self" => Some(Self::Itself),
            "@children" => Some(Self::Children),
            "@tree" => Some(Self::Tree),
            _ => None,
        }
    }
}

/// An action of a file rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileAction {
    /// `@read`: `DirReadBytes` and `DirReadText`.
    Read,
    /// `@replace`: the two write interfaces, for an existing file.
    Replace,
    /// `@create`: the two write interfaces, for a new file.
    Create,
    /// `@stat`: `DirStat` of a file.
    Stat,
}

impl FileAction {
    /// The action's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::Read => "@read",
            Self::Replace => "@replace",
            Self::Create => "@create",
            Self::Stat => "@stat",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        match atom {
            "@read" => Some(Self::Read),
            "@replace" => Some(Self::Replace),
            "@create" => Some(Self::Create),
            "@stat" => Some(Self::Stat),
            _ => None,
        }
    }
}

/// An action of a directory rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirAction {
    /// `@list`: `DirList`.
    List,
    /// `@stat`: `DirStat` of a directory.
    Stat,
}

impl DirAction {
    /// The action's atom.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::List => "@list",
            Self::Stat => "@stat",
        }
    }

    fn from_atom(atom: &str) -> Option<Self> {
        match atom {
            "@list" => Some(Self::List),
            "@stat" => Some(Self::Stat),
            _ => None,
        }
    }
}

/// A `@fileRule`: files below `prefix`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRule {
    /// The grant leaf whose recipe holds the directory, as its atom.
    pub root: String,
    /// The names the selected files lie below.
    pub prefix: RelativeNames,
    /// How many further names a selected file has.
    pub depth: FileDepth,
    /// The actions the rule permits, a set.
    pub actions: Vec<FileAction>,
    /// The test the file's own name passes.
    pub names: NameTest,
}

/// A `@directoryRule`: directories at or below `prefix`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryRule {
    /// The grant leaf whose recipe holds the directory, as its atom.
    pub root: String,
    /// The names of the selected directory, or of the one they lie below.
    pub prefix: RelativeNames,
    /// Which directories the rule selects.
    pub depth: DirDepth,
    /// The actions the rule permits, a set.
    pub actions: Vec<DirAction>,
}

/// A policy expression.
///
/// It drops, clones, compares, and prints without recursion. Two expressions
/// are equal when their canonical bytes are, and an expression prints as its
/// canonical bytes.
pub enum Expr {
    /// `@none`: permits nothing.
    None,
    /// `@allOf`: every item permits the request.
    AllOf(Vec<Self>),
    /// `@anyOf`: some item permits the request.
    AnyOf(Vec<Self>),
    /// A file rule.
    FileRule(FileRule),
    /// A directory rule.
    DirectoryRule(DirectoryRule),
}

/// A name test of a file rule.
///
/// It drops, clones, compares, and prints without recursion, as [`Expr`]
/// does.
pub enum NameTest {
    /// `@anyName`: every name.
    AnyName,
    /// `@exact`: the names of the set, compared as exact scalars.
    Exact(Vec<String>),
    /// `@suffix`: the names that end in the value, compared as exact
    /// scalars.
    Suffix(String),
    /// `@allOf`: every item accepts the name.
    AllOf(Vec<Self>),
    /// `@anyOf`: some item accepts the name.
    AnyOf(Vec<Self>),
}

/// The shape both trees share: a wrapper of items, or something else.
trait Tree: Sized {
    /// The kind and items of an `allOf` or `anyOf`.
    fn wrapper(&self) -> Option<(Join, &[Self])>;
    /// The wrapper of `join` over `items`.
    fn wrap(join: Join, items: Vec<Self>) -> Self;
    /// Moves a wrapper's items out, leaving it empty.
    fn take_items(&mut self) -> Vec<Self>;
}

impl Tree for Expr {
    fn wrapper(&self) -> Option<(Join, &[Self])> {
        match self {
            Self::AllOf(items) => Some((Join::AllOf, items)),
            Self::AnyOf(items) => Some((Join::AnyOf, items)),
            Self::None | Self::FileRule(_) | Self::DirectoryRule(_) => None,
        }
    }

    fn wrap(join: Join, items: Vec<Self>) -> Self {
        match join {
            Join::AllOf => Self::AllOf(items),
            Join::AnyOf => Self::AnyOf(items),
        }
    }

    fn take_items(&mut self) -> Vec<Self> {
        match self {
            Self::AllOf(items) | Self::AnyOf(items) => std::mem::take(items),
            Self::None | Self::FileRule(_) | Self::DirectoryRule(_) => Vec::new(),
        }
    }
}

impl Tree for NameTest {
    fn wrapper(&self) -> Option<(Join, &[Self])> {
        match self {
            Self::AllOf(items) => Some((Join::AllOf, items)),
            Self::AnyOf(items) => Some((Join::AnyOf, items)),
            Self::AnyName | Self::Exact(_) | Self::Suffix(_) => None,
        }
    }

    fn wrap(join: Join, items: Vec<Self>) -> Self {
        match join {
            Join::AllOf => Self::AllOf(items),
            Join::AnyOf => Self::AnyOf(items),
        }
    }

    fn take_items(&mut self) -> Vec<Self> {
        match self {
            Self::AllOf(items) | Self::AnyOf(items) => std::mem::take(items),
            Self::AnyName | Self::Exact(_) | Self::Suffix(_) => Vec::new(),
        }
    }
}

/// Drops a tree one node at a time: each node is emptied of its items before
/// it is dropped, so dropping it drops no wrapper.
fn drop_tree<T: Tree>(root: &mut T) {
    let mut stack = root.take_items();
    while let Some(mut node) = stack.pop() {
        stack.append(&mut node.take_items());
    }
}

impl Drop for Expr {
    fn drop(&mut self) {
        drop_tree(self);
    }
}

impl Drop for NameTest {
    fn drop(&mut self) {
        drop_tree(self);
    }
}

/// A wrapper whose items are being visited, in a post-order walk.
struct Pending<'a, T, R> {
    node: &'a T,
    rest: slice::Iter<'a, T>,
    done: Vec<R>,
}

/// Visits every node of `root` after its items, without recursion: `visit`
/// receives each node and the results of its items, in order, and the result
/// of the root is returned. The first error stops the walk.
fn fold<'a, T: Tree, R, E>(
    root: &'a T,
    mut visit: impl FnMut(&'a T, Vec<R>) -> Result<R, E>,
) -> Result<R, E> {
    let mut stack: Vec<Pending<'a, T, R>> = Vec::new();
    let mut node = root;
    loop {
        let mut value = match node.wrapper() {
            Some((_, items)) => {
                let mut rest = items.iter();
                if let Some(first) = rest.next() {
                    stack.push(Pending {
                        node,
                        rest,
                        done: Vec::with_capacity(items.len()),
                    });
                    node = first;
                    continue;
                }
                visit(node, Vec::new())?
            }
            None => visit(node, Vec::new())?,
        };
        loop {
            let Some(mut pending) = stack.pop() else {
                return Ok(value);
            };
            pending.done.push(value);
            if let Some(next) = pending.rest.next() {
                stack.push(pending);
                node = next;
                break;
            }
            value = visit(pending.node, pending.done)?;
        }
    }
}

/// Evaluates a tree of tests without recursion: a wrapper combines its items
/// as `allOf` or `anyOf`, stopping at the first item that decides it, and
/// `leaf` decides every other node.
fn evaluate<'a, T: Tree>(root: &'a T, mut leaf: impl FnMut(&'a T) -> bool) -> bool {
    let mut stack: Vec<(Join, slice::Iter<'a, T>)> = Vec::new();
    let mut node = root;
    loop {
        let value = match node.wrapper() {
            Some((join, items)) => {
                let mut rest = items.iter();
                if let Some(first) = rest.next() {
                    stack.push((join, rest));
                    node = first;
                    continue;
                }
                // An empty `allOf` permits; an empty `anyOf` does not.
                join == Join::AllOf
            }
            None => leaf(node),
        };
        loop {
            let Some((join, mut rest)) = stack.pop() else {
                return value;
            };
            let decided = match join {
                Join::AllOf => !value,
                Join::AnyOf => value,
            };
            if !decided && let Some(next) = rest.next() {
                stack.push((join, rest));
                node = next;
                break;
            }
            // Decided by this item, or every item agreed with it: either way
            // the wrapper's value is this item's, and it passes upwards.
        }
    }
}

/// Writes the canonical bytes of a tree without recursion: a wrapper as
/// `{kind: @allOf items: […]}`, and every other node by `leaf`.
fn write_tree<T: Tree>(
    root: &T,
    out: &mut String,
    mut leaf: impl FnMut(&T, &mut String),
) {
    let mut stack: Vec<slice::Iter<'_, T>> = Vec::new();
    let mut node = root;
    loop {
        match node.wrapper() {
            Some((join, items)) => {
                write_wrapper_open(join, out);
                let mut rest = items.iter();
                if let Some(first) = rest.next() {
                    stack.push(rest);
                    node = first;
                    continue;
                }
                out.push_str("]}");
            }
            None => leaf(node, out),
        }
        loop {
            let Some(mut rest) = stack.pop() else {
                return;
            };
            if let Some(next) = rest.next() {
                out.push(' ');
                stack.push(rest);
                node = next;
                break;
            }
            out.push_str("]}");
        }
    }
}

fn write_wrapper_open(join: Join, out: &mut String) {
    out.push_str("{kind: ");
    out.push_str(join.atom());
    out.push_str(" items: [");
}

fn write_names(names: &NameTest, out: &mut String) {
    write_tree(names, out, |node, out| match node {
        NameTest::AnyName => out.push_str("{kind: @anyName}"),
        NameTest::Exact(names) => {
            out.push_str("{kind: @exact names: [");
            write_strings(names.iter().map(String::as_str), out);
            out.push_str("]}");
        }
        NameTest::Suffix(value) => {
            out.push_str("{kind: @suffix value: ");
            out.push_str(&canonical_string(value));
            out.push('}');
        }
        // `write_tree` writes the wrappers itself.
        NameTest::AllOf(_) | NameTest::AnyOf(_) => {}
    });
}

fn write_strings<'s>(strings: impl Iterator<Item = &'s str>, out: &mut String) {
    for (index, text) in strings.enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&canonical_string(text));
    }
}

fn write_atoms<'s>(atoms: impl Iterator<Item = &'s str>, out: &mut String) {
    for (index, atom) in atoms.enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(atom);
    }
}

fn write_prefix(prefix: &RelativeNames, out: &mut String) {
    out.push_str(" prefix: [");
    write_strings(prefix.names().iter().map(|name| name.as_str()), out);
    out.push(']');
}

fn write_file_rule(rule: &FileRule, out: &mut String) {
    out.push_str("{kind: @fileRule root: ");
    out.push_str(&rule.root);
    write_prefix(&rule.prefix, out);
    out.push_str(" depth: ");
    out.push_str(rule.depth.atom());
    out.push_str(" actions: [");
    write_atoms(rule.actions.iter().map(|action| action.atom()), out);
    out.push_str("] names: ");
    write_names(&rule.names, out);
    out.push('}');
}

fn write_directory_rule(rule: &DirectoryRule, out: &mut String) {
    out.push_str("{kind: @directoryRule root: ");
    out.push_str(&rule.root);
    write_prefix(&rule.prefix, out);
    out.push_str(" depth: ");
    out.push_str(rule.depth.atom());
    out.push_str(" actions: [");
    write_atoms(rule.actions.iter().map(|action| action.atom()), out);
    out.push_str("]}");
}

fn write_expr(expression: &Expr, out: &mut String) {
    write_tree(expression, out, |node, out| match node {
        Expr::None => out.push_str("{kind: @none}"),
        Expr::FileRule(rule) => write_file_rule(rule, out),
        Expr::DirectoryRule(rule) => write_directory_rule(rule, out),
        // `write_tree` writes the wrappers itself.
        Expr::AllOf(_) | Expr::AnyOf(_) => {}
    });
}

impl Expr {
    /// The canonical bytes of the expression record, as written: fields in
    /// the order of the chapter's table, `kind` first. The expression is not
    /// normalized first.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        write_expr(self, &mut out);
        out
    }

    /// Every rule of the expression, in written order.
    #[must_use]
    pub fn rules(&self) -> Rules<'_> {
        Rules { stack: vec![self] }
    }
}

impl NameTest {
    /// The canonical bytes of the name-test record, as written.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        write_names(self, &mut out);
        out
    }
}

impl Clone for Expr {
    fn clone(&self) -> Self {
        let Ok(copy) = fold(self, |node, items| {
            Ok::<_, Infallible>(match node {
                Self::None => Self::None,
                Self::AllOf(_) => Self::AllOf(items),
                Self::AnyOf(_) => Self::AnyOf(items),
                Self::FileRule(rule) => Self::FileRule(rule.clone()),
                Self::DirectoryRule(rule) => Self::DirectoryRule(rule.clone()),
            })
        });
        copy
    }
}

impl Clone for NameTest {
    fn clone(&self) -> Self {
        let Ok(copy) = fold(self, |node, items| {
            Ok::<_, Infallible>(match node {
                Self::AnyName => Self::AnyName,
                Self::Exact(names) => Self::Exact(names.clone()),
                Self::Suffix(value) => Self::Suffix(value.clone()),
                Self::AllOf(_) => Self::AllOf(items),
                Self::AnyOf(_) => Self::AnyOf(items),
            })
        });
        copy
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_bytes() == other.canonical_bytes()
    }
}

impl Eq for Expr {}

impl PartialEq for NameTest {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_bytes() == other.canonical_bytes()
    }
}

impl Eq for NameTest {}

impl fmt::Debug for Expr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_bytes())
    }
}

impl fmt::Debug for NameTest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_bytes())
    }
}

/// One rule of an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule<'a> {
    /// A file rule.
    File(&'a FileRule),
    /// A directory rule.
    Directory(&'a DirectoryRule),
}

impl Rule<'_> {
    /// The rule's `root`, as its atom.
    #[must_use]
    pub fn root(&self) -> &str {
        match self {
            Self::File(rule) => &rule.root,
            Self::Directory(rule) => &rule.root,
        }
    }
}

/// The rules of an expression, in written order, found with an explicit
/// stack.
#[derive(Debug)]
pub struct Rules<'a> {
    stack: Vec<&'a Expr>,
}

impl<'a> Iterator for Rules<'a> {
    type Item = Rule<'a>;

    fn next(&mut self) -> Option<Rule<'a>> {
        while let Some(node) = self.stack.pop() {
            match node {
                Expr::FileRule(rule) => return Some(Rule::File(rule)),
                Expr::DirectoryRule(rule) => return Some(Rule::Directory(rule)),
                Expr::AllOf(items) | Expr::AnyOf(items) => {
                    self.stack.extend(items.iter().rev());
                }
                Expr::None => {}
            }
        }
        None
    }
}

/// The normal form is certainly over [`MAX_SIZE`] or [`MAX_DEPTH`]:
/// `ScopeError.policyTooComplex` for a narrowing, `@grant.policyTooComplex`
/// for a recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TooComplex;

impl fmt::Display for TooComplex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a policy has at most {MAX_SIZE} nodes and entries and a depth of at most \
             {MAX_DEPTH}"
        )
    }
}

impl std::error::Error for TooComplex {}

/// The size of an expression: its nodes and entries in total, and its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    /// The expression and name-test records, and the elements of every
    /// `prefix`, `actions`, and `names` list.
    pub total: usize,
    /// The nesting of expression and name-test records: one for a lone
    /// record.
    pub depth: usize,
}

impl Size {
    /// Whether a policy of this size is within the limits, for a grant with
    /// `narrowings` `restrict`s and `sub`s in its ancestry.
    #[must_use]
    pub const fn fits(self, narrowings: usize) -> bool {
        self.total <= MAX_SIZE && self.depth.saturating_add(narrowings) <= MAX_DEPTH
    }

    const fn record(entries: usize) -> Self {
        Self {
            total: entries.saturating_add(1),
            depth: 1,
        }
    }

    /// A record holding `inner`, with `entries` entries of its own.
    const fn around(inner: Self, entries: usize) -> Self {
        Self {
            total: inner.total.saturating_add(entries).saturating_add(1),
            depth: inner.depth.saturating_add(1),
        }
    }

    fn wrapper(items: impl Iterator<Item = Self>) -> Self {
        items.fold(Self::record(0), |size, item| Self {
            total: size.total.saturating_add(item.total),
            depth: size.depth.max(item.depth.saturating_add(1)),
        })
    }

    /// Whether the normal form that holds a subtree of this size is
    /// certainly over the limits. Normalizing an ancestor removes at most the
    /// subtree's own wrapper, by flattening it, and keeps one copy of every
    /// item it removes as a duplicate, so the normal form holds all but one
    /// of the subtree's nodes and all but one level of its depth.
    const fn proves_too_complex(self) -> bool {
        self.total > MAX_SIZE + 1 || self.depth > MAX_DEPTH + 1
    }
}

fn count_names(names: &NameTest) -> Size {
    let Ok(size) = fold(names, |node, items: Vec<Size>| {
        Ok::<_, Infallible>(match node {
            NameTest::AnyName | NameTest::Suffix(_) => Size::record(0),
            NameTest::Exact(names) => Size::record(names.len()),
            NameTest::AllOf(_) | NameTest::AnyOf(_) => Size::wrapper(items.into_iter()),
        })
    });
    size
}

/// The nodes, entries, and depth of `expression`, as it is written. A
/// policy's limits are checked on its normal form, which [`Policy`] holds.
#[must_use]
pub fn count(expression: &Expr) -> Size {
    let Ok(size) = fold(expression, |node, items: Vec<Size>| {
        Ok::<_, Infallible>(match node {
            Expr::None => Size::record(0),
            Expr::AllOf(_) | Expr::AnyOf(_) => Size::wrapper(items.into_iter()),
            Expr::FileRule(rule) => Size::around(
                count_names(&rule.names),
                rule.prefix.len().saturating_add(rule.actions.len()),
            ),
            Expr::DirectoryRule(rule) => {
                Size::record(rule.prefix.len().saturating_add(rule.actions.len()))
            }
        })
    });
    size
}

/// A normalized subtree: its shape, its canonical bytes, and its size.
struct Normal<T> {
    shape: Shape<T>,
    bytes: String,
    size: Size,
}

enum Shape<T> {
    Leaf(T),
    Wrap(Join, Vec<Normal<T>>),
}

impl<T> Normal<T> {
    fn leaf(value: T, bytes: String, size: Size) -> Result<Self, TooComplex> {
        Self {
            shape: Shape::Leaf(value),
            bytes,
            size,
        }
        .bounded()
    }

    /// The wrapper of `join` over normalized `children`: the children of the
    /// same kind flattened, duplicates removed, the items sorted by their
    /// canonical bytes, and a single item standing for the wrapper.
    fn join(join: Join, children: Vec<Self>) -> Result<Self, TooComplex> {
        let mut items = Vec::with_capacity(children.len());
        for child in children {
            match child.shape {
                Shape::Wrap(inner, grandchildren) if inner == join => {
                    items.extend(grandchildren);
                }
                shape => items.push(Self { shape, ..child }),
            }
        }
        items.sort_by(|left, right| left.bytes.cmp(&right.bytes));
        items.dedup_by(|later, earlier| later.bytes == earlier.bytes);
        if items.len() == 1
            && let Some(only) = items.pop()
        {
            return Ok(only);
        }
        let mut bytes = String::new();
        write_wrapper_open(join, &mut bytes);
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                bytes.push(' ');
            }
            bytes.push_str(&item.bytes);
        }
        bytes.push_str("]}");
        let size = Size::wrapper(items.iter().map(|item| item.size));
        Self {
            shape: Shape::Wrap(join, items),
            bytes,
            size,
        }
        .bounded()
    }

    fn bounded(self) -> Result<Self, TooComplex> {
        if self.size.proves_too_complex() {
            Err(TooComplex)
        } else {
            Ok(self)
        }
    }
}

/// A wrapper being rebuilt from its normal form.
struct Building<T> {
    join: Join,
    rest: std::vec::IntoIter<Normal<T>>,
    done: Vec<T>,
}

/// The tree a normal form describes, built without recursion.
fn rebuild<T: Tree>(root: Normal<T>) -> T {
    let mut stack: Vec<Building<T>> = Vec::new();
    let mut normal = root;
    loop {
        let mut value = match normal.shape {
            Shape::Leaf(value) => value,
            Shape::Wrap(join, items) => {
                let mut rest = items.into_iter();
                if let Some(first) = rest.next() {
                    stack.push(Building {
                        join,
                        done: Vec::with_capacity(rest.len().saturating_add(1)),
                        rest,
                    });
                    normal = first;
                    continue;
                }
                T::wrap(join, Vec::new())
            }
        };
        loop {
            let Some(mut building) = stack.pop() else {
                return value;
            };
            building.done.push(value);
            if let Some(next) = building.rest.next() {
                stack.push(building);
                normal = next;
                break;
            }
            value = T::wrap(building.join, building.done);
        }
    }
}

fn normalize_names(names: &NameTest) -> Result<Normal<NameTest>, TooComplex> {
    fold(names, |node, children| match node {
        NameTest::AnyName => Normal::leaf(
            NameTest::AnyName,
            "{kind: @anyName}".to_owned(),
            Size::record(0),
        ),
        NameTest::Exact(names) => {
            // A set: sorted by the order of `Str`, its scalars, and each name
            // kept once.
            let mut names = names.clone();
            names.sort();
            names.dedup();
            let size = Size::record(names.len());
            let test = NameTest::Exact(names);
            let bytes = test.canonical_bytes();
            Normal::leaf(test, bytes, size)
        }
        NameTest::Suffix(value) => {
            let test = NameTest::Suffix(value.clone());
            let bytes = test.canonical_bytes();
            Normal::leaf(test, bytes, Size::record(0))
        }
        NameTest::AllOf(_) => Normal::join(Join::AllOf, children),
        NameTest::AnyOf(_) => Normal::join(Join::AnyOf, children),
    })
}

fn normalize_expr(expression: &Expr) -> Result<Normal<Expr>, TooComplex> {
    fold(expression, |node, children| match node {
        Expr::None => {
            Normal::leaf(Expr::None, "{kind: @none}".to_owned(), Size::record(0))
        }
        Expr::AllOf(_) => Normal::join(Join::AllOf, children),
        Expr::AnyOf(_) => Normal::join(Join::AnyOf, children),
        Expr::FileRule(rule) => {
            let names = normalize_names(&rule.names)?;
            let names_size = names.size;
            let mut actions = rule.actions.clone();
            actions.sort_by_key(|action| action.atom());
            actions.dedup();
            let rule = FileRule {
                root: rule.root.clone(),
                prefix: rule.prefix.clone(),
                depth: rule.depth,
                actions,
                names: rebuild(names),
            };
            let size = Size::around(
                names_size,
                rule.prefix.len().saturating_add(rule.actions.len()),
            );
            let mut bytes = String::new();
            write_file_rule(&rule, &mut bytes);
            Normal::leaf(Expr::FileRule(rule), bytes, size)
        }
        Expr::DirectoryRule(rule) => {
            let mut actions = rule.actions.clone();
            actions.sort_by_key(|action| action.atom());
            actions.dedup();
            let rule = DirectoryRule {
                root: rule.root.clone(),
                prefix: rule.prefix.clone(),
                depth: rule.depth,
                actions,
            };
            let size =
                Size::record(rule.prefix.len().saturating_add(rule.actions.len()));
            let mut bytes = String::new();
            write_directory_rule(&rule, &mut bytes);
            Normal::leaf(Expr::DirectoryRule(rule), bytes, size)
        }
    })
}

/// The normal form of `expression`: nested `allOf` or `anyOf` of the same
/// kind flattened, duplicate items removed, items sorted by their canonical
/// bytes, single-item wrappers removed, and the sets (`actions`, the `names`
/// of `@exact`) sorted, each element once. A `prefix` keeps its order. The
/// rules apply to name tests as to expressions.
///
/// The `actions` sort by their atoms, which are their canonical bytes; the
/// `names` of `@exact` by the order of `Str`, their scalars.
///
/// # Errors
///
/// [`TooComplex`] as soon as one normalized subtree proves the whole normal
/// form over the limits; normalization stops there.
pub fn normalize(expression: &Expr) -> Result<Expr, TooComplex> {
    normalize_expr(expression).map(rebuild)
}

/// A filesystem policy in its normal form, within the limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    expression: Expr,
    size: Size,
}

impl Policy {
    /// The policy of `expression`, normalized, for a grant with `narrowings`
    /// `restrict`s and `sub`s in its ancestry: zero for a recipe's policy.
    ///
    /// # Errors
    ///
    /// [`TooComplex`] when the normal form is over the limits.
    pub fn new(expression: &Expr, narrowings: usize) -> Result<Self, TooComplex> {
        let expression = normalize(expression)?;
        let size = count(&expression);
        if size.fits(narrowings) {
            Ok(Self { expression, size })
        } else {
            Err(TooComplex)
        }
    }

    /// The normalized expression.
    #[must_use]
    pub const fn expression(&self) -> &Expr {
        &self.expression
    }

    /// The size of the normal form.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The canonical bytes of the normalized `@scopePolicy.v1` document:
    /// `format`, `family`, then `expression`.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        out.push_str("{format: ");
        out.push_str(FORMAT);
        out.push_str(" family: ");
        out.push_str(FILESYSTEM);
        out.push_str(" expression: ");
        write_expr(&self.expression, &mut out);
        out.push('}');
        out
    }
}

/// `sha256:` and 64 lower-case hexadecimal digits of the SHA-256 of `bytes`.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(71);
    text.push_str("sha256:");
    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The hash of a policy: `sha256:` over the canonical bytes of its normal
/// form. It identifies the normal form; two policies with the same meaning
/// may hash differently.
#[must_use]
pub fn hash(policy: &Policy) -> String {
    sha256(policy.canonical_bytes().as_bytes())
}

/// What a request asks for: a file action or a directory action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// A request that selects a file.
    File(FileAction),
    /// A request that selects a directory.
    Directory(DirAction),
}

/// One request a policy decides: the names below the grant's directory, in
/// the policy's coordinates, and the action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'r> {
    /// The names of the selected entry, from the directory the grant's root
    /// holds. The empty list selects that directory.
    pub names: &'r [String],
    /// The action, and whether it selects a file or a directory.
    pub action: Action,
}

/// Whether `rule` permits `request`: its prefix, depth, action, and name
/// test together.
fn file_rule_permits(rule: &FileRule, request: &Request<'_>) -> bool {
    // A file rule grants no listing and no directory `stat`.
    let Action::File(action) = request.action else {
        return false;
    };
    if !rule.prefix.is_prefix_of(request.names) {
        return false;
    }
    let further = request.names.len().saturating_sub(rule.prefix.len());
    let depth = match rule.depth {
        FileDepth::Children => further == 1,
        FileDepth::Tree => further >= 1,
    };
    let Some(name) = request.names.last() else {
        return false;
    };
    depth && rule.actions.contains(&action) && name_permits(&rule.names, name)
}

/// Whether `rule` permits `request`: its prefix, depth, and action together.
fn directory_rule_permits(rule: &DirectoryRule, request: &Request<'_>) -> bool {
    let Action::Directory(action) = request.action else {
        return false;
    };
    if !rule.prefix.is_prefix_of(request.names) {
        return false;
    }
    let further = request.names.len().saturating_sub(rule.prefix.len());
    let depth = match rule.depth {
        DirDepth::Itself => further == 0,
        DirDepth::Children => further == 1,
        DirDepth::Tree => true,
    };
    depth && rule.actions.contains(&action)
}

/// Whether `name` passes `test`, compared as exact Unicode scalars, with no
/// case folding and no normalization.
fn name_permits(test: &NameTest, name: &str) -> bool {
    evaluate(test, |node| match node {
        NameTest::AnyName => true,
        NameTest::Exact(names) => names.iter().any(|exact| exact == name),
        NameTest::Suffix(value) => name.ends_with(value.as_str()),
        // `evaluate` combines the wrappers itself.
        NameTest::AllOf(_) | NameTest::AnyOf(_) => false,
    })
}

/// Whether `expression` permits `request`.
///
/// Each rule decides the request on its own: a file rule matches prefix,
/// depth, action, and name together, and a directory rule prefix, depth, and
/// action. No field of one rule is combined with a field of another, so a
/// policy that permits reading under `a` and replacing under `b` does not
/// permit replacing under `a`. `allOf` is intersection and `anyOf` union.
#[must_use]
pub fn permits(expression: &Expr, request: &Request<'_>) -> bool {
    evaluate(expression, |node| match node {
        Expr::FileRule(rule) => file_rule_permits(rule, request),
        Expr::DirectoryRule(rule) => directory_rule_permits(rule, request),
        // `none` permits nothing; `evaluate` combines the wrappers itself.
        Expr::None | Expr::AllOf(_) | Expr::AnyOf(_) => false,
    })
}

/// Why a document is not a filesystem policy this release accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// Not a `@scopePolicy.v1` document of the closed schema: a grammar
    /// fault, a wrong form, an unknown field or kind, a missing field, an
    /// invalid `prefix` name, or an empty suffix.
    Malformed {
        /// Where, in the bytes decoded.
        span: ByteSpan,
        /// What is wrong.
        reason: String,
    },
    /// A `format` other than `@scopePolicy.v1`.
    UnsupportedVersion {
        /// The `format` value.
        span: ByteSpan,
    },
    /// The family `@http`, which is reserved: no caller of this release
    /// accepts it.
    ReservedFamily {
        /// The `family` value.
        span: ByteSpan,
    },
    /// The normal form is over the limits.
    TooComplex,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { reason, .. } => formatter.write_str(reason),
            Self::UnsupportedVersion { .. } => {
                write!(
                    formatter,
                    "this reader implements the format `{FORMAT}` and no other"
                )
            }
            Self::ReservedFamily { .. } => {
                formatter.write_str("the family `@http` is reserved")
            }
            Self::TooComplex => TooComplex.fmt(formatter),
        }
    }
}

impl std::error::Error for PolicyError {}

fn malformed(span: ByteSpan, reason: impl Into<String>) -> PolicyError {
    PolicyError::Malformed {
        span,
        reason: reason.into(),
    }
}

/// The values of a closed record's fields, in the order of `names`: every
/// field is required, and no other is admitted.
fn record<'a, const N: usize>(
    node: &'a DataNode,
    names: [&'static str; N],
) -> Result<[&'a DataNode; N], PolicyError> {
    let DataValue::Record(fields) = node.value() else {
        return Err(malformed(node.span(), "the value is a record"));
    };
    let mut found = Vec::with_capacity(N);
    for name in names {
        match fields.iter().find(|field| field.name() == name) {
            Some(field) => found.push(field.value()),
            None => {
                return Err(malformed(
                    node.span(),
                    format!("the required field `{name}` is missing"),
                ));
            }
        }
    }
    if let Some(extra) = fields.iter().find(|field| !names.contains(&field.name())) {
        return Err(malformed(
            extra.name_span(),
            format!("the record has no field `{}`", extra.name()),
        ));
    }
    found
        .try_into()
        .map_err(|_| malformed(node.span(), "the record's fields are its schema's"))
}

fn atom(node: &DataNode) -> Result<&str, PolicyError> {
    match node.value() {
        DataValue::Atom(atom) => Ok(atom),
        _ => Err(malformed(node.span(), "the value is an atom")),
    }
}

fn string(node: &DataNode) -> Result<&str, PolicyError> {
    match node.value() {
        DataValue::String(text) => Ok(text),
        _ => Err(malformed(node.span(), "the value is a string")),
    }
}

fn list(node: &DataNode) -> Result<&[DataNode], PolicyError> {
    match node.value() {
        DataValue::List(elements) => Ok(elements),
        _ => Err(malformed(node.span(), "the value is a list")),
    }
}

fn atoms_of<T>(
    node: &DataNode,
    from_atom: fn(&str) -> Option<T>,
) -> Result<Vec<T>, PolicyError> {
    list(node)?
        .iter()
        .map(|element| {
            from_atom(atom(element)?).ok_or_else(|| {
                malformed(element.span(), "the action is not one of the rule's")
            })
        })
        .collect()
}

fn prefix_of(node: &DataNode) -> Result<RelativeNames, PolicyError> {
    let elements = list(node)?;
    let names = elements.iter().map(string).collect::<Result<Vec<_>, _>>()?;
    RelativeNames::new(names).map_err(|invalid| {
        let span = elements
            .get(invalid.index)
            .map_or(node.span(), DataNode::span);
        malformed(span, invalid.error.to_string())
    })
}

/// The kind of an expression or name-test record, and where it is.
fn kind_of(node: &DataNode) -> Result<(&str, ByteSpan), PolicyError> {
    let DataValue::Record(fields) = node.value() else {
        return Err(malformed(
            node.span(),
            "the value is a record with a `kind`",
        ));
    };
    let Some(kind) = fields.iter().find(|field| field.name() == "kind") else {
        return Err(malformed(
            node.span(),
            "the required field `kind` is missing",
        ));
    };
    Ok((atom(kind.value())?, kind.value().span()))
}

/// A wrapper whose items are being decoded.
struct Open<'a, T> {
    join: Join,
    rest: slice::Iter<'a, DataNode>,
    done: Vec<T>,
}

/// Decodes a tree of records whose wrappers are `@allOf` and `@anyOf` with
/// `items`, without recursion; `leaf` decodes every other kind.
fn decode_tree<'a, T: Tree>(
    root: &'a DataNode,
    mut leaf: impl FnMut(&'a DataNode, &str, ByteSpan) -> Result<T, PolicyError>,
) -> Result<T, PolicyError> {
    let mut stack: Vec<Open<'a, T>> = Vec::new();
    let mut node = root;
    loop {
        let (kind, kind_span) = kind_of(node)?;
        let join = match kind {
            "@allOf" => Some(Join::AllOf),
            "@anyOf" => Some(Join::AnyOf),
            _ => None,
        };
        let mut value = match join {
            Some(join) => {
                let [_, items] = record(node, ["kind", "items"])?;
                let items = list(items)?;
                let mut rest = items.iter();
                if let Some(first) = rest.next() {
                    stack.push(Open {
                        join,
                        rest,
                        done: Vec::with_capacity(items.len()),
                    });
                    node = first;
                    continue;
                }
                T::wrap(join, Vec::new())
            }
            None => leaf(node, kind, kind_span)?,
        };
        loop {
            let Some(mut open) = stack.pop() else {
                return Ok(value);
            };
            open.done.push(value);
            if let Some(next) = open.rest.next() {
                stack.push(open);
                node = next;
                break;
            }
            value = T::wrap(open.join, open.done);
        }
    }
}

fn decode_names(node: &DataNode) -> Result<NameTest, PolicyError> {
    decode_tree(node, |node, kind, kind_span| match kind {
        "@anyName" => {
            record(node, ["kind"])?;
            Ok(NameTest::AnyName)
        }
        "@exact" => {
            let [_, names] = record(node, ["kind", "names"])?;
            let names = list(names)?
                .iter()
                .map(|name| string(name).map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NameTest::Exact(names))
        }
        "@suffix" => {
            let [_, value] = record(node, ["kind", "value"])?;
            let text = string(value)?;
            if text.is_empty() {
                return Err(malformed(value.span(), "an empty suffix is invalid"));
            }
            Ok(NameTest::Suffix(text.to_owned()))
        }
        _ => Err(malformed(
            kind_span,
            "a name test's kind is one of @anyName @exact @suffix @allOf @anyOf",
        )),
    })
}

fn decode_expr(node: &DataNode) -> Result<Expr, PolicyError> {
    decode_tree(node, |node, kind, kind_span| match kind {
        "@none" => {
            record(node, ["kind"])?;
            Ok(Expr::None)
        }
        "@fileRule" => {
            let [_, root, prefix, depth, actions, names] = record(
                node,
                ["kind", "root", "prefix", "depth", "actions", "names"],
            )?;
            let depth = FileDepth::from_atom(atom(depth)?).ok_or_else(|| {
                malformed(depth.span(), "a file rule's depth is @children or @tree")
            })?;
            Ok(Expr::FileRule(FileRule {
                root: atom(root)?.to_owned(),
                prefix: prefix_of(prefix)?,
                depth,
                actions: atoms_of(actions, FileAction::from_atom)?,
                names: decode_names(names)?,
            }))
        }
        "@directoryRule" => {
            let [_, root, prefix, depth, actions] =
                record(node, ["kind", "root", "prefix", "depth", "actions"])?;
            let depth = DirDepth::from_atom(atom(depth)?).ok_or_else(|| {
                malformed(
                    depth.span(),
                    "a directory rule's depth is @self, @children, or @tree",
                )
            })?;
            Ok(Expr::DirectoryRule(DirectoryRule {
                root: atom(root)?.to_owned(),
                prefix: prefix_of(prefix)?,
                depth,
                actions: atoms_of(actions, DirAction::from_atom)?,
            }))
        }
        _ => Err(malformed(
            kind_span,
            "an expression's kind is one of @none @allOf @anyOf @fileRule @directoryRule",
        )),
    })
}

/// Decodes one `@scopePolicy.v1` document of family `@filesystem` from its
/// VIBON bytes, and normalizes it.
///
/// # Errors
///
/// [`PolicyError`]: the first fault of the document, or [`PolicyError::TooComplex`]
/// when its normal form is over the limits.
pub fn decode(bytes: &[u8]) -> Result<Policy, PolicyError> {
    let document = parse_data(None, bytes);
    if let Some(fault) = document.diagnostics().first() {
        return Err(malformed(fault.primary_span(), fault.message()));
    }
    let Some(root) = document.value() else {
        return Err(malformed(
            ByteSpan::new(0, bytes.len()),
            "the document holds no value",
        ));
    };
    decode_value(root)
}

/// Decodes one `@scopePolicy.v1` document of family `@filesystem` from its
/// parsed VIBON value, and normalizes it. The `format` is selected first,
/// then the document's fields are checked, then the family, then the
/// expression.
///
/// # Errors
///
/// As [`decode`].
pub fn decode_value(root: &DataNode) -> Result<Policy, PolicyError> {
    let DataValue::Record(fields) = root.value() else {
        return Err(malformed(root.span(), "a policy document is a record"));
    };
    let Some(format) = fields.iter().find(|field| field.name() == "format") else {
        return Err(malformed(
            root.span(),
            "the required field `format` is missing",
        ));
    };
    match format.value().value() {
        DataValue::Atom(atom) if atom == FORMAT => {}
        DataValue::Atom(_) => {
            return Err(PolicyError::UnsupportedVersion {
                span: format.value().span(),
            });
        }
        _ => return Err(malformed(format.value().span(), "the format is an atom")),
    }
    let [_, family, expression] = record(root, ["format", "family", "expression"])?;
    match atom(family)? {
        FILESYSTEM => {}
        HTTP => {
            return Err(PolicyError::ReservedFamily {
                span: family.span(),
            });
        }
        _ => {
            return Err(malformed(
                family.span(),
                "the family is @filesystem or @http",
            ));
        }
    }
    let expression = decode_expr(expression)?;
    Policy::new(&expression, 0).map_err(|TooComplex| PolicyError::TooComplex)
}
