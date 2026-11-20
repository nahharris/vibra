# X2-01 remainder

Status: work list of package
[X2-01](plan-v0.0.2.md#x2-01--rebaseline-against-the-new-contracts).

The native track is compiled against `vibra-diagnostics-v1`,
`vibra-syntax-v1`, `vibra-ir-v1`, `vibra-host-v1`, and `vibra-native-v1`,
which stay in the workspace until X2-01 moves it to the new crates. Its
integration tests lived in `vibra-conformance-v1`, where they built checked
IR with the v0.0.1 checker from v0.0.1 source. That checker, its source
surface, and its corpus are not in the repository, so these tests cannot be
re-pointed mechanically, and they are not in the workspace. X2-01 writes
their successors on the checked IR of `vibra-types`, or records why a row
has none.

## Executor columns

The v0.0.1 runner compared two executors with its interpreter on every
executable case of its corpus: the reference stepper (the `native` column)
and the linked Windows executable (the `windows` column). The runner of the
new corpus has no executor column; X2-04 adds the native one with the parity
inventory of the new corpus.

## Tests

The first seven files hold the native track's tests; the parity inventory of
the new corpus is X2-04's. The last three tested `vibra-native-v1` and
`vibra-host-v1` through the v0.0.1 interpreter, its formatter, and its corpus
loader.

| File | Test |
| --- | --- |
| `crates/vibra-conformance-v1/src/native_exe.rs` | `a_completed_run_is_read_from_its_dump` |
| `crates/vibra-conformance-v1/src/native_exe.rs` | `a_truncated_dump_is_unobserved_never_matched` |
| `crates/vibra-conformance-v1/src/native_read.rs` | `the_two_sources_read_the_same_value` |
| `crates/vibra-conformance-v1/src/native_read.rs` | `a_dump_that_was_cut_off_is_not_observable` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `a_linked_demo_runs_as_an_exe` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `linked_programs_agree_with_the_stepper` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `linked_hand_built_programs_agree_with_the_stepper` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `the_output_path_may_contain_spaces_and_non_ascii` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `a_link_failure_reports_the_command_and_the_linker_output` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `the_link_command_is_the_recorded_one` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `the_entry_outcome_is_computed_from_the_entry_result_type` |
| `crates/vibra-conformance-v1/tests/it/linked_exe.rs` | `the_slice_cannot_express_an_err_entry_yet` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `literals_are_scalars_constants_and_bool_enums` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_sequence_evaluates_in_order_and_drops_what_it_discards` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `variables_are_read_from_parameters_and_bindings` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_let_binds_for_the_rest_of_the_body_and_a_discard_is_dropped` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `an_if_evaluates_only_the_selected_arm` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_direct_call_passes_its_operands_and_returns_its_result` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `an_indirect_call_evaluates_its_callee_once_before_its_operands` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_module_function_used_as_a_value_is_called_through_its_wrapper` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_closure_reads_its_captures_and_its_parameters` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `return_leaves_the_activation_and_releases_what_it_holds` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_module_value_is_evaluated_once_on_its_first_read` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_host_operation_happens_once_with_its_operands_borrowed` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_host_call_carries_the_origin_of_its_source_span` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_tail_call_lowers_to_a_tail_transfer_for_every_kind_of_callee` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_hundred_thousand_tail_calls_run_in_bounded_frames_and_live_size` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_hundred_thousand_tail_calls_through_a_function_value_stay_bounded` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `a_form_outside_the_slice_is_not_lowered_and_names_the_form` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `every_lowered_program_verifies` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `lowering_is_deterministic` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `frames_are_as_small_as_a_simple_allocator_gives` |
| `crates/vibra-conformance-v1/tests/it/lowering_slice.rs` | `code_that_cannot_run_is_lowered_and_discarded` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `the_committed_fixture_is_the_template_at_one_thousand_rounds` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `the_fixture_case_of_the_corpus_is_the_example` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `the_division_variant_differs_from_the_fixture_only_in_the_test_of_its_loop` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `the_linked_fixture_equals_the_interpreter_on_result_trap_origin_and_trace` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `quota_one_has_one_stdout_event_and_yields_at_every_checkpoint` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `peak_frames_and_live_size_do_not_grow_with_the_loop_count` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `a_program_mutated_after_verification_is_rejected_by_verify` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `an_object_with_another_abi_is_refused_by_the_launcher` |
| `crates/vibra-conformance-v1/tests/it/m1_evidence.rs` | `a_descriptor_naming_an_unknown_symbol_is_refused` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `every_operation_of_every_integer_type_agrees_across_the_executors` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `the_written_meaning_agrees_with_the_shared_vectors_on_the_edges` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `compare_answers_less_equal_and_greater_in_every_executor` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `each_operation_lowers_balanced_and_answers_in_both_executors` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `division_and_remainder_test_the_divisor_and_trap_with_no_origin` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `a_zero_divisor_traps_in_every_executor_for_every_type` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `the_checker_still_rejects_what_has_no_such_operation` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `the_other_numeric_operations_are_not_lowered_and_name_the_form` |
| `crates/vibra-conformance-v1/tests/it/unchecked_integers.rs` | `the_runtime_names_each_operation_as_the_execution_ir_does` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `the_table_names_every_executable_case_once` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `a_pending_cell_names_a_package_of_the_plan` (ignored) |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `every_form_the_lowering_refuses_has_an_owning_package` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `the_checking_logic_reports_a_wrong_cell_and_its_reason` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `the_cells_have_two_forms_and_no_third` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `the_windows_checking_logic_reports_a_wrong_cell_and_a_row_that_should_move` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `every_windows_row_matches_its_linked_executable` |
| `crates/vibra-conformance-v1/tests/it/native_parity_inventory.rs` | `a_cell_that_matches_but_is_marked_pending_fails` |
| `crates/vibra-conformance-v1/tests/it/natives.rs` | `every_arm_of_the_interpreters_native_dispatch_calls_the_native_code` |
| `crates/vibra-conformance-v1/tests/it/natives.rs` | `no_native_has_a_second_implementation_in_the_interpreter` |
| `crates/vibra-conformance-v1/tests/it/natives.rs` | `the_manifest_native_list_is_exactly_the_registry_natives` |
| `crates/vibra-conformance-v1/tests/it/natives.rs` | `the_import_table_names_the_native_imports_and_the_looping_primitive_rows` |
| `crates/vibra-conformance-v1/tests/it/natives.rs` | `a_manifest_that_lists_a_native_the_toolchain_does_not_implement_is_a_provenance_defect` |
| `crates/vibra-conformance-v1/tests/it/host_documents_fmt.rs` | `the_default_world_and_a_full_world_are_canonical` |
| `crates/vibra-conformance-v1/tests/it/host_documents_fmt.rs` | `an_audit_trace_is_canonical` |
| `crates/vibra-conformance-v1/tests/it/host_documents_fmt.rs` | `the_io_module_is_canonical_source` |
| `crates/vibra-conformance-v1/tests/it/host_documents_fmt.rs` | `the_standard_library_manifest_is_canonical_and_the_formatter_is_idempotent_over_it` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `a_workspace_case_may_name_a_host_world_and_the_loader_keeps_the_path` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `only_workspace_run_and_workspace_test_take_a_host_world` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `the_world_path_is_a_relative_vibon_file_of_the_case` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `a_case_with_a_valid_world_loads_a_fresh_copy_each_time` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `a_malformed_world_is_reported_once_per_diagnostic_at_the_document` |
| `crates/vibra-conformance-v1/tests/it/host_world_corpus.rs` | `a_malformed_world_fails_its_case_and_not_the_corpus` |
