#![forbid(unsafe_code)]

fn main() {
    println!("{}", startup_banner());
}

fn startup_banner() -> String {
    let build = gto_solver::build_info();
    let profile = gto_solver::SolverProfile::placeholder();

    format!(
        "{name} {version}\nstatus: milestone M0 workspace bootstrap\nsolver-profile: {profile}\nwasm-safe-core: {wasm_safe}",
        name = build.crate_name,
        version = build.crate_version,
        profile = profile.name(),
        wasm_safe = build.wasm_safe && build.core.wasm_safe,
    )
}

#[cfg(test)]
mod tests {
    use super::startup_banner;

    #[test]
    fn startup_banner_mentions_bootstrap_state() {
        let banner = startup_banner();

        assert!(banner.contains("gto-solver"));
        assert!(banner.contains("milestone M0 workspace bootstrap"));
        assert!(banner.contains("bootstrap-placeholder"));
        assert!(banner.contains("wasm-safe-core: true"));
    }
}
