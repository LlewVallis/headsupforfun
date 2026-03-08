use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::rc::Rc;

use gto_core::{
    Card, HandOutcome, HandPhase, HoldemConfig, HoldemHandState, HoldemStateError, HoleCards,
    Player, PlayerAction, Range,
};

use crate::{
    AbstractionProfile, AbstractAction, CfrCheckpoint, CfrCheckpointError, CfrPlusSolver,
    ExtensiveGameState, GameNode, HoldemInfoSetKey, abstract_actions,
};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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

    pub fn into_artifact(
        self,
        spot: ScriptedRiverSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> RiverStrategyArtifact {
        RiverStrategyArtifact::from_solver_result(
            spot,
            button_range,
            big_blind_range,
            profile,
            self,
        )
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct RiverStrategyArtifact {
    pub format_version: u32,
    pub spot: ScriptedRiverSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub iterations: u64,
    pub entries: Vec<RiverStrategyEntry>,
}

impl RiverStrategyArtifact {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn from_solver_result(
        spot: ScriptedRiverSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
        result: RiverSolverResult,
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

    pub fn to_solver_result(&self) -> Result<RiverSolverResult, RiverArtifactError> {
        self.validate_version()?;
        Ok(RiverSolverResult::from_strategy_snapshot(
            self.iterations,
            entries_to_snapshot(&self.entries),
        ))
    }

    fn validate_version(&self) -> Result<(), RiverArtifactError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(RiverArtifactError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, RiverArtifactError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self).map_err(|error| RiverArtifactError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, RiverArtifactError> {
        let artifact = serde_json::from_str::<Self>(input)
            .map_err(|error| RiverArtifactError::Decode(error.to_string()))?;
        artifact.validate_version()?;
        Ok(artifact)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct RiverTrainingCheckpoint {
    pub format_version: u32,
    pub spot: ScriptedRiverSpot,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub checkpoint: CfrCheckpoint<AbstractAction, HoldemInfoSetKey>,
}

impl RiverTrainingCheckpoint {
    pub const FORMAT_VERSION: u32 = 1;

    fn validate_version(&self) -> Result<(), RiverCheckpointError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(RiverCheckpointError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, RiverCheckpointError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| RiverCheckpointError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, RiverCheckpointError> {
        let checkpoint = serde_json::from_str::<Self>(input)
            .map_err(|error| RiverCheckpointError::Decode(error.to_string()))?;
        checkpoint.validate_version()?;
        Ok(checkpoint)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverTrainingProfile {
    Smoke,
    Dev,
    Full,
}

impl RiverTrainingProfile {
    pub const fn total_iterations(self) -> u64 {
        match self {
            Self::Smoke => 2_000,
            Self::Dev => 8_000,
            Self::Full => 25_000,
        }
    }

    pub const fn checkpoint_interval(self) -> u64 {
        match self {
            Self::Smoke => 500,
            Self::Dev => 2_000,
            Self::Full => 5_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiverTrainingSession {
    spot: ScriptedRiverSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    solver: CfrPlusSolver<RiverGameState>,
}

impl RiverTrainingSession {
    pub fn new(
        spot: ScriptedRiverSpot,
        button_range: Range,
        big_blind_range: Range,
        profile: AbstractionProfile,
    ) -> Result<Self, RiverSolveError> {
        let definition = Rc::new(RiverGameDefinition::new(
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
            solver: CfrPlusSolver::new(RiverGameState::root(definition)),
        })
    }

    pub fn from_checkpoint(
        checkpoint: RiverTrainingCheckpoint,
    ) -> Result<Self, RiverTrainingError> {
        checkpoint.validate_version()?;
        let definition = Rc::new(RiverGameDefinition::new(
            checkpoint.spot.clone(),
            checkpoint.button_range.clone(),
            checkpoint.big_blind_range.clone(),
            checkpoint.profile.clone(),
        )?);
        let solver = CfrPlusSolver::from_checkpoint(
            RiverGameState::root(definition),
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

    pub fn checkpoint(&self) -> RiverTrainingCheckpoint {
        RiverTrainingCheckpoint {
            format_version: RiverTrainingCheckpoint::FORMAT_VERSION,
            spot: self.spot.clone(),
            button_range: self.button_range.clone(),
            big_blind_range: self.big_blind_range.clone(),
            profile: self.profile.clone(),
            checkpoint: self.solver.checkpoint(),
        }
    }

    pub fn strategy_artifact(&self) -> RiverStrategyArtifact {
        self.solver_result().into_artifact(
            self.spot.clone(),
            self.button_range.clone(),
            self.big_blind_range.clone(),
            self.profile.clone(),
        )
    }

    pub fn solver_result(&self) -> RiverSolverResult {
        RiverSolverResult::from_strategy_snapshot(
            self.solver.iterations(),
            self.solver.average_strategy_snapshot(),
        )
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct RiverStrategyEntry {
    pub infoset: HoldemInfoSetKey,
    pub actions: Vec<RiverActionProbability>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct RiverActionProbability {
    pub action: AbstractAction,
    pub probability: f64,
}

pub fn solve_river_spot(
    spot: ScriptedRiverSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    iterations: u64,
) -> Result<RiverSolverResult, RiverSolveError> {
    let mut training = RiverTrainingSession::new(spot, button_range, big_blind_range, profile)?;
    training.train_iterations(iterations);
    Ok(training.solver_result())
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiverArtifactError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for RiverArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported river artifact format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode river artifact: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode river artifact: {error}"),
        }
    }
}

impl Error for RiverArtifactError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiverCheckpointError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for RiverCheckpointError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported river checkpoint format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode river checkpoint: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode river checkpoint: {error}"),
        }
    }
}

impl Error for RiverCheckpointError {}

#[derive(Debug)]
pub enum RiverTrainingError {
    Solve(RiverSolveError),
    Checkpoint(RiverCheckpointError),
    Cfr(CfrCheckpointError),
}

impl Display for RiverTrainingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solve(error) => write!(formatter, "{error}"),
            Self::Checkpoint(error) => write!(formatter, "{error}"),
            Self::Cfr(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for RiverTrainingError {}

impl From<RiverSolveError> for RiverTrainingError {
    fn from(value: RiverSolveError) -> Self {
        Self::Solve(value)
    }
}

impl From<RiverCheckpointError> for RiverTrainingError {
    fn from(value: RiverCheckpointError) -> Self {
        Self::Checkpoint(value)
    }
}

impl From<CfrCheckpointError> for RiverTrainingError {
    fn from(value: CfrCheckpointError) -> Self {
        Self::Cfr(value)
    }
}

fn snapshot_to_entries(
    strategy: HashMap<HoldemInfoSetKey, Vec<(AbstractAction, f64)>>,
) -> Vec<RiverStrategyEntry> {
    strategy
        .into_iter()
        .map(|(infoset, actions)| RiverStrategyEntry {
            infoset,
            actions: actions
                .into_iter()
                .map(|(action, probability)| RiverActionProbability {
                    action,
                    probability,
                })
                .collect(),
        })
        .collect()
}

fn entries_to_snapshot(
    entries: &[RiverStrategyEntry],
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

fn sort_strategy_entries(entries: &mut [RiverStrategyEntry]) {
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
    use gto_core::{HoldemStateError, Player, PlayerAction, Range};

    use crate::{
        AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, RaiseSize,
        StreetProfile,
    };

    use super::{
        RiverStrategyArtifact, RiverTrainingCheckpoint, RiverTrainingProfile,
        RiverTrainingSession, ScriptedRiverSpot, solve_river_spot,
    };

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

    #[test]
    fn river_artifact_json_round_trips_without_losing_strategy_queries() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = river_profile_without_raises();
        let result = solve_river_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            2_000,
        )
        .unwrap();
        let artifact = result.into_artifact(
            spot.clone(),
            button_range,
            big_blind_range,
            profile,
        );

        let encoded = artifact.to_json_string().unwrap();
        let decoded = RiverStrategyArtifact::from_json_str(&encoded).unwrap();
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
            Some(AbstractAction::Fold)
        );
    }

    #[test]
    fn river_artifact_rejects_unknown_format_versions() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = river_profile_without_raises();
        let result = solve_river_spot(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
            100,
        )
        .unwrap();
        let mut artifact = result.into_artifact(spot, button_range, big_blind_range, profile);
        artifact.format_version += 1;

        let error = artifact.to_solver_result().expect_err("version mismatch should fail");
        assert_eq!(
            error.to_string(),
            format!(
                "unsupported river artifact format version {}; expected {}",
                RiverStrategyArtifact::FORMAT_VERSION + 1,
                RiverStrategyArtifact::FORMAT_VERSION,
            )
        );
    }

    #[test]
    fn river_training_session_resume_matches_uninterrupted_training() {
        let spot = sample_spot();
        let button_range: Range = "QhJc".parse().unwrap();
        let big_blind_range: Range = "KhKd".parse().unwrap();
        let profile = river_profile_without_raises();

        let mut uninterrupted = RiverTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        uninterrupted.train_iterations(2_000);

        let mut resumed = RiverTrainingSession::new(
            spot.clone(),
            button_range.clone(),
            big_blind_range.clone(),
            profile.clone(),
        )
        .unwrap();
        resumed.train_iterations(1_000);
        let checkpoint = resumed.checkpoint();
        let json = checkpoint.to_json_string().unwrap();
        let decoded = RiverTrainingCheckpoint::from_json_str(&json).unwrap();
        let mut resumed = RiverTrainingSession::from_checkpoint(decoded).unwrap();
        resumed.train_iterations(1_000);

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
    fn training_profiles_order_iterations_and_checkpoints() {
        assert!(RiverTrainingProfile::Smoke.total_iterations() < RiverTrainingProfile::Dev.total_iterations());
        assert!(RiverTrainingProfile::Dev.total_iterations() < RiverTrainingProfile::Full.total_iterations());
        assert!(RiverTrainingProfile::Smoke.checkpoint_interval() <= RiverTrainingProfile::Smoke.total_iterations());
    }
}
