#![forbid(unsafe_code)]
#![doc = "Portable building blocks for exact poker rules and domain types."]

/// Static build metadata that can be shared across frontends and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreBuildInfo {
    pub crate_name: &'static str,
    pub crate_version: &'static str,
    pub wasm_safe: bool,
}

/// Returns immutable metadata about the current core crate build.
pub const fn build_info() -> CoreBuildInfo {
    CoreBuildInfo {
        crate_name: env!("CARGO_PKG_NAME"),
        crate_version: env!("CARGO_PKG_VERSION"),
        wasm_safe: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreBuildInfo, build_info};

    #[test]
    fn build_info_matches_crate_metadata() {
        assert_eq!(
            build_info(),
            CoreBuildInfo {
                crate_name: "gto-core",
                crate_version: env!("CARGO_PKG_VERSION"),
                wasm_safe: true,
            }
        );
    }
}
