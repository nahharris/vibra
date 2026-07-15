use std::path::PathBuf;

use vibra::code::{
    ChangeSet, CodeErrorKind, Entry, Form, NodeKind, Path, Pattern, Query, Segment, SemanticIndex,
    SemanticKind, SourceDatabase,
};

fn path(segments: impl IntoIterator<Item = Segment>) -> Path {
    Path::new(segments.into_iter().collect())
}

fn database(source: &str) -> SourceDatabase {
    SourceDatabase::from_sources(
        PathBuf::from("workspace"),
        [(PathBuf::from("main.vibra"), source.to_string())],
    )
    .unwrap()
}

#[test]
fn exact_typed_paths_resolve_lossless_nodes() {
    let db = database(
        "=comment: module\nmain:\n  $function: $void\n  do:\n    - =comment: keep\n      $io.println: 'hello'\n",
    );
    let document = db.document("main.vibra").unwrap();
    let node = document
        .at(&path([
            Segment::key("main"),
            Segment::key("do"),
            Segment::index(0),
            Segment::key("$io.println"),
        ]))
        .unwrap();

    assert_eq!(node.source(), "'hello'");
    assert_eq!(node.form(), &Form::String("hello".into()));
    assert_eq!(
        node.path().to_compact_strings(),
        vec!["main", "do", "0", "$io.println"]
    );
    assert!(!node.fingerprint().is_empty());
    assert_eq!(node.revision(), document.revision());
}

#[test]
fn replacement_is_lossless_outside_the_target() {
    let db = database(
        "=comment: module\nmain:\n  $function: $void\n  do:\n    - =comment: keep\n      $io.println: 'hello'\n",
    );
    let target = db
        .document("main.vibra")
        .unwrap()
        .at(&path([
            Segment::key("main"),
            Segment::key("do"),
            Segment::index(0),
            Segment::key("$io.println"),
        ]))
        .unwrap();
    let changes = ChangeSet::new().replace(target.locator(), Form::String("changed".into()));

    let applied = db.apply(&changes).unwrap();
    assert_eq!(
        applied.database.document("main.vibra").unwrap().source(),
        "=comment: module\nmain:\n  $function: $void\n  do:\n    - =comment: keep\n      $io.println: changed\n"
    );
    assert_eq!(applied.changed_files, vec![PathBuf::from("main.vibra")]);
}

#[test]
fn stale_revisions_fail_without_partial_application() {
    let original = database("main:\n  value: old\nother: untouched\n");
    let target = original
        .document("main.vibra")
        .unwrap()
        .at(&path([Segment::key("main"), Segment::key("value")]))
        .unwrap();
    let changes = ChangeSet::new().replace(target.locator(), Form::String("new".into()));
    let changed = database("main:\n  value: externally-changed\nother: untouched\n");

    let error = changed.apply(&changes).unwrap_err();
    assert_eq!(error.kind, CodeErrorKind::StaleRevision);
    assert_eq!(
        changed.document("main.vibra").unwrap().source(),
        "main:\n  value: externally-changed\nother: untouched\n"
    );
}

#[test]
fn overlapping_edits_are_rejected_before_mutation() {
    let db = database("main:\n  nested:\n    value: old\n");
    let document = db.document("main.vibra").unwrap();
    let parent = document.at(&path([Segment::key("main")])).unwrap();
    let child = document
        .at(&path([
            Segment::key("main"),
            Segment::key("nested"),
            Segment::key("value"),
        ]))
        .unwrap();
    let changes = ChangeSet::new()
        .replace(parent.locator(), Form::Mapping(Vec::new()))
        .replace(child.locator(), Form::String("new".into()));

    let error = db.apply(&changes).unwrap_err();
    assert_eq!(error.kind, CodeErrorKind::OverlappingEdits);
    assert_eq!(
        db.document("main.vibra").unwrap().source(),
        "main:\n  nested:\n    value: old\n"
    );
}

#[test]
fn independent_edits_resolve_against_the_original_snapshot() {
    let db = database("items:\n  - first\n  - second\n  - third\n");
    let document = db.document("main.vibra").unwrap();
    let first = document
        .at(&path([Segment::key("items"), Segment::index(0)]))
        .unwrap();
    let second = document
        .at(&path([Segment::key("items"), Segment::index(1)]))
        .unwrap();

    let applied = db
        .apply(
            &ChangeSet::new()
                .delete(first.locator())
                .replace(second.locator(), Form::String("changed".into())),
        )
        .unwrap();

    assert_eq!(
        applied.database.document("main.vibra").unwrap().source(),
        "items:\n  - changed\n  - third\n"
    );
}

#[test]
fn move_can_relocate_a_child_within_its_original_parent() {
    let db = database("main:\n  old:\n    =comment: keep\n    $literal: 'value'\n  kept: true\n");
    let document = db.document("main.vibra").unwrap();
    let source = document
        .at(&path([Segment::key("main"), Segment::key("old")]))
        .unwrap();
    let target = document.at(&path([Segment::key("main")])).unwrap();

    let applied = db
        .apply(&ChangeSet::new().move_node(source.locator(), target.locator(), Segment::key("new")))
        .unwrap();

    assert_eq!(
        applied.database.document("main.vibra").unwrap().source(),
        "main:\n  kept: true\n  new:\n    =comment: keep\n    $literal: 'value'\n"
    );
}

#[test]
fn sequence_move_uses_original_destination_index() {
    let db = database("items:\n  - first\n  - second\n  - third\n");
    let document = db.document("main.vibra").unwrap();
    let source = document
        .at(&path([Segment::key("items"), Segment::index(0)]))
        .unwrap();
    let target = document.at(&path([Segment::key("items")])).unwrap();

    let applied = db
        .apply(&ChangeSet::new().move_node(source.locator(), target.locator(), Segment::index(2)))
        .unwrap();

    assert_eq!(
        applied.database.document("main.vibra").unwrap().source(),
        "items:\n  - second\n  - first\n  - third\n"
    );
}

#[test]
fn documents_expose_parent_and_ordered_children_by_path() {
    let db = database("main:\n  do:\n    - first\n    - second\n");
    let document = db.document("main.vibra").unwrap();
    let sequence = document
        .at(&path([Segment::key("main"), Segment::key("do")]))
        .unwrap();

    let children = sequence.children().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].kind(), NodeKind::Scalar);
    assert_eq!(
        children[1].path().to_compact_strings(),
        vec!["main", "do", "1"]
    );
    assert_eq!(
        children[1]
            .parent(&document)
            .unwrap()
            .unwrap()
            .path()
            .to_compact_strings(),
        vec!["main", "do"]
    );
}

#[test]
fn edit_primitives_operate_on_typed_paths() {
    let db = database("main:\n  old-name: old\n  items:\n    - one\n    - two\n");
    let document = db.document("main.vibra").unwrap();
    let old_name = document
        .at(&path([Segment::key("main"), Segment::key("old-name")]))
        .unwrap();
    let items = document
        .at(&path([Segment::key("main"), Segment::key("items")]))
        .unwrap();

    let renamed = db
        .apply(&ChangeSet::new().rename_mapping_key(old_name.locator(), "new-name"))
        .unwrap()
        .database;
    let items = renamed
        .document("main.vibra")
        .unwrap()
        .at(items.path())
        .unwrap();
    let inserted = renamed
        .apply(&ChangeSet::new().insert_sequence(items.locator(), 1, Form::String("middle".into())))
        .unwrap()
        .database;
    let inserted_document = inserted.document("main.vibra").unwrap();
    let main = inserted_document.at(&path([Segment::key("main")])).unwrap();
    let upserted = inserted
        .apply(&ChangeSet::new().upsert_mapping(
            main.locator(),
            "added",
            Form::Mapping(vec![Entry {
                key: "enabled".into(),
                value: Form::Bool(true),
            }]),
        ))
        .unwrap()
        .database;

    assert_eq!(
        upserted.document("main.vibra").unwrap().source(),
        "main:\n  new-name: old\n  items:\n    - one\n    - middle\n    - two\n  added:\n    enabled: true\n"
    );
}

#[test]
fn delete_and_splice_are_atomic_structural_operations() {
    let db = database("main:\n  remove-me: old\n  items:\n    - one\n    - two\n    - three\n");
    let document = db.document("main.vibra").unwrap();
    let remove_me = document
        .at(&path([Segment::key("main"), Segment::key("remove-me")]))
        .unwrap();
    let deleted = db
        .apply(&ChangeSet::new().delete(remove_me.locator()))
        .unwrap()
        .database;
    let items = deleted
        .document("main.vibra")
        .unwrap()
        .at(&path([Segment::key("main"), Segment::key("items")]))
        .unwrap();
    let spliced = deleted
        .apply(&ChangeSet::new().splice_sequence(
            items.locator(),
            1,
            2,
            vec![Form::String("replacement".into())],
        ))
        .unwrap()
        .database;

    assert_eq!(
        spliced.document("main.vibra").unwrap().source(),
        "main:\n  items:\n    - one\n    - replacement\n"
    );
}

#[test]
fn workspace_discovery_keeps_module_parts_as_physical_documents() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::create_dir_all(dir.path().join("target")).unwrap();
    std::fs::create_dir_all(dir.path().join("dep/std")).unwrap();
    std::fs::write(dir.path().join("src/math.vibra"), "main: 1\n").unwrap();
    std::fs::write(dir.path().join("src/math.test.vibra"), "test-case: 2\n").unwrap();
    std::fs::write(dir.path().join("target/generated.vibra"), "ignored: 1\n").unwrap();
    std::fs::write(dir.path().join("dep/std/io.vibra"), "ignored: 1\n").unwrap();

    let db = SourceDatabase::discover(dir.path()).unwrap();
    let paths: Vec<_> = db.paths().map(PathBuf::from).collect();

    assert_eq!(
        paths,
        vec![
            PathBuf::from("src/math.test.vibra"),
            PathBuf::from("src/math.vibra"),
        ]
    );
}

#[test]
fn compiler_loading_retains_physical_module_part_origins() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("math.vibra");
    let part = dir.path().join("math.test.vibra");
    std::fs::write(&base, "value: 1\n").unwrap();
    std::fs::write(&part, "other: 2\n").unwrap();

    let loaded = vibra::load::load_program(&base).unwrap();
    let canonical_base = std::fs::canonicalize(&base).unwrap();
    let canonical_part = std::fs::canonicalize(&part).unwrap();

    let mut expected_parts = vec![canonical_base.clone(), canonical_part.clone()];
    expected_parts.sort();
    assert_eq!(
        loaded.module_parts.get(&canonical_base),
        Some(&expected_parts)
    );
    assert!(loaded.sources.document(&canonical_base).is_ok());
    assert!(loaded.sources.document(&canonical_part).is_ok());
}

#[test]
fn structural_queries_return_ordered_paths_and_captures() {
    let db = database(
        "main:\n  do:\n    - $io.println: first\n    - $io.println: second\n    - $io.eprintln: ignored\n",
    );
    let document = db.document("main.vibra").unwrap();
    let query = Query::descendants(path([Segment::key("main"), Segment::key("do")]))
        .with_mapping_key("$io.println")
        .with_pattern(Pattern::capture("message", Pattern::Any));

    let matches = query.execute(&document).unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].node.path().to_compact_strings(),
        vec!["main", "do", "0", "$io.println"]
    );
    assert_eq!(
        matches[1].captures.get("message"),
        Some(&Form::String("second".into()))
    );
}

#[test]
fn semantic_rename_updates_definition_local_and_imported_references() {
    let db = SourceDatabase::from_sources(
        PathBuf::from("workspace"),
        [
            (
                PathBuf::from("a.vibra"),
                "helper:\n  $function: $void\n  return: $void\n  do:\n    - $let:\n        value: 1\n"
                    .to_string(),
            ),
            (
                PathBuf::from("b.vibra"),
                "util:\n  $import: ./a.vibra\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $util.helper: null\n"
                    .to_string(),
            ),
        ],
    )
    .unwrap();
    let index = SemanticIndex::build(&db).unwrap();
    let imported_calls = index.query(Some(SemanticKind::Call), Some("util.helper"));
    assert_eq!(imported_calls.len(), 1);
    assert_eq!(
        imported_calls[0].path.to_compact_strings(),
        vec!["main", "do", "0", "$util.helper"]
    );
    let changes = index
        .rename_symbol(&db, "a.vibra", "helper", "assist")
        .unwrap();

    let renamed = db.apply(&changes).unwrap().database;

    assert!(renamed
        .document("a.vibra")
        .unwrap()
        .source()
        .contains("assist:"));
    assert!(renamed
        .document("a.vibra")
        .unwrap()
        .source()
        .contains("assist:"));
    assert!(renamed
        .document("b.vibra")
        .unwrap()
        .source()
        .contains("$util.assist:"));
    let validation = tempfile::tempdir().unwrap();
    std::fs::write(
        validation.path().join("a.vibra"),
        renamed.document("a.vibra").unwrap().source(),
    )
    .unwrap();
    std::fs::write(
        validation.path().join("b.vibra"),
        renamed.document("b.vibra").unwrap().source(),
    )
    .unwrap();
    let loaded = vibra::load::load_program(&validation.path().join("b.vibra")).unwrap();
    vibra::lower::lower_program(&loaded).unwrap();
}

#[test]
fn semantic_rename_updates_same_module_references() {
    let db = database(
        "helper:\n  $function: $void\n  return: $void\n  do:\n    - $let:\n        value: 1\nmain:\n  $function: $void\n  return: $void\n  do:\n    - $helper: null\n",
    );
    let changes = SemanticIndex::build(&db)
        .unwrap()
        .rename_symbol(&db, "main.vibra", "helper", "assist")
        .unwrap();
    let renamed = db.apply(&changes).unwrap().database;
    assert!(renamed
        .document("main.vibra")
        .unwrap()
        .source()
        .contains("$assist:"));
}

#[test]
fn semantic_index_ignores_annotation_contents() {
    let db = database(
        "=comment: '$helper is prose'\nhelper:\n  =comment: '$helper remains prose'\n  $function: $void\n  return: $void\n  do: []\n",
    );
    let index = SemanticIndex::build(&db).unwrap();

    assert!(index
        .query(Some(SemanticKind::Definition), Some("=comment"))
        .is_empty());
    assert!(index
        .query(Some(SemanticKind::Reference), Some("helper"))
        .is_empty());

    let renamed = index
        .rename_symbol(&db, "main.vibra", "helper", "renamed")
        .unwrap();
    let applied = db.apply(&renamed).unwrap();
    let source = applied.database.document("main.vibra").unwrap();
    assert!(source.source().contains("=comment: '$helper is prose'"));
    assert!(source.source().contains("renamed:"));
}

#[test]
fn semantic_import_alias_rename_is_local_and_collision_checked() {
    let db = SourceDatabase::from_sources(
        PathBuf::from("workspace"),
        [
            (PathBuf::from("a.vibra"), "helper: 1\n".to_string()),
            (
                PathBuf::from("b.vibra"),
                "util:\n  $import: ./a.vibra\nother: 2\ncall:\n  $util.helper: null\n".to_string(),
            ),
        ],
    )
    .unwrap();
    let index = SemanticIndex::build(&db).unwrap();
    let changes = index
        .rename_import_alias(&db, "b.vibra", "util", "lib")
        .unwrap();
    let renamed = db.apply(&changes).unwrap().database;

    assert_eq!(
        renamed.document("b.vibra").unwrap().source(),
        "lib:\n  $import: ./a.vibra\nother: 2\ncall:\n  $lib.helper: null\n"
    );

    let error = index
        .rename_import_alias(&db, "b.vibra", "util", "other")
        .unwrap_err();
    assert_eq!(error.kind, CodeErrorKind::EditConflict);
}

#[test]
fn copy_and_move_use_source_snapshot_paths_across_documents() {
    let db = SourceDatabase::from_sources(
        PathBuf::from("workspace"),
        [
            (
                PathBuf::from("source.vibra"),
                "source:\n  value: copied\nmove-me: moved\n".to_string(),
            ),
            (
                PathBuf::from("target.vibra"),
                "target:\n  existing: true\nitems:\n  - first\n".to_string(),
            ),
        ],
    )
    .unwrap();
    let source_document = db.document("source.vibra").unwrap();
    let target_document = db.document("target.vibra").unwrap();
    let copied = source_document
        .at(&path([Segment::key("source"), Segment::key("value")]))
        .unwrap();
    let moved = source_document
        .at(&path([Segment::key("move-me")]))
        .unwrap();
    let target = target_document.at(&path([Segment::key("target")])).unwrap();
    let items = target_document.at(&path([Segment::key("items")])).unwrap();

    let copied_db = db
        .apply(&ChangeSet::new().copy(copied.locator(), target.locator(), Segment::key("copied")))
        .unwrap()
        .database;
    let moved = copied_db
        .document("source.vibra")
        .unwrap()
        .at(moved.path())
        .unwrap();
    let items = copied_db
        .document("target.vibra")
        .unwrap()
        .at(items.path())
        .unwrap();
    let moved_db = copied_db
        .apply(&ChangeSet::new().move_node(moved.locator(), items.locator(), Segment::index(1)))
        .unwrap()
        .database;

    assert_eq!(
        moved_db.document("source.vibra").unwrap().source(),
        "source:\n  value: copied\n"
    );
    assert_eq!(
        moved_db.document("target.vibra").unwrap().source(),
        "target:\n  existing: true\n  copied: copied\nitems:\n  - first\n  - moved\n"
    );
}

#[test]
fn mapping_insert_rejects_existing_keys_while_upsert_replaces() {
    let db = database("main:\n  value: old\n");
    let main = db
        .document("main.vibra")
        .unwrap()
        .at(&path([Segment::key("main")]))
        .unwrap();
    let error = db
        .apply(&ChangeSet::new().insert_mapping(
            main.locator(),
            "value",
            Form::String("new".into()),
        ))
        .unwrap_err();
    assert_eq!(error.kind, CodeErrorKind::EditConflict);

    let upserted = db
        .apply(&ChangeSet::new().upsert_mapping(
            main.locator(),
            "value",
            Form::String("new".into()),
        ))
        .unwrap()
        .database;
    assert_eq!(
        upserted.document("main.vibra").unwrap().source(),
        "main:\n  value: new\n"
    );
}
