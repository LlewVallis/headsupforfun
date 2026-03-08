use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gto_core::{HistoryEvent, HoldemHandState, Player, PlayerAction, Range, Street};

use crate::{
    AbstractionProfile, BlueprintBot, BlueprintBotError, FullHandBlueprintArtifact,
    HoldemInfoSetKey, OpeningSize, RaiseSize, ScriptedFlopSpot, ScriptedRiverSpot,
    ScriptedTurnSpot, StubBot, solve_flop_spot, solve_river_spot, solve_turn_spot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostflopSolverBotConfig {
    pub profile: AbstractionProfile,
    pub button_base_range: Range,
    pub big_blind_base_range: Range,
    pub max_opponent_combos: usize,
    pub flop_iterations: u64,
    pub turn_iterations: u64,
    pub river_iterations: u64,
}

impl PostflopSolverBotConfig {
    pub fn smoke_default() -> Self {
        let postflop = crate::StreetProfile {
            opening_sizes: vec![
                OpeningSize::PotFractionBps(3_300),
                OpeningSize::PotFractionBps(6_600),
                OpeningSize::PotFractionBps(10_000),
            ],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
            include_all_in: true,
        };

        Self {
            profile: AbstractionProfile::new(
                crate::StreetProfile {
                    opening_sizes: vec![
                        OpeningSize::BigBlindMultipleBps(25_000),
                        OpeningSize::BigBlindMultipleBps(40_000),
                        OpeningSize::BigBlindMultipleBps(70_000),
                    ],
                    raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
                    include_all_in: true,
                },
                postflop.clone(),
                postflop.clone(),
                postflop,
            ),
            button_base_range: "22+,A2s+,K9s+,QTs+,JTs,T9s,98s,87s,AJo+,KQo"
                .parse()
                .expect("default button range should parse"),
            big_blind_base_range: "22+,A2s+,K8s+,Q9s+,J9s+,T9s,98s,87s,76s,A9o+,KTo+,QTo+,JTo"
                .parse()
                .expect("default big blind range should parse"),
            max_opponent_combos: 2,
            flop_iterations: 0,
            turn_iterations: 1,
            river_iterations: 1,
        }
    }

    pub fn from_blueprint_profile(
        blueprint_profile: AbstractionProfile,
        postflop_profile: HybridPostflopProfile,
    ) -> Self {
        let defaults = Self::smoke_default();
        let (profile, max_opponent_combos, flop_iterations, turn_iterations, river_iterations) =
            match postflop_profile {
                HybridPostflopProfile::Fast => {
                    (defaults.profile.clone(), 2, 0, 1, 2)
                }
                HybridPostflopProfile::Play => {
                    let preflop = blueprint_profile.for_street(Street::Preflop).clone();
                    let postflop = crate::StreetProfile {
                        opening_sizes: vec![
                            OpeningSize::PotFractionBps(3_300),
                            OpeningSize::PotFractionBps(10_000),
                        ],
                        raise_sizes: vec![],
                        include_all_in: true,
                    };
                    (
                        AbstractionProfile::new(
                            preflop,
                            postflop.clone(),
                            postflop.clone(),
                            postflop,
                        ),
                        1,
                        1,
                        1,
                        4,
                    )
                }
            };

        Self {
            profile,
            button_base_range: defaults.button_base_range,
            big_blind_base_range: defaults.big_blind_base_range,
            max_opponent_combos,
            flop_iterations,
            turn_iterations,
            river_iterations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostflopSolverBot {
    config: PostflopSolverBotConfig,
}

impl Default for PostflopSolverBot {
    fn default() -> Self {
        Self::new(PostflopSolverBotConfig::smoke_default())
    }
}

impl PostflopSolverBot {
    pub const fn new(config: PostflopSolverBotConfig) -> Self {
        Self { config }
    }

    pub const fn config(&self) -> &PostflopSolverBotConfig {
        &self.config
    }

    pub const fn uses_runtime_solver_for_street(&self, street: Street) -> bool {
        match street {
            Street::Preflop => false,
            Street::Flop => self.config.flop_iterations > 0,
            Street::Turn => self.config.turn_iterations > 0,
            Street::River => self.config.river_iterations > 0,
        }
    }

    pub fn choose_action(
        &self,
        bot_player: Player,
        state: &HoldemHandState,
    ) -> Result<PlayerAction, PostflopSolverBotError> {
        self.choose_action_with_fallback(bot_player, state, SolverFallbackMode::Stub)
    }

    pub fn try_choose_action(
        &self,
        bot_player: Player,
        state: &HoldemHandState,
    ) -> Result<PlayerAction, PostflopSolverBotError> {
        self.choose_action_with_fallback(bot_player, state, SolverFallbackMode::Error)
    }

    fn choose_action_with_fallback(
        &self,
        bot_player: Player,
        state: &HoldemHandState,
        fallback_mode: SolverFallbackMode,
    ) -> Result<PlayerAction, PostflopSolverBotError> {
        if state.current_actor() != Some(bot_player) {
            return Err(PostflopSolverBotError::NotActorsTurn {
                expected: state.current_actor(),
                actual: bot_player,
            });
        }

        if state.street() == Street::Preflop {
            return self.fallback_or_error(
                PostflopSolverBotError::UnsupportedStreetState(Street::Preflop),
                state,
                fallback_mode,
            );
        }

        if !self.uses_runtime_solver_for_street(state.street()) {
            return self.fallback_or_error(
                PostflopSolverBotError::RuntimeSolveDisabled(state.street()),
                state,
                fallback_mode,
            );
        }

        let bot_hole_cards = state.player(bot_player).hole_cards;
        let opponent_player = bot_player.opponent();
        let Some(opponent_range) = self.opponent_range(opponent_player, state, bot_hole_cards) else {
            return self.fallback_or_error(
                PostflopSolverBotError::OpponentRangeEmpty,
                state,
                fallback_mode,
            );
        };
        let bot_range = Range::from_hole_cards([bot_hole_cards]);
        let (button_range, big_blind_range) = if bot_player == Player::Button {
            (bot_range, opponent_range)
        } else {
            (opponent_range, bot_range)
        };
        let infoset = HoldemInfoSetKey::from_state(bot_player, bot_hole_cards, state, Vec::new());

        let action = match state.street() {
            Street::Flop => {
                let spot = scripted_flop_spot_from_state(state)?;
                let result = solve_flop_spot(
                    spot,
                    button_range,
                    big_blind_range,
                    self.config.profile.clone(),
                    self.config.flop_iterations,
                )
                .map_err(PostflopSolverBotError::FlopSolve)?;
                result.choose_action_max(&infoset)
            }
            Street::Turn => {
                let spot = scripted_turn_spot_from_state(state)?;
                let result = solve_turn_spot(
                    spot,
                    button_range,
                    big_blind_range,
                    self.config.profile.clone(),
                    self.config.turn_iterations,
                )
                .map_err(PostflopSolverBotError::TurnSolve)?;
                result.choose_action_max(&infoset)
            }
            Street::River => {
                let spot = scripted_river_spot_from_state(state)?;
                let result = solve_river_spot(
                    spot,
                    button_range,
                    big_blind_range,
                    self.config.profile.clone(),
                    self.config.river_iterations,
                )
                .map_err(PostflopSolverBotError::RiverSolve)?;
                result.choose_action_max(&infoset)
            }
            Street::Preflop => unreachable!("preflop should have returned above"),
        }
        .ok_or(PostflopSolverBotError::NoStrategyAction)?;

        Ok(action.to_player_action())
    }

    fn fallback_or_error(
        &self,
        reason: PostflopSolverBotError,
        state: &HoldemHandState,
        fallback_mode: SolverFallbackMode,
    ) -> Result<PlayerAction, PostflopSolverBotError> {
        match fallback_mode {
            SolverFallbackMode::Stub => {
                StubBot.choose_action(state).map_err(PostflopSolverBotError::Fallback)
            }
            SolverFallbackMode::Error => Err(reason),
        }
    }

    fn opponent_range(
        &self,
        opponent_player: Player,
        state: &HoldemHandState,
        bot_hole_cards: gto_core::HoleCards,
    ) -> Option<Range> {
        let base_range = match opponent_player {
            Player::Button => &self.config.button_base_range,
            Player::BigBlind => &self.config.big_blind_base_range,
        };
        let dead_cards = state.board().mask().union(bot_hole_cards.mask());
        let filtered = base_range.without_dead_cards(dead_cards);
        let limited = Range::from_hole_cards(
            filtered
                .iter()
                .copied()
                .take(self.config.max_opponent_combos),
        );
        (!limited.is_empty()).then_some(limited)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridPostflopProfile {
    Fast,
    Play,
}

impl Default for HybridPostflopProfile {
    fn default() -> Self {
        Self::Fast
    }
}

impl HybridPostflopProfile {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Play => "play",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridBotConfig {
    pub blueprint_artifact: FullHandBlueprintArtifact,
    pub postflop_profile: HybridPostflopProfile,
}

impl HybridBotConfig {
    pub const fn new(
        blueprint_artifact: FullHandBlueprintArtifact,
        postflop_profile: HybridPostflopProfile,
    ) -> Self {
        Self {
            blueprint_artifact,
            postflop_profile,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridBot {
    blueprint_bot: BlueprintBot,
    postflop_solver_bot: PostflopSolverBot,
    postflop_profile: HybridPostflopProfile,
}

impl HybridBot {
    pub fn new(config: HybridBotConfig) -> Self {
        let blueprint_bot = BlueprintBot::new(config.blueprint_artifact);
        let postflop_solver_bot = PostflopSolverBot::new(
            PostflopSolverBotConfig::from_blueprint_profile(
                blueprint_bot.profile().clone(),
                config.postflop_profile,
            ),
        );

        Self {
            blueprint_bot,
            postflop_solver_bot,
            postflop_profile: config.postflop_profile,
        }
    }

    pub fn blueprint_profile(&self) -> &AbstractionProfile {
        self.blueprint_bot.profile()
    }

    pub const fn postflop_profile(&self) -> HybridPostflopProfile {
        self.postflop_profile
    }

    pub const fn uses_runtime_solver_for_street(&self, street: Street) -> bool {
        self.postflop_solver_bot.uses_runtime_solver_for_street(street)
    }

    pub fn choose_action(
        &self,
        bot_player: Player,
        state: &HoldemHandState,
    ) -> Result<PlayerAction, HybridBotError> {
        if state.street() == Street::Preflop || !self.uses_runtime_solver_for_street(state.street()) {
            return self
                .blueprint_bot
                .choose_action(bot_player, state)
                .map_err(HybridBotError::BlueprintOnly);
        }

        match self.postflop_solver_bot.try_choose_action(bot_player, state) {
            Ok(action) => Ok(action),
            Err(postflop_error) => self
                .blueprint_bot
                .choose_action(bot_player, state)
                .map_err(|blueprint_error| HybridBotError::PostflopAndBlueprint {
                    postflop: postflop_error,
                    blueprint: blueprint_error,
                }),
        }
    }
}

#[derive(Debug)]
pub enum HybridBotError {
    BlueprintOnly(BlueprintBotError),
    PostflopAndBlueprint {
        postflop: PostflopSolverBotError,
        blueprint: BlueprintBotError,
    },
}

impl Display for HybridBotError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlueprintOnly(error) => write!(formatter, "{error}"),
            Self::PostflopAndBlueprint { postflop, blueprint } => write!(
                formatter,
                "hybrid bot could not act; runtime postflop solve failed with `{postflop}` and blueprint fallback failed with `{blueprint}`"
            ),
        }
    }
}

impl Error for HybridBotError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SolverFallbackMode {
    Stub,
    Error,
}

fn scripted_flop_spot_from_state(
    state: &HoldemHandState,
) -> Result<ScriptedFlopSpot, PostflopSolverBotError> {
    let board = state.board().cards();
    if board.len() < 3 {
        return Err(PostflopSolverBotError::UnsupportedStreetState(state.street()));
    }

    Ok(ScriptedFlopSpot {
        config: state.config(),
        button_starting_stack: Some(state.starting_stack(Player::Button)),
        big_blind_starting_stack: Some(state.starting_stack(Player::BigBlind)),
        preflop_actions: actions_for_street(state, Street::Preflop),
        flop: [board[0], board[1], board[2]],
        flop_prefix_actions: actions_for_street(state, Street::Flop),
    })
}

fn scripted_turn_spot_from_state(
    state: &HoldemHandState,
) -> Result<ScriptedTurnSpot, PostflopSolverBotError> {
    let board = state.board().cards();
    if board.len() < 4 {
        return Err(PostflopSolverBotError::UnsupportedStreetState(state.street()));
    }

    Ok(ScriptedTurnSpot {
        config: state.config(),
        button_starting_stack: Some(state.starting_stack(Player::Button)),
        big_blind_starting_stack: Some(state.starting_stack(Player::BigBlind)),
        preflop_actions: actions_for_street(state, Street::Preflop),
        flop: [board[0], board[1], board[2]],
        flop_actions: actions_for_street(state, Street::Flop),
        turn: board[3],
        turn_prefix_actions: actions_for_street(state, Street::Turn),
    })
}

fn scripted_river_spot_from_state(
    state: &HoldemHandState,
) -> Result<ScriptedRiverSpot, PostflopSolverBotError> {
    let board = state.board().cards();
    if board.len() < 5 {
        return Err(PostflopSolverBotError::UnsupportedStreetState(state.street()));
    }

    Ok(ScriptedRiverSpot {
        config: state.config(),
        button_starting_stack: Some(state.starting_stack(Player::Button)),
        big_blind_starting_stack: Some(state.starting_stack(Player::BigBlind)),
        preflop_actions: actions_for_street(state, Street::Preflop),
        flop: [board[0], board[1], board[2]],
        flop_actions: actions_for_street(state, Street::Flop),
        turn: board[3],
        turn_actions: actions_for_street(state, Street::Turn),
        river: board[4],
        river_prefix_actions: actions_for_street(state, Street::River),
    })
}

fn actions_for_street(state: &HoldemHandState, street: Street) -> Vec<PlayerAction> {
    state
        .history()
        .iter()
        .filter_map(|event| match event {
            HistoryEvent::ActionApplied {
                street: event_street,
                action,
                ..
            } if *event_street == street => Some(*action),
            _ => None,
        })
        .collect()
}

#[derive(Debug)]
pub enum PostflopSolverBotError {
    OpponentRangeEmpty,
    NoStrategyAction,
    RuntimeSolveDisabled(Street),
    UnsupportedStreetState(Street),
    NotActorsTurn {
        expected: Option<Player>,
        actual: Player,
    },
    FlopSolve(crate::FlopSolveError),
    TurnSolve(crate::TurnSolveError),
    RiverSolve(crate::RiverSolveError),
    Fallback(crate::StubBotError),
}

impl Display for PostflopSolverBotError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpponentRangeEmpty => formatter.write_str("postflop solver bot had no opponent range"),
            Self::NoStrategyAction => formatter.write_str("postflop solver bot had no strategy action"),
            Self::RuntimeSolveDisabled(street) => {
                write!(formatter, "postflop solver bot runtime solving is disabled on {street}")
            }
            Self::UnsupportedStreetState(street) => {
                write!(formatter, "postflop solver bot cannot build a scripted spot for {street}")
            }
            Self::NotActorsTurn { expected, actual } => write!(
                formatter,
                "postflop solver bot expected actor {:?}, got {actual}",
                expected
            ),
            Self::FlopSolve(error) => write!(formatter, "{error}"),
            Self::TurnSolve(error) => write!(formatter, "{error}"),
            Self::RiverSolve(error) => write!(formatter, "{error}"),
            Self::Fallback(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for PostflopSolverBotError {}

#[cfg(test)]
mod tests {
    use gto_core::{Deck, HandPhase, HoldemConfig, HoldemHandState, HoleCards, Player, PlayerAction, default_rng};

    use super::{
        HybridBot, HybridBotConfig, HybridPostflopProfile, PostflopSolverBot,
        PostflopSolverBotConfig, scripted_flop_spot_from_state, scripted_river_spot_from_state,
        scripted_turn_spot_from_state,
    };
    use crate::{AbstractionProfile, OpeningSize, RaiseSize, StreetProfile};

    #[test]
    fn scripted_spot_builders_replay_public_history() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        let flop_spot = scripted_flop_spot_from_state(&state).unwrap();
        assert_eq!(flop_spot.preflop_actions, vec![PlayerAction::Call, PlayerAction::Check]);
        assert_eq!(flop_spot.flop_prefix_actions, vec![PlayerAction::BetTo(100)]);

        state.apply_action(PlayerAction::Call).unwrap();
        state.deal_turn("5s".parse().unwrap()).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();

        let turn_spot = scripted_turn_spot_from_state(&state).unwrap();
        assert_eq!(turn_spot.flop_actions, vec![PlayerAction::BetTo(100), PlayerAction::Call]);
        assert_eq!(turn_spot.turn_prefix_actions, vec![PlayerAction::Check]);

        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_river("7c".parse().unwrap()).unwrap();

        let river_spot = scripted_river_spot_from_state(&state).unwrap();
        assert_eq!(river_spot.turn_actions, vec![PlayerAction::Check, PlayerAction::Check]);
        assert!(river_spot.river_prefix_actions.is_empty());
    }

    #[test]
    fn scripted_spot_builders_preserve_unequal_starting_stacks() {
        let mut state = HoldemHandState::new_with_starting_stacks(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
            4_000,
            10_000,
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();

        let flop_spot = scripted_flop_spot_from_state(&state).unwrap();
        assert_eq!(flop_spot.button_starting_stack, Some(4_000));
        assert_eq!(flop_spot.big_blind_starting_stack, Some(10_000));
    }

    #[test]
    fn postflop_solver_bot_returns_a_legal_action_on_the_flop() {
        let bot = PostflopSolverBot::default();
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        let action = bot.choose_action(gto_core::Player::Button, &state).unwrap();
        state.apply_action(action).unwrap();
    }

    #[test]
    fn postflop_solver_bot_falls_back_when_opponent_range_filters_to_empty() {
        let bot = PostflopSolverBot::new(PostflopSolverBotConfig {
            profile: AbstractionProfile::new(
                StreetProfile {
                    opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
            ),
            button_base_range: "AsAh".parse().unwrap(),
            big_blind_base_range: "QcQh".parse().unwrap(),
            max_opponent_combos: 1,
            flop_iterations: 1,
            turn_iterations: 1,
            river_iterations: 1,
        });
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["Ah".parse().unwrap(), "Ad".parse().unwrap(), "2c".parse().unwrap()])
            .unwrap();

        let action = bot.choose_action(Player::BigBlind, &state).unwrap();
        state.apply_action(action).unwrap();
    }

    #[test]
    fn strict_postflop_solver_reports_empty_opponent_range() {
        let bot = PostflopSolverBot::new(PostflopSolverBotConfig {
            profile: AbstractionProfile::new(
                StreetProfile {
                    opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
                StreetProfile {
                    opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
                    raise_sizes: vec![],
                    include_all_in: false,
                },
            ),
            button_base_range: "AsAh".parse().unwrap(),
            big_blind_base_range: "QcQh".parse().unwrap(),
            max_opponent_combos: 1,
            flop_iterations: 1,
            turn_iterations: 1,
            river_iterations: 1,
        });
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["Ah".parse().unwrap(), "Ad".parse().unwrap(), "2c".parse().unwrap()])
            .unwrap();

        let error = bot.try_choose_action(Player::BigBlind, &state).unwrap_err();
        assert!(matches!(error, super::PostflopSolverBotError::OpponentRangeEmpty));
    }

    #[test]
    fn hybrid_bot_uses_blueprint_preflop_and_runtime_postflop() {
        let hybrid = HybridBot::new(HybridBotConfig::new(
            crate::FullHandBlueprintArtifact::smoke_default(),
            HybridPostflopProfile::Fast,
        ));
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKs".parse().unwrap(),
            "QhJh".parse().unwrap(),
        )
        .unwrap();

        let preflop_action = hybrid.choose_action(Player::Button, &state).unwrap();
        let mut replay = state.clone();
        replay.apply_action(preflop_action).unwrap();

        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKs".parse().unwrap(),
            "QhJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "7d".parse().unwrap(), "Th".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_turn("9c".parse().unwrap()).unwrap();

        let turn_action = hybrid.choose_action(Player::BigBlind, &state).unwrap();
        state.apply_action(turn_action).unwrap();
    }

    #[test]
    fn hybrid_bot_falls_back_to_blueprint_when_runtime_solver_cannot_act() {
        let mut artifact = crate::FullHandBlueprintArtifact::smoke_default();
        artifact.starting_ranges.button_open_limp = "AsKd".parse().unwrap();
        artifact.starting_ranges.big_blind_defend_vs_open = "QcJh".parse().unwrap();
        let hybrid = HybridBot::new(HybridBotConfig::new(
            artifact,
            HybridPostflopProfile::Fast,
        ));
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["Ah".parse().unwrap(), "Ad".parse().unwrap(), "2c".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_turn("3d".parse().unwrap()).unwrap();

        let action = hybrid.choose_action(Player::BigBlind, &state).unwrap();
        state.apply_action(action).unwrap();
    }

    #[test]
    fn hybrid_fast_profile_skips_runtime_flop_solving() {
        let hybrid = HybridBot::new(HybridBotConfig::new(
            crate::FullHandBlueprintArtifact::smoke_default(),
            HybridPostflopProfile::Fast,
        ));
        assert!(!hybrid.uses_runtime_solver_for_street(gto_core::Street::Flop));
        assert!(hybrid.uses_runtime_solver_for_street(gto_core::Street::Turn));
        assert!(hybrid.uses_runtime_solver_for_street(gto_core::Street::River));
    }

    #[test]
    fn postflop_solver_bot_handles_larger_action_profile_on_flop() {
        let profile = AbstractionProfile::new(
            StreetProfile {
                opening_sizes: vec![
                    OpeningSize::BigBlindMultipleBps(25_000),
                    OpeningSize::BigBlindMultipleBps(40_000),
                ],
                raise_sizes: vec![
                    RaiseSize::CurrentBetMultipleBps(25_000),
                    RaiseSize::PotFractionAfterCallBps(10_000),
                ],
                include_all_in: true,
            },
            StreetProfile {
                opening_sizes: vec![
                    OpeningSize::PotFractionBps(3_300),
                    OpeningSize::PotFractionBps(6_600),
                    OpeningSize::PotFractionBps(10_000),
                ],
                raise_sizes: vec![
                    RaiseSize::CurrentBetMultipleBps(25_000),
                    RaiseSize::PotFractionAfterCallBps(10_000),
                ],
                include_all_in: true,
            },
            StreetProfile {
                opening_sizes: vec![
                    OpeningSize::PotFractionBps(3_300),
                    OpeningSize::PotFractionBps(10_000),
                ],
                raise_sizes: vec![
                    RaiseSize::CurrentBetMultipleBps(25_000),
                    RaiseSize::PotFractionAfterCallBps(10_000),
                ],
                include_all_in: true,
            },
            StreetProfile {
                opening_sizes: vec![
                    OpeningSize::PotFractionBps(3_300),
                    OpeningSize::PotFractionBps(10_000),
                ],
                raise_sizes: vec![
                    RaiseSize::CurrentBetMultipleBps(25_000),
                    RaiseSize::PotFractionAfterCallBps(10_000),
                ],
                include_all_in: true,
            },
        );
        let bot = PostflopSolverBot::new(PostflopSolverBotConfig {
            profile,
            button_base_range: "AsKd,AhQh,KsQs".parse().unwrap(),
            big_blind_base_range: "QcJh,QdJd,7c7d".parse().unwrap(),
            max_opponent_combos: 3,
            flop_iterations: 1,
            turn_iterations: 1,
            river_iterations: 1,
        });
        let mut state = HoldemHandState::new(
            HoldemConfig::new(600, 50, 100).unwrap(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        let action = bot.choose_action(Player::Button, &state).unwrap();
        state.apply_action(action).unwrap();
    }

    #[test]
    #[ignore]
    fn postflop_solver_bot_larger_profile_self_play_stays_legal() {
        let postflop = StreetProfile {
            opening_sizes: vec![
                OpeningSize::PotFractionBps(3_300),
                OpeningSize::PotFractionBps(6_600),
                OpeningSize::PotFractionBps(10_000),
            ],
            raise_sizes: vec![
                RaiseSize::CurrentBetMultipleBps(25_000),
                RaiseSize::PotFractionAfterCallBps(10_000),
            ],
            include_all_in: true,
        };
        let bot = PostflopSolverBot::new(PostflopSolverBotConfig {
            profile: AbstractionProfile::new(
                StreetProfile {
                    opening_sizes: vec![
                        OpeningSize::BigBlindMultipleBps(25_000),
                        OpeningSize::BigBlindMultipleBps(40_000),
                    ],
                    raise_sizes: vec![
                        RaiseSize::CurrentBetMultipleBps(25_000),
                        RaiseSize::PotFractionAfterCallBps(10_000),
                    ],
                    include_all_in: true,
                },
                postflop.clone(),
                postflop.clone(),
                postflop,
            ),
            button_base_range: "AsKd,AhQh,KsQs".parse().unwrap(),
            big_blind_base_range: "QcJh,QdJd,7c7d".parse().unwrap(),
            max_opponent_combos: 3,
            flop_iterations: 1,
            turn_iterations: 1,
            river_iterations: 1,
        });
        let mut rng = default_rng();

        for _ in 0..4 {
            let mut deck = Deck::standard();
            deck.shuffle(&mut rng);
            let button = HoleCards::new(deck.draw().unwrap(), deck.draw().unwrap()).unwrap();
            let big_blind = HoleCards::new(deck.draw().unwrap(), deck.draw().unwrap()).unwrap();
            let board = [
                deck.draw().unwrap(),
                deck.draw().unwrap(),
                deck.draw().unwrap(),
                deck.draw().unwrap(),
                deck.draw().unwrap(),
            ];
            let mut state =
                HoldemHandState::new(HoldemConfig::new(600, 50, 100).unwrap(), button, big_blind)
                    .unwrap();

            loop {
                match state.phase() {
                    HandPhase::BettingRound { .. } if state.street() == gto_core::Street::Preflop => {
                        let action = crate::StubBot.choose_action(&state).unwrap();
                        state.apply_action(action).unwrap();
                    }
                    HandPhase::BettingRound { actor, .. } => {
                        let action = bot.choose_action(actor, &state).unwrap();
                        state.apply_action(action).unwrap();
                    }
                    HandPhase::AwaitingBoard { next_street } => match next_street {
                        gto_core::Street::Flop => state.deal_flop([board[0], board[1], board[2]]).unwrap(),
                        gto_core::Street::Turn => state.deal_turn(board[3]).unwrap(),
                        gto_core::Street::River => state.deal_river(board[4]).unwrap(),
                        gto_core::Street::Preflop => panic!("cannot await preflop cards"),
                    },
                    HandPhase::Terminal { .. } => break,
                }
            }
        }
    }
}
