//! Step 11's deep-nesting evidence for the v1 reader.

use vibra_syntax_v1::{DocumentMode, parse_data, parse_source};

#[test]
fn deeply_nested_source_and_data_survive_traversal_and_drop() {
    const SOURCE_DEPTH: usize = 20_000;
    let source_text = format!(
        "{}value{}",
        "(".repeat(SOURCE_DEPTH),
        ")".repeat(SOURCE_DEPTH)
    );
    let source = parse_source("deep.vib", &source_text)
        .expect("deep source selects source mode");
    assert_eq!(source.mode(), DocumentMode::Source);
    assert!(source.accepted());
    assert_eq!(source.root().to_source(), source_text);
    drop(source);

    const DATA_DEPTH: usize = 2_000;
    let data_text = format!(
        "{}@value{}",
        "(array ".repeat(DATA_DEPTH),
        ")".repeat(DATA_DEPTH)
    );
    let data =
        parse_data("deep.vibon", &data_text).expect("deep data selects data mode");
    assert!(data.accepted());
    assert!(data.data().is_some());
    assert_eq!(data.root().to_source(), data_text);
    drop(data);
}
