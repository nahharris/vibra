#![allow(clippy::expect_used, clippy::print_stdout, missing_docs)]
use vibra_types::check_source;

#[test]
fn dump() {
    let path = std::env::var("SCRATCH_VIB").expect("SCRATCH_VIB");
    let source = std::fs::read_to_string(&path).expect("read");
    let result = check_source("scratch.vib", &source);
    for d in result.diagnostics() {
        let p = d.primary_span();
        println!(
            "{} {}..{} `{}` {} related={:?}",
            d.code().as_atom(),
            p.start(),
            p.end(),
            &source[p.start()..p.end()],
            d.message(),
            d.related()
                .iter()
                .map(|r| (r.span.start(), r.span.end()))
                .collect::<Vec<_>>()
        );
    }
    if let Some(program) = result.program() {
        match vibra_interp::run(program) {
            Ok(execution) => println!("value={}", execution.canonical_result()),
            Err(error) => println!("error={error}"),
        }
    }
}
