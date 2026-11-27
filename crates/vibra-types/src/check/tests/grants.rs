//! Unit tests of grant types. Expectations come from the effects chapter,
//! "Capability values" (a type that holds a grant is not `Encodable`; a
//! declared type that holds one may implement `Equatable` and `Ordered`),
//! the type chapter, "Interfaces" (`Encodable`) and "Dict keys, equality,
//! and order", the programs chapter, "Assertions" (`assert.equal` is bound
//! by `T: Encodable`), and the runtime chapter, "What has no encoding". An
//! unsatisfied bound is reported once, at the operand that fixes `T`.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};

use super::{at, faults, faults_of, workspace};

/// The imports every fixture of this file uses.
const IMPORTS: &str = "use @std.assert enduse\n\nuse @std.test.TestWorld enduse\n\n\
                       use @std.fs.DirReadText enduse\n\nuse @std.fs.DirList enduse\n\n";

/// The code and span of every diagnostic of the test module
/// `@tests.grants` holding `source`, beside a one-function library module.
fn test_faults(source: &str) -> Result<Vec<(DiagnosticCode, ByteSpan)>, String> {
    let result = workspace(
        &[(
            "@app.main",
            "fn answer returns r: I32 do return 42i32 endfn\n",
        )],
        &[("@tests.grants", source)],
        &["@tests"],
    )?;
    Ok(faults_of(&result))
}

/// A test module whose one test binds `reader` and `lister`, the
/// `readText` and `list` grants of a fresh directory, then runs `body`;
/// `declarations` come before the test.
fn with_grants(declarations: &str, body: &str) -> String {
    format!(
        "{IMPORTS}{declarations}test \"grants\"\n  takes {{world: TestWorld}}\n  \
         performs {{TestWorld}}\n  do\n    \
         let {{readText: reader list: lister}} be await TestWorld.directory(world) \
         endlet\n    {body}\nendtest\n"
    )
}

/// The span of `inner` inside the first occurrence of `outer` in `source`.
fn inside(source: &str, outer: &str, inner: &str) -> ByteSpan {
    let base = at(source, outer, 0).start();
    let offset = outer.find(inner).unwrap_or(usize::MAX);
    let start = base.saturating_add(offset);
    ByteSpan::new(start, start.saturating_add(inner.len()))
}

/// The one `@type.unsatisfiedBound` of `call`, at its first operand `left`,
/// which fixes `T`. `_right` is the other operand, which is not reported.
fn both(
    source: &str,
    call: &str,
    left: &str,
    _right: &str,
) -> Vec<(DiagnosticCode, ByteSpan)> {
    vec![(
        DiagnosticCode::TypeUnsatisfiedBound,
        inside(source, call, left),
    )]
}

#[test]
fn a_bare_grant_is_not_encodable() -> Result<(), String> {
    let call = "assert.equal(reader reader)";
    let source = with_grants("", call);
    assert_eq!(
        test_faults(&source)?,
        both(&source, call, "reader", "reader")
    );
    Ok(())
}

#[test]
fn an_option_of_a_grant_is_not_encodable() -> Result<(), String> {
    let call = "assert.equal(Option.some(value: reader) Option.none())";
    let source = with_grants("", call);
    assert_eq!(
        test_faults(&source)?,
        both(&source, call, "Option.some(value: reader)", "Option.none()")
    );
    Ok(())
}

#[test]
fn a_record_and_a_list_that_hold_a_grant_are_not_encodable() -> Result<(), String> {
    let call = "assert.equal({held: reader n: 1i32} {held: reader n: 1i32})";
    let source = with_grants("", call);
    assert_eq!(
        test_faults(&source)?,
        both(
            &source,
            call,
            "{held: reader n: 1i32}",
            "{held: reader n: 1i32}"
        )
    );
    let call = "assert.equal([lister] [lister lister])";
    let source = with_grants("", call);
    assert_eq!(
        test_faults(&source)?,
        both(&source, call, "[lister]", "[lister lister]")
    );
    Ok(())
}

#[test]
fn a_declared_type_that_holds_a_grant_is_not_encodable() -> Result<(), String> {
    // A brand, a record type, a variant, a recursive type, and a generic
    // type applied to a grant: each holds one through its representation or
    // its argument.
    let declarations = "type Brand of DirReadText endtype\n\n\
                        type Pair of {reader: DirReadText count: I32} endtype\n\n\
                        type Chain\n  case end\n  case link {held: DirList next: List(Chain)}\nendtype\n\n\
                        type Box(T) of {item: T} endtype\n\n";
    for (call, left, right) in [
        (
            "assert.equal(Brand(reader) Brand(reader))",
            "Brand(reader)",
            "Brand(reader)",
        ),
        (
            "assert.equal(Pair(reader: reader count: 1i32) Pair(reader: reader count: 1i32))",
            "Pair(reader: reader count: 1i32)",
            "Pair(reader: reader count: 1i32)",
        ),
        (
            "assert.equal(Chain.end() Chain.link(held: lister next: [Chain.end()]))",
            "Chain.end()",
            "Chain.link(held: lister next: [Chain.end()])",
        ),
        (
            "assert.equal(Box(item: reader) Box(item: reader))",
            "Box(item: reader)",
            "Box(item: reader)",
        ),
    ] {
        let source = with_grants(declarations, call);
        assert_eq!(
            test_faults(&source)?,
            both(&source, call, left, right),
            "{call}"
        );
    }
    Ok(())
}

#[test]
fn a_result_of_a_grant_read_is_encodable() -> Result<(), String> {
    // The typed result of an operation holds no grant: "ordinary typed
    // results" (03, Capability values).
    let source = format!(
        "{IMPORTS}use @std.fs.RelativePath enduse\n\n\
         test \"read\"\n  takes {{world: TestWorld}}\n  performs {{TestWorld DirReadText}}\n  \
         do\n    let Result.ok(value: path) be RelativePath([\"a.txt\"]) endlet\n    \
         let {{readText: reader}} be await TestWorld.directory(world files: [path: \"A\"]) \
         endlet\n    assert.equal(Result.ok(value: \"A\") await DirReadText.read(reader path))\n\
         endtest\n"
    );
    assert_eq!(test_faults(&source)?, []);
    Ok(())
}

#[test]
fn a_type_that_holds_no_grant_is_encodable() -> Result<(), String> {
    // A generic type applied to an ordinary type, and a recursive type with
    // no grant: the walk ends and finds none.
    let declarations = "type Box(T) of {item: T} endtype\n\n\
                        type Chain\n  case end\n  case link {count: I32 next: List(Chain)}\nendtype\n\n";
    let source = with_grants(
        declarations,
        "assert.equal(Box(item: 1i32) Box(item: 1i32))\n    \
         assert.equal(Chain.end() Chain.link(count: 1i32 next: [Chain.end()]))",
    );
    assert_eq!(test_faults(&source)?, []);
    Ok(())
}

#[test]
fn a_dict_keyed_by_a_grant_is_an_invalid_key() {
    for (key, written) in [
        ("DirReadText", "Dict(DirReadText Str)"),
        (
            "{held: DirReadText n: I32}",
            "Dict({held: DirReadText n: I32} Str)",
        ),
        ("Option(DirReadText)", "Dict(Option(DirReadText) Str)"),
    ] {
        let source = format!(
            "use @std.fs.DirReadText enduse\n\n\
             fn size takes {{entries: {written}}} returns n: U64 do return 0u64 endfn\n"
        );
        assert_eq!(
            faults(&source),
            [(
                DiagnosticCode::TypeInvalidDictKey,
                inside(&source, written, key)
            )],
            "{written}"
        );
    }
}

#[test]
fn a_brand_over_a_grant_is_a_key_when_it_is_ordered() {
    // The bound decides: the type implements `Ordered`, so it is a key.
    let source = "use @std.fs.DirReadText enduse\n\n\
                  type Key of DirReadText\n  \
                  impl Equatable\n    fn equal takes {left: Self right: Self} returns same: Bool \
                  do return true endfn\n  endimpl\n  \
                  impl Ordered\n    fn compare takes {left: Self right: Self} returns order: \
                  Ordering do return Ordering.equal() endfn\n  endimpl\nendtype\n\n\
                  fn size takes {entries: Dict(Key Str)} returns n: U64 do return 0u64 endfn\n";
    assert_eq!(faults(source), []);
    // The same type with no `Ordered` of its own is no key.
    let unordered = source.replace(
        "  impl Ordered\n    fn compare takes {left: Self right: Self} returns order: \
         Ordering do return Ordering.equal() endfn\n  endimpl\n",
        "",
    );
    assert_ne!(unordered, source);
    assert_eq!(
        faults(&unordered),
        [(
            DiagnosticCode::TypeInvalidDictKey,
            inside(&unordered, "Dict(Key Str)", "Key")
        )]
    );
}

#[test]
fn a_relative_path_stays_a_dict_key() {
    let source = "use @std.fs.RelativePath enduse\n\n\
                  fn size takes {entries: Dict(RelativePath Str)} returns n: U64 \
                  do return 0u64 endfn\n";
    assert_eq!(faults(source), []);
}

/// A labelled default is a constant, which has an encoding, and a type that
/// holds a grant has none (`docs/spec/06-runtime.md`, "What has no
/// encoding"): such a default is `@type.notConstant` at the default whatever
/// its value, through an `Option`, a list, a record, a dict's values, and a
/// brand's representation, at a `fn`, an anonymous `fn`, and an interface
/// member. No checked program reaches the checked IR's own refusal of such
/// a constant (`@runtime.invalidCheckedProgram`).
#[test]
fn a_default_whose_type_holds_a_grant_is_not_constant() {
    let imports =
        "use @std.fs.DirReadText enduse\n\nuse @std.fs.DirWriteText enduse\n\n";
    let brand = "type LogDir of DirWriteText\n  visibility @public\n  representation @private\nendtype\n";
    let cases = [
        ("Option(DirReadText)", "Option.none()"),
        ("List(DirReadText)", "[]"),
        ("{r: Option(DirReadText)}", "{r: Option.none()}"),
        ("Dict(Str DirReadText)", "[:]"),
        ("Option(LogDir)", "Option.none()"),
        (
            "Option(Option(DirWriteText))",
            "Option.some(value: Option.none())",
        ),
    ];
    for (ty, default) in cases {
        let sites = [
            format!(
                "fn take labelled {{held: {ty} be {default}}} returns r: Void do return void endfn\n"
            ),
            format!(
                "fn outer returns r: Void do let f be fn labelled {{held: {ty} be {default}}} \
                 returns s: Void do return void endfn endlet return f() endfn\n"
            ),
            format!(
                "interface Takes\n  fn take\n    takes {{value: Self}}\n    \
                 labelled {{held: {ty} be {default}}}\n    returns r: Void\n  endfn\nendinterface\n"
            ),
        ];
        for site in sites {
            let source = format!("{imports}{brand}{site}");
            let checked = crate::check::check_source(super::SOURCE, &source);
            assert!(
                checked
                    .diagnostics()
                    .iter()
                    .all(|diagnostic| diagnostic.code()
                        != DiagnosticCode::RuntimeInvalidCheckedProgram),
                "{source}"
            );
            assert_eq!(
                faults(&source),
                [(DiagnosticCode::TypeNotConstant, at(&source, default, 0))],
                "{source}"
            );
        }
    }
    // A default that holds no grant is a constant.
    let source = format!(
        "{imports}fn take labelled {{held: Option(Str) be Option.none()}} returns r: Void do return void endfn\n"
    );
    assert_eq!(faults(&source), Vec::new());
}
