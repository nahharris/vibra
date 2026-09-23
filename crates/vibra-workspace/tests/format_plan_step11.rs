#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! Snapshot-based atomic formatter plan tests for M2 Step 11.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_workspace::format_plan::{FormatPlanError, apply_format, plan_format};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn project(label: &str, source: &str) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-format-plan-step11-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src/hello")).expect("create source root");
        fs::write(
            root.join("project.vibon"),
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/hello\" entry: @app.main.main effects: (array))) dependencies: (map))\n",
        )
        .expect("write project marker");
        fs::write(root.join("src/hello/main.vib"), source).expect("write source");
        Self(root)
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
fn plan_uses_snapshot_binding_facts_and_apply_writes_the_canonical_result() {
    let source = "(defn answer () i32 (let f choose (f 3i32 second: 11i32 first: 9i32)))\n(defn choose (fallback i32) i32 labelled: (first i32 7i32 second i32 8i32) first)\n";
    let root = TempDir::project("binding", source);

    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("create a safe format plan");

    assert!(plan.changed());
    assert!(
        plan.formatted_text()
            .contains("(f 3i32 first: 9i32 second: 11i32)")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/hello/main.vib"))
            .expect("preview leaves source in place"),
        source
    );
    apply_format(&plan).expect("apply the current revision");
    assert_eq!(
        fs::read_to_string(root.path().join("src/hello/main.vib"))
            .expect("read formatted source"),
        plan.formatted_text()
    );
}

#[test]
fn stale_plan_is_refused_without_overwriting_the_newer_bytes() {
    let root = TempDir::project("stale", "(defn main () void (do))\n");
    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("create format plan");
    let newer = "(defn main () void (do) ; newer\n)\n";
    let path = root.path().join("src/hello/main.vib");
    fs::write(&path, newer).expect("change source after planning");

    let result = apply_format(&plan);

    assert!(matches!(result, Err(FormatPlanError::StaleRevision { .. })));
    assert_eq!(fs::read_to_string(path).expect("read latest source"), newer);
}

#[test]
fn recovered_and_unknown_extension_inputs_are_never_rewritten() {
    let root = TempDir::project("recovered", "(defn main () void\r\n");
    let path = root.path().join("src/hello/main.vib");
    let before = fs::read(&path).expect("read recovered source bytes");

    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("recovered sources can be previewed");
    assert!(!plan.changed());
    assert_eq!(plan.formatted_text().as_bytes(), before);
    apply_format(&plan).expect("a no-op plan does not write");
    assert_eq!(fs::read(&path).expect("read after no-op"), before);

    assert!(matches!(
        plan_format(root.path(), Path::new("src/hello/main.txt")),
        Err(FormatPlanError::UnsupportedExtension(_))
    ));
}
