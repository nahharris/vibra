//! The inputs of the recursion probes: long chains of declarations, calls,
//! imports, and packages, long flat forms, and the chains the reader keeps
//! flat. Each probe is a binary project whose entry and one test read the
//! probe's value.
//!
//! The tests of this crate (`recursion`) run the stages over them, and the
//! tests of `vibra-cli` include this file by path to run `vibra check` over
//! them, so both read the same inputs.

use std::fmt::Write as _;

/// The entry of every probe's binary target, which reads `value`.
fn entry(value: &str) -> String {
    format!(
        "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do let _ be {value} endlet return ExitCode(0u8) endproc\n"
    )
}

fn project(dependencies: &str) -> String {
    let dependencies = if dependencies.is_empty() {
        ":"
    } else {
        dependencies
    };
    format!(
        "{{format: @project.v2 package: {{name: \"probe\" version: \"0.1.0\"}} targets: [{{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}}] dependencies: [{dependencies}]}}"
    )
}

/// The probes of chains and flat forms the limit does not bound: a name and
/// the sizes of the ladder, in order.
pub(crate) const PROBES: &[(&str, &[usize])] = &[
    ("typeChain", &[1_000, 10_000, 100_000]),
    ("callChain", &[1_000, 10_000, 100_000]),
    ("procChain", &[1_000, 10_000, 100_000]),
    ("moduleLetChain", &[1_000, 10_000, 100_000]),
    ("importChain", &[100, 1_000, 10_000]),
    ("sequentialLets", &[1_000, 10_000, 100_000]),
    ("wideList", &[1_000, 10_000, 100_000]),
    ("ifChain", &[1_000, 10_000]),
    ("matchArms", &[1_000, 10_000]),
    ("dependencyChain", &[100, 1_000]),
];

/// The chains the reader keeps flat that later stages nest, each probed at
/// 100,000 links.
pub(crate) const CHAINS: &[&str] = &[
    "applicationChain",
    "fieldChain",
    "mixedPostfixChain",
    "returnChain",
    "tryChain",
    "awaitChain",
    "mixedPrefixChain",
];

/// The links of every chain probe.
pub(crate) const LINKS: usize = 100_000;

/// The declarations of the chain `name` of `n` links, ending in the module
/// value `x`, and the byte offset in them of the first token nested more
/// than 1,024 levels (`docs/spec/05-tooling.md`, "Limits").
///
/// In a postfix chain each application and field read encloses the form
/// before it, so the primary is nested `n` levels and is the first token
/// past the limit. In a prefix chain the `k`-th word is nested `k - 1`
/// levels, so the 1,026th word is the first.
pub(crate) fn chain(name: &str, n: usize) -> (String, usize) {
    let head = entry("x");
    let fun = "fn f returns r: I32 do return 1i32 endfn\n";
    let value = |init: &str| format!("let x: I32 be {init} endlet\n");
    let (body, offset) = match name {
        "applicationChain" => {
            let start = format!("{fun}let x: I32 be ");
            let offset = start.len();
            (format!("{start}f{} endlet\n", "()".repeat(n)), offset)
        }
        "fieldChain" => {
            let start = "let y: I32 be 1i32 endlet\nlet x: I32 be ";
            (format!("{start}y{} endlet\n", ".a".repeat(n)), start.len())
        }
        "mixedPostfixChain" => {
            let start = format!("{fun}let x: I32 be ");
            let offset = start.len();
            (format!("{start}f{} endlet\n", "().a".repeat(n / 2)), offset)
        }
        "returnChain" | "tryChain" | "awaitChain" | "mixedPrefixChain" => {
            let words: &[&str] = match name {
                "returnChain" => &["return"],
                "tryChain" => &["try"],
                "awaitChain" => &["await"],
                _ => &["return", "try", "await"],
            };
            let start = "proc p returns r: Result(I32 I32) do ";
            let mut text = start.to_owned();
            let mut offset = 0;
            for k in 0..n {
                if k == 1_025 {
                    offset = text.len();
                }
                let word = words.get(k % words.len()).copied().unwrap_or("return");
                text.push_str(word);
                text.push(' ');
            }
            text.push_str("Result.ok(value: 1i32) endproc\n");
            text.push_str(&value("1i32"));
            (text, offset)
        }
        _ => panic!("no chain {name}"),
    };
    (format!("{head}{body}"), head.len() + offset)
}

/// The files of the probe `name` of size `n`: each path, relative to the
/// project root, and its text.
pub(crate) fn probe_files(name: &str, n: usize) -> Vec<(String, String)> {
    let mut directory = Files(Vec::new());
    let mut main = String::new();
    let mut test = String::from("use @std.assert enduse\n\nuse @app.main enduse\n\n");
    let mut dependencies = String::new();
    match name {
        // `type A1 of A2`, …, `type An of I32`, a maker for each, and a test
        // that compares two values: the grant walk, the `Encodable` walk,
        // layout, construction, and encoding all follow the chain.
        "typeChain" => {
            main.push_str(&entry("m1()"));
            for k in 1..n {
                let _ = writeln!(main, "type A{k} of A{} endtype", k + 1);
                let _ = writeln!(
                    main,
                    "fn m{k} returns r: A{k} visibility @public do return A{k}(m{}()) endfn",
                    k + 1
                );
            }
            let _ = writeln!(main, "type A{n} of I32 endtype");
            let _ = writeln!(
                main,
                "fn m{n} returns r: A{n} visibility @public do return A{n}(1i32) endfn"
            );
            test.push_str(
                "test \"compares\" do assert.equal(main.m1() main.m1()) endtest\n",
            );
        }
        // `f1` calls `f2`, …: effect inference, contract obligations (each
        // ensures its result is positive), lowering, and the run.
        "callChain" => {
            main.push_str(&entry("f1()"));
            for k in 1..n {
                let _ = writeln!(
                    main,
                    "fn f{k} returns r: I32 ensures Ordered.greater(r 0i32) visibility @public do return f{}() endfn",
                    k + 1
                );
            }
            let _ = writeln!(
                main,
                "fn f{n} returns r: I32 ensures Ordered.greater(r 0i32) do return 1i32 endfn"
            );
            test.push_str("test \"calls\" do assert.equal(main.f1() 1i32) endtest\n");
        }
        // `p1` awaits `p2`, …: the witness chain of effect inference.
        "procChain" => {
            main.push_str(&entry("await p1()"));
            for k in 1..n {
                let _ = writeln!(
                    main,
                    "proc p{k} returns r: I32 visibility @public do return await p{}() endproc",
                    k + 1
                );
            }
            let _ = writeln!(main, "proc p{n} returns r: I32 do return 1i32 endproc");
            test.push_str(
                "test \"awaits\" do let _ be await main.p1() endlet endtest\n",
            );
        }
        // `v1` is `v2`, …: the initializer order of module values.
        "moduleLetChain" => {
            main.push_str(&entry("v1"));
            for k in 1..n {
                let _ = writeln!(
                    main,
                    "let v{k}: I32 visibility @public be v{} endlet",
                    k + 1
                );
            }
            let _ = writeln!(main, "let v{n}: I32 be 1i32 endlet");
            test.push_str("test \"reads\" do assert.equal(main.v1 1i32) endtest\n");
        }
        // Module `m1` imports `m2`, …, each value naming the next.
        "importChain" => {
            main.push_str("use @app.m1 enduse\n\n");
            main.push_str(&entry("m1.v"));
            for k in 1..n {
                directory.write(
                    &format!("src/app/m{k}.vib"),
                    &format!(
                        "use @app.m{next} enduse\n\nlet v: I32 visibility @public be m{next}.v endlet\n",
                        next = k + 1
                    ),
                );
            }
            directory.write(
                &format!("src/app/m{n}.vib"),
                "let v: I32 visibility @public be 1i32 endlet\n",
            );
            test.push_str("test \"reads\" do assert.equal(main.x 1i32) endtest\n");
            main.push_str("let x: I32 visibility @public be m1.v endlet\n");
        }
        // One body of `n` sequential `let`s, each naming the one before.
        "sequentialLets" => {
            main.push_str(&entry("f()"));
            main.push_str(
                "fn f returns r: I32 visibility @public do\n  let a0 be 1i32 endlet\n",
            );
            for k in 1..n {
                let _ = writeln!(main, "  let a{k} be a{} endlet", k - 1);
            }
            let _ = writeln!(main, "  return a{}\nendfn", n - 1);
            test.push_str("test \"binds\" do assert.equal(main.f() 1i32) endtest\n");
        }
        // One list literal of `n` elements.
        "wideList" => {
            main.push_str(&entry("x"));
            main.push_str("let x: List(I32) visibility @public be [");
            for _ in 0..n {
                main.push_str("1i32 ");
            }
            main.push_str("] endlet\n");
            test.push_str("test \"compares\" do assert.equal(main.x main.x) endtest\n");
        }
        // `if … else if … else … endif …`: each `if` is nested in the `else`
        // of the one before, so past the limit this is the limit.
        "ifChain" => {
            main.push_str(&entry("f(3i32)"));
            main.push_str(
                "fn f takes {v: I32} returns r: I32 visibility @public do return ",
            );
            for k in 0..n {
                let _ = write!(main, "if Equatable.equal(v {k}i32) then {k}i32 else ");
            }
            main.push_str("0i32");
            for _ in 0..n {
                main.push_str(" endif");
            }
            main.push_str(" endfn\n");
            test.push_str(
                "test \"chooses\" do assert.equal(main.f(3i32) 3i32) endtest\n",
            );
        }
        // One `match` of `n` arms and a default.
        "matchArms" => {
            main.push_str(&entry("f(3i32)"));
            main.push_str("fn f takes {v: I32} returns r: I32 visibility @public do return match v\n");
            for k in 0..n {
                let _ = writeln!(main, "  case {k}i32 {k}i32");
            }
            main.push_str("  case _ 0i32\nendmatch endfn\n");
            test.push_str(
                "test \"chooses\" do assert.equal(main.f(3i32) 3i32) endtest\n",
            );
        }
        // Package `p1` depends on `p2`, …, each beneath the workspace root.
        "dependencyChain" => {
            dependencies.push_str("\"p1\": {kind: @path path: \"deps/p1\"}");
            main.push_str("use @p1.m enduse\n\n");
            main.push_str(&entry("m.v"));
            main.push_str("let x: I32 visibility @public be m.v endlet\n");
            for k in 1..=n {
                let next = if k < n {
                    format!("\"p{}\": {{kind: @path path: \"../p{}\"}}", k + 1, k + 1)
                } else {
                    ":".to_owned()
                };
                directory.write(
                    &format!("deps/p{k}/project.vibon"),
                    &format!(
                        "{{format: @project.v2 package: {{name: \"p{k}\" version: \"0.1.0\"}} targets: [{{name: @p{k} kind: @lib root: \"src\"}}] dependencies: [{next}]}}"
                    ),
                );
                let module = if k < n {
                    format!(
                        "use @p{}.m enduse\n\nlet v: I32 visibility @public be m.v endlet\n",
                        k + 1
                    )
                } else {
                    "let v: I32 visibility @public be 1i32 endlet\n".to_owned()
                };
                directory.write(&format!("deps/p{k}/src/m.vib"), &module);
            }
            test.push_str("test \"reads\" do assert.equal(main.x 1i32) endtest\n");
        }
        // A chain of one kind of link, 100,000 links past the limit: the
        // reader keeps each flat, and the measure counts each link a level.
        "applicationChain" | "fieldChain" | "mixedPostfixChain" | "returnChain"
        | "tryChain" | "awaitChain" | "mixedPrefixChain" => {
            let (source, _) = chain(name, n);
            main.push_str(&source);
            test.push_str("test \"reads\" do let _ be main.x endlet endtest\n");
        }
        _ => panic!("no probe {name}"),
    }
    directory.write("project.vibon", &project(&dependencies));
    directory.write("src/app/main.vib", &main);
    directory.write("tests/main.vib", &test);
    directory.0
}

/// The files a probe writes, in order.
struct Files(Vec<(String, String)>);

impl Files {
    fn write(&mut self, path: &str, text: &str) {
        self.0.push((path.to_owned(), text.to_owned()));
    }
}
