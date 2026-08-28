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

/// The sequence recorded by the actual compiler entry point.
#[derive(Debug, Default)]
pub struct CompilationPipeline {
    passes: Vec<CompilationPass>,
}

impl CompilationPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run one real compiler stage and record it in the same sequence used by
    /// the hardening-order invariant. A non-hardening stage after hardening is
    /// rejected before its body executes.
    pub fn run<T>(&mut self, pass: CompilationPass, stage: impl FnOnce() -> T) -> T {
        assert!(
            !self
                .passes
                .last()
                .is_some_and(|previous| previous.is_hardening() && !pass.is_hardening()),
            "compiler stage {pass:?} follows terminal hardening"
        );
        self.passes.push(pass);
        stage()
    }

    pub fn passes(&self) -> &[CompilationPass] {
        &self.passes
    }
}

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
    fn actual_pipeline_rejects_a_stage_after_hardening() {
        let mut pipeline = CompilationPipeline::new();
        pipeline.run(CompilationPass::Reachability, || {});
        pipeline.run(CompilationPass::WasmEmission, || {});
        pipeline.run(CompilationPass::Hardening, || {});

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline.run(CompilationPass::WasmEmission, || {});
        }));
        assert!(result.is_err());
    }

    #[test]
    fn configured_pipeline_keeps_hardening_last() {
        let mut pipeline = CompilationPipeline::new();
        pipeline.run(CompilationPass::Reachability, || {});
        pipeline.run(CompilationPass::WasmEmission, || {});
        pipeline.run(CompilationPass::Hardening, || {});
        assert!(hardening_passes_are_last(pipeline.passes()));
    }

    #[test]
    fn non_hardening_stage_after_hardening_is_rejected() {
        assert!(!hardening_passes_are_last(&[
            CompilationPass::Hardening,
            CompilationPass::WasmEmission,
        ]));
    }
}
