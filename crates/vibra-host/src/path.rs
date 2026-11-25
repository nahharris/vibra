//! Path names: one name and a list of them, each under the name profile of
//! the scope policies.
//!
//! `docs/spec/03-effects.md`, "Filesystem policies", fixes the name profile
//! of a `RelativePath`; its one definition is [`vibra_ir::names`], which the
//! compiler row `fs.validNames` shares. A path is a list of names, possibly
//! empty: the empty path names the directory a grant holds.
//!
//! The vectors of `tests/it/names.tsv` hold the profile to the chapter. The
//! file-scope adapter of `vibra-fs-scope` checks the same profile with a
//! function of its own, and is run over the same vectors.

use std::fmt;

use vibra_ir::names::{NameError, check_name};

/// One name that satisfies the name profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    /// The name `text`.
    ///
    /// # Errors
    ///
    /// The first rule of the name profile that `text` breaks.
    pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
        let text = text.into();
        check_name(&text)?;
        Ok(Self(text))
    }

    /// The name's scalars.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A list of names, each satisfying the name profile: the representation of
/// a `RelativePath`. The empty list is valid and names the directory a grant
/// holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeNames(Vec<Name>);

/// A name of a [`RelativeNames`] that failed the profile: its position in the
/// list and the rule it broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidName {
    /// The position of the name in the list, from zero.
    pub index: usize,
    /// The rule it broke.
    pub error: NameError,
}

impl RelativeNames {
    /// The empty path.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// The path of `names`, in order.
    ///
    /// # Errors
    ///
    /// The first name that breaks the profile.
    pub fn new<I, S>(names: I) -> Result<Self, InvalidName>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                Name::new(name).map_err(|error| InvalidName { index, error })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// The names, in order.
    #[must_use]
    pub fn names(&self) -> &[Name] {
        &self.0
    }

    /// The number of names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this is the empty path.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `names` begins with this path, compared name by name, never
    /// as strings: `reports` does not cover `reportsOld`.
    #[must_use]
    pub fn is_prefix_of(&self, names: &[String]) -> bool {
        self.0.len() <= names.len()
            && self
                .0
                .iter()
                .zip(names)
                .all(|(own, other)| own.as_str() == other.as_str())
    }
}
