//! The interpreter's half of the host value-access interface.
//!
//! `docs/spec/06-runtime.md`, "WebAssembly boundary": each host operation is
//! written once, in `vibra-host`, against an interface that reads arguments and
//! builds results. This module supplies that interface over the interpreter's
//! own values and holds no algorithm of an operation: it reads a `str`, a
//! `bytes`, a `path`, or a `u64` out of a [`RuntimeValue`] and builds the
//! `(result t e)` value of a completed call from the pieces `vibra-host` names.

use vibra_host::{HostEnum, HostRecord, HostValues, ReadError};
use vibra_ir::external::host::{FS_KIND_VARIANTS, HostErrorType};
use vibra_ir::{Type, TypeDefinition, Value};

use crate::value::RuntimeValue;

/// The values of one host call: the checked `(result t e)` type of the call
/// and the declared types of the program, which name the enums and the record
/// a result is built of.
pub(crate) struct Values<'p> {
    result: &'p Type,
    types: &'p [TypeDefinition],
}

impl<'p> Values<'p> {
    pub(crate) const fn new(result: &'p Type, types: &'p [TypeDefinition]) -> Self {
        Self { result, types }
    }

    /// The `n`th type argument of the checked `result`.
    fn argument(&self, n: usize) -> Type {
        match self.result {
            Type::Applied(_, arguments) => {
                arguments.get(n).cloned().unwrap_or(Type::Void)
            }
            _ => Type::Void,
        }
    }

    /// The declared type whose canonical path is `path`.
    fn declared(&self, path: &str) -> Type {
        self.types
            .iter()
            .find(|definition| definition.id().path() == path)
            .map_or(Type::Void, |definition| {
                Type::Declared(definition.id().clone())
            })
    }
}

fn text(value: &RuntimeValue) -> Option<&str> {
    match value {
        RuntimeValue::Primitive(Value::Str(text)) => Some(text),
        _ => None,
    }
}

impl HostValues for Values<'_> {
    type Value = RuntimeValue;

    fn read_text(&mut self, value: &RuntimeValue) -> Result<String, ReadError> {
        text(value).map(str::to_owned).ok_or(ReadError::Value)
    }

    fn read_bytes(&mut self, value: &RuntimeValue) -> Result<Vec<u8>, ReadError> {
        match value {
            RuntimeValue::Primitive(Value::Bytes(bytes)) => Ok(bytes.clone()),
            _ => Err(ReadError::Value),
        }
    }

    fn read_path(&mut self, value: &RuntimeValue) -> Result<String, ReadError> {
        match value {
            RuntimeValue::Wrapper { value, .. } => {
                text(value).map(str::to_owned).ok_or(ReadError::Value)
            }
            _ => Err(ReadError::Value),
        }
    }

    fn read_u64(&mut self, value: &RuntimeValue) -> Result<u64, ReadError> {
        match value {
            RuntimeValue::Primitive(Value::U64(count)) => Ok(*count),
            _ => Err(ReadError::Value),
        }
    }

    fn build_void(&mut self) -> RuntimeValue {
        RuntimeValue::Primitive(Value::Void)
    }

    fn build_text(&mut self, text: &str) -> RuntimeValue {
        RuntimeValue::Primitive(Value::Str(text.to_owned()))
    }

    fn build_bytes(&mut self, bytes: &[u8]) -> RuntimeValue {
        RuntimeValue::Primitive(Value::Bytes(bytes.to_vec()))
    }

    fn build_i64(&mut self, value: i64) -> RuntimeValue {
        RuntimeValue::Primitive(Value::I64(value))
    }

    fn build_u64(&mut self, value: u64) -> RuntimeValue {
        RuntimeValue::Primitive(Value::U64(value))
    }

    fn build_array(&mut self, elements: Vec<RuntimeValue>) -> RuntimeValue {
        RuntimeValue::Array {
            value_type: Type::Array(Box::new(Type::Str)),
            values: elements.into(),
        }
    }

    fn build_enum(
        &mut self,
        ty: HostEnum,
        variant: u32,
        payload: Option<RuntimeValue>,
    ) -> RuntimeValue {
        let position = usize::try_from(variant).unwrap_or(usize::MAX);
        let error = |kind: HostErrorType| {
            kind.variants()
                .get(position)
                .copied()
                .unwrap_or_default()
                .to_owned()
        };
        let (value_type, name) = match ty {
            HostEnum::Result => (
                self.result.clone(),
                if variant == 0 { "ok" } else { "err" }.to_owned(),
            ),
            HostEnum::FsKind => (
                self.declared("std.fs.fs-kind"),
                FS_KIND_VARIANTS
                    .get(position)
                    .copied()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            HostEnum::IoError => (self.argument(1), error(HostErrorType::Io)),
            HostEnum::FsError => (self.argument(1), error(HostErrorType::Fs)),
            HostEnum::EnvError => (self.argument(1), error(HostErrorType::Env)),
            HostEnum::TimeError => (self.argument(1), error(HostErrorType::Time)),
            HostEnum::RandomError => (self.argument(1), error(HostErrorType::Random)),
        };
        RuntimeValue::Enum {
            value_type,
            variant: name,
            payload: payload.and_then(crate::present_payload),
        }
    }

    fn build_record(
        &mut self,
        ty: HostRecord,
        fields: Vec<RuntimeValue>,
    ) -> RuntimeValue {
        let names: &[&str] = match ty {
            HostRecord::FsMetadata => &["kind", "size"],
        };
        RuntimeValue::Record {
            value_type: self.argument(0),
            fields: names
                .iter()
                .map(|name| (*name).to_owned())
                .zip(fields)
                .collect::<Vec<_>>()
                .into(),
        }
    }
}
