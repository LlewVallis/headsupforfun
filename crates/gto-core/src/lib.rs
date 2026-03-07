#![forbid(unsafe_code)]
#![doc = "Portable building blocks for exact poker rules and domain types."]

mod cards;
mod deck;
mod hand_eval;
mod holdem;
mod range;
mod rng;

pub use cards::{
    Board, BoardError, Card, CardMask, DuplicateCardError, HoleCards, ParseCardError, Rank,
    Suit,
};
pub use deck::Deck;
pub use hand_eval::{
    EvaluateHandError, HandCategory, HandRank, HeadsUpPayout, OddChipRecipient, ShowdownError,
    ShowdownResult, award_pot_heads_up, evaluate_five, evaluate_seven, resolve_holdem_showdown,
};
pub use holdem::{
    Chips, HandOutcome, HandPhase, HistoryEvent, HoldemConfig, HoldemConfigError,
    HoldemHandState, HoldemStateError, LegalActions, Player, PlayerAction, PlayerSnapshot,
    Street, WagerRange,
};
pub use range::{ParseRangeError, Range};
pub use rng::{DEFAULT_RNG_SEED, DeterministicRng, default_rng, rng_from_seed};

/// Static build metadata that can be shared across frontends and tests.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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
