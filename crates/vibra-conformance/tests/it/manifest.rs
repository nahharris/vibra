//! The manifest rows of the step 02 test matrix, and the corpus layout and
//! rule-status rows that need a corpus on disk.

use crate::support::{reader_case, rules, scratch_directory, write};

use vibra_conformance::{
    CaseManifest, ConformanceOperation, ConformanceProfile, Corpus, CorpusError,
    RuleIndex, Verdict, parse_identity,
};

/// A reader manifest with `profile` and `operation` substituted.
fn manifest(id: &str, profile: &str, operation: &str, extra: &str) -> String {
    format!(
        "id = \"{id}\"\nrule = \"{id}\"\nprofile = \"{profile}\"\n\
         operation = \"{operation}\"\n\n[inputs]\ndata = [\"input.vibon\"]\n\n\
         [expect]\nverdict = \"accepted\"\n{extra}"
    )
}

fn error_of(text: &str) -> String {
    CaseManifest::from_str(text)
        .expect_err("the manifest is rejected")
        .to_string()
}

#[test]
fn every_profile_spelling_is_accepted() {
    for spelling in [
        "readerV2",
        "staticV2",
        "interpreterV2",
        "toolingV2",
        "nativeV1",
        "fullV2",
    ] {
        let profile = spelling
            .parse::<ConformanceProfile>()
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(profile.as_str(), spelling);
    }
    assert_eq!(ConformanceProfile::ALL.len(), 6);
}

/// Each operation and the profile it belongs to, from the profile table of
/// `docs/spec/07-diagnostics-and-conformance.md`, "Conformance profiles".
const OWN_PROFILES: &[(&str, &str)] = &[
    ("reader", "readerV2"),
    ("projectDecode", "staticV2"),
    ("sourceGraph", "staticV2"),
    ("resolve", "staticV2"),
    ("workspaceCheck", "staticV2"),
    ("format", "toolingV2"),
    ("query", "toolingV2"),
    ("interpret", "interpreterV2"),
    ("workspaceRun", "interpreterV2"),
    ("workspaceTest", "interpreterV2"),
];

/// The profile `operation` belongs to.
fn own_profile(operation: &str) -> &'static str {
    OWN_PROFILES
        .iter()
        .find(|(name, _)| *name == operation)
        .map(|(_, profile)| *profile)
        .unwrap_or_else(|| panic!("`{operation}` is an operation of the table"))
}

#[test]
fn manifest_the_profile_of_the_operation() {
    for (operation, profile) in OWN_PROFILES {
        let case = CaseManifest::from_str(&manifest_of_operation(operation))
            .unwrap_or_else(|error| panic!("{operation}: {error}"));
        assert_eq!(case.profile.as_str(), *profile, "{operation}");
        assert_eq!(case.operation.profile(), case.profile, "{operation}");
    }
    assert_eq!(OWN_PROFILES.len(), ConformanceOperation::ALL.len());
}

#[test]
fn manifest_the_profile_of_the_operation_in_the_whole_corpus() {
    let corpus = Corpus::discover(vibra_conformance::corpus_root())
        .expect("every case of the corpus loads");
    assert!(!corpus.is_empty());
    for case in &corpus {
        let manifest = case.manifest();
        assert_eq!(
            manifest.profile.as_str(),
            own_profile(manifest.operation.as_str()),
            "{}",
            manifest.id
        );
    }
}

/// `manifest_of_operation(operation)` naming `profile` instead of the
/// operation's own.
fn manifest_naming(operation: &str, profile: &str) -> String {
    manifest_of_operation(operation).replace(
        &format!("profile = \"{}\"", own_profile(operation)),
        &format!("profile = \"{profile}\""),
    )
}

#[test]
fn manifest_another_profile() {
    let error = error_of(&manifest_naming("workspaceCheck", "interpreterV2"));
    assert!(
        error.contains("`workspaceCheck`")
            && error.contains("`staticV2`")
            && error.contains("`interpreterV2`"),
        "{error}"
    );
    // Every operation under every profile that is not its own.
    for (operation, own) in OWN_PROFILES {
        for profile in ConformanceProfile::ALL {
            if profile.as_str() == *own {
                continue;
            }
            let error = error_of(&manifest_naming(operation, profile.as_str()));
            assert!(
                error.contains(&format!("`{operation}`"))
                    && error.contains(&format!("`{own}`")),
                "{operation} under {profile}: {error}"
            );
        }
    }
}

#[test]
fn manifest_a_profile_with_no_operation_of_its_own() {
    for profile in ["fullV2", "nativeV1"] {
        let error = error_of(&manifest_naming("interpret", profile));
        assert!(
            error.contains("`interpret`")
                && error.contains("`interpreterV2`")
                && error.contains(&format!("`{profile}`")),
            "{profile}: {error}"
        );
    }
}

#[test]
fn a_kebab_case_profile_is_an_unknown_profile() {
    let error = error_of(&manifest("V2-SRC-READER-x", "reader-v2", "reader", ""));
    assert!(
        error.contains("unknown conformance profile `reader-v2`"),
        "{error}"
    );
}

/// A manifest of `operation` with the inputs its shape requires.
fn manifest_of_operation(operation: &str) -> String {
    let inputs = match operation {
        "reader" => "data = [\"input.vibon\"]",
        "projectDecode" => "project = \"project.vibon\"",
        "interpret" => "source = \"main.vib\"",
        "format" => {
            "tree = \"workspace\"\nproject = \"workspace/project.vibon\"\n\
             source = \"workspace/src/main.vib\""
        }
        _ => "tree = \"workspace\"\nproject = \"workspace/project.vibon\"",
    };
    // An operation outside the table (a misspelling) keeps a profile that
    // parses, so that the operation's own error is the one reported.
    let profile = OWN_PROFILES
        .iter()
        .find(|(name, _)| *name == operation)
        .map_or("readerV2", |(_, profile)| *profile);
    format!(
        "id = \"V2-TOOL-x\"\nrule = \"V2-TOOL-x\"\nprofile = \"{profile}\"\n\
         operation = \"{operation}\"\n\n[inputs]\n{inputs}\n\n\
         [expect]\nverdict = \"accepted\"\n"
    )
}

#[test]
fn every_operation_spelling_is_accepted() {
    let spellings = [
        "reader",
        "projectDecode",
        "sourceGraph",
        "resolve",
        "workspaceCheck",
        "format",
        "query",
        "interpret",
        "workspaceRun",
        "workspaceTest",
    ];
    for spelling in spellings {
        let case = CaseManifest::from_str(&manifest_of_operation(spelling))
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(case.operation.as_str(), spelling);
    }
    assert_eq!(
        ConformanceOperation::ALL
            .iter()
            .map(|operation| operation.as_str())
            .collect::<Vec<_>>(),
        spellings
    );
}

#[test]
fn a_kebab_case_operation_is_an_unknown_operation() {
    let error = error_of(&manifest_of_operation("workspace-check"));
    assert!(
        error.contains("unknown conformance operation `workspace-check`"),
        "{error}"
    );
}

#[test]
fn well_formed_identities_are_accepted() {
    for id in ["V2-SRC-READER-x", "V1-PROJECT-x"] {
        CaseManifest::from_str(&manifest(id, "readerV2", "reader", ""))
            .unwrap_or_else(|error| panic!("{id}: {error}"));
    }
    let parts = parse_identity("V2-SRC-READER-x").expect("an identity");
    assert_eq!(
        (parts.version, parts.prefix, parts.suffix),
        ("V2", "SRC-READER", "x")
    );
}

#[test]
fn malformed_identities_are_rejected() {
    for (id, reason) in [
        ("V3-SRC-READER-x", "the version is not `V1` or `V2`"),
        ("V2-SRC-x", "the section prefix is not one of the twenty"),
        (
            "V2-SRC-READER-X",
            "the suffix is not lower-case letters, digits, and hyphens",
        ),
    ] {
        let error = error_of(&manifest(id, "readerV2", "reader", ""));
        assert!(
            error.contains(id) && error.contains(reason),
            "{id}: {error}"
        );
    }
}

#[test]
fn a_case_identity_is_its_rule_or_the_rule_and_a_further_suffix() {
    let case = |id: &str| {
        format!(
            "id = \"{id}\"\nrule = \"V2-SRC-READER-x\"\nprofile = \"readerV2\"\n\
             operation = \"reader\"\n\n[inputs]\ndata = [\"input.vibon\"]\n\n\
             [expect]\nverdict = \"accepted\"\n"
        )
    };
    CaseManifest::from_str(&case("V2-SRC-READER-x-second")).expect("a further suffix");
    for id in [
        "V2-SRC-READER-y",
        "V2-SRC-READER-xy",
        "V2-SRC-READER-x-Second",
    ] {
        let error = error_of(&case(id));
        assert!(error.contains(id), "{id}: {error}");
    }
}

#[test]
fn an_unknown_key_in_expect_is_an_error_naming_the_key() {
    let error = error_of(&manifest(
        "V2-SRC-READER-x",
        "readerV2",
        "reader",
        "extra = true\n",
    ));
    assert!(error.contains("unknown field `extra`"), "{error}");
}

#[test]
fn an_unknown_key_anywhere_is_an_error() {
    let base = manifest("V2-SRC-READER-x", "readerV2", "reader", "");
    for (text, key) in [
        (format!("index = \"x\"\n{base}"), "index"),
        (
            base.replace("[inputs]\n", "[inputs]\nworld = \"w.vibon\"\n"),
            "world",
        ),
        (
            format!(
                "{base}\n[[expect.diagnostics]]\ncode = \"@syntax.invalidForm\"\n\
                 level = \"@error\"\nmessage = \"m\"\nspan = {{ start = 0, end = 1 }}\n"
            ),
            "message",
        ),
        (
            format!(
                "{base}\n[[expect.diagnostics]]\ncode = \"@syntax.invalidForm\"\n\
                 level = \"@error\"\nspan = {{ start = 0, end = 1, width = 1 }}\n"
            ),
            "width",
        ),
    ] {
        let error = error_of(&text);
        assert!(
            error.contains(&format!("unknown field `{key}`")),
            "{key}: {error}"
        );
    }
}

#[test]
fn the_expectation_keys_decode() {
    let text = format!(
        "{}\n[[expect.diagnostics]]\ncode = \"@syntax.unmatchedDelimiter\"\n\
         level = \"@error\"\nsource = \"input.vibon\"\nspan = {{ start = 4, end = 5 }}\n\
         related = [{{ source = \"input.vibon\", span = {{ start = 0, end = 1 }} }}]\n\
         notes = [\"a note\"]\nfixes = []\n",
        manifest(
            "V2-SRC-READER-x",
            "readerV2",
            "reader",
            "tree = \"tree.vibon\"\nformatted = \"formatted.vibon\"\n\
             graph = \"g.vibon\"\nresolved = \"r.vibon\"\ntypes = \"t.vibon\"\n\
             result = \"v.vibon\"\ntestRun = \"run.vibon\"\ntargetRun = \"target.vibon\"\n",
        )
    );
    let case = CaseManifest::from_str(&text).expect("a valid manifest");
    let expect = &case.expectations;
    assert_eq!(expect.verdict, Verdict::Accepted);
    assert_eq!(
        expect
            .snapshots()
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        [
            "tree",
            "formatted",
            "graph",
            "resolved",
            "types",
            "result",
            "testRun",
            "targetRun"
        ]
    );
    let [diagnostic] = expect.diagnostics.as_slice() else {
        panic!("one diagnostic");
    };
    assert_eq!(diagnostic.code.as_atom(), "@syntax.unmatchedDelimiter");
    assert_eq!(diagnostic.source_id.as_deref(), Some("input.vibon"));
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.notes, ["a note"]);
    assert_eq!(diagnostic.fixes, Some(Vec::new()));
}

#[test]
fn the_host_event_key_is_camel_case() {
    let text = format!(
        "{}hostEvent = \"@runtime.memoryExhausted\"\n",
        manifest_of_operation("interpret")
    );
    let case = CaseManifest::from_str(&text).expect("a valid manifest");
    assert_eq!(
        case.expectations.host_event.map(|code| code.as_atom()),
        Some("@runtime.memoryExhausted")
    );
    let text = format!(
        "{}host_event = \"@runtime.memoryExhausted\"\n",
        manifest_of_operation("interpret")
    );
    let error =
        CaseManifest::from_str(&text).expect_err("the snake-case key is unknown");
    assert!(
        error.to_string().contains("unknown field `host_event`"),
        "{error}"
    );
}

#[test]
fn a_fix_on_a_code_without_fix_capability_is_rejected() {
    let text = format!(
        "{}\n[[expect.diagnostics]]\ncode = \"@style.casing\"\nlevel = \"@warning\"\n\
         span = {{ start = 0, end = 1 }}\nfixes = [{{ safe = true }}]\n",
        manifest("V2-SRC-READER-x", "readerV2", "reader", "")
    );
    let error = CaseManifest::from_str(&text).expect_err("@none offers no fix");
    assert!(
        error.to_string().contains("fix capability @none"),
        "{error}"
    );
    let text = text.replace("{ safe = true }", "{ safe = true, kind = 1 }");
    let error = CaseManifest::from_str(&text).expect_err("an unknown fix key");
    assert!(error.to_string().contains("kind"), "{error}");
}

#[test]
fn every_verdict_decodes_and_another_is_rejected() {
    for verdict in ["accepted", "refuted", "unknown", "unsupported"] {
        let text = manifest("V2-SRC-READER-x", "readerV2", "reader", "").replace(
            "verdict = \"accepted\"",
            &format!("verdict = \"{verdict}\""),
        );
        let case = CaseManifest::from_str(&text).expect("a verdict");
        assert_eq!(case.expectations.verdict.as_str(), verdict);
    }
    let text = manifest("V2-SRC-READER-x", "readerV2", "reader", "")
        .replace("verdict = \"accepted\"", "verdict = \"passed\"");
    assert!(error_of(&text).contains("unknown verdict `passed`"));
}

#[test]
fn a_level_that_differs_from_the_registry_is_rejected() {
    let text = format!(
        "{}\n[[expect.diagnostics]]\ncode = \"@style.casing\"\nlevel = \"@error\"\n\
         span = {{ start = 0, end = 1 }}\n",
        manifest("V2-SRC-READER-x", "readerV2", "reader", "")
    );
    let error = error_of(&text);
    assert!(
        error.contains("the registry fixes it at @warning"),
        "{error}"
    );
}

#[test]
fn a_code_outside_the_registry_is_rejected() {
    let text = format!(
        "{}\n[[expect.diagnostics]]\ncode = \"@syntax.unmatched-delimiter\"\n\
         level = \"@error\"\nspan = {{ start = 0, end = 1 }}\n",
        manifest("V2-SRC-READER-x", "readerV2", "reader", "")
    );
    assert!(error_of(&text).contains("is not in the registry"));
}

#[test]
fn a_reader_case_declares_exactly_one_document() {
    let two = manifest("V2-SRC-READER-x", "readerV2", "reader", "").replace(
        "data = [\"input.vibon\"]",
        "data = [\"input.vibon\"]\nsource = \"input.vib\"",
    );
    assert!(error_of(&two).contains("exactly one document"));
}

#[test]
fn a_host_world_belongs_only_to_run_and_test_cases() {
    let text = manifest("V2-SRC-READER-x", "readerV2", "reader", "")
        .replace("[inputs]\n", "[inputs]\nhostWorld = \"world.vibon\"\n");
    assert!(error_of(&text).contains("take no hostWorld input"));
    let run = manifest_of_operation("workspaceRun")
        .replace("[inputs]\n", "[inputs]\nhostWorld = \"world.vibon\"\n");
    let case = CaseManifest::from_str(&run).expect("a run case with a world");
    assert_eq!(case.inputs.host_world.as_deref(), Some("world.vibon"));
}

/// A corpus root `cases` with the rule index beside it.
fn corpus_with(
    rule_rows: &[(&str, &str)],
    cases: &[(&str, String)],
) -> std::path::PathBuf {
    let base = scratch_directory("corpus");
    write(&base, "rules.tsv", &rules(rule_rows));
    std::fs::create_dir_all(base.join("cases")).expect("create the corpus root");
    for (directory, manifest) in cases {
        write(&base, &format!("cases/{directory}/case.toml"), manifest);
        write(&base, &format!("cases/{directory}/input.vibon"), "{}");
    }
    base.join("cases")
}

#[test]
fn a_case_under_its_prefix_directory_loads() {
    let id = "V2-SRC-READER-x";
    let root = corpus_with(
        &[(id, "active")],
        &[(&format!("SRC-READER/{id}"), reader_case(id, id))],
    );
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    assert_eq!(corpus.len(), 1);
    let case = &corpus.cases()[0];
    let documents = case.input_documents().expect("the input reads");
    assert_eq!(documents[0].source_id, "input.vibon");
    assert_eq!(documents[0].bytes, b"{}");
}

#[test]
fn a_case_whose_rule_is_retired_or_absent_fails_the_load_naming_the_case() {
    let id = "V2-SRC-READER-x";
    for (rows, problem) in [
        (vec![(id, "retired")], "retired"),
        (vec![("V2-SRC-READER-other", "active")], "does not list"),
    ] {
        let root =
            corpus_with(&rows, &[(&format!("SRC-READER/{id}"), reader_case(id, id))]);
        let error = Corpus::discover(&root).expect_err("the load fails");
        let message = error.to_string();
        assert!(
            matches!(error, CorpusError::InvalidCase { .. }),
            "{message}"
        );
        assert!(
            message.contains(&format!("case `{id}`")) && message.contains(problem),
            "{message}"
        );
    }
}

#[test]
fn a_case_under_the_wrong_prefix_directory_is_a_corpus_error_naming_both() {
    let id = "V2-SRC-READER-x";
    let root = corpus_with(
        &[(id, "active")],
        &[(&format!("PROJECT/{id}"), reader_case(id, id))],
    );
    let message = Corpus::discover(&root)
        .expect_err("the load fails")
        .to_string();
    assert!(
        message.contains("`PROJECT`") && message.contains("`SRC-READER`"),
        "{message}"
    );
}

#[test]
fn a_case_directory_must_be_named_by_its_identity() {
    let id = "V2-SRC-READER-x";
    let root = corpus_with(
        &[(id, "active")],
        &[("SRC-READER/V2-SRC-READER-y", reader_case(id, id))],
    );
    let message = Corpus::discover(&root)
        .expect_err("the load fails")
        .to_string();
    assert!(message.contains("must equal the case id"), "{message}");
}

#[test]
fn a_case_two_levels_deep_is_found_and_a_stray_entry_is_an_error() {
    let id = "V2-SRC-READER-x";
    let root = corpus_with(
        &[(id, "active")],
        &[(&format!("SRC-READER/{id}"), reader_case(id, id))],
    );
    write(&root, "README.md", "stray");
    let message = Corpus::discover(&root)
        .expect_err("the load fails")
        .to_string();
    assert!(message.contains("only prefix directories"), "{message}");

    let root = corpus_with(&[(id, "active")], &[]);
    std::fs::create_dir_all(root.join("SRC-READER").join(id)).expect("an empty case");
    let message = Corpus::discover(&root)
        .expect_err("the load fails")
        .to_string();
    assert!(message.contains("has no case.toml"), "{message}");
}

#[test]
fn an_empty_corpus_root_loads_as_empty_without_a_rule_index() {
    let root = scratch_directory("empty-corpus");
    let corpus = Corpus::discover(&root).expect("an empty corpus");
    assert!(corpus.is_empty());
}

#[test]
fn the_rule_index_decodes_its_header_and_rows() {
    let index = RuleIndex::parse(&rules(&[
        ("V2-SRC-READER-x", "active"),
        ("V1-PROJECT-y", "retired"),
    ]))
    .expect("a valid index");
    assert_eq!(index.len(), 2);
    assert_eq!(
        index.status("V1-PROJECT-y").map(|status| status.as_str()),
        Some("retired")
    );
    assert!(RuleIndex::parse("identity\tstatus\n").is_err());
    assert!(
        RuleIndex::parse("identity\tstatus\tanchor\nV2-SRC-READER-x\tlive\ta\n")
            .is_err()
    );
    assert!(
        RuleIndex::parse("identity\tstatus\tanchor\nV2-SRC-READER-x\tactive\ta\nV2-SRC-READER-x\tactive\ta\n")
            .is_err()
    );
    assert!(
        RuleIndex::parse("identity\tstatus\tanchor\nV2-SRC-x\tactive\ta\n").is_err()
    );
}

#[test]
fn the_committed_rule_index_has_its_header_and_decodes() {
    let path = vibra_conformance::workspace_root().join("conformance/rules.tsv");
    let text = std::fs::read_to_string(&path).expect("the rule index exists");
    assert!(text.starts_with("identity\tstatus\tanchor\n"), "{text:?}");
    RuleIndex::parse(&text).expect("it decodes");
}

/// The repository root, two levels above this crate.
fn repository() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 06 "Host world": the shipped launcher has no user-selectable world, and
/// conformance never launches on the process.
#[test]
fn the_handler_constructs_no_system_launch() {
    let mut pending =
        vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut searched = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("a source directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file");
                assert!(!text.contains("Launch::System"), "{}", path.display());
                searched += 1;
            }
        }
    }
    assert!(searched > 0);
}

/// 07 "Persistent observations": the hello run against its hand-written
/// observation matches, and one changed byte of stdout is a mismatch that
/// names `stdout`.
#[test]
fn the_target_run_observation_is_compared_field_by_field() {
    use vibra_workspace::run::{Launch, run_target};
    use vibra_workspace::{
        HostWorld, MemoryBudget, SourceSnapshot, discover_project_at,
    };

    let case = repository().join("conformance/cases/RUNTIME/V2-RUNTIME-run-hello");
    let snapshot = discover_project_at(case.join("tree"))
        .and_then(SourceSnapshot::capture)
        .expect("the tree captures");
    let world = HostWorld::default();
    let observed = run_target(
        &snapshot,
        None,
        Launch::World(&world),
        MemoryBudget::DEFAULT,
    )
    .document();
    let path = "target-run.vibon";
    let expected = std::fs::read_to_string(case.join(path)).expect("the observation");
    assert_eq!(
        vibra_conformance::compare_target_run(path, expected.as_bytes(), &observed),
        Ok(())
    );
    let changed = expected.replacen("values: [104u8", "values: [105u8", 1);
    assert_ne!(changed, expected);
    let mismatch =
        vibra_conformance::compare_target_run(path, changed.as_bytes(), &observed)
            .expect_err("a mismatch");
    assert!(mismatch.contains("`stdout`"), "{mismatch}");
}
