#![forbid(unsafe_code)]
#![doc = "Portable solver interfaces and strategy infrastructure."]

use gto_core::{CoreBuildInfo, build_info as core_build_info};

/// Static build metadata for the solver crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolverBuildInfo {
    pub crate_name: &'static str,
    pub crate_version: &'static str,
    pub wasm_safe: bool,
    pub parallel_feature_enabled: bool,
    pub core: CoreBuildInfo,
}

/// Returns immutable metadata about the current solver crate build.
pub const fn build_info() -> SolverBuildInfo {
    SolverBuildInfo {
        crate_name: env!("CARGO_PKG_NAME"),
        crate_version: env!("CARGO_PKG_VERSION"),
        wasm_safe: true,
        parallel_feature_enabled: cfg!(feature = "parallel"),
        core: core_build_info(),
    }
}

/// Minimal placeholder bot profile used while the solver stack is being built out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolverProfile {
    name: &'static str,
}

impl SolverProfile {
    pub const fn placeholder() -> Self {
        Self {
            name: "bootstrap-placeholder",
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::{SolverBuildInfo, SolverProfile, build_info};

    #[test]
    fn build_info_exposes_core_metadata() {
        assert_eq!(
            build_info(),
            SolverBuildInfo {
                crate_name: "gto-solver",
                crate_version: env!("CARGO_PKG_VERSION"),
                wasm_safe: true,
                parallel_feature_enabled: false,
                core: gto_core::build_info(),
            }
        );
    }

    #[test]
    fn placeholder_profile_has_stable_name() {
        assert_eq!(SolverProfile::placeholder().name(), "bootstrap-placeholder");
    }
}
