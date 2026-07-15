use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Segment {
    Key(String),
    Index(usize),
}

impl Segment {
    pub fn key(value: impl Into<String>) -> Self {
        Self::Key(value.into())
    }

    pub const fn index(value: usize) -> Self {
        Self::Index(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path(Vec<Segment>);

impl Path {
    pub const fn new(segments: Vec<Segment>) -> Self {
        Self(segments)
    }

    pub fn root() -> Self {
        Self::default()
    }

    pub fn segments(&self) -> &[Segment] {
        &self.0
    }

    pub fn parent(&self) -> Option<Self> {
        (!self.0.is_empty()).then(|| Self(self.0[..self.0.len() - 1].to_vec()))
    }

    pub fn last(&self) -> Option<&Segment> {
        self.0.last()
    }

    pub fn push(&mut self, segment: Segment) {
        self.0.push(segment);
    }

    pub fn child(&self, segment: Segment) -> Self {
        let mut path = self.clone();
        path.push(segment);
        path
    }

    pub fn is_prefix_of(&self, other: &Self) -> bool {
        self.0.len() <= other.0.len() && self.0 == other.0[..self.0.len()]
    }

    pub fn to_compact_strings(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|segment| match segment {
                Segment::Key(key) => key.clone(),
                Segment::Index(index) => index.to_string(),
            })
            .collect()
    }
}
