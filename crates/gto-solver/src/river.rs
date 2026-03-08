use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::rc::Rc;

use gto_core::{
    Card, HandOutcome, HandPhase, HoldemConfig, HoldemHandState, HoldemStateError, HoleCards,
    Player, PlayerAction, Range,
};

use crate::{
    AbstractionProfile, AbstractAction, CfrPlusSolver, ExtensiveGameState, GameNode,
    HoldemInfoSetKey, abstract_actions,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedRiverSpot {
    pub config: HoldemConfig,
    pub preflop_actions: Vec<PlayerAction>,
    pub flop: [Card; 3],
    pub flop_actions: Vec<PlayerAction>,
    pub turn: Card,
    pub turn_actions: Vec<PlayerAction>,
    pub river: Card,
    pub river_prefix_actions: Vec<PlayerAction>,
}

impl ScriptedRiverSpot {
    pub fn build_state(
        &self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, RiverSolveError> {
        let mut state = HoldemHandState::new(self.config, button_hole_cards, big_blind_hole_cards)
            .map_err(RiverSolveError::State)?;

        for action in &self.preflop_actions {
            state.apply_action(*action).map_err(RiverSolveError::State)?;
        }
        state.deal_flop(self.flop).map_err(RiverSolveError::State)?;
        for action in &self.flop_actions {
            state.apply_action(*action).map_err(RiverSolveError::State)?;
        }
        state.deal_turn(self.turn).map_err(RiverSolveError::State)?;
        for action in &self.turn_actions {
            state.apply_action(*action).map_err(RiverSolveError::State)?;
        }
        state.deal_river(self.river).map_err(RiverSolveError::State)?;
        for action in &self.river_prefix_actions {
            state.apply_action(*action).map_err(RiverSolveError::State)?;
        }

        match state.phase() {
            HandPhase::BettingRound { street, .. } if street == gto_core::Street::River => Ok(state),
            HandPhase::Terminal { .. } => Err(RiverSolveError::SpotAlreadyTerminal),
            phase => Err(RiverSolveError::UnexpectedPhase(phase)),
        }
    }

    pub fn board_cards(&self) -> [Card; 5] {
        [
            self.flop[0],
            self.flop[1],
            self.flop[2],
            self.turn,
            self.river,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct RiverSolverResult {
    iterations: u64,
    strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
}

impl RiverSolverResult {
    pub const fn iterations(&self) -> u64 {
        self.iterations
    }

    pub fn strategy_for(
        &self,
        infoset: &HoldemInfoSetKey,
    ) -> Option<&[(AbstractAction, f64)]> {
        self.strategy.get(infoset).map(Vec::as_slice)
    }

    pub fn choose_action_max(&self, infoset: &HoldemInfoSetKey) -> Option<AbstractAction> {
        self.strategy_for(infoset).and_then(|actions| {
            actions
                .iter()
                .copied()
                .max_by(|left, right| left.1.total_cmp(&right.1))
                .map(|(action, _)| action)
        })
    }
}

pub fn solve_river_spot(
    spot: ScriptedRiverSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    iterations: u64,
) -> Result<RiverSolverResult, RiverSolveError> {
    let definition = Rc::new(RiverGameDefinition::new(
        spot,
        button_range,
        big_blind_range,
        profile,
    )?);
    let mut solver = CfrPlusSolver::new(RiverGameState::root(definition));
    solver.train_iterations(iterations);

    Ok(RiverSolverResult {
        iterations: solver.iterations(),
        strategy: solver.average_strategy_snapshot(),
    })
}

#[derive(Debug, Clone)]
struct RiverGameDefinition {
    profile: AbstractionProfile,
    outcomes: Vec<RiverChanceOutcome>,
}

impl RiverGameDefinition {
    fn new(
        spot: ScriptedRiverSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, RiverSolveError> {
        let board_cards = spot.board_cards();
        let board_mask = gto_core::CardMask::from_cards(board_cards);
        let button_range = button_range.without_dead_cards(board_mask);
        let big_blind_range = big_blind_range.without_dead_cards(board_mask);

        let mut outcomes = Vec::new();
        for button_hole_cards in button_range.iter().copied() {
            for big_blind_hole_cards in big_blind_range.iter().copied() {
                if button_hole_cards.mask().intersects(big_blind_hole_cards.mask()) {
                    continue;
                }

                let state = spot.build_state(button_hole_cards, big_blind_hole_cards)?;
                outcomes.push(RiverChanceOutcome {
                    hole_cards: [button_hole_cards, big_blind_hole_cards],
                    state,
                });
            }
        }

        if outcomes.is_empty() {
            return Err(RiverSolveError::NoValidDeals);
        }

        Ok(Self { profile, outcomes })
    }
}

#[derive(Debug, Clone)]
struct RiverChanceOutcome {
    hole_cards: [HoleCards; 2],
    state: HoldemHandState,
}

#[derive(Debug, Clone)]
struct RiverGameState {
    definition: Rc<RiverGameDefinition>,
    active: Option<RiverChanceOutcome>,
    history: Vec<AbstractAction>,
}

impl RiverGameState {
    fn root(definition: Rc<RiverGameDefinition>) -> Self {
        Self {
            definition,
            active: None,
            history: Vec::new(),
        }
    }
}

impl ExtensiveGameState for RiverGameState {
    type Action = AbstractAction;
    type InfoSet = HoldemInfoSetKey;

    fn node(&self) -> GameNode<Self::Action, Self::InfoSet, Self> {
        if self.active.is_none() {
            let probability = 1.0 / self.definition.outcomes.len() as f64;
            return GameNode::Chance {
                outcomes: self
                    .definition
                    .outcomes
                    .iter()
                    .cloned()
                    .map(|outcome| {
                        (
                            Self {
                                definition: Rc::clone(&self.definition),
                                active: Some(outcome),
                                history: Vec::new(),
                            },
                            probability,
                        )
                    })
                    .collect(),
            };
        }

        let active = self.active.as_ref().expect("active river state should exist");
        match active.state.phase() {
            HandPhase::Terminal { outcome } => {
                let button_snapshot = active.state.player(Player::Button);
                let big_blind_snapshot = active.state.player(Player::BigBlind);
                let (button_payout, big_blind_payout) = match outcome {
                    HandOutcome::Uncontested { payout, .. } | HandOutcome::Showdown { payout, .. } => {
                        (payout.player_one as f64, payout.player_two as f64)
                    }
                };

                GameNode::Terminal {
                    utilities: [
                        button_payout - button_snapshot.total_contribution as f64,
                        big_blind_payout - big_blind_snapshot.total_contribution as f64,
                    ],
                }
            }
            HandPhase::BettingRound { actor, .. } => {
                let hole_cards = active.hole_cards[actor.index()];
                let infoset = HoldemInfoSetKey::from_state(
                    actor,
                    hole_cards,
                    &active.state,
                    self.history.clone(),
                );
                let actions = abstract_actions(&active.state, &self.definition.profile)
                    .expect("river abstraction should produce legal actions");

                GameNode::Decision {
                    player: actor.index(),
                    infoset,
                    actions,
                }
            }
            phase => panic!("river solver encountered unexpected phase {phase:?}"),
        }
    }

    fn next_state(&self, action: &Self::Action) -> Self {
        let mut next = self.clone();
        let active = next.active.as_mut().expect("active river state should exist");
        active
            .state
            .apply_action(action.to_player_action())
            .expect("abstract action should map to a legal exact action");
        next.history.push(*action);
        next
    }
}

#[derive(Debug)]
pub enum RiverSolveError {
    NoValidDeals,
    SpotAlreadyTerminal,
    UnexpectedPhase(HandPhase),
    State(HoldemStateError),
}

impl Display for RiverSolveError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValidDeals => formatter.write_str("river spot has no valid non-overlapping deals"),
            Self::SpotAlreadyTerminal => formatter.write_str("river spot is already terminal"),
            Self::UnexpectedPhase(phase) => write!(formatter, "river spot ended in unexpected phase {phase:?}"),
            Self::State(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for RiverSolveError {}

#[cfg(test)]
mod tests {
    use gto_core::{HoldemStateError, Player, PlayerAction, Range};

    use crate::{
        AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, RaiseSize, StreetProfile,
    };

    use super::{ScriptedRiverSpot, solve_river_spot};

    fn river_profile_without_raises() -> AbstractionProfile {
        let preflop = StreetProfile {
            opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
            include_all_in: false,
        };
        let postflop = StreetProfile {
            opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
            raise_sizes: vec![],
            include_all_in: false,
        };
        AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop)
    }

    fn sample_spot() -> ScriptedRiverSpot {
        ScriptedRiverSpot {
            config: gto_core::HoldemConfig::default(),
            preflop_actions: vec![PlayerAction::Call],
            flop: [
                "Kc".parse().unwrap(),
                "8d".parse().unwrap(),
                "4s".parse().unwrap(),
            ],
            flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
            turn: "3h".parse().unwrap(),
            turn_actions: vec![PlayerAction::Check, PlayerAction::Check],
            river: "2d".parse().unwrap(),
            river_prefix_actions: vec![PlayerAction::BetTo(100)],
        }
    }

    #[test]
    fn scripted_spot_reaches_a_river_decision() {
        let spot = sample_spot();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();

        assert_eq!(state.street(), gto_core::Street::River);
        assert_eq!(state.current_actor(), Some(Player::Button));
    }

    #[test]
    fn river_solver_returns_normalized_root_strategy() {
        let spot = sample_spot();
        let result = solve_river_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            river_profile_without_raises(),
            2_000,
        )
        .unwrap();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::Button,
            "QhJc".parse().unwrap(),
            &state,
            Vec::new(),
        );

        let strategy = result.strategy_for(&infoset).unwrap();
        let probability_sum = strategy.iter().map(|(_, probability)| probability).sum::<f64>();
        assert!((probability_sum - 1.0).abs() < 1e-9);
        assert!(strategy.iter().all(|(_, probability)| probability.is_finite()));
    }

    #[test]
    fn river_solver_learns_to_fold_a_dead_hand_facing_a_bet() {
        let spot = sample_spot();
        let result = solve_river_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            river_profile_without_raises(),
            5_000,
        )
        .unwrap();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::Button,
            "QhJc".parse().unwrap(),
            &state,
            Vec::new(),
        );

        let strategy = result.strategy_for(&infoset).unwrap();
        let fold_probability = strategy
            .iter()
            .find_map(|(action, probability)| match action {
                AbstractAction::Fold => Some(*probability),
                _ => None,
            })
            .unwrap_or(0.0);

        assert!(fold_probability > 0.95, "unexpected fold probability {fold_probability}");
        assert_eq!(result.choose_action_max(&infoset), Some(AbstractAction::Fold));
    }

    #[test]
    fn invalid_public_script_surfaces_exact_state_errors() {
        let mut spot = sample_spot();
        spot.flop[0] = "As".parse().unwrap();

        let error = spot
            .build_state("AsKd".parse().unwrap(), "QcJh".parse().unwrap())
            .expect_err("spot should reject duplicate cards");

        assert!(matches!(error, super::RiverSolveError::State(HoldemStateError::CardAlreadyInUse { .. })));
    }
}
