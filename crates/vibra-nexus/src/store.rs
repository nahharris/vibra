//! The value store of one execution context.
//!
//! `docs/spec/06-runtime.md`, "The value arena" and "Reclamation". The store is
//! one growable block of bytes that the context owns. A value is a block of it
//! with the header and payload layouts recorded in `vibra_ir::arena`, the
//! layout the toolchain-owned native code already reads, and a [`Ref`] is the
//! block's offset: a handle that means nothing outside its context, never a
//! pointer. Nothing moves, so a handle is stable for the life of its value.
//!
//! Reclamation is precise reference counting. Values cannot form cycles (the
//! runtime chapter gives the argument), so a count that reaches zero frees the
//! block, and the blocks it referred to are released through a worklist
//! threaded through the freed blocks themselves: releasing a value nested to
//! any depth uses no stack and allocates nothing.
//!
//! Every byte of a live block, and every byte a context charges for its
//! frames, counts against one limit. Reaching it is [`StoreError::Exhausted`],
//! the host event `@runtime.memory-exhausted`, never a trap.

use vibra_ir::arena::{
    STRIDE_BYTES, STRIDE_CELLS, STRIDE_CHARS, cells_offset, class, header,
};

/// The first offset a block may have. Offset `0` is never a value.
const BASE: u32 = 32;
/// The smallest and largest block sizes, as powers of two.
const MIN_SHIFT: u32 = 5;
const MAX_SHIFT: u32 = 30;
const CLASSES: usize = (MAX_SHIFT - MIN_SHIFT + 1) as usize;
/// The header word that links a block that is not live: into the release
/// worklist while its children are dropped, and then into its free list. It is
/// the variant word, which only a live value has a use for.
const LINK: u32 = header::VARIANT;

/// Why a store operation did not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The memory limit was reached or the host refused to grow the store:
    /// the host event of "Activations and memory".
    Exhausted,
    /// A handle, index, or kind the store does not admit. Verified code never
    /// produces one, so it is a defect of the toolchain.
    Fault,
}

/// A handle to a value of one store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ref(u32);

impl Ref {
    /// The offset of the value's block in its store, which is how the
    /// toolchain-owned native code names an arena object.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.0
    }
}

/// One cell: what a frame slot or a component of an object holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The class byte of `vibra_ir::arena::class`.
    pub class: u8,
    /// The scalar's bits, or a handle in the low 32 bits.
    pub bits: u64,
}

impl Cell {
    /// An empty slot, which is also the `void` value: a scalar zero.
    pub const EMPTY: Self = Self {
        class: class::I32,
        bits: 0,
    };

    /// A 32-bit-slot scalar.
    #[must_use]
    pub const fn i32(bits: u32) -> Self {
        Self {
            class: class::I32,
            bits: bits as u64,
        }
    }

    /// A 64-bit integer scalar.
    #[must_use]
    pub const fn i64(bits: u64) -> Self {
        Self {
            class: class::I64,
            bits,
        }
    }

    /// A reference to a value.
    #[must_use]
    pub const fn reference(value: Ref) -> Self {
        Self {
            class: class::REF,
            bits: value.0 as u64,
        }
    }

    /// The value the cell refers to, when it holds a reference.
    #[must_use]
    pub fn as_ref(self) -> Option<Ref> {
        if self.class == class::REF {
            u32::try_from(self.bits).ok().map(Ref)
        } else {
            None
        }
    }
}

type Done<T> = Result<T, StoreError>;

/// The value store.
#[derive(Debug)]
pub struct Store {
    memory: Vec<u8>,
    /// The offset of the first byte never handed out.
    bump: u32,
    /// The head of the free list of each size class, `0` when it is empty.
    free: [u32; CLASSES],
    live: u64,
    /// The greatest value `live` has had. It never falls.
    peak: u64,
    limit: u64,
}

impl Store {
    /// An empty store that admits at most `limit` live bytes.
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            memory: Vec::new(),
            bump: BASE,
            free: [0; CLASSES],
            live: 0,
            peak: 0,
            limit,
        }
    }

    /// The bytes now live: every block allocated and not freed, at its block
    /// size, and every byte charged and not returned.
    #[must_use]
    pub const fn live_size(&self) -> u64 {
        self.live
    }

    /// The most bytes [`Store::live_size`] has been since the store was made.
    /// It is monotone: releasing a value does not lower it.
    #[must_use]
    pub const fn peak_size(&self) -> u64 {
        self.peak
    }

    /// A view of the store's bytes for `vibra_native::arena`, the native code
    /// that implements `vibra_native::Source` and `vibra_native::Sink` over
    /// arena objects. A [`Ref::offset`] names an object in it. The store
    /// shares the layout with that code through `vibra_ir::arena` alone.
    pub fn native_memory(&mut self) -> vibra_native::arena::Memory<'_> {
        vibra_native::arena::memory(&mut self.memory)
    }

    /// Counts `bytes` held outside the store, such as a frame, against the
    /// limit.
    ///
    /// # Errors
    ///
    /// [`StoreError::Exhausted`] when the limit does not admit them.
    pub fn charge(&mut self, bytes: u64) -> Done<()> {
        let live = self.live.checked_add(bytes).ok_or(StoreError::Exhausted)?;
        if live > self.limit {
            return Err(StoreError::Exhausted);
        }
        self.live = live;
        self.peak = self.peak.max(live);
        Ok(())
    }

    /// Returns bytes that [`Store::charge`] counted.
    pub fn uncharge(&mut self, bytes: u64) {
        self.live = self.live.saturating_sub(bytes);
    }

    fn bytes(&self, at: u64, len: usize) -> Done<&[u8]> {
        let start = usize::try_from(at).map_err(|_| StoreError::Fault)?;
        let end = start.checked_add(len).ok_or(StoreError::Fault)?;
        self.memory.get(start..end).ok_or(StoreError::Fault)
    }

    fn bytes_mut(&mut self, at: u64, len: usize) -> Done<&mut [u8]> {
        let start = usize::try_from(at).map_err(|_| StoreError::Fault)?;
        let end = start.checked_add(len).ok_or(StoreError::Fault)?;
        self.memory.get_mut(start..end).ok_or(StoreError::Fault)
    }

    fn load8(&self, at: u64) -> Done<u8> {
        self.bytes(at, 1)?.first().copied().ok_or(StoreError::Fault)
    }

    fn load32(&self, at: u64) -> Done<u32> {
        let bytes = self.bytes(at, 4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().map_err(|_| StoreError::Fault)?,
        ))
    }

    fn load64(&self, at: u64) -> Done<u64> {
        let bytes = self.bytes(at, 8)?;
        Ok(u64::from_le_bytes(
            bytes.try_into().map_err(|_| StoreError::Fault)?,
        ))
    }

    fn store8(&mut self, at: u64, value: u8) -> Done<()> {
        self.bytes_mut(at, 1)?.fill(value);
        Ok(())
    }

    fn store32(&mut self, at: u64, value: u32) -> Done<()> {
        self.bytes_mut(at, 4)?.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn store64(&mut self, at: u64, value: u64) -> Done<()> {
        self.bytes_mut(at, 8)?.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn field(value: Ref, offset: u32) -> u64 {
        u64::from(value.0) + u64::from(offset)
    }

    /// Allocates a value of `kind` with `len` components of `stride` bytes,
    /// all zero, holding one reference.
    ///
    /// # Errors
    ///
    /// [`StoreError::Exhausted`] when the limit or the host does not admit the
    /// block, and [`StoreError::Fault`] for a stride that is no layout.
    pub fn alloc(
        &mut self,
        kind: u8,
        stride: u32,
        len: u32,
        variant: u32,
    ) -> Done<Ref> {
        let payload = match stride {
            STRIDE_CELLS => {
                u64::from(len.div_ceil(8)) * 8
                    + u64::from(len) * u64::from(STRIDE_CELLS)
            }
            STRIDE_CHARS | STRIDE_BYTES => u64::from(len) * u64::from(stride),
            _ => return Err(StoreError::Fault),
        };
        let needed = u64::from(header::SIZE) + payload;
        let shift = needed.next_power_of_two().trailing_zeros().max(MIN_SHIFT);
        if shift > MAX_SHIFT {
            return Err(StoreError::Exhausted);
        }
        let size = 1_u32 << shift;
        let slot = usize::try_from(shift - MIN_SHIFT).map_err(|_| StoreError::Fault)?;
        self.charge(u64::from(size))?;
        let head = self.free.get(slot).copied().ok_or(StoreError::Fault)?;
        let at = if head == 0 {
            match self.grow(size) {
                Ok(at) => at,
                Err(error) => {
                    self.uncharge(u64::from(size));
                    return Err(error);
                }
            }
        } else {
            // A free block keeps the next block of its list in its link word.
            let next = self.load32(u64::from(head) + u64::from(LINK))?;
            if let Some(entry) = self.free.get_mut(slot) {
                *entry = next;
            }
            head
        };
        let value = Ref(at);
        let block = usize::try_from(size).map_err(|_| StoreError::Fault)?;
        self.bytes_mut(u64::from(at), block)?.fill(0);
        self.store32(Self::field(value, header::COUNT), 1)?;
        self.store32(Self::field(value, header::BLOCK_SIZE), size)?;
        self.store8(Self::field(value, header::KIND), kind)?;
        self.store8(
            Self::field(value, header::STRIDE),
            u8::try_from(stride).map_err(|_| StoreError::Fault)?,
        )?;
        self.store32(Self::field(value, header::LEN), len)?;
        self.store32(Self::field(value, header::VARIANT), variant)?;
        Ok(value)
    }

    /// Takes `size` fresh bytes at the bump pointer.
    fn grow(&mut self, size: u32) -> Done<u32> {
        let at = self.bump;
        let end = at.checked_add(size).ok_or(StoreError::Exhausted)?;
        let needed = usize::try_from(end).map_err(|_| StoreError::Exhausted)?;
        if needed > self.memory.len() {
            let more = needed - self.memory.len();
            self.memory
                .try_reserve(more)
                .map_err(|_| StoreError::Exhausted)?;
            self.memory.resize(needed, 0);
        }
        self.bump = end;
        Ok(at)
    }

    /// Takes one more reference to a value.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a handle that names no live value.
    pub fn dup(&mut self, value: Ref) -> Done<()> {
        let at = Self::field(value, header::COUNT);
        let count = self.live_count(value)?;
        self.store32(at, count.checked_add(1).ok_or(StoreError::Exhausted)?)
    }

    fn live_count(&self, value: Ref) -> Done<u32> {
        if value.0 < BASE || value.0 >= self.bump {
            return Err(StoreError::Fault);
        }
        match self.load32(Self::field(value, header::COUNT))? {
            0 => Err(StoreError::Fault),
            count => Ok(count),
        }
    }

    /// Drops one reference, freeing the value and everything only it reached
    /// when it was the last.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a handle that names no live value.
    pub fn release(&mut self, value: Ref) -> Done<()> {
        if !self.decrement(value)? {
            return Ok(());
        }
        // The worklist: blocks whose count reached zero, linked through the
        // link word, which nothing reads of a value that is no longer live.
        let mut pending = value.0;
        self.store32(Self::field(value, LINK), 0)?;
        while pending != 0 {
            let block = Ref(pending);
            pending = self.load32(Self::field(block, LINK))?;
            if u32::from(self.load8(Self::field(block, header::STRIDE))?)
                == STRIDE_CELLS
            {
                let len = self.load32(Self::field(block, header::LEN))?;
                for index in 0..len {
                    let cell = self.read_cell(block, len, index)?;
                    if let Some(child) = cell.as_ref()
                        && self.decrement(child)?
                    {
                        self.store32(Self::field(child, LINK), pending)?;
                        pending = child.0;
                    }
                }
            }
            let size = self.load32(Self::field(block, header::BLOCK_SIZE))?;
            let slot = usize::try_from(
                size.trailing_zeros()
                    .checked_sub(MIN_SHIFT)
                    .ok_or(StoreError::Fault)?,
            )
            .map_err(|_| StoreError::Fault)?;
            let head = self.free.get_mut(slot).ok_or(StoreError::Fault)?;
            let next = std::mem::replace(head, block.0);
            // The count stays zero, so a stale handle is a fault, never a value.
            self.store32(Self::field(block, LINK), next)?;
            self.uncharge(u64::from(size));
        }
        Ok(())
    }

    /// Lowers a count by one and answers whether it reached zero.
    fn decrement(&mut self, value: Ref) -> Done<bool> {
        let count = self.live_count(value)? - 1;
        self.store32(Self::field(value, header::COUNT), count)?;
        Ok(count == 0)
    }

    /// Takes one more reference to what a cell holds, if it holds a value.
    ///
    /// # Errors
    ///
    /// As [`Store::dup`].
    pub fn dup_cell(&mut self, cell: Cell) -> Done<()> {
        cell.as_ref().map_or(Ok(()), |value| self.dup(value))
    }

    /// Drops the reference a cell holds, if it holds a value.
    ///
    /// # Errors
    ///
    /// As [`Store::release`].
    pub fn drop_cell(&mut self, cell: Cell) -> Done<()> {
        cell.as_ref().map_or(Ok(()), |value| self.release(value))
    }

    fn layout(&self, value: Ref, stride: u32) -> Done<u32> {
        self.live_count(value)?;
        if u32::from(self.load8(Self::field(value, header::STRIDE))?) != stride {
            return Err(StoreError::Fault);
        }
        self.load32(Self::field(value, header::LEN))
    }

    /// The kind code of a value.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a handle that names no live value.
    pub fn kind(&self, value: Ref) -> Done<u8> {
        self.live_count(value)?;
        self.load8(Self::field(value, header::KIND))
    }

    /// The component count of a value.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a handle that names no live value.
    pub fn len(&self, value: Ref) -> Done<u32> {
        self.live_count(value)?;
        self.load32(Self::field(value, header::LEN))
    }

    /// The variant index of an enum or the member index of a union.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a handle that names no live value.
    pub fn variant(&self, value: Ref) -> Done<u32> {
        self.live_count(value)?;
        self.load32(Self::field(value, header::VARIANT))
    }

    fn read_cell(&self, value: Ref, len: u32, index: u32) -> Done<Cell> {
        if index >= len {
            return Err(StoreError::Fault);
        }
        Ok(Cell {
            class: self.load8(Self::field(value, header::SIZE) + u64::from(index))?,
            bits: self
                .load64(Self::field(value, cells_offset(len)) + u64::from(index) * 8)?,
        })
    }

    /// One cell of a cells-layout value, borrowed: the caller takes its own
    /// reference with [`Store::dup_cell`] if it keeps it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a value of another layout or an index outside
    /// it.
    pub fn cell(&self, value: Ref, index: u32) -> Done<Cell> {
        let len = self.layout(value, STRIDE_CELLS)?;
        self.read_cell(value, len, index)
    }

    /// Moves `cell` into a cells-layout value that is being built. The value
    /// takes over the reference the cell holds.
    ///
    /// # Errors
    ///
    /// As [`Store::cell`].
    pub fn set_cell(&mut self, value: Ref, index: u32, cell: Cell) -> Done<()> {
        let len = self.layout(value, STRIDE_CELLS)?;
        if index >= len {
            return Err(StoreError::Fault);
        }
        self.store8(
            Self::field(value, header::SIZE) + u64::from(index),
            cell.class,
        )?;
        self.store64(
            Self::field(value, cells_offset(len)) + u64::from(index) * 8,
            cell.bits,
        )
    }

    /// A new chars-layout value of `kind` (a `str` or an atom) holding `text`.
    ///
    /// # Errors
    ///
    /// As [`Store::alloc`].
    pub fn new_text(&mut self, kind: u8, text: &str) -> Done<Ref> {
        let len =
            u32::try_from(text.chars().count()).map_err(|_| StoreError::Exhausted)?;
        let value = self.alloc(kind, STRIDE_CHARS, len, 0)?;
        let mut at = Self::field(value, header::SIZE);
        for scalar in text.chars() {
            self.store32(at, u32::from(scalar))?;
            at += u64::from(STRIDE_CHARS);
        }
        Ok(value)
    }

    /// The scalars of a chars-layout value as text.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a value of another layout or a cell that is
    /// not a Unicode scalar.
    pub fn text(&self, value: Ref) -> Done<String> {
        let len = self.layout(value, STRIDE_CHARS)?;
        let mut text = String::new();
        let mut at = Self::field(value, header::SIZE);
        for _ in 0..len {
            text.push(char::from_u32(self.load32(at)?).ok_or(StoreError::Fault)?);
            at += u64::from(STRIDE_CHARS);
        }
        Ok(text)
    }

    /// A new bytes-layout value of `kind` holding `bytes`.
    ///
    /// # Errors
    ///
    /// As [`Store::alloc`].
    pub fn new_bytes(&mut self, kind: u8, bytes: &[u8]) -> Done<Ref> {
        let len = u32::try_from(bytes.len()).map_err(|_| StoreError::Exhausted)?;
        let value = self.alloc(kind, STRIDE_BYTES, len, 0)?;
        self.bytes_mut(Self::field(value, header::SIZE), bytes.len())?
            .copy_from_slice(bytes);
        Ok(value)
    }

    /// The bytes of a bytes-layout value.
    ///
    /// # Errors
    ///
    /// [`StoreError::Fault`] for a value of another layout.
    pub fn byte_values(&self, value: Ref) -> Done<Vec<u8>> {
        let len = self.layout(value, STRIDE_BYTES)?;
        let len = usize::try_from(len).map_err(|_| StoreError::Fault)?;
        Ok(self.bytes(Self::field(value, header::SIZE), len)?.to_vec())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use vibra_ir::arena::kind;

    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn tuple(store: &mut Store, cells: &[Cell]) -> Ref {
        let len = u32::try_from(cells.len()).unwrap();
        let value = store.alloc(kind::TUPLE, STRIDE_CELLS, len, 0).unwrap();
        for (index, cell) in cells.iter().enumerate() {
            store
                .set_cell(value, u32::try_from(index).unwrap(), *cell)
                .unwrap();
        }
        value
    }

    #[test]
    fn dup_and_release_balance_to_an_empty_store() {
        let mut store = Store::new(MIB);
        let text = store.new_text(kind::STR, "héllo").unwrap();
        let pair = tuple(&mut store, &[Cell::reference(text), Cell::i64(7)]);
        store.dup(pair).unwrap();
        assert_eq!(store.text(text).unwrap(), "héllo");
        assert_eq!(store.cell(pair, 1).unwrap(), Cell::i64(7));
        store.release(pair).unwrap();
        assert!(store.live_size() > 0, "one reference still holds the pair");
        store.release(pair).unwrap();
        assert_eq!(store.live_size(), 0, "the pair released the text it held");
    }

    #[test]
    fn a_released_handle_is_a_fault_and_never_a_value() {
        let mut store = Store::new(MIB);
        let value = tuple(&mut store, &[Cell::i64(1)]);
        store.release(value).unwrap();
        assert_eq!(store.cell(value, 0), Err(StoreError::Fault));
        assert_eq!(store.dup(value), Err(StoreError::Fault));
        assert_eq!(store.release(value), Err(StoreError::Fault));
        assert_eq!(store.kind(Ref(0)), Err(StoreError::Fault));
    }

    #[test]
    fn a_freed_block_serves_the_next_request_of_its_class() {
        let mut store = Store::new(MIB);
        let first = tuple(&mut store, &[Cell::i64(1)]);
        store.release(first).unwrap();
        let second = tuple(&mut store, &[Cell::i64(2)]);
        assert_eq!(first, second, "the free list is used before fresh bytes");
        assert_eq!(store.cell(second, 0).unwrap(), Cell::i64(2));
    }

    #[test]
    fn the_limit_is_the_host_event_and_a_freed_byte_is_usable_again() {
        let mut store = Store::new(64);
        let value = tuple(&mut store, &[Cell::i64(1)]);
        assert_eq!(store.live_size(), 64);
        assert_eq!(
            store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0),
            Err(StoreError::Exhausted)
        );
        store.release(value).unwrap();
        assert!(store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0).is_ok());
    }

    #[test]
    fn a_value_nested_a_hundred_thousand_deep_releases_without_stack() {
        // A small thread stack: a release that recursed per level would
        // overflow it long before the last level.
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let mut store = Store::new(64 * MIB);
                let mut value = tuple(&mut store, &[Cell::i64(0)]);
                for _ in 0..100_000 {
                    value = tuple(&mut store, &[Cell::reference(value)]);
                }
                store.release(value).unwrap();
                assert_eq!(store.live_size(), 0);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn a_wide_value_releases_every_child() {
        let mut store = Store::new(64 * MIB);
        let children = (0..1000)
            .map(|n| Cell::reference(tuple(&mut store, &[Cell::i64(n)])))
            .collect::<Vec<_>>();
        let wide = tuple(&mut store, &children);
        store.release(wide).unwrap();
        assert_eq!(store.live_size(), 0);
    }

    // The tests below are the arena host tests of milestone 4
    // (`crates/vibra-conformance/tests/it/arena_runtime.rs` and the unit tests
    // of `vibra-wasm`'s layout), driven on the store directly. A constant
    // that differs from the M4 one says so, and why.

    /// A block of `len` bytes that is allocated and released at once.
    fn bytes_round_trip(store: &mut Store, len: u32) -> Done<()> {
        let value = store.alloc(kind::BYTES, STRIDE_BYTES, len, 0)?;
        store.release(value)
    }

    /// Runs `body` on a thread with the 64 KiB stack that the M4 deep-release
    /// tests gave the engine, so a release that recursed could not finish.
    fn on_a_small_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(body)
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn blocks_are_the_smallest_power_of_two_class_from_32_bytes() {
        // M4 `size_classes_round_up_to_a_power_of_two`, observed as the bytes
        // a value of that request size makes live.
        let live_after = |len: u32| {
            let mut store = Store::new(MIB);
            store.alloc(kind::BYTES, STRIDE_BYTES, len, 0).unwrap();
            store.live_size()
        };
        let header = header::SIZE;
        assert_eq!(live_after(0), 32, "the smallest class holds a bare header");
        assert_eq!(live_after(32 - header), 32, "a request of exactly 32");
        assert_eq!(
            live_after(33 - header),
            64,
            "one byte more is the next class"
        );
        assert_eq!(live_after(64 - header), 64);
        assert_eq!(live_after(65 - header), 128);
    }

    #[test]
    fn payload_sizes_follow_the_stride() {
        // M4 `payload_sizes_follow_the_stride`: the request is the header and
        // the payload of the layout, and the live size is its class.
        let live_after = |stride: u32, len: u32| {
            let mut store = Store::new(MIB);
            store.alloc(kind::TUPLE, stride, len, 0).unwrap();
            store.live_size()
        };
        let header = u64::from(header::SIZE);
        let request = |payload: u64| (header + payload).next_power_of_two();
        assert_eq!(live_after(STRIDE_CELLS, 0), request(0));
        assert_eq!(live_after(STRIDE_CELLS, 1), request(8 + 8));
        assert_eq!(live_after(STRIDE_CELLS, 9), request(16 + 72));
        assert_eq!(live_after(STRIDE_CHARS, 3), request(12));
        assert_eq!(live_after(STRIDE_BYTES, 5), request(5));
        assert_eq!(live_after(STRIDE_CELLS, 9), 128, "112 bytes in a 128 class");
        assert_eq!(
            Store::new(MIB).alloc(kind::TUPLE, STRIDE_CHARS + 1, 1, 0),
            Err(StoreError::Fault),
            "a stride that is no layout is a defect, never a block"
        );
    }

    #[test]
    fn dup_and_drop_balance_to_a_live_size_equal_to_the_start() {
        // M4 `dup_and_drop_balance_to_a_live_size_equal_to_the_start`: fifty
        // extra references taken and dropped leave the object live with its
        // one, and its last drop returns the store to its start.
        let mut store = Store::new(MIB);
        let object = tuple(&mut store, &[Cell::i64(1), Cell::i64(2), Cell::i64(3)]);
        let held = store.live_size();
        assert_eq!(held, 64, "24 + 8 + 3 * 8 bytes in a 64-byte class");
        for _ in 0..50 {
            store.dup(object).unwrap();
        }
        for _ in 0..50 {
            store.release(object).unwrap();
        }
        assert_eq!(store.live_size(), held, "the count is back to one");
        assert_eq!(store.cell(object, 2).unwrap(), Cell::i64(3));
        store.release(object).unwrap();
        assert_eq!(store.live_size(), 0);
    }

    #[test]
    fn the_free_list_serves_its_own_class_and_only_partly_satisfies_a_larger_request() {
        // M4 test of the same name.
        let mut store = Store::new(MIB);
        let header = header::SIZE;
        // A 32-byte block, freed.
        let a = store.alloc(kind::BYTES, STRIDE_BYTES, 8, 0).unwrap();
        store.release(a).unwrap();
        // A request one class up cannot use it.
        let b = store
            .alloc(kind::BYTES, STRIDE_BYTES, 64 - header - 10, 0)
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(store.live_size(), 64);
        // A request of its own class takes it back.
        let c = store.alloc(kind::BYTES, STRIDE_BYTES, 8, 0).unwrap();
        assert_eq!(a, c);
        store.release(b).unwrap();
        store.release(c).unwrap();
        assert_eq!(store.live_size(), 0);
    }

    /// A chain of `depth` tuples, each holding the one before, the first
    /// holding a scalar. The answer is the head.
    fn chain(store: &mut Store, depth: u32) -> Done<Ref> {
        let mut head = store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0)?;
        for _ in 0..depth {
            let node = store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0)?;
            store.set_cell(node, 0, Cell::reference(head))?;
            head = node;
        }
        Ok(head)
    }

    #[test]
    fn a_value_nested_deep_releases_with_bounded_stack() {
        // M4 `a_value_nested_deep_releases_with_bounded_engine_stack`: 5,000
        // and 100,000 levels on a 64 KiB stack. A node is a 64-byte block. The
        // M4 live size while held was `(depth + 1) * 64 + 256`
        // (320,320 and 6,400,320): the 256 bytes were the Wasm handle table,
        // which a native context does not have (N7), so the Nexus live size is
        // 320,064 and 6,400,064.
        on_a_small_stack(|| {
            for (depth, held) in [(5_000_u32, 320_064_u64), (100_000, 6_400_064)] {
                let mut store = Store::new(64 * MIB);
                let head = chain(&mut store, depth).unwrap();
                assert_eq!(store.live_size(), held, "depth {depth}: held");
                assert_eq!(store.peak_size(), held);
                store.release(head).unwrap();
                assert_eq!(store.live_size(), 0, "depth {depth}: released");
                assert_eq!(store.peak_size(), held, "the peak is not lowered");
            }
        });
    }

    #[test]
    fn a_worklist_that_grows_past_a_page_releases_and_returns_every_byte() {
        // M4 test of the same name: one block of 100,000 references to blocks
        // that die with it, so all of them wait at once.
        const WIDTH: u32 = 100_000;
        on_a_small_stack(|| {
            let mut store = Store::new(64 * MIB);
            let root = store.alloc(kind::ARRAY, STRIDE_CELLS, WIDTH, 0).unwrap();
            for index in 0..WIDTH {
                let leaf = store.alloc(kind::BYTES, STRIDE_BYTES, 0, 0).unwrap();
                store.set_cell(root, index, Cell::reference(leaf)).unwrap();
            }
            assert_eq!(store.len(root), Ok(WIDTH));
            let held = store.live_size();
            assert!(held > 4 * u64::from(WIDTH) * 8, "{held}");
            // The root is a 1 MiB block (the 100,000 class bytes, the same
            // number of cells, and the header) and each leaf a 32-byte block.
            assert_eq!(held, 1_048_576 + u64::from(WIDTH) * 32);
            store.release(root).unwrap();
            assert_eq!(store.live_size(), 0);
        });
    }

    /// The live size and the peak after a loop of `count` iterations that
    /// builds a fresh tuple and drops the previous one: the allocating tail
    /// loop of "Reclamation".
    fn allocating_loop(count: u32) -> (u64, u64) {
        let mut store = Store::new(MIB);
        let mut kept = tuple(&mut store, &[Cell::EMPTY, Cell::EMPTY]);
        for _ in 0..count {
            let fresh = tuple(&mut store, &[Cell::EMPTY, Cell::EMPTY]);
            store.release(kept).unwrap();
            kept = fresh;
        }
        (store.live_size(), store.peak_size())
    }

    #[test]
    fn an_allocating_loop_has_a_live_size_that_does_not_grow_with_its_iterations() {
        let [small, large] = [10_000_u32, 100_000].map(allocating_loop);
        assert_eq!(
            small.0, large.0,
            "the live size after 100,000 iterations equals the one after 10,000"
        );
        // M4 held 320 bytes at both counts: the 64-byte tuple and the 256-byte
        // handle table. A native context has no handle table (N7), so it
        // holds the tuple alone.
        assert_eq!(large.0, 64);
        assert_eq!(small.1, large.1, "the peak does not grow either");
        assert_eq!(large.1, 128, "the old tuple and the new one, at once");
    }

    #[test]
    fn running_out_of_memory_is_the_host_event_and_never_a_defect() {
        // M4 `running_out_of_memory_is_the_host_event_and_never_a_trap`: a
        // chain that is never released, in a 1 MiB store.
        let mut store = Store::new(MIB);
        assert_eq!(chain(&mut store, 1_000_000), Err(StoreError::Exhausted));
        // The store still answers, holds what it built, and the limit is
        // exactly reached: 16,384 nodes of 64 bytes.
        assert_eq!(store.live_size(), MIB);
        assert_eq!(store.peak_size(), MIB);
        // A fresh store runs normally.
        let mut fresh = Store::new(MIB);
        let head = chain(&mut fresh, 10).unwrap();
        fresh.release(head).unwrap();
        assert_eq!(fresh.live_size(), 0);
    }

    #[test]
    fn a_store_that_ran_out_of_memory_recovers_when_values_are_released() {
        // The recovery half of the M4 test: the nodes that were built are
        // still values, and releasing them makes room.
        let mut store = Store::new(MIB);
        let mut heads = Vec::new();
        while let Ok(head) = store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0) {
            heads.push(head);
        }
        assert_eq!(
            store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0),
            Err(StoreError::Exhausted)
        );
        let last = heads.pop().unwrap();
        store.release(last).unwrap();
        assert!(store.alloc(kind::TUPLE, STRIDE_CELLS, 1, 0).is_ok());
    }

    #[test]
    fn the_largest_size_class_is_allocated_or_exhausts_and_nothing_above_it_exists() {
        // M4 test of the same name. The limit is 64 MiB, as in M4.
        let largest = (1_u32 << MAX_SHIFT) - header::SIZE;
        let mut store = Store::new(64 * MIB);
        // 2^25 bytes fit the limit and are returned to the free list.
        bytes_round_trip(&mut store, (1 << 25) - header::SIZE).unwrap();
        assert_eq!(store.live_size(), 0);
        // The largest class is a class: the limit refuses it.
        assert_eq!(
            bytes_round_trip(&mut store, largest),
            Err(StoreError::Exhausted)
        );
        // One byte more has no class at all, and stops the same way, whatever
        // the limit: a store that admits everything has no larger block.
        assert_eq!(
            bytes_round_trip(&mut store, largest + 1),
            Err(StoreError::Exhausted)
        );
        let mut unlimited = Store::new(u64::MAX);
        assert_eq!(
            bytes_round_trip(&mut unlimited, largest + 1),
            Err(StoreError::Exhausted),
            "nothing is above the largest class"
        );
        // A length whose size overflows 32 bits is no wrapped small request.
        assert_eq!(
            bytes_round_trip(&mut store, u32::MAX),
            Err(StoreError::Exhausted)
        );
        assert_eq!(
            bytes_round_trip(&mut unlimited, u32::MAX),
            Err(StoreError::Exhausted)
        );
        assert_eq!(store.live_size(), 0, "a refused request charges nothing");
        assert_eq!(unlimited.live_size(), 0);
    }

    #[test]
    fn a_block_that_exactly_fills_the_limit_runs_and_one_byte_under_does_not() {
        // M4 `a_block_that_exactly_fills_the_limit_runs_and_one_page_under_does_not`
        // measured three 64 KiB pages: the page of the module's own state, and
        // the 128 KiB block that a 64 KiB request (its header spills over a
        // page) is rounded to. The store keeps no state in its bytes (N7), so
        // the limit that admits the request is the block alone, 131,072
        // bytes, and the step is a byte, not a page.
        const NEED: u64 = 131_072;
        let runs = |limit: u64| {
            let mut store = Store::new(limit);
            bytes_round_trip(&mut store, 1 << 16)
        };
        assert_eq!(runs(NEED - 1), Err(StoreError::Exhausted), "one byte under");
        assert_eq!(runs(NEED), Ok(()), "exactly at the need");
        assert_eq!(runs(NEED + 1), Ok(()));
        // The limit counts the block, not the request.
        let mut store = Store::new(NEED);
        let value = store.alloc(kind::BYTES, STRIDE_BYTES, 1 << 16, 0).unwrap();
        assert_eq!(store.live_size(), NEED);
        store.release(value).unwrap();
    }

    #[test]
    fn the_peak_is_monotone_and_at_least_the_live_size() {
        let mut store = Store::new(MIB);
        let mut peaks = vec![store.peak_size()];
        let mut check = |store: &Store| {
            assert!(store.peak_size() >= store.live_size());
            peaks.push(store.peak_size());
        };
        check(&store);
        let first = tuple(&mut store, &[Cell::i64(1)]);
        check(&store);
        let second = tuple(&mut store, &[Cell::reference(first), Cell::i64(2)]);
        check(&store);
        let high = store.live_size();
        assert_eq!(store.peak_size(), high);
        store.release(second).unwrap();
        check(&store);
        assert_eq!(store.live_size(), 0);
        assert_eq!(store.peak_size(), high, "release does not lower the peak");
        store.charge(10).unwrap();
        check(&store);
        assert_eq!(store.peak_size(), high, "10 bytes stay below the old peak");
        store.charge(high).unwrap();
        check(&store);
        assert_eq!(store.peak_size(), high + 10, "charged bytes raise it");
        store.uncharge(high + 10);
        check(&store);
        assert_eq!(store.peak_size(), high + 10);
        assert!(peaks.windows(2).all(|pair| pair[0] <= pair[1]), "{peaks:?}");
    }

    #[test]
    fn the_native_arena_reads_a_store_value() {
        use vibra_ir::external::CompilerIntrinsic;
        use vibra_native::arena::{Fault, call};

        let row = |intrinsic: CompilerIntrinsic| intrinsic.native_import().unwrap();
        let text = "héllo, wörld";
        let mut store = Store::new(MIB);
        let value = store.new_text(kind::STR, text).unwrap();
        let equal = store.new_text(kind::STR, text).unwrap();
        let other = store.new_text(kind::STR, "héllo").unwrap();
        let bytes = store.new_bytes(kind::BYTES, b"abc").unwrap();
        let at = |value: Ref| i64::from(value.offset());
        let live = store.live_size();

        let memory = store.native_memory();
        let chars = i64::try_from(text.chars().count()).unwrap();
        assert_eq!(
            call(&row(CompilerIntrinsic::StrLength), memory, &[at(value)]),
            Ok(chars)
        );
        let compare = |a: Ref, b: Ref| {
            call(&row(CompilerIntrinsic::StrEqual), memory, &[at(a), at(b)])
        };
        assert_eq!(compare(value, equal), Ok(1));
        assert_eq!(compare(value, other), Ok(0));
        // An object of another layout is refused, never misread.
        assert_eq!(
            call(&row(CompilerIntrinsic::StrLength), memory, &[at(bytes)]),
            Err(Fault::WrongObject)
        );
        // The text itself: the row that builds `bytes` from a `str` measures
        // with a destination of 0 and then fills the destination.
        let to_utf8 = row(CompilerIntrinsic::StrToUtf8);
        let measured = call(&to_utf8, memory, &[at(value), 0]).unwrap();
        assert_eq!(measured, i64::try_from(text.len()).unwrap());

        let destination = store.new_bytes(kind::BYTES, &vec![0; text.len()]).unwrap();
        let memory = store.native_memory();
        assert_eq!(
            call(&to_utf8, memory, &[at(value), at(destination)]),
            Ok(measured)
        );
        assert_eq!(store.byte_values(destination).unwrap(), text.as_bytes());
        assert_eq!(store.text(value).unwrap(), text, "the store reads it too");
        let after = store.live_size();
        assert!(after > live, "only the destination was added");
    }
}
