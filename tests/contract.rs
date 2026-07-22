//! CLI-surface contract tests (tasks.md's `tests/contract/` — Cargo only
//! auto-discovers top-level `tests/*.rs` files, so this is the one entry
//! point declaring each file under `contract/` as a submodule).

#[path = "support.rs"]
mod support;

#[path = "contract/test_error_paths.rs"]
mod test_error_paths;
#[path = "contract/test_model_unknown.rs"]
mod test_model_unknown;
#[path = "contract/test_no_standalone_remove.rs"]
mod test_no_standalone_remove;
#[path = "contract/test_progress_suppressed_errors.rs"]
mod test_progress_suppressed_errors;

#[cfg(feature = "integration")]
#[path = "contract/test_json_output.rs"]
mod test_json_output;
#[cfg(feature = "integration")]
#[path = "contract/test_list_models.rs"]
mod test_list_models;
#[cfg(feature = "integration")]
#[path = "contract/test_model_selection.rs"]
mod test_model_selection;
#[cfg(feature = "integration")]
#[path = "contract/test_progress_env_var.rs"]
mod test_progress_env_var;
#[cfg(feature = "integration")]
#[path = "contract/test_progress_non_interactive.rs"]
mod test_progress_non_interactive;
#[cfg(feature = "integration")]
#[path = "contract/test_progress_quick_feedback.rs"]
mod test_progress_quick_feedback;
#[cfg(feature = "integration")]
#[path = "contract/test_progress_stdout_untouched.rs"]
mod test_progress_stdout_untouched;
#[cfg(feature = "integration")]
#[path = "contract/test_progress_suppressed.rs"]
mod test_progress_suppressed;
#[cfg(feature = "integration")]
#[path = "contract/test_srt_output.rs"]
mod test_srt_output;
#[cfg(feature = "integration")]
#[path = "contract/test_stdout_contract.rs"]
mod test_stdout_contract;
