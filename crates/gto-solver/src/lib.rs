#![forbid(unsafe_code)]
#![doc = "Portable solver interfaces and strategy infrastructure."]

mod abstraction;
mod cfr;
mod kuhn;
mod river;
mod tree;

use gto_core::{CoreBuildInfo, HoldemHandState, HoldemStateError, PlayerAction, build_info as core_build_info};

pub use abstraction::{
    AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, PublicStateKey,
    RaiseSize, StreetProfile, abstract_actions,
};
pub use cfr::{
    CfrCheckpoint, CfrCheckpointError, CfrInfoSetCheckpoint, CfrPlusSolver,
    ExtensiveGameState, GameNode,
};
pub use kuhn::{KuhnAction, KuhnCard, KuhnInfoSet, KuhnState};
pub use river::{
    RiverActionProbability, RiverArtifactError, RiverCheckpointError, RiverSolveError,
    RiverSolverResult, RiverStrategyArtifact, RiverStrategyEntry, RiverTrainingCheckpoint,
    RiverTrainingError, RiverTrainingProfile, RiverTrainingSession, ScriptedRiverSpot,
    solve_river_spot,
};
pub use tree::{PublicTree, PublicTreeEdge, PublicTreeNode, PublicTreeNodeKind, build_public_tree};

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

/// Deterministic placeholder bot used to validate CLI and crate boundaries before solver logic exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StubBot;

impl StubBot {
    pub fn choose_action(self, state: &HoldemHandState) -> Result<PlayerAction, StubBotError> {
        let legal = state.legal_actions().map_err(StubBotError::State)?;

        if legal.check {
            return Ok(PlayerAction::Check);
        }
        if legal.call_amount.is_some() {
            return Ok(PlayerAction::Call);
        }
        if let Some(range) = legal.bet_range {
            return Ok(PlayerAction::BetTo(range.min_total));
        }
        if let Some(range) = legal.raise_range {
            return Ok(PlayerAction::RaiseTo(range.min_total));
        }
        if legal.all_in_to.is_some() {
            return Ok(PlayerAction::AllIn);
        }
        if legal.fold {
            return Ok(PlayerAction::Fold);
        }

        Err(StubBotError::NoLegalAction)
    }
}

#[derive(Debug)]
pub enum StubBotError {
    NoLegalAction,
    State(HoldemStateError),
}

impl std::fmt::Display for StubBotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLegalAction => formatter.write_str("stub bot could not find a legal action"),
            Self::State(error) => write!(formatter, "stub bot could not inspect state: {error}"),
        }
    }
}

impl std::error::Error for StubBotError {}

#[cfg(test)]
mod tests {
    use super::{
        CfrPlusSolver, KuhnAction, KuhnCard, KuhnInfoSet, KuhnState, SolverBuildInfo,
        SolverProfile, StubBot, build_info,
    };
    use gto_core::{HoldemConfig, HoldemHandState, PlayerAction};

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

    #[test]
    fn stub_bot_checks_when_it_can() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();

        assert_eq!(StubBot.choose_action(&state).unwrap(), PlayerAction::Check);
    }

    #[test]
    fn stub_bot_calls_when_facing_a_bet() {
        let mut state = HoldemHandState::new(
            HoldemConfig::new(250, 50, 100).unwrap(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        assert_eq!(StubBot.choose_action(&state).unwrap(), PlayerAction::Call);
    }

    #[test]
    fn kuhn_cfr_converges_to_the_known_game_value() {
        let mut solver = CfrPlusSolver::new(KuhnState::new());
        solver.train_iterations(20_000);

        let value = solver.expected_value()[0];
        assert!((value - (-1.0 / 18.0)).abs() < 0.03, "unexpected value {value}");
    }

    #[test]
    fn kuhn_average_strategy_is_normalized() {
        let mut solver = CfrPlusSolver::new(KuhnState::new());
        solver.train_iterations(5_000);

        let infoset = KuhnInfoSet {
            player: 0,
            private_card: KuhnCard::King,
            history: Vec::new(),
        };
        let strategy = solver.average_strategy(&infoset).unwrap();
        let probability_sum = strategy.iter().map(|(_, probability)| probability).sum::<f64>();

        assert!((probability_sum - 1.0).abs() < 1e-9);
        assert!(strategy.iter().all(|(_, probability)| probability.is_finite()));
        assert_eq!(
            strategy.iter().map(|(action, _)| *action).collect::<Vec<_>>(),
            vec![KuhnAction::Check, KuhnAction::Bet]
        );
    }
}
