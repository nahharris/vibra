use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Number, Value};

use super::{CodeError, CodeErrorKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub value: Form,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Form {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Sequence(Vec<Form>),
    Mapping(Vec<Entry>),
}

impl Form {
    pub fn parse(source: &str) -> Result<Self, CodeError> {
        let source = dedent_fragment(source);
        let value: Value = serde_yaml::from_str(&source).map_err(|error| {
            CodeError::new(
                CodeErrorKind::InvalidSource,
                format!("parse structural form: {error}"),
            )
        })?;
        Self::from_yaml_value(value)
    }

    pub fn render(&self) -> Result<String, CodeError> {
        let mut rendered = serde_yaml::to_string(&self.to_yaml_value()).map_err(|error| {
            CodeError::new(
                CodeErrorKind::InvalidForm,
                format!("render structural form: {error}"),
            )
        })?;
        while rendered.ends_with('\n') {
            rendered.pop();
        }
        Ok(rendered)
    }

    pub fn from_yaml_value(value: Value) -> Result<Self, CodeError> {
        match value {
            Value::Null => Ok(Self::Null),
            Value::Bool(value) => Ok(Self::Bool(value)),
            Value::Number(value) => number_to_form(value),
            Value::String(value) => Ok(Self::String(value)),
            Value::Sequence(values) => values
                .into_iter()
                .map(Self::from_yaml_value)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Sequence),
            Value::Mapping(values) => values
                .into_iter()
                .map(|(key, value)| {
                    let key = key.as_str().ok_or_else(|| {
                        CodeError::new(
                            CodeErrorKind::InvalidForm,
                            "Vibra structural mapping keys must be strings",
                        )
                    })?;
                    Ok(Entry {
                        key: key.to_string(),
                        value: Self::from_yaml_value(value)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Mapping),
            Value::Tagged(_) => Err(CodeError::new(
                CodeErrorKind::InvalidForm,
                "YAML tags are not part of the Vibra source subset",
            )),
        }
    }

    pub fn to_yaml_value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Int(value) => Value::Number(Number::from(*value)),
            Self::Float(value) => serde_yaml::to_value(value).expect("f64 serializes to YAML"),
            Self::String(value) => Value::String(value.clone()),
            Self::Sequence(values) => {
                Value::Sequence(values.iter().map(Self::to_yaml_value).collect())
            }
            Self::Mapping(entries) => {
                let mut mapping = Mapping::new();
                for entry in entries {
                    mapping.insert(
                        Value::String(entry.key.clone()),
                        entry.value.to_yaml_value(),
                    );
                }
                Value::Mapping(mapping)
            }
        }
    }
}

fn dedent_fragment(source: &str) -> String {
    let indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|character| *character == ' ')
                .count()
        })
        .min()
        .unwrap_or(0);
    if indent == 0 {
        return source.to_string();
    }
    source
        .split_inclusive('\n')
        .map(|line| {
            let removable = line
                .chars()
                .take_while(|character| *character == ' ')
                .count()
                .min(indent);
            &line[removable..]
        })
        .collect()
}

fn number_to_form(value: Number) -> Result<Form, CodeError> {
    if let Some(value) = value.as_i64() {
        return Ok(Form::Int(value));
    }
    if let Some(value) = value.as_f64() {
        return Ok(Form::Float(value));
    }
    Err(CodeError::new(
        CodeErrorKind::InvalidForm,
        "numeric form is outside Vibra's supported range",
    ))
}
