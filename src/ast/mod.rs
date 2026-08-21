//! Typed native-language surface tree.
//!
//! Unlike the legacy structural-code layer, this tree models grammar roles
//! directly. Rewriters replace typed nodes and retain [`Origin`] metadata
//! instead of addressing generic mapping keys or sequence indices.

mod scope;
mod surface;
mod typed_macro_expand;

pub use scope::*;
pub use surface::*;
pub use typed_macro_expand::*;
