use serde_json::Value;

#[test]
fn async_contract_schema_and_vectors_are_well_formed() {
    let root = env!("CARGO_MANIFEST_DIR");
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/schemas/async-task-trace.schema.json"))
            .expect("read async trace schema"),
    )
    .expect("async trace schema is JSON");
    assert_eq!(
        schema["$id"],
        "https://vibra.dev/schemas/async-task-trace.schema.json"
    );
    assert_eq!(schema["additionalProperties"], false);

    let vectors: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/tests/conformance/async-task-vectors.json"))
            .expect("read async vectors"),
    )
    .expect("async vectors are JSON");
    assert_eq!(vectors["contract-version"], "1");
    let cases = vectors["vectors"].as_array().expect("vectors array");
    assert!(cases.len() >= 7, "cover every major semantic dimension");

    let allowed = schema["$defs"]["event"]["properties"]["kind"]["enum"]
        .as_array()
        .expect("event kind enum");
    for case in cases {
        let name = case["name"].as_str().expect("vector name");
        assert!(
            !case["script"].as_array().expect("script").is_empty(),
            "{name}"
        );
        assert_eq!(case["expected"]["contract-version"], "1", "{name}");
        let events = case["expected"]["events"].as_array().expect("events");
        assert!(!events.is_empty(), "{name}");
        let mut last = 0;
        for event in events {
            let at = event["at"].as_u64().expect("non-negative time");
            assert!(at >= last, "{name}: trace time must be monotonic");
            last = at;
            assert!(
                allowed.contains(&event["kind"]),
                "{name}: unknown event kind"
            );
            assert!(event["scope"].as_str().is_some(), "{name}: scope required");
        }
    }
}
