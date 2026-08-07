//! Ordered compiler stages whose security-sensitive suffix is explicit.
//!
//! The current backend has no layout-changing hardening transformation. The
//! terminal marker is still part of the pipeline contract so a future
//! hardening pass cannot silently be inserted before a later compiler stage.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationPass {
    Reachability,
    WasmEmission,
    Hardening,
}

impl CompilationPass {
    const fn is_hardening(self) -> bool {
        matches!(self, Self::Hardening)
    }
}

/// The backend's pass order. Hardening is a terminal stage by contract.
pub const COMPILATION_PASSES: &[CompilationPass] = &[
    CompilationPass::Reachability,
    CompilationPass::WasmEmission,
    CompilationPass::Hardening,
];

/// Return whether every hardening pass is in the final contiguous suffix.
pub fn hardening_passes_are_last(passes: &[CompilationPass]) -> bool {
    let mut hardening_seen = false;
    for pass in passes {
        if pass.is_hardening() {
            hardening_seen = true;
        } else if hardening_seen {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_pipeline_keeps_hardening_last() {
        assert!(hardening_passes_are_last(COMPILATION_PASSES));
    }

    #[test]
    fn non_hardening_stage_after_hardening_is_rejected() {
        assert!(!hardening_passes_are_last(&[
            CompilationPass::Hardening,
            CompilationPass::WasmEmission,
        ]));
    }
}
