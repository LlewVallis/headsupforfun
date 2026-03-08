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
pub struct ScriptedFlopSpot {
    pub config: HoldemConfig,
    pub preflop_actions: Vec<PlayerAction>,
    pub flop: [Card; 3],
    pub flop_prefix_actions: Vec<PlayerAction>,
}

impl ScriptedFlopSpot {
    pub fn build_state(
        &self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, FlopSolveError> {
        let mut state = HoldemHandState::new(self.config, button_hole_cards, big_blind_hole_cards)
            .map_err(FlopSolveError::State)?;

        for action in &self.preflop_actions {
            state.apply_action(*action).map_err(FlopSolveError::State)?;
        }
        state.deal_flop(self.flop).map_err(FlopSolveError::State)?;
        for action in &self.flop_prefix_actions {
            state.apply_action(*action).map_err(FlopSolveError::State)?;
        }

        match state.phase() {
            HandPhase::BettingRound { street, .. } if street == Street::Flop => Ok(state),
            HandPhase::Terminal { .. } => Err(FlopSolveError::SpotAlreadyTerminal),
            phase => Err(FlopSolveError::UnexpectedPhase(phase)),
        }
    }

    pub fn board_cards(&self) -> [Card; 3] {
        self.flop
    }
}

#[derive(Debug, Clone)]
pub struct FlopSolverResult {
    iterations: u64,
    strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
}

impl FlopSolverResult {
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
        spot: ScriptedFlopSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> FlopStrategyArtifact {
        FlopStrategyArtifact::from_solver_result(
            spot,
            button_range,
            big_blind_range,
            profile,
            self,
        )
    }
}

pub type FlopStrategyEntry = crate::river::RiverStrategyEntry;
pub type FlopActionProbability = crate::river::RiverActionProbability;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct FlopStrategyArtifact {
    pub format_version: u32,
    pub spot: ScriptedFlopSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub iterations: u64,
    pub entries: Vec<FlopStrategyEntry>,
}

impl FlopStrategyArtifact {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn from_solver_result(
        spot: ScriptedFlopSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
        result: FlopSolverResult,
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

    pub fn to_solver_result(&self) -> Result<FlopSolverResult, FlopArtifactError> {
        self.validate_version()?;
        Ok(FlopSolverResult::from_strategy_snapshot(
            self.iterations,
            entries_to_snapshot(&self.entries),
        ))
    }

    fn validate_version(&self) -> Result<(), FlopArtifactError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(FlopArtifactError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, FlopArtifactError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self).map_err(|error| FlopArtifactError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, FlopArtifactError> {
        let artifact = serde_json::from_str::<Self>(input)
            .map_err(|error| FlopArtifactError::Decode(error.to_string()))?;
        artifact.validate_version()?;
        Ok(artifact)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct FlopTrainingCheckpoint {
    pub format_version: u32,
    pub spot: ScriptedFlopSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub checkpoint: CfrCheckpoint<AbstractAction, HoldemInfoSetKey>,
}

impl FlopTrainingCheckpoint {
    pub const FORMAT_VERSION: u32 = 1;

    fn validate_version(&self) -> Result<(), FlopCheckpointError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(FlopCheckpointError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, FlopCheckpointError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| FlopCheckpointError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, FlopCheckpointError> {
        let checkpoint = serde_json::from_str::<Self>(input)
            .map_err(|error| FlopCheckpointError::Decode(error.to_string()))?;
        checkpoint.validate_version()?;
        Ok(checkpoint)
    }
}

pub type FlopTrainingProfile = TrainingProfile;

#[derive(Debug, Clone)]
pub struct FlopTrainingSession {
    spot: ScriptedFlopSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    solver: CfrPlusSolver<FlopGameState>,
}

impl FlopTrainingSession {
    pub fn new(
        spot: ScriptedFlopSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, FlopSolveError> {
        let definition = Rc::new(FlopGameDefinition::new(
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
            solver: CfrPlusSolver::new(FlopGameState::root(definition)),
        })
    }

    pub fn from_checkpoint(
        checkpoint: FlopTrainingCheckpoint,
    ) -> Result<Self, FlopTrainingError> {
        checkpoint.validate_version()?;
        let definition = Rc::new(FlopGameDefinition::new(
            checkpoint.spot.clone(),
            checkpoint.button_range.clone(),
            checkpoint.big_blind_range.clone(),
            checkpoint.profile.clone(),
        )?);
        let solver = CfrPlusSolver::from_checkpoint(
            FlopGameState::root(definition),
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

    pub fn checkpoint(&self) -> FlopTrainingCheckpoint {
        FlopTrainingCheckpoint {
            format_version: FlopTrainingCheckpoint::FORMAT_VERSION,
            spot: self.spot.clone(),
            button_range: self.button_range.clone(),
            big_blind_range: self.big_blind_range.clone(),
            profile: self.profile.clone(),
            checkpoint: self.solver.checkpoint(),
        }
    }

    pub fn strategy_artifact(&self) -> FlopStrategyArtifact {
        self.solver_result().into_artifact(
            self.spot.clone(),
            self.button_range.clone(),
            self.big_blind_range.clone(),
            self.profile.clone(),
        )
    }

    pub fn solver_result(&self) -> FlopSolverResult {
        FlopSolverResult::from_strategy_snapshot(
            self.solver.iterations(),
            self.solver.average_strategy_snapshot(),
        )
    }
}

pub fn solve_flop_spot(
    spot: ScriptedFlopSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    iterations: u64,
) -> Result<FlopSolverResult, FlopSolveError> {
    let mut training = FlopTrainingSession::new(spot, button_range, big_blind_range, profile)?;
    training.train_iterations(iterations);
    Ok(training.solver_result())
}

#[derive(Debug, Clone)]
struct FlopGameDefinition {
    profile: AbstractionProfile,
    outcomes: Vec<FlopChanceOutcome>,
}

impl FlopGameDefinition {
    fn new(
        spot: ScriptedFlopSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, FlopSolveError> {
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
                outcomes.push(FlopChanceOutcome {
                    hole_cards: [button_hole_cards, big_blind_hole_cards],
                    state,
                });
            }
        }

        if outcomes.is_empty() {
            return Err(FlopSolveError::NoValidDeals);
        }

        Ok(Self { profile, outcomes })
    }
}

#[derive(Debug, Clone)]
struct FlopChanceOutcome {
    hole_cards: [HoleCards; 2],
    state: HoldemHandState,
}

#[derive(Debug, Clone)]
struct FlopGameState {
    definition: Rc<FlopGameDefinition>,
    active: Option<FlopChanceOutcome>,
    history: Vec<AbstractAction>,
}

impl FlopGameState {
    fn root(definition: Rc<FlopGameDefinition>) -> Self {
        Self {
            definition,
            active: None,
            history: Vec::new(),
        }
    }
}

impl ExtensiveGameState for FlopGameState {
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

        let active = self.active.as_ref().expect("active flop state should exist");
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
                    .expect("flop abstraction should produce legal actions");

                GameNode::Decision {
                    player: actor.index(),
                    infoset,
                    actions,
                }
            }
            HandPhase::AwaitingBoard { next_street } if next_street == Street::Turn => {
                board_chance_node(self, active, Street::Turn)
            }
            HandPhase::AwaitingBoard { next_street } if next_street == Street::River => {
                board_chance_node(self, active, Street::River)
            }
            phase => panic!("flop solver encountered unexpected phase {phase:?}"),
        }
    }

    fn next_state(&self, action: &Self::Action) -> Self {
        let mut next = self.clone();
        let active = next.active.as_mut().expect("active flop state should exist");
        active
            .state
            .apply_action(action.to_player_action())
            .expect("abstract action should map to a legal exact action");
        next.history.push(*action);
        next
    }
}

#[derive(Debug)]
pub enum FlopSolveError {
    NoValidDeals,
    SpotAlreadyTerminal,
    UnexpectedPhase(HandPhase),
    State(HoldemStateError),
}

impl Display for FlopSolveError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValidDeals => formatter.write_str("flop spot has no valid non-overlapping deals"),
            Self::SpotAlreadyTerminal => formatter.write_str("flop spot is already terminal"),
            Self::UnexpectedPhase(phase) => write!(formatter, "flop spot ended in unexpected phase {phase:?}"),
            Self::State(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for FlopSolveError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlopArtifactError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for FlopArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported flop artifact format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode flop artifact: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode flop artifact: {error}"),
        }
    }
}

impl Error for FlopArtifactError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlopCheckpointError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for FlopCheckpointError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported flop checkpoint format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode flop checkpoint: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode flop checkpoint: {error}"),
        }
    }
}

impl Error for FlopCheckpointError {}

#[derive(Debug)]
pub enum FlopTrainingError {
    Solve(FlopSolveError),
    Checkpoint(FlopCheckpointError),
    Cfr(CfrCheckpointError),
}

impl Display for FlopTrainingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solve(error) => write!(formatter, "{error}"),
            Self::Checkpoint(error) => write!(formatter, "{error}"),
            Self::Cfr(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for FlopTrainingError {}

impl From<FlopSolveError> for FlopTrainingError {
    fn from(value: FlopSolveError) -> Self {
        Self::Solve(value)
    }
}

impl From<FlopCheckpointError> for FlopTrainingError {
    fn from(value: FlopCheckpointError) -> Self {
        Self::Checkpoint(value)
    }
}

impl From<CfrCheckpointError> for FlopTrainingError {
    fn from(value: CfrCheckpointError) -> Self {
        Self::Cfr(value)
    }
}

fn board_chance_node(
    game_state: &FlopGameState,
    active: &FlopChanceOutcome,
    next_street: Street,
) -> GameNode<AbstractAction, HoldemInfoSetKey, FlopGameState> {
    let board_cards = available_board_cards(active);
    let probability = 1.0 / board_cards.len() as f64;

    GameNode::Chance {
        outcomes: board_cards
            .into_iter()
            .map(|card| {
                let mut next_state = active.state.clone();
                match next_street {
                    Street::Turn => next_state
                        .deal_turn(card)
                        .expect("turn card should be legal for flop chance expansion"),
                    Street::River => next_state
                        .deal_river(card)
                        .expect("river card should be legal for flop chance expansion"),
                    Street::Preflop | Street::Flop => unreachable!("unexpected board chance street"),
                }

                (
                    FlopGameState {
                        definition: Rc::clone(&game_state.definition),
                        active: Some(FlopChanceOutcome {
                            hole_cards: active.hole_cards,
                            state: next_state,
                        }),
                        history: game_state.history.clone(),
                    },
                    probability,
                )
            })
            .collect(),
    }
}

fn terminal_node(
    state: &HoldemHandState,
    outcome: HandOutcome,
) -> GameNode<AbstractAction, HoldemInfoSetKey, FlopGameState> {
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

fn available_board_cards(active: &FlopChanceOutcome) -> Vec<Card> {
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
) -> Vec<FlopStrategyEntry> {
    strategy
        .into_iter()
        .map(|(infoset, actions)| FlopStrategyEntry {
            infoset,
            actions: actions
                .into_iter()
                .map(|(action, probability)| FlopActionProbability {
                    action,
                    probability,
                })
                .collect(),
        })
        .collect()
}

fn entries_to_snapshot(
    entries: &[FlopStrategyEntry],
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

fn sort_strategy_entries(entries: &mut [FlopStrategyEntry]) {
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
        FlopSolveError, FlopStrategyArtifact, FlopTrainingCheckpoint, FlopTrainingProfile,
        FlopTrainingSession, ScriptedFlopSpot, solve_flop_spot,
    };

    fn flop_profile_without_raises() -> AbstractionProfile {
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

    fn sample_spot() -> ScriptedFlopSpot {
        ScriptedFlopSpot {
            config: gto_core::HoldemConfig::default(),
            preflop_actions: vec![PlayerAction::Call, PlayerAction::Check],
            flop: [
                "Kc".parse().unwrap(),
                "8d".parse().unwrap(),
                "4s".parse().unwrap(),
            ],
            flop_prefix_actions: vec![PlayerAction::BetTo(100)],
        }
    }

    #[test]
    fn scripted_spot_reaches_a_flop_decision() {
        let spot = sample_spot();
        let state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();

        assert_eq!(state.street(), Street::Flop);
        assert_eq!(state.current_actor(), Some(Player::Button));
    }

    #[test]
    fn flop_solver_returns_normalized_flop_strategy() {
        let spot = sample_spot();
        let result = solve_flop_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            flop_profile_without_raises(),
            2,
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
    }

    #[test]
    fn flop_solver_contains_turn_infosets_after_flop_call() {
        let spot = sample_spot();
        let result = solve_flop_spot(
            spot.clone(),
            "QhJc".parse::<Range>().unwrap(),
            "KhKd".parse::<Range>().unwrap(),
            flop_profile_without_raises(),
            2,
        )
        .unwrap();
        let mut state = spot
            .build_state("QhJc".parse().unwrap(), "KhKd".parse().unwrap())
            .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.deal_turn("2d".parse().unwrap()).unwrap();
        let infoset = HoldemInfoSetKey::from_state(
            Player::BigBlind,
            "KhKd".parse().unwrap(),
            &state,
            vec![AbstractAction::Call],
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

        assert!(matches!(error, FlopSolveError::State(HoldemStateError::CardAlreadyInUse { .. })));
    }

    #[test]
    fn flop_artifact_json_round_trips_without_losing_strategy_queries() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = flop_profile_without_raises();
        let result = solve_flop_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            2,
        )
        .unwrap();
        let artifact = result.clone().into_artifact(
            spot.clone(),
            button_range,
            big_blind_range,
            profile,
        );

        let encoded = artifact.to_json_string().unwrap();
        let decoded = FlopStrategyArtifact::from_json_str(&encoded).unwrap();
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
    fn flop_artifact_rejects_unknown_format_versions() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = flop_profile_without_raises();
        let result = solve_flop_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            1,
        )
        .unwrap();
        let mut artifact = result.into_artifact(spot, button_range, big_blind_range, profile);
        artifact.format_version += 1;

        let error = artifact.to_solver_result().expect_err("version mismatch should fail");
        assert_eq!(
            error.to_string(),
            format!(
                "unsupported flop artifact format version {}; expected {}",
                FlopStrategyArtifact::FORMAT_VERSION + 1,
                FlopStrategyArtifact::FORMAT_VERSION,
            )
        );
    }

    #[test]
    fn flop_training_session_resume_matches_uninterrupted_training() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = flop_profile_without_raises();

        let mut uninterrupted = FlopTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        uninterrupted.train_iterations(2);

        let mut resumed = FlopTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        resumed.train_iterations(1);
        let checkpoint = resumed.checkpoint();
        let json = checkpoint.to_json_string().unwrap();
        let decoded = FlopTrainingCheckpoint::from_json_str(&json).unwrap();
        let mut resumed = FlopTrainingSession::from_checkpoint(decoded).unwrap();
        resumed.train_iterations(1);

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
    fn flop_training_profile_alias_exposes_training_schedule() {
        assert!(FlopTrainingProfile::Smoke.total_iterations() < FlopTrainingProfile::Dev.total_iterations());
        assert!(FlopTrainingProfile::Dev.total_iterations() < FlopTrainingProfile::Full.total_iterations());
        assert!(FlopTrainingProfile::Smoke.checkpoint_interval() <= FlopTrainingProfile::Smoke.total_iterations());
    }
}
