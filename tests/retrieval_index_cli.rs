use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn vibra_cmd() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn write_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("project.vib"),
        "(project\n  (package \"retrieval-fixture\" \"0.1.0\")\n  (target app kind: @bin root: \"src\" entry: \"main.vib\"))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("src/helper.vib"),
        r#"(def error-kind (enum (missing void)))
(defn
  identity
  (value t)
  t
  (do ; identity comment
    (return value))
  where: (t any)
)
(defn hidden (value int64) int64 (do (return value)) visibility: @private)
(def result (enum (ok t) (err e)) where: (t any e any))
(defn maybe (value int64) (result int64 error-kind)
  (do (return (result.ok value))))
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("src/main.vib"),
        r#"(import helper "./helper.vib")
(def display (interface (show (fn-type (self self) str))))
(def box (enum (boxed int64))
  impls: (
    (impl display methods: (
      (method show (fn (self self) str (do (return "boxed"))))
    ))
  ))
(deffect clock
  (defn now () int64 (do (return 1)) effects: ()))
(defn render (value box) str (do (let text (display.show value)) (return text)))
(defn main () void (do (let value (box.boxed 1)) (let rendered (render value))))
"#,
    )
    .unwrap();
    root
}

fn run_index(root: &Path, extra: &[&str]) -> std::process::Output {
    let entry = root.join("src/main.vib");
    let mut command = vibra_cmd();
    command.current_dir(root).arg("index").arg(path(&entry));
    command.args(extra).output().unwrap()
}

fn assert_sorted_object_keys(value: &Value) {
    match value {
        Value::Object(object) => {
            let keys = object.keys().cloned().collect::<Vec<_>>();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "JSON object keys must be sorted");
            for child in object.values() {
                assert_sorted_object_keys(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_sorted_object_keys(item);
            }
        }
        _ => {}
    }
}

fn assert_schema_valid(index: &Value) {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/index.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    assert_eq!(schema["$id"], "https://vibra.dev/schemas/index.schema.json");
    jsonschema::meta::validate(&schema).expect("index schema must be a valid JSON Schema");
    let validator =
        jsonschema::validator_for(&schema).expect("index schema must compile for validation");
    validator
        .validate(index)
        .unwrap_or_else(|error| panic!("index output failed JSON Schema validation: {error}"));
}

#[test]
fn index_defaults_to_byte_identical_json() {
    let root = write_fixture();
    let first = run_index(root.path(), &[]);
    assert!(
        first.status.success(),
        "index failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run_index(root.path(), &[]);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);

    let explicit = run_index(root.path(), &["--format", "json"]);
    assert!(explicit.status.success());
    assert_eq!(first.stdout, explicit.stdout);

    let index: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_sorted_object_keys(&index);
    assert_schema_valid(&index);
}

#[test]
fn index_records_semantics_private_functions_and_normalized_source() {
    let root = write_fixture();
    let output = run_index(root.path(), &["--format", "json"]);
    assert!(
        output.status.success(),
        "index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let index: Value = serde_json::from_slice(&output.stdout).unwrap();
    let records = index["records"].as_array().unwrap();

    let names = records
        .iter()
        .map(|record| record["qualified-name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for name in [
        "main",
        "render",
        "clock.now",
        "helper.identity",
        "helper.hidden",
        "helper.maybe",
    ] {
        assert!(names.contains(name), "missing function record `{name}`");
    }

    let hidden = records
        .iter()
        .find(|record| record["qualified-name"] == "helper.hidden")
        .unwrap();
    assert_eq!(hidden["visibility"], "private");
    assert_eq!(hidden["module-path"], "src/helper.vib");

    let generic = records
        .iter()
        .find(|record| record["qualified-name"] == "helper.identity")
        .unwrap();
    assert_eq!(generic["signature"]["type-parameters"][0]["name"], "t");
    assert_eq!(generic["signature"]["parameters"][0]["type"], "t");
    assert!(generic["source"]
        .as_str()
        .unwrap()
        .contains("identity comment"));

    let operation = records
        .iter()
        .find(|record| record["qualified-name"] == "clock.now")
        .unwrap();
    assert!(!operation["effects"]["declared"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(operation["effects"]["inferred"].is_null());

    let render = records
        .iter()
        .find(|record| record["qualified-name"] == "render")
        .unwrap();
    assert!(render["call-edges"]
        .as_array()
        .unwrap()
        .iter()
        .any(|edge| edge == "box.display.show"));

    let maybe = records
        .iter()
        .find(|record| record["qualified-name"] == "helper.maybe")
        .unwrap();
    assert!(maybe["error-types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error == "helper.error-kind"));

    let generated = records
        .iter()
        .filter(|record| record["generated"] == true)
        .collect::<Vec<_>>();
    assert!(
        !generated.is_empty(),
        "inline impl should exercise the generated guard"
    );
    assert!(generated
        .iter()
        .all(|record| record["source"].as_str().unwrap().is_empty()));
}
