//! The Nexus half of the host value-access interface.
//!
//! `vibra-host` writes each host operation once, against an interface that
//! reads arguments and builds results. This module supplies that interface
//! over the cells of a [`Store`], so a host call in a native executable runs
//! the same operation code, and builds the same audit event, as one in the
//! reference interpreter.
//!
//! A value of the store carries no type: the registry row fixes what each
//! argument and result is, so a builder needs no type to build with.

use vibra_host::{HostEnum, HostRecord, HostValues, ReadError};
use vibra_ir::arena::{STRIDE_CELLS, class, kind};

use crate::store::{Cell, Store, StoreError};

/// The values of one host call, and the first failure of a builder, which the
/// interface has no way to return.
pub(crate) struct Values<'s> {
    store: &'s mut Store,
    failure: Option<StoreError>,
}

impl<'s> Values<'s> {
    pub(crate) const fn new(store: &'s mut Store) -> Self {
        Self {
            store,
            failure: None,
        }
    }

    /// The first failure of a builder, if one failed.
    pub(crate) const fn failure(&self) -> Option<StoreError> {
        self.failure
    }

    fn built(&mut self, value: Result<Cell, StoreError>) -> Cell {
        value.unwrap_or_else(|error| {
            self.failure.get_or_insert(error);
            Cell::EMPTY
        })
    }

    fn object(&mut self, kind: u8, variant: u32, cells: Vec<Cell>) -> Cell {
        let built = (|| {
            let len = u32::try_from(cells.len()).map_err(|_| StoreError::Exhausted)?;
            let value = self.store.alloc(kind, STRIDE_CELLS, len, variant)?;
            for (index, cell) in (0..len).zip(cells) {
                self.store.set_cell(value, index, cell)?;
            }
            Ok(Cell::reference(value))
        })();
        self.built(built)
    }
}

impl HostValues for Values<'_> {
    type Value = Cell;

    fn read_text(&mut self, value: &Cell) -> Result<String, ReadError> {
        let value = value.as_ref().ok_or(ReadError::Value)?;
        self.store.text(value).map_err(|_| ReadError::Value)
    }

    fn read_bytes(&mut self, value: &Cell) -> Result<Vec<u8>, ReadError> {
        let value = value.as_ref().ok_or(ReadError::Value)?;
        self.store.byte_values(value).map_err(|_| ReadError::Value)
    }

    fn read_path(&mut self, value: &Cell) -> Result<String, ReadError> {
        let wrapper = value.as_ref().ok_or(ReadError::Value)?;
        let text = self.store.cell(wrapper, 0).map_err(|_| ReadError::Value)?;
        self.read_text(&text)
    }

    fn read_u64(&mut self, value: &Cell) -> Result<u64, ReadError> {
        if value.class == class::I64 {
            Ok(value.bits)
        } else {
            Err(ReadError::Value)
        }
    }

    fn build_void(&mut self) -> Cell {
        Cell::EMPTY
    }

    fn build_text(&mut self, text: &str) -> Cell {
        let built = self.store.new_text(kind::STR, text).map(Cell::reference);
        self.built(built)
    }

    fn build_bytes(&mut self, bytes: &[u8]) -> Cell {
        let built = self
            .store
            .new_bytes(kind::BYTES, bytes)
            .map(Cell::reference);
        self.built(built)
    }

    fn build_i64(&mut self, value: i64) -> Cell {
        Cell::i64(u64::from_ne_bytes(value.to_ne_bytes()))
    }

    fn build_u64(&mut self, value: u64) -> Cell {
        Cell::i64(value)
    }

    fn build_array(&mut self, elements: Vec<Cell>) -> Cell {
        self.object(kind::ARRAY, 0, elements)
    }

    fn build_enum(
        &mut self,
        _ty: HostEnum,
        variant: u32,
        payload: Option<Cell>,
    ) -> Cell {
        self.object(kind::ENUM, variant, payload.into_iter().collect())
    }

    fn build_record(&mut self, _ty: HostRecord, fields: Vec<Cell>) -> Cell {
        self.object(kind::RECORD, 0, fields)
    }
}
