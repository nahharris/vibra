//! Snapshot-based atomic formatter plan tests for M2 Step 11.

use std::fs;
use std::io;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

use vibra_diagnostics::Level;
use vibra_workspace::format_plan::{FormatPlanError, apply_format, plan_format};

use crate::support::{TempDir, manifest};

/// A project whose binary target is `src/hello` holding `source` as its entry.
fn project(label: &str, source: &str) -> TempDir {
    let directory = TempDir::plain(label);
    let document = manifest(
        "demo",
        "(record name: @app kind: @bin root: \"src/hello\" entry: @app.main.main effects: (array))",
        "(dict)",
    );
    directory.write("project.vibon", &document);
    directory.write("src/hello/main.vib", source);
    directory
}

fn link_directory(link: &Path, target: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let link = link.to_string_lossy().replace('/', "\\");
        let target = target.to_string_lossy().replace('/', "\\");
        let command = format!("mklink /J {link} {target}");
        let output = Command::new("cmd").arg("/C").arg(&command).output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "junction command {command:?} failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory links are unavailable on this platform",
        ))
    }
}

#[test]
fn plan_uses_snapshot_binding_facts_and_apply_writes_the_canonical_result() {
    let source = "(defn answer () i32 (let f choose) (f 3i32 second: 11i32 first: 9i32))\n(defn choose (fallback i32) i32 labelled: (first i32 7i32 second i32 8i32) first)\n";
    let root = project("binding", source);

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
fn untracked_source_keeps_label_order_without_snapshot_binding_facts() {
    let root = project("untracked-binding", "(defn main () void (do))\n");
    let untracked = root.path().join("untracked.vib");
    let source = "(defn answer () i32 (let f choose) (f 3i32 second: 11i32 first: 9i32))\n(defn choose (fallback i32) i32 labelled: (first i32 7i32 second i32 8i32) first)\n";
    fs::write(&untracked, source).expect("write accepted untracked source");

    let plan = plan_format(root.path(), Path::new("untracked.vib"))
        .expect("preview accepted untracked source without snapshot binding facts");

    assert!(plan.changed());
    assert!(
        plan.formatted_text()
            .contains("(f 3i32 second: 11i32 first: 9i32)"),
        "untracked source must preserve labelled argument order; got:\n{}",
        plan.formatted_text()
    );
}

#[test]
fn stale_plan_is_refused_without_overwriting_the_newer_bytes() {
    let root = project("stale", "(defn main () void (do))\n");
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
    let root = project("recovered", "(defn main () void\r\n");
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

#[test]
fn apply_rejects_a_parent_replaced_by_an_outside_directory_link() {
    let mut root = project("parent-link", "(defn main () void    (do))\n");
    let outside = root.sibling_dir("outside");
    let source_parent = root.path().join("src/hello");
    let outside_parent = outside.join("hello");
    let source = outside_parent.join("main.vib");
    let before = fs::read(root.path().join("src/hello/main.vib"))
        .expect("read original source bytes");
    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("create format plan before parent replacement");

    fs::rename(&source_parent, &outside_parent)
        .expect("move source parent outside workspace");
    link_directory(&source_parent, &outside_parent)
        .expect("replace parent with directory link");

    let error = apply_format(&plan).expect_err("apply must re-confine the parent path");

    assert!(matches!(
        error,
        FormatPlanError::InvalidPath(_) | FormatPlanError::Workspace(_)
    ));
    assert_eq!(fs::read(source).expect("read outside source"), before);
}

#[test]
fn formatter_rejects_the_canonical_dependency_vendor_tree() {
    let root = project("vendor", "(defn main () void (do))\n");
    fs::create_dir_all(root.path().join("dep/std"))
        .expect("create canonical vendored dependency path");
    fs::write(
        root.path().join("dep/std/lib.vib"),
        "(defn main () void (do))\n",
    )
    .expect("write vendored source");

    let error = plan_format(root.path(), Path::new("dep/std/lib.vib"))
        .expect_err("vendored dependency source cannot be formatted");

    assert!(matches!(error, FormatPlanError::InvalidPath(_)));
}

#[test]
fn ill_typed_source_diagnostics_are_reported_and_application_is_refused() {
    let original = "(defn main () str  1i32)\n";
    let root = project("ill-typed", original);
    let path = root.path().join("src/hello/main.vib");
    let plan = plan_format(root.path(), Path::new("src/hello/main.vib"))
        .expect("ill-typed source still has a preview plan");

    assert!(plan.changed());
    assert!(
        plan.diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.level() == Level::Error })
    );
    let error = apply_format(&plan).expect_err("ill-typed source must not be written");

    assert!(matches!(error, FormatPlanError::Postcondition(_)));
    assert_eq!(
        fs::read_to_string(path).expect("read source after refusal"),
        original
    );
}
