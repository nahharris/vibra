//! Resolution inside bodies: scopes, binders, constant patterns, dotted
//! names, the result name, and `Self` (`docs/spec/01-source-language.md`,
//! "Names and paths", "Bodies and statements", "Patterns";
//! `docs/spec/02-type-system.md`, "Namespaces and resolution", "Constant
//! expressions and constant patterns").

use vibra_diagnostics::DiagnosticCode as C;
use vibra_resolve::{Anchor, DeclarationKind as K, Resolution};

use crate::support::{
    PRELUDE, STD_PACKAGE, assert_clean, assert_one, codes, flat, resolve_main,
    resolve_modules, span_of, std_identity,
};

/// The bytes of `needle` inside the first `context` of `text`.
fn inside(text: &str, context: &str, needle: &str) -> (usize, usize) {
    let (start, _) = span_of(text, context, 0);
    let offset = context
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in `{context}`"));
    (start + offset, start + offset + needle.len())
}

/// The last `len` bytes of the first `context` of `text`.
fn tail(text: &str, context: &str, len: usize) -> (usize, usize) {
    let (_, end) = span_of(text, context, 0);
    (end - len, end)
}

/// The identity of a declaration of `@app.main`.
fn main_id(owner: &str, kind: K, name: &str) -> String {
    Anchor::new("hello", "0.1.0", "@app.main", owner, kind, name)
        .identity()
        .as_str()
        .to_owned()
}

/// The identity of a member of a declaration of the standard library.
fn std_member(module: &str, owner: &str, kind: K, name: &str) -> String {
    Anchor::new(STD_PACKAGE.0, STD_PACKAGE.1, module, owner, kind, name)
        .identity()
        .as_str()
        .to_owned()
}

/// The references of `@app.main`, as start, end, and identity.
fn references(resolution: &Resolution) -> Vec<(usize, usize, String)> {
    resolution
        .references()
        .iter()
        .filter(|reference| reference.source_id() == "src/app/main.vib")
        .map(|reference| {
            (
                reference.span().start(),
                reference.span().end(),
                reference.to().as_str().to_owned(),
            )
        })
        .collect()
}

/// Whether `@app.main` has a reference at `span`.
fn has_reference(resolution: &Resolution, span: (usize, usize), to: &str) -> bool {
    references(resolution).contains(&(span.0, span.1, to.to_owned()))
}

/// Asserts that no reference starts inside `span`.
fn no_reference_in(resolution: &Resolution, span: (usize, usize)) {
    let found = references(resolution)
        .into_iter()
        .filter(|(start, _, _)| (span.0..span.1).contains(start))
        .collect::<Vec<_>>();
    assert!(found.is_empty(), "{found:?}");
}

// Task 1: scopes.

#[test]
fn a_let_name_is_visible_after_endlet() {
    let text = "fn f returns r: I32 do let a be 1i32 endlet return a endfn\n";
    assert_clean(&resolve_main(text));
}

#[test]
fn a_let_name_is_not_visible_in_its_initializer() {
    let text = "fn f returns r: I32 do let a be a endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "be a endlet", "a"),
        &[],
    );
}

#[test]
fn a_let_name_is_not_visible_in_its_fallback() {
    let text = "fn f takes {x: Option(I32)} returns r: I32 do let Option.some(value: a) be x else return a endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "return a endlet", "a"),
        &[],
    );
}

#[test]
fn sibling_scopes_reuse_a_name() {
    let text = "fn f takes {c: Bool} returns r: I32 do if c then let a be 1i32 endlet return a else let a be 2i32 endlet return a endif endfn\n";
    assert_clean(&resolve_main(text));
}

#[test]
fn a_binding_of_a_do_block_ends_with_it() {
    let text = "fn f returns r: I32 do do let b be 1i32 endlet enddo return b endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "return b", "b"),
        &[],
    );
}

#[test]
fn a_binding_of_an_arm_ends_with_it() {
    let text = "fn f takes {o: Option(I32)} returns r: I32 do match o case Option.some(value: v) return v case Option.none() return v endmatch endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "case Option.none() return v", "v"),
        &[],
    );
}

#[test]
fn a_name_used_before_its_let_is_unknown() {
    // v1 B02.
    let text = "fn take takes {v: I32} returns r: I32 do return v endfn\nfn f returns r: I32 do let u be take(a) endlet let a be 1i32 endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        tail(text, "take(a", 1),
        &[],
    );
}

#[test]
fn an_anonymous_callable_sees_the_locals_before_it_only() {
    // v1 B04 and B13.
    let later = "fn f returns r: I32 do let h be fn returns y: I32 do return g endfn endlet let g be 1i32 endlet return g endfn\n";
    assert_one(
        &resolve_main(later),
        C::NameUnknownSymbol,
        inside(later, "return g endfn", "g"),
        &[],
    );
    let earlier = "fn f returns r: Fn({I32} I32) do let g be 1i32 endlet return fn takes {x: I32} returns y: I32 do return g endfn endfn\n";
    assert_clean(&resolve_main(earlier));
}

#[test]
fn an_anonymous_callable_sees_an_enclosing_local() {
    let text = "fn f returns r: Fn({I32} I32) do let outer be 1i32 endlet return fn takes {x: I32} returns y: I32 do return I32.add(x outer) endfn endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "I32.add", 0),
        &std_member("@std.builtin", "I32", K::Fn, "add"),
    ));
}

#[test]
fn a_fallback_binds_in_its_own_scope_before_the_pattern_binds() {
    // v1 B11 and B12.
    let text = "fn f takes {o: Option(I32)} returns r: I32 do let Option.some(value: z) be o else do let z be 1i32 endlet return z enddo endlet return z endfn\n";
    assert_clean(&resolve_main(text));
}

#[test]
fn a_let_else_binder_is_not_visible_in_its_value() {
    // v1 B09.
    let text = "fn f takes {o: Option(I32)} returns r: I32 do let Option.some(value: x) be x else return 0i32 endlet return x endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "be x else", "x"),
        &[],
    );
}

#[test]
fn the_visible_locals_at_every_statement_follow_the_chapter() {
    let text = "\
fn f
  takes {p: I32 {q _}: {I32 I32}}
  labelled {flag: Bool be false}
  returns r: I32
  do
    let a be p endlet
    if flag then
      let b be a endlet
      return b
    else
      let Option.some(value: c) be Option.some(value: q) else
        return a
      endlet
      return c
    endif
    match a
      case zero
        return a
      case n
        let d be n endlet
        return d
    endmatch
    do
      let e be a endlet
      return e
    enddo
    return a
endfn
let zero: I32 be 0i32 endlet
";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    let base = ["p", "q", "flag"];
    let with = |extra: &[&str]| {
        base.iter()
            .chain(extra)
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    let expected = [
        ("let a be p endlet", with(&[])),
        ("if flag then", with(&["a"])),
        ("let b be a endlet", with(&["a"])),
        ("return b", with(&["a", "b"])),
        ("let Option.some(value: c)", with(&["a"])),
        ("return a\n      endlet", with(&["a"])),
        ("return c", with(&["a", "c"])),
        ("match a", with(&["a"])),
        ("return a\n      case n", with(&["a"])),
        ("let d be n endlet", with(&["a", "n"])),
        ("return d", with(&["a", "n", "d"])),
        ("do\n      let e", with(&["a"])),
        ("let e be a endlet", with(&["a"])),
        ("return e", with(&["a", "e"])),
        ("return a\nendfn", with(&["a"])),
    ];
    let found = resolution
        .visible_locals()
        .iter()
        .map(|visible| {
            assert_eq!(visible.source_id(), "src/app/main.vib");
            (visible.span().start(), visible.names().to_vec())
        })
        .collect::<Vec<_>>();
    let expected = expected
        .iter()
        .map(|(snippet, names)| (span_of(text, snippet, 0).0, names.clone()))
        .collect::<Vec<_>>();
    assert_eq!(found, expected);
}

// Task 2: binders.

#[test]
fn a_binder_may_not_shadow_a_parameter() {
    let text =
        "fn f takes {a: I32} returns r: I32 do let a be 2i32 endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "let a be", "a"),
        &[tail(text, "{a", 1)],
    );
}

#[test]
fn a_binder_may_not_shadow_a_local_of_an_enclosing_scope() {
    // v1 B07.
    let text = "fn f takes {c: Bool} returns r: I32 do let a be 1i32 endlet if c then let a be 2i32 endlet return a endif return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "let a be 2i32", "a"),
        &[inside(text, "let a be 1i32", "a")],
    );
}

#[test]
fn a_later_let_of_the_same_name_is_a_redeclaration() {
    // v1 B05.
    let text = "fn f returns r: I32 do let a be 1i32 endlet let a be 2i32 endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "let a be 2i32", "a"),
        &[inside(text, "let a be 1i32", "a")],
    );
}

#[test]
fn a_binder_may_not_shadow_a_name_of_the_module_scope() {
    let import = "use @std.core enduse\nfn f returns r: I32 do let core be 1i32 endlet return core endfn\n";
    assert_one(
        &resolve_main(import),
        C::NameRedeclaration,
        inside(import, "let core", "core"),
        &[span_of(import, "core", 0)],
    );
    let declared = "type Point of {x: I32} endtype\nfn f returns r: I32 do let Point be 1i32 endlet return Point endfn\n";
    assert_one(
        &resolve_main(declared),
        C::NameRedeclaration,
        inside(declared, "let Point", "Point"),
        &[span_of(declared, "Point", 0)],
    );
}

#[test]
fn a_binder_may_not_shadow_a_type_parameter() {
    let text = "fn f(T) takes {T: T} returns r: T do return T endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "{T: T}", "T"),
        &[span_of(text, "T", 0)],
    );
}

#[test]
fn a_binder_may_not_shadow_a_non_constant_module_value() {
    let text = "fn seed returns s: I32 do return 1i32 endfn\nlet limit: I32 be seed() endlet\nfn f returns r: I32 do let limit be 2i32 endlet return limit endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "let limit be", "limit"),
        &[span_of(text, "limit", 0)],
    );
}

#[test]
fn a_let_of_a_constant_module_value_is_a_constant_pattern() {
    let text = "let limit: I32 be 1i32 endlet\nfn f returns r: I32 do let limit be 1i32 endlet return limit endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    let id = main_id("", K::Value, "limit");
    assert!(has_reference(
        &resolution,
        inside(text, "let limit be", "limit"),
        &id
    ));
    assert!(has_reference(
        &resolution,
        inside(text, "return limit", "limit"),
        &id
    ));
}

#[test]
fn a_pattern_may_not_bind_one_name_twice() {
    let text = "fn f takes {t: {I32 I32}} returns r: I32 do let {a a} be t endlet return a endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        tail(text, "{a a", 1),
        &[inside(text, "{a a}", "a")],
    );
}

#[test]
fn the_later_binder_still_binds() {
    let text = "fn f takes {t: {I32 I32}} returns r: I32 do let {a a} be t endlet return a endfn\n";
    assert_eq!(codes(&resolve_main(text)), [C::NameRedeclaration]);
}

#[test]
fn discards_repeat_and_bind_nothing() {
    // v1 B14.
    let text = "fn f takes {t: {I32 I32 I32} _: I32} returns r: I32 do let {_ _ c} be t endlet let _ be c endlet return c endfn\n";
    assert_clean(&resolve_main(text));
}

#[test]
fn a_constant_module_value_is_a_constant_pattern() {
    let text = "let zero: I32 be 0i32 endlet\nfn f takes {x: I32} returns r: I32 do match x case zero return 1i32 case _ return 0i32 endmatch endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        inside(text, "case zero", "zero"),
        &main_id("", K::Value, "zero"),
    ));
    let visible = resolution
        .visible_locals()
        .iter()
        .find(|visible| visible.span().start() == span_of(text, "return 1i32", 0).0)
        .map(|visible| visible.names().to_vec());
    assert_eq!(visible, Some(vec!["x".to_owned()]));
}

#[test]
fn true_and_false_are_constant_patterns() {
    let text = "fn f takes {b: Bool} returns r: I32 do match b case true return 1i32 case false return 0i32 endmatch endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        inside(text, "case true", "true"),
        &std_identity("@std.bool", K::Value, "true"),
    ));
    assert!(has_reference(
        &resolution,
        inside(text, "case false", "false"),
        &std_identity("@std.bool", K::Value, "false"),
    ));
}

#[test]
fn a_constant_reaches_through_names_and_constructions() {
    let text = "type Point of {x: I32} endtype\nlet origin: Point be Point(x: zero) endlet\nlet zero: I32 be other endlet\nlet other: I32 be 0i32 endlet\nfn f takes {p: Point} returns r: I32 do match p case origin return 1i32 case _ return 0i32 endmatch endfn\n";
    assert_clean(&resolve_main(text));
}

#[test]
fn a_cycle_of_module_values_is_not_constant() {
    let text = "let a: I32 be b endlet\nlet b: I32 be a endlet\nfn f takes {x: I32} returns r: I32 do match x case a return 1i32 case _ return 0i32 endmatch endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        tail(text, "case a", 1),
        &[span_of(text, "a", 0)],
    );
}

#[test]
fn a_function_in_pattern_position_is_a_redeclaration() {
    let text = "fn helper returns h: I32 do return 0i32 endfn\nfn f takes {x: I32} returns r: I32 do match x case helper return 1i32 case _ return 0i32 endmatch endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "case helper", "helper"),
        &[span_of(text, "helper", 0)],
    );
}

#[test]
fn a_non_constant_module_value_in_pattern_position_is_a_redeclaration() {
    let text = "fn seed returns s: I32 do return 1i32 endfn\nlet limit: I32 be seed() endlet\nfn f takes {x: I32} returns r: I32 do match x case limit return 1i32 case _ return 0i32 endmatch endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameRedeclaration,
        inside(text, "case limit", "limit"),
        &[span_of(text, "limit", 0)],
    );
}

#[test]
fn a_dotted_name_in_a_pattern_never_binds() {
    let text = "fn f takes {o: Ordering} returns r: I32 do match o case Ordering.less() return 1i32 case _ return 0i32 endmatch endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "Ordering.less", 0),
        &std_member("@std.core", "Ordering", K::Case, "less"),
    ));
}

// Prelude spellings at every binder site (the v1 reserved-binder matrix;
// reserved words are the reader's).

/// The prelude spellings a binder may not use, `Any` and `Self` included;
/// `true` and `false` are constant patterns at a pattern site and have their
/// own test.
fn prelude_spellings() -> Vec<&'static str> {
    PRELUDE
        .map(|(name, _, _)| name)
        .into_iter()
        .chain(["Any", "Self"])
        .filter(|name| !matches!(*name, "true" | "false"))
        .collect()
}

/// Asserts that `text` has one `@name.reservedDeclaration`, at `binder`, so
/// that the binder still binds and its use resolves.
fn reserved(text: &str, binder: (usize, usize), spelling: &str) {
    assert_eq!(
        flat(&resolve_main(text)),
        [(
            C::NameReservedDeclaration,
            "src/app/main.vib".to_owned(),
            binder.0,
            binder.1,
            Vec::new()
        )],
        "{spelling}: {text}"
    );
}

#[test]
fn a_prelude_spelling_is_reserved_at_every_binder_site_and_still_binds() {
    for name in prelude_spellings() {
        let takes = format!(
            "fn f takes {{{name}: I32}} returns r: I32 do return {name} endfn\n"
        );
        reserved(
            &takes,
            tail(&takes, &format!("takes {{{name}"), name.len()),
            name,
        );
        let binding = format!(
            "fn f returns r: I32 do let {name} be 1i32 endlet let ok be {name} endlet return ok endfn\n"
        );
        reserved(
            &binding,
            tail(&binding, &format!("let {name}"), name.len()),
            name,
        );
        let fallback = format!(
            "fn f takes {{o: Option(I32)}} returns r: I32 do let Option.some(value: {name}) be o else return 0i32 endlet return {name} endfn\n"
        );
        reserved(
            &fallback,
            tail(&fallback, &format!("(value: {name}"), name.len()),
            name,
        );
        let arm = format!(
            "fn f takes {{o: Option(I32)}} returns r: I32 do match o case Option.some(value: {name}) return {name} case _ return 0i32 endmatch endfn\n"
        );
        reserved(
            &arm,
            tail(&arm, &format!("(value: {name}"), name.len()),
            name,
        );
        let anonymous = format!(
            "fn f returns r: Fn({{I32}} I32) do return fn takes {{{name}: I32}} returns y: I32 do return {name} endfn endfn\n"
        );
        reserved(
            &anonymous,
            tail(&anonymous, &format!("takes {{{name}"), name.len()),
            name,
        );
        let labelled = format!(
            "fn f labelled {{{name}: I32 be 0i32}} returns r: I32 do return {name} endfn\n"
        );
        reserved(
            &labelled,
            tail(&labelled, &format!("labelled {{{name}"), name.len()),
            name,
        );
        let nested = format!(
            "fn f takes {{p: {{I32 I32}}}} returns r: I32 do let {{a {name}}} be p endlet return a endfn\n"
        );
        reserved(
            &nested,
            tail(&nested, &format!("{{a {name}"), name.len()),
            name,
        );
        let result = format!("fn f returns {name}: I32 do return 0i32 endfn\n");
        reserved(
            &result,
            tail(&result, &format!("returns {name}"), name.len()),
            name,
        );
    }
}

#[test]
fn true_and_false_are_reserved_where_a_binder_is_one_name() {
    for name in ["true", "false"] {
        let labelled = format!(
            "fn f labelled {{{name}: I32 be 0i32}} returns r: I32 do return 0i32 endfn\n"
        );
        reserved(
            &labelled,
            tail(&labelled, &format!("labelled {{{name}"), name.len()),
            name,
        );
        let result = format!("fn f returns {name}: I32 do return 0i32 endfn\n");
        reserved(
            &result,
            tail(&result, &format!("returns {name}"), name.len()),
            name,
        );
    }
}

#[test]
fn ordinary_binders_and_discards_are_accepted() {
    let text = "fn f takes {value: I32 _: I32} returns r: I32 do let first be 1i32 endlet let _ be 2i32 endlet let second be first endlet return second endfn\n";
    assert_clean(&resolve_main(text));
}

// Task 3: dotted names.

#[test]
fn a_local_s_later_parts_are_field_reads() {
    let text = "type User of {names: List(Str)} endtype\nfn f takes {user: User} returns r: U64 do return List.length(user.names) endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "List.length", 0),
        &std_member("@std.builtin", "List", K::Fn, "length"),
    ));
    no_reference_in(&resolution, span_of(text, "user.names", 0));
}

#[test]
fn a_module_value_s_later_parts_are_field_reads() {
    let text = "type Point of {x: I32} endtype\nlet origin: Point be Point(x: 0i32) endlet\nfn f returns r: I32 do return origin.x endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        inside(text, "return origin.x", "origin"),
        &main_id("", K::Value, "origin"),
    ));
}

#[test]
fn the_longest_entity_prefix_is_the_path() {
    let model = "type User of {x: I32}\n  visibility @public\n  fn nameLength takes {u: Self} returns n: User visibility @public do return u endfn\nendtype\n";
    let main = "use @app.model enduse\nfn f takes {u: model.User} returns r: I32 do return model.User.nameLength(u).x endfn\n";
    let resolution = resolve_modules(&[("@app.model", model), ("@app.main", main)]);
    assert_clean(&resolution);
    let to = Anchor::new("hello", "0.1.0", "@app.model", "User", K::Fn, "nameLength")
        .identity()
        .as_str()
        .to_owned();
    assert!(has_reference(
        &resolution,
        span_of(main, "model.User.nameLength", 0),
        &to
    ));
}

#[test]
fn a_first_part_naming_nothing_is_unknown() {
    let text = "fn f returns r: I32 do return nothere.field endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        span_of(text, "nothere", 0),
        &[],
    );
}

#[test]
fn a_member_path_through_a_prelude_type_reaches_its_member() {
    let text = "fn f returns r: Option(I32) do return Option.none() endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "Option.none", 0),
        &std_member("@std.option", "Option", K::Case, "none"),
    ));
}

#[test]
fn an_unknown_member_of_a_prelude_type_is_unknown() {
    let text = "fn f returns r: Option(I32) do return Option.nothere() endfn\n";
    let resolution = resolve_main(text);
    let option = include_str!("../../../../stdlib/src/std/option.vib");
    let (start, end) = span_of(option, "Option", 0);
    assert_eq!(
        flat(&resolution),
        [(
            C::NameUnknownSymbol,
            "src/app/main.vib".to_owned(),
            span_of(text, "nothere", 0).0,
            span_of(text, "nothere", 0).1,
            vec![("@std.option".to_owned(), start, end)],
        )]
    );
}

#[test]
fn a_field_is_not_reached_through_its_type() {
    let text = "type User of {name: Str} endtype\nfn f returns r: Str do return User.name endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        inside(text, "User.name", "name"),
        &[span_of(text, "User", 0)],
    );
}

#[test]
fn a_module_function_is_a_function_value() {
    let text = "fn helper takes {x: I32} returns h: I32 do return x endfn\nfn f returns r: Fn({I32} I32) do return helper endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        inside(text, "return helper", "helper"),
        &main_id("", K::Fn, "helper"),
    ));
}

// Task 4: the result name.

#[test]
fn the_result_name_is_not_visible_in_the_body() {
    let text = "fn f returns r: I32 do return r endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        tail(text, "return r", 1),
        &[],
    );
}

#[test]
fn the_result_name_is_not_visible_in_requires() {
    let text = "fn f takes {x: I32} returns r: I32 requires Ordered.greater(r x) do return x endfn\n";
    assert_one(
        &resolve_main(text),
        C::NameUnknownSymbol,
        tail(text, "greater(r", 1),
        &[],
    );
}

#[test]
fn the_result_name_is_visible_in_ensures_and_its_consequence() {
    let text = "fn f takes {x: I32} returns r: I32 requires Ordered.greater(x 0i32) ensures Ordered.greater(r x) else r do return x endfn\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "Ordered.greater", 1),
        &std_member("@std.core", "Ordered", K::Fn, "greater"),
    ));
}

#[test]
fn a_labelled_name_is_a_local_of_the_body_and_its_default_sees_no_parameter() {
    let text = "let zero: I32 be 0i32 endlet\nfn f takes {x: I32} labelled {n: I32 be zero} returns r: I32 do return I32.add(x n) endfn\n";
    assert_clean(&resolve_main(text));
    let parameter =
        "fn f takes {x: I32} labelled {n: I32 be x} returns r: I32 do return n endfn\n";
    assert_one(
        &resolve_main(parameter),
        C::NameUnknownSymbol,
        inside(parameter, "be x", "x"),
        &[],
    );
}

// Task 5: `Self`.

#[test]
fn self_is_unknown_in_a_module_level_callable() {
    let signature = "fn f takes {s: Self} returns r: I32 do return 0i32 endfn\n";
    assert_one(
        &resolve_main(signature),
        C::NameUnknownSymbol,
        span_of(signature, "Self", 0),
        &[],
    );
    let body = "fn f returns r: I32 do return Self.make() endfn\n";
    assert_one(
        &resolve_main(body),
        C::NameUnknownSymbol,
        span_of(body, "Self", 0),
        &[],
    );
}

#[test]
fn self_reaches_the_members_of_its_type() {
    let text = "type Counter of I32\n  fn zero returns c: Self do return Self.make() endfn\n  fn make returns c: Self do return Counter(0i32) endfn\nendtype\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "Self.make", 0),
        &main_id("Counter", K::Fn, "make"),
    ));
}

#[test]
fn a_where_operand_is_resolved_in_the_type_scope() {
    let text = "type Positive of I32\n  where Self.valid\n  fn valid takes {p: I32} returns ok: Bool do return Ordered.greater(p 0i32) endfn\nendtype\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        span_of(text, "Self.valid", 0),
        &main_id("Positive", K::Fn, "valid"),
    ));
}

// Other bodies.

#[test]
fn a_test_body_is_one_scope() {
    let test = "test \"adds\" do let a be 1i32 endlet let b be a endlet endtest\n";
    let resolution = resolve_modules(&[("@tests.t", test)]);
    assert!(
        resolution.diagnostics().is_empty(),
        "{:#?}",
        flat(&resolution)
    );
    let unknown =
        "test \"adds\" do do let a be 1i32 endlet enddo let b be a endlet endtest\n";
    assert_eq!(
        codes(&resolve_modules(&[("@tests.t", unknown)])),
        [C::NameUnknownSymbol]
    );
}

#[test]
fn a_module_value_initializer_sees_the_module_scope_only() {
    let text = "fn seed returns s: I32 do return 1i32 endfn\nlet limit: I32 be seed() endlet\n";
    let resolution = resolve_main(text);
    assert_clean(&resolution);
    assert!(has_reference(
        &resolution,
        inside(text, "be seed()", "seed"),
        &main_id("", K::Fn, "seed"),
    ));
}

/// A chain of 20,000 module values, each naming the next, is decided on a
/// 256 KiB stack: the values of a chain are decided on an explicit stack,
/// not by host recursion in proportion to its length.
#[test]
fn a_long_chain_of_module_values_is_decided_without_host_recursion() {
    const LENGTH: usize = 20_000;
    let mut text = String::new();
    for k in 1..LENGTH {
        text.push_str(&format!("let v{k}: I32 be v{} endlet\n", k + 1));
    }
    text.push_str(&format!("let v{LENGTH}: I32 be 1i32 endlet\n"));
    text.push_str(
        "fn f takes {x: I32} returns r: I32 do match x case v1 return 1i32 case _ return 0i32 endmatch endfn\n",
    );
    let resolution = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || resolve_main(&text))
        .expect("a thread")
        .join()
        .expect("resolution finishes");
    assert_clean(&resolution);
}
