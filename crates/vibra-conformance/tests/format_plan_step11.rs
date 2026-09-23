//! Snapshot-backed formatter conformance for M2 Step 11.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_types::check_source;
use vibra_workspace::format_plan::plan_format;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "vibra-format-conformance-step11-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn snapshot_binding_normalization_preserves_the_program_result() {
    let root = TempDir::new();
    let project = r#"(record format: @project.v1 package: (record name: "hello" version: "0.1.0") targets: (array (record name: @hello kind: @bin root: "src/hello" entry: @hello.main.main effects: (array))) dependencies: (map))"#;
    let source = r#"
(defn main () i32 (choose 3i32 second: 11i32 first: 9i32))
(defn choose (fallback i32) i32
  labelled: (first i32 7i32 second i32 8i32)
  first)
"#;
    let source_path = root.path().join("src/hello/main.vib");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source root");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("project.vibon"), project).expect("write project");
    fs::write(&source_path, source).expect("write source");

    let original = check_source("src/hello/main.vib", source);
    assert!(original.accepted(), "{:?}", original.diagnostics());
    let original_value =
        vibra_interp::run(original.program().expect("checked original"))
            .expect("run original")
            .value()
            .clone();

    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("plan uses the confined source snapshot");
    let formatted = plan.formatted_text();
    let first = formatted.find("first: 9i32").expect("first labelled arg");
    let second = formatted
        .find("second: 11i32")
        .expect("second labelled arg");
    assert!(first < second, "{formatted}");

    let reformatted = check_source("src/hello/main.vib", formatted);
    assert!(reformatted.accepted(), "{:?}", reformatted.diagnostics());
    let reformatted_value =
        vibra_interp::run(reformatted.program().expect("checked formatted source"))
            .expect("run formatted source")
            .value()
            .clone();
    assert_eq!(reformatted_value, original_value);
}
