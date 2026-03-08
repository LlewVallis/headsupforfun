use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::rc::Rc;

use gto_core::{
    Card, CardMask, HandOutcome, HandPhase, HoldemConfig, HoldemHandState, HoldemStateError,
    HoleCards, Player, PlayerAction, Range, Street,
};

use crate::{
    AbstractionProfile, AbstractAction, CfrCheckpoint, CfrCheckpointError, CfrPlusSolver,
    ExtensiveGameState, GameNode, HoldemInfoSetKey, TrainingProfile, abstract_actions,
};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedTurnSpot {
    pub config: HoldemConfig,
    pub preflop_actions: Vec<PlayerAction>,
    pub flop: [Card; 3],
    pub flop_actions: Vec<PlayerAction>,
    pub turn: Card,
    pub turn_prefix_actions: Vec<PlayerAction>,
}

impl ScriptedTurnSpot {
    pub fn build_state(
        &self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, TurnSolveError> {
        let mut state = HoldemHandState::new(self.config, button_hole_cards, big_blind_hole_cards)
            .map_err(TurnSolveError::State)?;

        for action in &self.preflop_actions {
            state.apply_action(*action).map_err(TurnSolveError::State)?;
        }
        state.deal_flop(self.flop).map_err(TurnSolveError::State)?;
        for action in &self.flop_actions {
            state.apply_action(*action).map_err(TurnSolveError::State)?;
        }
        state.deal_turn(self.turn).map_err(TurnSolveError::State)?;
        for action in &self.turn_prefix_actions {
            state.apply_action(*action).map_err(TurnSolveError::State)?;
        }

        match state.phase() {
            HandPhase::BettingRound { street, .. } if street == Street::Turn => Ok(state),
            HandPhase::Terminal { .. } => Err(TurnSolveError::SpotAlreadyTerminal),
            phase => Err(TurnSolveError::UnexpectedPhase(phase)),
        }
    }

    pub fn board_cards(&self) -> [Card; 4] {
        [self.flop[0], self.flop[1], self.flop[2], self.turn]
    }
}

#[derive(Debug, Clone)]
pub struct TurnSolverResult {
    iterations: u64,
    strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
}

impl TurnSolverResult {
    fn from_strategy_snapshot(
        iterations: u64,
        strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
    ) -> Self {
        Self {
            iterations,
            strategy,
        }
    }

    pub const fn iterations(&self) -> u64 {
        self.iterations
    }

    pub fn strategy_for(&self, infoset: &HoldemInfoSetKey) -> Option<&[(AbstractAction, f64)]> {
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

    pub fn into_artifact(
        self,
        spot: ScriptedTurnSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> TurnStrategyArtifact {
        TurnStrategyArtifact::from_solver_result(
            spot,
            button_range,
            big_blind_range,
            profile,
            self,
        )
    }
}

pub type TurnStrategyEntry = crate::river::RiverStrategyEntry;
pub type TurnActionProbability = crate::river::RiverActionProbability;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct TurnStrategyArtifact {
    pub format_version: u32,
    pub spot: ScriptedTurnSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub iterations: u64,
    pub entries: Vec<TurnStrategyEntry>,
}

impl TurnStrategyArtifact {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn from_solver_result(
        spot: ScriptedTurnSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
        result: TurnSolverResult,
    ) -> Self {
        let mut entries = snapshot_to_entries(result.strategy);
        sort_strategy_entries(&mut entries);

        Self {
            format_version: Self::FORMAT_VERSION,
            spot,
            button_range,
            big_blind_range,
            profile,
            iterations: result.iterations,
            entries,
        }
    }

    pub fn to_solver_result(&self) -> Result<TurnSolverResult, TurnArtifactError> {
        self.validate_version()?;
        Ok(TurnSolverResult::from_strategy_snapshot(
            self.iterations,
            entries_to_snapshot(&self.entries),
        ))
    }

    fn validate_version(&self) -> Result<(), TurnArtifactError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(TurnArtifactError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, TurnArtifactError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self).map_err(|error| TurnArtifactError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, TurnArtifactError> {
        let artifact = serde_json::from_str::<Self>(input)
            .map_err(|error| TurnArtifactError::Decode(error.to_string()))?;
        artifact.validate_version()?;
        Ok(artifact)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct TurnTrainingCheckpoint {
    pub format_version: u32,
    pub spot: ScriptedTurnSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub checkpoint: CfrCheckpoint<AbstractAction, HoldemInfoSetKey>,
}

impl TurnTrainingCheckpoint {
    pub const FORMAT_VERSION: u32 = 1;

    fn validate_version(&self) -> Result<(), TurnCheckpointError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(TurnCheckpointError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, TurnCheckpointError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| TurnCheckpointError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, TurnCheckpointError> {
        let checkpoint = serde_json::from_str::<Self>(input)
            .map_err(|error| TurnCheckpointError::Decode(error.to_string()))?;
        checkpoint.validate_version()?;
        Ok(checkpoint)
    }
}

pub type TurnTrainingProfile = TrainingProfile;

#[derive(Debug, Clone)]
pub struct TurnTrainingSession {
    spot: ScriptedTurnSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    solver: CfrPlusSolver<TurnGameState>,
}

impl TurnTrainingSession {
    pub fn new(
        spot: ScriptedTurnSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, TurnSolveError> {
        let definition = Rc::new(TurnGameDefinition::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )?);

        Ok(Self {
            spot,
            button_range,
            big_blind_range,
            profile,
            solver: CfrPlusSolver::new(TurnGameState::root(definition)),
        })
    }

    pub fn from_checkpoint(
        checkpoint: TurnTrainingCheckpoint,
    ) -> Result<Self, TurnTrainingError> {
        checkpoint.validate_version()?;
        let definition = Rc::new(TurnGameDefinition::new(
            checkpoint.spot.clone(),
            checkpoint.button_range.clone(),
            checkpoint.big_blind_range.clone(),
            checkpoint.profile.clone(),
        )?);
        let solver = CfrPlusSolver::from_checkpoint(
            TurnGameState::root(definition),
            checkpoint.checkpoint,
        )?;

        Ok(Self {
            spot: checkpoint.spot,
            button_range: checkpoint.button_range,
            big_blind_range: checkpoint.big_blind_range,
            profile: checkpoint.profile,
            solver,
        })
    }

    pub fn train_iterations(&mut self, iterations: u64) {
        self.solver.train_iterations(iterations);
    }

    pub const fn iterations(&self) -> u64 {
        self.solver.iterations()
    }

    pub fn checkpoint(&self) -> TurnTrainingCheckpoint {
        TurnTrainingCheckpoint {
            format_version: TurnTrainingCheckpoint::FORMAT_VERSION,
            spot: self.spot.clone(),
            button_range: self.button_range.clone(),
            big_blind_range: self.big_blind_range.clone(),
            profile: self.profile.clone(),
            checkpoint: self.solver.checkpoint(),
        }
    }

    pub fn strategy_artifact(&self) -> TurnStrategyArtifact {
        self.solver_result().into_artifact(
            self.spot.clone(),
            self.button_range.clone(),
            self.big_blind_range.clone(),
            self.profile.clone(),
        )
    }

    pub fn solver_result(&self) -> TurnSolverResult {
        TurnSolverResult::from_strategy_snapshot(
            self.solver.iterations(),
            self.solver.average_strategy_snapshot(),
        )
    }
}

pub fn solve_turn_spot(
    spot: ScriptedTurnSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    iterations: u64,
) -> Result<TurnSolverResult, TurnSolveError> {
    let mut training = TurnTrainingSession::new(spot, button_range, big_blind_range, profile)?;
    training.train_iterations(iterations);
    Ok(training.solver_result())
}

#[derive(Debug, Clone)]
struct TurnGameDefinition {
    profile: AbstractionProfile,
    outcomes: Vec<TurnChanceOutcome>,
}

impl TurnGameDefinition {
    fn new(
        spot: ScriptedTurnSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, TurnSolveError> {
        let board_mask = CardMask::from_cards(spot.board_cards());
        let button_range = button_range.without_dead_cards(board_mask);
        let big_blind_range = big_blind_range.without_dead_cards(board_mask);

        let mut outcomes = Vec::new();
        for button_hole_cards in button_range.iter().copied() {
            for big_blind_hole_cards in big_blind_range.iter().copied() {
                if button_hole_cards.mask().intersects(big_blind_hole_cards.mask()) {
                    continue;
                }

                let state = spot.build_state(button_hole_cards, big_blind_hole_cards)?;
                outcomes.push(TurnChanceOutcome {
                    hole_cards: [button_hole_cards, big_blind_hole_cards],
                    state,
                });
            }
        }

        if outcomes.is_empty() {
            return Err(TurnSolveError::NoValidDeals);
        }

        Ok(Self { profile, outcomes })
    }
}

#[derive(Debug, Clone)]
struct TurnChanceOutcome {
    hole_cards: [HoleCards; 2],
    state: HoldemHandState,
}

#[derive(Debug, Clone)]
struct TurnGameState {
    definition: Rc<TurnGameDefinition>,
    active: Option<TurnChanceOutcome>,
    history: Vec<AbstractAction>,
}

impl TurnGameState {
    fn root(definition: Rc<TurnGameDefinition>) -> Self {
        Self {
            definition,
            active: None,
            history: Vec::new(),
        }
    }
}

impl ExtensiveGameState for TurnGameState {
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

        let active = self.active.as_ref().expect("active turn state should exist");
        match active.state.phase() {
            HandPhase::Terminal { outcome } => terminal_node(&active.state, outcome),
            HandPhase::BettingRound { actor, .. } => {
                let hole_cards = active.hole_cards[actor.index()];
                let infoset = HoldemInfoSetKey::from_state(
                    actor,
                    hole_cards,
                    &active.state,
                    self.history.clone(),
                );
                let actions = abstract_actions(&active.state, &self.definition.profile)
                    .expect("turn abstraction should produce legal actions");

                GameNode::Decision {
                    player: actor.index(),
                    infoset,
                    actions,
                }
            }
            HandPhase::AwaitingBoard { next_street: Street::River } => {
                let river_cards = available_river_cards(active);
                let probability = 1.0 / river_cards.len() as f64;
                GameNode::Chance {
                    outcomes: river_cards
                        .into_iter()
                        .map(|river| {
                            let mut next_state = active.state.clone();
                            next_state
                                .deal_river(river)
                                .expect("river card should be legal for turn chance expansion");
                            (
                                Self {
                                    definition: Rc::clone(&self.definition),
                                    active: Some(TurnChanceOutcome {
                                        hole_cards: active.hole_cards,
                                        state: next_state,
                                    }),
                                    history: self.history.clone(),
                                },
                                probability,
                            )
                        })
                        .collect(),
                }
            }
            phase => panic!("turn solver encountered unexpected phase {phase:?}"),
        }
    }

    fn next_state(&self, action: &Self::Action) -> Self {
        let mut next = self.clone();
        let active = next.active.as_mut().expect("active turn state should exist");
        active
            .state
            .apply_action(action.to_player_action())
            .expect("abstract action should map to a legal exact action");
        next.history.push(*action);
        next
    }
}

#[derive(Debug)]
pub enum TurnSolveError {
    NoValidDeals,
    SpotAlreadyTerminal,
    UnexpectedPhase(HandPhase),
    State(HoldemStateError),
}

impl Display for TurnSolveError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValidDeals => formatter.write_str("turn spot has no valid non-overlapping deals"),
            Self::SpotAlreadyTerminal => formatter.write_str("turn spot is already terminal"),
            Self::UnexpectedPhase(phase) => write!(formatter, "turn spot ended in unexpected phase {phase:?}"),
            Self::State(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for TurnSolveError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnArtifactError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for TurnArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported turn artifact format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode turn artifact: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode turn artifact: {error}"),
        }
    }
}

impl Error for TurnArtifactError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnCheckpointError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for TurnCheckpointError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported turn checkpoint format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode turn checkpoint: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode turn checkpoint: {error}"),
        }
    }
}

impl Error for TurnCheckpointError {}

#[derive(Debug)]
pub enum TurnTrainingError {
    Solve(TurnSolveError),
    Checkpoint(TurnCheckpointError),
    Cfr(CfrCheckpointError),
}

impl Display for TurnTrainingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solve(error) => write!(formatter, "{error}"),
            Self::Checkpoint(error) => write!(formatter, "{error}"),
            Self::Cfr(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for TurnTrainingError {}

impl From<TurnSolveError> for TurnTrainingError {
    fn from(value: TurnSolveError) -> Self {
        Self::Solve(value)
    }
}

impl From<TurnCheckpointError> for TurnTrainingError {
    fn from(value: TurnCheckpointError) -> Self {
        Self::Checkpoint(value)
    }
}

impl From<CfrCheckpointError> for TurnTrainingError {
    fn from(value: CfrCheckpointError) -> Self {
        Self::Cfr(value)
    }
}

fn terminal_node(
    state: &HoldemHandState,
    outcome: HandOutcome,
) -> GameNode<AbstractAction, HoldemInfoSetKey, TurnGameState> {
    let button_snapshot = state.player(Player::Button);
    let big_blind_snapshot = state.player(Player::BigBlind);
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

fn available_river_cards(active: &TurnChanceOutcome) -> Vec<Card> {
    let dead_cards = active.state.board().mask().union(
        active.hole_cards[0]
            .mask()
            .union(active.hole_cards[1].mask()),
    );

    (0..52)
        .filter_map(Card::from_index)
        .filter(|card| !dead_cards.contains(*card))
        .collect()
}

fn snapshot_to_entries(
    strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
) -> Vec<TurnStrategyEntry> {
    strategy
        .into_iter()
        .map(|(infoset, actions)| TurnStrategyEntry {
            infoset,
            actions: actions
                .into_iter()
                .map(|(action, probability)| TurnActionProbability {
                    action,
                    probability,
                })
                .collect(),
        })
        .collect()
}

fn entries_to_snapshot(
    entries: &[TurnStrategyEntry],
) -> HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>> {
    entries
        .iter()
        .map(|entry| {
            (
                entry.infoset.clone(),
                entry
                    .actions
                    .iter()
                    .map(|action| (action.action, action.probability))
                    .collect(),
            )
        })
        .collect()
}

fn sort_strategy_entries(entries: &mut [TurnStrategyEntry]) {
    for entry in entries.iter_mut() {
        entry.actions.sort_by(|left, right| {
            stable_action_key(left.action).cmp(&stable_action_key(right.action))
        });
    }
    entries.sort_by(|left, right| {
        stable_infoset_key(&left.infoset).cmp(&stable_infoset_key(&right.infoset))
    });
}

fn stable_infoset_key(infoset: &HoldemInfoSetKey) -> String {
    let public = &infoset.public_state;
    let board = public
        .board
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("");
    let actor = public
        .actor
        .map(|player| player.to_string())
        .unwrap_or_else(|| "-".to_string());
    let history = infoset
        .public_history
        .iter()
        .map(|action| stable_action_key(*action))
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        infoset.player,
        infoset.hole_cards,
        public.street,
        board,
        actor,
        public.pot,
        public.button_stack,
        public.big_blind_stack,
        public.button_total_contribution,
        public.big_blind_total_contribution,
        public.button_street_contribution,
        public.big_blind_street_contribution,
        history,
    )
}

fn stable_action_key(action: AbstractAction) -> String {
    match action {
        AbstractAction::Fold => "fold".to_string(),
        AbstractAction::Check => "check".to_string(),
        AbstractAction::Call => "call".to_string(),
        AbstractAction::BetTo(total) => format!("bet:{total}"),
        AbstractAction::RaiseTo(total) => format!("raise:{total}"),
        AbstractAction::AllIn(total) => format!("allin:{total}"),
    }
}

#[cfg(test)]
mod tests {
    use gto_core::{HoldemStateError, Player, PlayerAction, Range, Street};

    use crate::{
        AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, RaiseSize,
        StreetProfile,
    };

    use super::{
        ScriptedTurnSpot, TurnSolveError, TurnStrategyArtifact, TurnTrainingCheckpoint,
        TurnTrainingProfile, TurnTrainingSession, solve_turn_spot,
    };

    fn turn_profile_without_raises() -> AbstractionProfile {
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

    fn sample_spot() -> ScriptedTurnSpot {
        ScriptedTurnSpot {
            config: gto_core::HoldemConfig::default(),
            preflop_actions: vec![PlayerAction::Call, PlayerAction::Check],
            flop: [
                "Kc".parse().unwrap(),
                "8d".parse().unwrap(),
                "4s".parse().unwrap(),
            ],
            flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
            turn: "3h".parse().unwrap(),
            turn_prefix_actions: vec![PlayerAction::Check],
        }
    }

    #[test]
    fn scripted_spot_reaches_a_turn_decision() {
        let spot = sample_spot();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();

        assert_eq!(state.street(), Street::Turn);
        assert_eq!(state.current_actor(), Some(Player::Button));
    }

    #[test]
    fn turn_solver_returns_normalized_turn_strategy() {
        let spot = sample_spot();
        let result = solve_turn_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            turn_profile_without_raises(),
            20,
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
    fn turn_solver_contains_river_infosets_after_turn_check() {
        let spot = sample_spot();
        let result = solve_turn_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            turn_profile_without_raises(),
            20,
        )
        .unwrap();
        let mut state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_river("2d".parse().unwrap()).unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::BigBlind,
            "KhKd".parse().unwrap(),
            &state,
            vec![AbstractAction::Check],
        );

        let strategy = result.strategy_for(&infoset).unwrap();
        let probability_sum = strategy.iter().map(|(_, probability)| probability).sum::<f64>();
        assert!((probability_sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn invalid_public_script_surfaces_exact_state_errors() {
        let mut spot = sample_spot();
        spot.flop[0] = "As".parse().unwrap();

        let error = spot
            .build_state("AsKd".parse().unwrap(), "QcJh".parse().unwrap())
            .expect_err("spot should reject duplicate cards");

        assert!(matches!(error, TurnSolveError::State(HoldemStateError::CardAlreadyInUse { .. })));
    }

    #[test]
    fn turn_artifact_json_round_trips_without_losing_strategy_queries() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = turn_profile_without_raises();
        let result = solve_turn_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            20,
        )
        .unwrap();
        let artifact = result.clone().into_artifact(
            spot.clone(),
            button_range,
            big_blind_range,
            profile,
        );

        let encoded = artifact.to_json_string().unwrap();
        let decoded = TurnStrategyArtifact::from_json_str(&encoded).unwrap();
        let restored = decoded.to_solver_result().unwrap();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::Button,
            "QhJc".parse().unwrap(),
            &state,
            Vec::new(),
        );

        assert_eq!(
            restored.choose_action_max(&infoset),
            result.choose_action_max(&infoset)
        );
    }

    #[test]
    fn turn_artifact_rejects_unknown_format_versions() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = turn_profile_without_raises();
        let result = solve_turn_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            5,
        )
        .unwrap();
        let mut artifact = result.into_artifact(spot, button_range, big_blind_range, profile);
        artifact.format_version += 1;

        let error = artifact.to_solver_result().expect_err("version mismatch should fail");
        assert_eq!(
            error.to_string(),
            format!(
                "unsupported turn artifact format version {}; expected {}",
                TurnStrategyArtifact::FORMAT_VERSION + 1,
                TurnStrategyArtifact::FORMAT_VERSION,
            )
        );
    }

    #[test]
    fn turn_training_session_resume_matches_uninterrupted_training() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = turn_profile_without_raises();

        let mut uninterrupted = TurnTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        uninterrupted.train_iterations(10);

        let mut resumed = TurnTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        resumed.train_iterations(5);
        let checkpoint = resumed.checkpoint();
        let json = checkpoint.to_json_string().unwrap();
        let decoded = TurnTrainingCheckpoint::from_json_str(&json).unwrap();
        let mut resumed = TurnTrainingSession::from_checkpoint(decoded).unwrap();
        resumed.train_iterations(5);

        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::Button,
            "QhJc".parse().unwrap(),
            &state,
            Vec::new(),
        );

        assert_eq!(
            uninterrupted.solver_result().choose_action_max(&infoset),
            resumed.solver_result().choose_action_max(&infoset)
        );
    }

    #[test]
    fn turn_training_profile_alias_exposes_training_schedule() {
        assert!(TurnTrainingProfile::Smoke.total_iterations() < TurnTrainingProfile::Dev.total_iterations());
        assert!(TurnTrainingProfile::Dev.total_iterations() < TurnTrainingProfile::Full.total_iterations());
        assert!(TurnTrainingProfile::Smoke.checkpoint_interval() <= TurnTrainingProfile::Smoke.total_iterations());
    }
}
