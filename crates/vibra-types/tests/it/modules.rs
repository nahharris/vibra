//! Every module of the embedded library parses in standard-library mode and
//! is already the formatter's output.

use vibra_syntax::parse_source_with;
use vibra_syntax::source::SourceOptions;
use vibra_types::StdlibInputs;

const MODULES: [&str; 9] = [
    "@std.assert",
    "@std.bool",
    "@std.builtin",
    "@std.bytes",
    "@std.core",
    "@std.iter",
    "@std.option",
    "@std.result",
    "@std.str",
];

fn library_mode() -> SourceOptions {
    SourceOptions {
        standard_library: true,
    }
}

#[test]
fn the_embedded_modules_are_those_of_the_manifest() {
    let paths = StdlibInputs::embedded()
        .modules
        .iter()
        .map(|(path, _)| *path)
        .collect::<Vec<_>>();
    assert_eq!(paths, MODULES);
}

#[test]
fn every_module_parses() {
    for (path, bytes) in StdlibInputs::embedded().modules {
        let document = parse_source_with(Some(path), bytes, library_mode());
        assert!(
            document.diagnostics().is_empty(),
            "`{path}` has reader diagnostics: {:?}",
            document.diagnostics()
        );
    }
}

#[test]
fn every_module_formats_canonically() {
    for (path, bytes) in StdlibInputs::embedded().modules {
        let document = parse_source_with(Some(path), bytes, library_mode());
        assert!(document.diagnostics().is_empty(), "`{path}` does not parse");
        let formatted = vibra_fmt::format_document(&document);
        let source = String::from_utf8_lossy(bytes);
        assert_eq!(
            formatted.as_deref(),
            Some(source.as_ref()),
            "`{path}` is not the formatter's output"
        );
    }
}

#[test]
fn every_module_is_lf_utf8_without_a_byte_order_mark() {
    for (path, bytes) in StdlibInputs::embedded().modules {
        assert!(std::str::from_utf8(bytes).is_ok(), "`{path}` is not UTF-8");
        assert!(
            !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
            "`{path}` has a BOM"
        );
        assert!(!bytes.contains(&b'\r'), "`{path}` has a carriage return");
    }
}
