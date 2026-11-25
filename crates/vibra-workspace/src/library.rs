//! The text of the embedded standard library's modules, by source identity.
//!
//! `docs/spec/04-programs-and-packages.md`, "Discovery and the source
//! snapshot": an embedded module's source identity is its module atom, such
//! as `@std.io`. A span, a related span, or a source origin can lie in one,
//! and a tool derives its display positions from that module's text, which
//! is compiled into the toolchain, never read from disk.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use vibra_types::load_stdlib;

/// The text of the embedded module whose source identity is `source_id`;
/// `None` for any other identity, and when the embedded library is
/// defective (which every command reports on its own).
#[must_use]
pub fn library_source(source_id: &str) -> Option<&'static str> {
    static TEXTS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TEXTS
        .get_or_init(|| {
            load_stdlib()
                .map(|library| {
                    library
                        .modules()
                        .iter()
                        .filter_map(|module| {
                            let text = std::str::from_utf8(module.bytes()).ok()?;
                            Some((module.source_id().to_owned(), text.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .get(source_id)
        .map(String::as_str)
}
