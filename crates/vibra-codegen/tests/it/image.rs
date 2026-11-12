//! Reading an emitted object back with the `object` crate: symbols,
//! relocations, and the bytes of the data sections.

use std::collections::{BTreeSet, HashMap};
use std::mem::offset_of;

use object::{
    Object, ObjectSection, ObjectSymbol, RelocationTarget, SectionKind, SymbolKind,
    SymbolSection,
};
use vibra_nexus_bridge::VibraTable;

/// A place in the object: a section, by index, and an offset in it.
pub type Place = (usize, u64);

/// The sections of an object, with every relocation resolved to a place.
pub struct Image {
    pub sections: HashMap<usize, Vec<u8>>,
    /// The place each relocated pointer field points to.
    pub pointers: HashMap<Place, Place>,
}

pub fn parse(bytes: &[u8]) -> object::File<'_> {
    object::File::parse(bytes).expect("an object file")
}

fn place_of_symbol(
    file: &object::File<'_>,
    symbol: &object::Symbol<'_, '_>,
) -> Option<Place> {
    match symbol.section() {
        SymbolSection::Section(index) => {
            let section = file.section_by_index(index).expect("a section");
            Some((index.0, symbol.address() - section.address()))
        }
        _ => None,
    }
}

/// The place of the defined symbol `name`.
pub fn symbol_place(file: &object::File<'_>, name: &str) -> Option<Place> {
    file.symbols()
        .find(|symbol| symbol.name() == Ok(name))
        .and_then(|symbol| place_of_symbol(file, &symbol))
}

/// The names of the defined global symbols.
pub fn defined_globals(file: &object::File<'_>) -> BTreeSet<String> {
    file.symbols()
        .filter(|symbol| {
            symbol.is_global()
                && !symbol.is_undefined()
                && !matches!(symbol.kind(), SymbolKind::Section | SymbolKind::File)
        })
        .map(|symbol| symbol.name().expect("a name").to_owned())
        .collect()
}

/// The names of the undefined symbols.
pub fn undefined(file: &object::File<'_>) -> BTreeSet<String> {
    file.symbols()
        .filter(|symbol| symbol.is_undefined())
        .map(|symbol| symbol.name().expect("a name").to_owned())
        .collect()
}

/// The names of the defined symbols that are not global and not sections.
pub fn locals(file: &object::File<'_>) -> BTreeSet<String> {
    file.symbols()
        .filter(|symbol| {
            !symbol.is_global()
                && !symbol.is_undefined()
                && !matches!(symbol.kind(), SymbolKind::Section | SymbolKind::File)
        })
        .map(|symbol| symbol.name().expect("a name").to_owned())
        .collect()
}

/// The sections that hold code.
pub fn code_sections<'d, 'f>(
    file: &'f object::File<'d>,
) -> impl Iterator<Item = object::Section<'d, 'f>> {
    file.sections()
        .filter(|section| section.kind() == SectionKind::Text)
}

impl Image {
    pub fn new(file: &object::File<'_>) -> Self {
        let mut sections = HashMap::new();
        let mut pointers = HashMap::new();
        for section in file.sections() {
            sections.insert(section.index().0, section.data().expect("data").to_vec());
        }
        for section in file.sections() {
            for (offset, relocation) in section.relocations() {
                // Pointer fields are 64-bit absolute; code and unwind data hold other kinds.
                if relocation.size() != 64 || section.kind() == SectionKind::Text {
                    continue;
                }
                let data = &sections[&section.index().0];
                let start = usize::try_from(offset).expect("an offset");
                let implicit = if relocation.has_implicit_addend() {
                    let width = usize::from(relocation.size() / 8);
                    let field = data.get(start..start + width).expect("a field");
                    let mut wide = [0_u8; 8];
                    wide[..width].copy_from_slice(field);
                    if width < 8 && field[width - 1] & 0x80 != 0 {
                        wide[width..].fill(0xff);
                    }
                    i64::from_le_bytes(wide)
                } else {
                    0
                };
                let addend = relocation.addend() + implicit;
                let target = match relocation.target() {
                    RelocationTarget::Symbol(index) => {
                        let symbol = file.symbol_by_index(index).expect("a symbol");
                        place_of_symbol(file, &symbol)
                    }
                    RelocationTarget::Section(index) => Some((index.0, 0)),
                    _ => None,
                };
                if let Some((target_section, base)) = target {
                    let at =
                        u64::try_from(i64::try_from(base).expect("small") + addend)
                            .expect("a place");
                    pointers.insert((section.index().0, offset), (target_section, at));
                }
            }
        }
        Self { sections, pointers }
    }

    pub fn bytes(&self, place: Place, len: usize) -> &[u8] {
        let start = usize::try_from(place.1).expect("an offset");
        self.sections[&place.0]
            .get(start..start + len)
            .expect("bytes inside the section")
    }

    pub fn u32(&self, place: Place) -> u32 {
        u32::from_le_bytes(self.bytes(place, 4).try_into().expect("four bytes"))
    }

    pub fn u64(&self, place: Place) -> u64 {
        u64::from_le_bytes(self.bytes(place, 8).try_into().expect("eight bytes"))
    }

    /// The pointer stored at `place`, if a relocation stores one.
    pub fn pointer(&self, place: Place) -> Option<Place> {
        self.pointers.get(&place).copied()
    }

    /// A `VibraTable` at `place`: where it points and how many elements.
    /// A zero length has a null pointer, and a positive one has a relocation.
    pub fn table(&self, place: Place) -> (Option<Place>, usize) {
        let len = usize::try_from(self.u64((
            place.0,
            place.1 + u64::try_from(offset_of!(VibraTable<u8>, len)).expect("small"),
        )))
        .expect("a count");
        let pointer = self.pointer(place);
        if len == 0 {
            assert!(pointer.is_none(), "an empty table is a null pointer");
            assert_eq!(self.u64(place), 0);
        } else {
            assert!(pointer.is_some(), "a table with elements has an address");
        }
        (pointer, len)
    }
}
