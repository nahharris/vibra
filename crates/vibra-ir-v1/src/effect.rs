//! Effect roots and effect rows (`docs/spec/03-effects.md`, "Function effect
//! rows").
//!
//! A root is a nominal effect declaration: its identity is the canonical
//! declaration identity, which names the defining package, and `path` is the
//! atom path (`unit.module.root`) that the canonical encodings render. Two
//! roots of one spelling in different packages are different roots.
//!
//! A row is a record everywhere: this type, the VIBON text `(record roots:
//! (array @root...))`, and the JSON object `{"roots": [...]}`. The `tail` field
//! is the absence of a row variable, which v1 excludes; a consumer that copies
//! a row keeps the field.

use std::fmt;

/// A nominal effect root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectRoot {
    // `path` leads so the derived order is the canonical order of the
    // specification: the UTF-8 bytes of the atom path, then the identity.
    path: String,
    id: String,
}

impl EffectRoot {
    /// Creates a root from its canonical declaration identity and its atom
    /// path without the leading `@`.
    #[must_use]
    pub fn new(id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            id: id.into(),
        }
    }

    /// The canonical declaration identity, which includes the package.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The canonical atom path without its leading `@`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The atom the canonical value encoding renders.
    #[must_use]
    pub fn atom(&self) -> String {
        format!("@{}", self.path)
    }
}

impl fmt::Display for EffectRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.path)
    }
}

/// The row variable of a later version. It has no value, so `tail` of every
/// v1 row is `None`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowTail {}

/// A finite effect row: the record `{roots, tail}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectRow {
    roots: Vec<EffectRoot>,
    tail: Option<RowTail>,
}

impl EffectRow {
    /// The empty row, which every omitted `effects:` denotes.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            roots: Vec::new(),
            tail: None,
        }
    }

    /// A row of `roots`, sorted by canonical identity and duplicate-free.
    #[must_use]
    pub fn new(roots: impl IntoIterator<Item = EffectRoot>) -> Self {
        let mut roots = roots.into_iter().collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        Self { roots, tail: None }
    }

    /// The roots in canonical order.
    #[must_use]
    pub fn roots(&self) -> &[EffectRoot] {
        &self.roots
    }

    /// The row variable, which v1 rows never have.
    #[must_use]
    pub const fn tail(&self) -> Option<&RowTail> {
        self.tail.as_ref()
    }

    /// Whether the row has no root.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Whether `root` is one of the row's roots.
    #[must_use]
    pub fn contains(&self, root: &EffectRoot) -> bool {
        self.roots.binary_search(root).is_ok()
    }

    /// Whether every root of this row is a root of `other`: a plain subset
    /// test over canonical identities that consults no effect declaration.
    #[must_use]
    pub fn is_subset(&self, other: &Self) -> bool {
        self.roots.iter().all(|root| other.contains(root))
    }

    /// The roots of this row that `other` lacks, in canonical order.
    #[must_use]
    pub fn difference(&self, other: &Self) -> Vec<EffectRoot> {
        self.roots
            .iter()
            .filter(|root| !other.contains(root))
            .cloned()
            .collect()
    }

    /// The union of two rows.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self::new(self.roots.iter().chain(&other.roots).cloned())
    }

    /// This row with `root` added.
    #[must_use]
    pub fn with_root(mut self, root: EffectRoot) -> Self {
        if let Err(position) = self.roots.binary_search(&root) {
            self.roots.insert(position, root);
        }
        self
    }

    /// The canonical VIBON text `(record roots: (array @root...))`.
    #[must_use]
    pub fn canonical_vibon(&self) -> String {
        if self.roots.is_empty() {
            return "(record roots: (array))".to_owned();
        }
        let roots = self
            .roots
            .iter()
            .map(EffectRoot::atom)
            .collect::<Vec<_>>()
            .join(" ");
        format!("(record roots: (array {roots}))")
    }
}

impl fmt::Display for EffectRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("(")?;
        for (index, root) in self.roots.iter().enumerate() {
            if index != 0 {
                formatter.write_str(" ")?;
            }
            write!(formatter, "{root}")?;
        }
        formatter.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(package: &str, path: &str) -> EffectRoot {
        EffectRoot::new(format!("@{package}@1.0.0/{path}"), path)
    }

    #[test]
    fn a_row_is_sorted_by_path_bytes_and_duplicate_free() {
        let row = EffectRow::new([
            root("a", "std.io.stdout"),
            root("a", "app.audit.record"),
            root("a", "std.io.stdout"),
            root("a", "std.fs.write"),
        ]);
        let paths = row.roots().iter().map(EffectRoot::path).collect::<Vec<_>>();
        assert_eq!(paths, ["app.audit.record", "std.fs.write", "std.io.stdout"]);
    }

    #[test]
    fn textually_equal_roots_of_different_packages_are_different_roots() {
        let row = EffectRow::new([
            root("one", "app.log.write"),
            root("two", "app.log.write"),
        ]);
        assert_eq!(row.roots().len(), 2);
        assert!(
            !EffectRow::new([root("one", "app.log.write")])
                .is_subset(&EffectRow::new([root("two", "app.log.write")]))
        );
    }

    #[test]
    fn containment_is_a_plain_subset_test() {
        let owner = EffectRow::new([root("a", "app.audit")]);
        let both = EffectRow::new([root("a", "app.audit"), root("a", "std.fs.write")]);
        assert!(owner.is_subset(&both));
        assert!(!both.is_subset(&owner));
        assert_eq!(both.difference(&owner), vec![root("a", "std.fs.write")]);
    }

    #[test]
    fn a_row_is_a_record_in_vibon() {
        assert_eq!(
            EffectRow::empty().canonical_vibon(),
            "(record roots: (array))"
        );
        let row =
            EffectRow::new([root("a", "std.io.stdout"), root("a", "std.fs.write")]);
        assert_eq!(
            row.canonical_vibon(),
            "(record roots: (array @std.fs.write @std.io.stdout))"
        );
        assert!(row.tail().is_none());
    }
}
