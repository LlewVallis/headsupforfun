use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gto_core::{Card, Chips, HoldemHandState, HoleCards, Player, PlayerAction, Range, Street};

use crate::{
    AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, RaiseSize,
    ScriptedFlopSpot, ScriptedRiverSpot, ScriptedTurnSpot, abstract_actions, solve_flop_spot,
    solve_river_spot, solve_turn_spot,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverSpotSuite {
    pub format_version: u32,
    pub suite_name: String,
    pub spots: Vec<TexasSolverEvalSpot>,
}

impl TexasSolverSpotSuite {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn to_json_string(&self) -> Result<String, TexasSolverEvalError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| TexasSolverEvalError::Encode(error.to_string()))
    }

    pub fn from_json_str(input: &str) -> Result<Self, TexasSolverEvalError> {
        let suite = serde_json::from_str::<Self>(input)
            .map_err(|error| TexasSolverEvalError::Decode(error.to_string()))?;
        suite.validate_version()?;
        Ok(suite)
    }

    fn validate_version(&self) -> Result<(), TexasSolverEvalError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(TexasSolverEvalError::UnsupportedSuiteFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverEvalSpot {
    pub id: String,
    pub description: String,
    pub button_hole_cards: HoleCards,
    pub big_blind_hole_cards: HoleCards,
    pub button_range: Range,
    pub big_blind_range: Range,
    pub profile: AbstractionProfile,
    pub iterations: u64,
    pub script: PostflopScriptedSpot,
}

impl TexasSolverEvalSpot {
    pub fn street(&self) -> Street {
        self.script.street()
    }

    pub fn build_state(&self) -> Result<HoldemHandState, TexasSolverEvalError> {
        self.script
            .build_state(self.button_hole_cards, self.big_blind_hole_cards)
    }

    fn build_street_start_state(&self) -> Result<HoldemHandState, TexasSolverEvalError> {
        self.script
            .build_street_start_state(self.button_hole_cards, self.big_blind_hole_cards)
    }

    fn current_street_prefix_actions(&self) -> &[PlayerAction] {
        self.script.current_street_prefix_actions()
    }

    fn actor_hole_cards(&self, actor: Player) -> HoleCards {
        match actor {
            Player::Button => self.button_hole_cards,
            Player::BigBlind => self.big_blind_hole_cards,
        }
    }

    fn current_street_public_history(
        &self,
    ) -> Result<(Vec<AbstractAction>, Vec<NormalizedTexasAction>), TexasSolverEvalError> {
        let mut state = self.build_street_start_state()?;
        let mut abstract_history = Vec::with_capacity(self.current_street_prefix_actions().len());
        let mut target_path = Vec::with_capacity(self.current_street_prefix_actions().len());

        for action in self.current_street_prefix_actions() {
            let abstract_action = abstract_actions(&state, &self.profile)
                .map_err(TexasSolverEvalError::State)?
                .into_iter()
                .find(|candidate| candidate.to_player_action() == *action)
                .ok_or_else(|| TexasSolverEvalError::ActionNotInAbstraction {
                    spot_id: self.id.clone(),
                    action: *action,
                })?;
            abstract_history.push(abstract_action);
            target_path.push(normalize_action_for_texassolver(&state, *action)?);
            state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
        }

        Ok((abstract_history, target_path))
    }

    fn our_infoset(&self) -> Result<HoldemInfoSetKey, TexasSolverEvalError> {
        let state = self.build_state()?;
        let actor = state
            .current_actor()
            .ok_or_else(|| TexasSolverEvalError::NoActor(self.id.clone()))?;
        let (history, _) = self.current_street_public_history()?;
        Ok(HoldemInfoSetKey::from_state(
            actor,
            self.actor_hole_cards(actor),
            &state,
            history,
        ))
    }

    fn our_action(&self) -> Result<AbstractAction, TexasSolverEvalError> {
        let infoset = self.our_infoset()?;
        match &self.script {
            PostflopScriptedSpot::Flop(spot) => {
                solve_flop_spot(
                    spot.clone(),
                    self.button_range.clone(),
                    self.big_blind_range.clone(),
                    self.profile.clone(),
                    self.iterations,
                )
                .map_err(TexasSolverEvalError::FlopSolve)?
                .choose_action_max(&infoset)
                .ok_or_else(|| TexasSolverEvalError::MissingOurAction(self.id.clone()))
            }
            PostflopScriptedSpot::Turn(spot) => {
                solve_turn_spot(
                    spot.clone(),
                    self.button_range.clone(),
                    self.big_blind_range.clone(),
                    self.profile.clone(),
                    self.iterations,
                )
                .map_err(TexasSolverEvalError::TurnSolve)?
                .choose_action_max(&infoset)
                .ok_or_else(|| TexasSolverEvalError::MissingOurAction(self.id.clone()))
            }
            PostflopScriptedSpot::River(spot) => {
                solve_river_spot(
                    spot.clone(),
                    self.button_range.clone(),
                    self.big_blind_range.clone(),
                    self.profile.clone(),
                    self.iterations,
                )
                .map_err(TexasSolverEvalError::RiverSolve)?
                .choose_action_max(&infoset)
                .ok_or_else(|| TexasSolverEvalError::MissingOurAction(self.id.clone()))
            }
        }
    }

    pub fn export_for_texassolver(&self) -> Result<TexasSolverExport, TexasSolverEvalError> {
        let start_state = self.build_street_start_state()?;
        validate_texassolver_state(self, &start_state)?;
        let (_, target_path) = self.current_street_public_history()?;
        let actor = self
            .build_state()?
            .current_actor()
            .ok_or_else(|| TexasSolverEvalError::NoActor(self.id.clone()))?;
        let actor_combo = self.actor_hole_cards(actor).to_string();
        let script = texassolver_script(self, &start_state)?;
        let target_path_labels = target_path
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let signature = build_signature(
            self,
            &start_state,
            &target_path_labels,
            &actor_combo,
            &script,
        );

        Ok(TexasSolverExport {
            spot_id: self.id.clone(),
            street: self.street(),
            actor,
            actor_combo,
            signature,
            script,
            target_path: target_path_labels,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "street", content = "spot", rename_all = "lowercase")]
pub enum PostflopScriptedSpot {
    Flop(ScriptedFlopSpot),
    Turn(ScriptedTurnSpot),
    River(ScriptedRiverSpot),
}

impl PostflopScriptedSpot {
    fn street(&self) -> Street {
        match self {
            Self::Flop(_) => Street::Flop,
            Self::Turn(_) => Street::Turn,
            Self::River(_) => Street::River,
        }
    }

    fn build_state(
        &self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, TexasSolverEvalError> {
        match self {
            Self::Flop(spot) => spot
                .build_state(button_hole_cards, big_blind_hole_cards)
                .map_err(TexasSolverEvalError::FlopSolve),
            Self::Turn(spot) => spot
                .build_state(button_hole_cards, big_blind_hole_cards)
                .map_err(TexasSolverEvalError::TurnSolve),
            Self::River(spot) => spot
                .build_state(button_hole_cards, big_blind_hole_cards)
                .map_err(TexasSolverEvalError::RiverSolve),
        }
    }

    fn current_street_prefix_actions(&self) -> &[PlayerAction] {
        match self {
            Self::Flop(spot) => &spot.flop_prefix_actions,
            Self::Turn(spot) => &spot.turn_prefix_actions,
            Self::River(spot) => &spot.river_prefix_actions,
        }
    }

    fn board_cards(&self) -> Vec<Card> {
        match self {
            Self::Flop(spot) => spot.board_cards().to_vec(),
            Self::Turn(spot) => spot.board_cards().to_vec(),
            Self::River(spot) => spot.board_cards().to_vec(),
        }
    }

    fn build_street_start_state(
        &self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, TexasSolverEvalError> {
        match self {
            Self::Flop(spot) => {
                let mut state = new_scripted_state(
                    spot.config,
                    button_hole_cards,
                    big_blind_hole_cards,
                    spot.button_starting_stack,
                    spot.big_blind_starting_stack,
                )?;
                for action in &spot.preflop_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_flop(spot.flop).map_err(TexasSolverEvalError::State)?;
                Ok(state)
            }
            Self::Turn(spot) => {
                let mut state = new_scripted_state(
                    spot.config,
                    button_hole_cards,
                    big_blind_hole_cards,
                    spot.button_starting_stack,
                    spot.big_blind_starting_stack,
                )?;
                for action in &spot.preflop_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_flop(spot.flop).map_err(TexasSolverEvalError::State)?;
                for action in &spot.flop_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_turn(spot.turn).map_err(TexasSolverEvalError::State)?;
                Ok(state)
            }
            Self::River(spot) => {
                let mut state = new_scripted_state(
                    spot.config,
                    button_hole_cards,
                    big_blind_hole_cards,
                    spot.button_starting_stack,
                    spot.big_blind_starting_stack,
                )?;
                for action in &spot.preflop_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_flop(spot.flop).map_err(TexasSolverEvalError::State)?;
                for action in &spot.flop_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_turn(spot.turn).map_err(TexasSolverEvalError::State)?;
                for action in &spot.turn_actions {
                    state.apply_action(*action).map_err(TexasSolverEvalError::State)?;
                }
                state.deal_river(spot.river).map_err(TexasSolverEvalError::State)?;
                Ok(state)
            }
        }
    }
}

fn new_scripted_state(
    config: gto_core::HoldemConfig,
    button_hole_cards: HoleCards,
    big_blind_hole_cards: HoleCards,
    button_starting_stack: Option<Chips>,
    big_blind_starting_stack: Option<Chips>,
) -> Result<HoldemHandState, TexasSolverEvalError> {
    HoldemHandState::new_with_starting_stacks(
        config,
        button_hole_cards,
        big_blind_hole_cards,
        button_starting_stack.unwrap_or(config.starting_stack),
        big_blind_starting_stack.unwrap_or(config.starting_stack),
    )
    .map_err(TexasSolverEvalError::State)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverExport {
    pub spot_id: String,
    pub street: Street,
    pub actor: Player,
    pub actor_combo: String,
    pub signature: String,
    pub script: String,
    pub target_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverReferenceSuite {
    pub format_version: u32,
    pub suite_name: String,
    pub references: Vec<TexasSolverSpotReference>,
}

impl TexasSolverReferenceSuite {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn to_json_string(&self) -> Result<String, TexasSolverEvalError> {
        self.validate_version()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| TexasSolverEvalError::Encode(error.to_string()))
    }

    pub fn from_json_str(input: &str) -> Result<Self, TexasSolverEvalError> {
        let suite = serde_json::from_str::<Self>(input)
            .map_err(|error| TexasSolverEvalError::Decode(error.to_string()))?;
        suite.validate_version()?;
        Ok(suite)
    }

    fn validate_version(&self) -> Result<(), TexasSolverEvalError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(TexasSolverEvalError::UnsupportedReferenceFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverSpotReference {
    pub spot_id: String,
    pub signature: String,
    pub root: TexasSolverActionNode,
    #[serde(default)]
    pub ev_root: Option<TexasSolverEvNode>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverActionNode {
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub player: Option<u8>,
    #[serde(default)]
    pub strategy: Option<TexasSolverStrategyNode>,
    #[serde(default, rename = "childrens")]
    pub childrens: BTreeMap<String, TexasSolverActionNode>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverStrategyNode {
    pub actions: Vec<String>,
    pub strategy: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverEvNode {
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub evs: BTreeMap<String, Vec<f64>>,
    #[serde(default, rename = "childrens")]
    pub childrens: BTreeMap<String, TexasSolverEvNode>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverGradeReport {
    pub format_version: u32,
    pub suite_name: String,
    pub results: Vec<TexasSolverSpotGrade>,
}

impl TexasSolverGradeReport {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn to_json_string(&self) -> Result<String, TexasSolverEvalError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| TexasSolverEvalError::Encode(error.to_string()))
    }

    pub fn summary(&self) -> TexasSolverGradeSummary {
        let mut summary = TexasSolverGradeSummary::default();
        summary.total_spots = self.results.len();

        for result in &self.results {
            if result.status == TexasSolverGradeStatus::Graded {
                summary.graded_spots += 1;
            } else {
                summary.ungraded_spots += 1;
            }

            if result.action_matches == Some(true) {
                summary.action_matches += 1;
            }
            if let Some(gap) = result.ev_gap {
                summary.ev_gap_spots += 1;
                summary.total_ev_gap += gap;
            }
        }

        summary
    }

    pub fn terminal_summary(&self) -> String {
        let summary = self.summary();
        let average_ev_gap = if summary.ev_gap_spots == 0 {
            None
        } else {
            Some(summary.total_ev_gap / summary.ev_gap_spots as f64)
        };

        let mut lines = vec![
            format!("suite: {}", self.suite_name),
            format!("spots: {}", summary.total_spots),
            format!("graded: {}", summary.graded_spots),
            format!("ungraded: {}", summary.ungraded_spots),
            format!("action matches: {}", summary.action_matches),
            format!("ev-graded spots: {}", summary.ev_gap_spots),
        ];
        if let Some(value) = average_ev_gap {
            lines.push(format!("average ev gap: {value:.4}"));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TexasSolverGradeSummary {
    pub total_spots: usize,
    pub graded_spots: usize,
    pub ungraded_spots: usize,
    pub action_matches: usize,
    pub ev_gap_spots: usize,
    pub total_ev_gap: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TexasSolverSpotGrade {
    pub spot_id: String,
    pub street: Street,
    pub status: TexasSolverGradeStatus,
    pub our_action: Option<String>,
    pub reference_best_action: Option<String>,
    pub action_matches: Option<bool>,
    pub chosen_action_ev: Option<f64>,
    pub best_action_ev: Option<f64>,
    pub ev_gap: Option<f64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TexasSolverGradeStatus {
    Graded,
    MissingReference,
    SignatureMismatch,
    MissingNode,
    MissingStrategy,
    MissingCombo,
    ActionUnavailable,
    SolveFailed,
    ExportUnsupported,
}

pub fn grade_texassolver_suite(
    suite: &TexasSolverSpotSuite,
    references: &TexasSolverReferenceSuite,
) -> TexasSolverGradeReport {
    let reference_map = references
        .references
        .iter()
        .map(|reference| (reference.spot_id.as_str(), reference))
        .collect::<BTreeMap<_, _>>();

    let mut results = Vec::with_capacity(suite.spots.len());
    for spot in &suite.spots {
        results.push(grade_texassolver_spot(spot, reference_map.get(spot.id.as_str()).copied()));
    }

    TexasSolverGradeReport {
        format_version: TexasSolverGradeReport::FORMAT_VERSION,
        suite_name: suite.suite_name.clone(),
        results,
    }
}

pub fn texassolver_smoke_suite() -> TexasSolverSpotSuite {
    let config = gto_core::HoldemConfig::default();
    let preflop = crate::StreetProfile {
        opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
        raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
        include_all_in: true,
    };
    let postflop = crate::StreetProfile {
        opening_sizes: vec![
            OpeningSize::PotFractionBps(3_300),
            OpeningSize::PotFractionBps(6_600),
            OpeningSize::PotFractionBps(10_000),
        ],
        raise_sizes: vec![RaiseSize::PotFractionAfterCallBps(10_000)],
        include_all_in: true,
    };
    let profile = AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop);

    TexasSolverSpotSuite {
        format_version: TexasSolverSpotSuite::FORMAT_VERSION,
        suite_name: "smoke".to_string(),
        spots: vec![
            TexasSolverEvalSpot {
                id: "river_oop_root".to_string(),
                description: "river root with oop to act".to_string(),
                button_hole_cards: "AsQh".parse().unwrap(),
                big_blind_hole_cards: "JdTc".parse().unwrap(),
                button_range: "AsQh".parse().unwrap(),
                big_blind_range: "JdTc".parse().unwrap(),
                profile: profile.clone(),
                iterations: 8,
                script: PostflopScriptedSpot::River(ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![PlayerAction::RaiseTo(250), PlayerAction::Call],
                    flop: ["Qs".parse().unwrap(), "8d".parse().unwrap(), "4c".parse().unwrap()],
                    flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    turn: "2h".parse().unwrap(),
                    turn_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    river: "7s".parse().unwrap(),
                    river_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "river_oop_root_alt".to_string(),
                description: "second river root spot".to_string(),
                button_hole_cards: "AcKd".parse().unwrap(),
                big_blind_hole_cards: "QhJh".parse().unwrap(),
                button_range: "AcKd".parse().unwrap(),
                big_blind_range: "QhJh".parse().unwrap(),
                profile: profile.clone(),
                iterations: 8,
                script: PostflopScriptedSpot::River(ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![PlayerAction::Call, PlayerAction::Check],
                    flop: ["Kh".parse().unwrap(), "9d".parse().unwrap(), "3c".parse().unwrap()],
                    flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    turn: "2s".parse().unwrap(),
                    turn_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    river: "7d".parse().unwrap(),
                    river_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "river_oop_root_third".to_string(),
                description: "third river root spot".to_string(),
                button_hole_cards: "AhJd".parse().unwrap(),
                big_blind_hole_cards: "Td9c".parse().unwrap(),
                button_range: "AhJd".parse().unwrap(),
                big_blind_range: "Td9c".parse().unwrap(),
                profile,
                iterations: 8,
                script: PostflopScriptedSpot::River(ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![PlayerAction::Call, PlayerAction::Check],
                    flop: ["Jh".parse().unwrap(), "8s".parse().unwrap(), "5d".parse().unwrap()],
                    flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    turn: "2c".parse().unwrap(),
                    turn_actions: vec![PlayerAction::Check, PlayerAction::Check],
                    river: "7h".parse().unwrap(),
                    river_prefix_actions: vec![],
                }),
            },
        ],
    }
}

fn grade_texassolver_spot(
    spot: &TexasSolverEvalSpot,
    reference: Option<&TexasSolverSpotReference>,
) -> TexasSolverSpotGrade {
    let export = match spot.export_for_texassolver() {
        Ok(export) => export,
        Err(error) => {
            return TexasSolverSpotGrade {
                spot_id: spot.id.clone(),
                street: spot.street(),
                status: TexasSolverGradeStatus::ExportUnsupported,
                our_action: None,
                reference_best_action: None,
                action_matches: None,
                chosen_action_ev: None,
                best_action_ev: None,
                ev_gap: None,
                note: Some(error.to_string()),
            };
        }
    };

    let Some(reference) = reference else {
        return TexasSolverSpotGrade {
            spot_id: spot.id.clone(),
            street: spot.street(),
            status: TexasSolverGradeStatus::MissingReference,
            our_action: None,
            reference_best_action: None,
            action_matches: None,
            chosen_action_ev: None,
            best_action_ev: None,
            ev_gap: None,
            note: Some("no reference spot found".to_string()),
        };
    };

    if reference.signature != export.signature {
        return TexasSolverSpotGrade {
            spot_id: spot.id.clone(),
            street: spot.street(),
            status: TexasSolverGradeStatus::SignatureMismatch,
            our_action: None,
            reference_best_action: None,
            action_matches: None,
            chosen_action_ev: None,
            best_action_ev: None,
            ev_gap: None,
            note: Some("spot signature mismatch".to_string()),
        };
    }

    let strategy_node = match action_node_at_path(&reference.root, &export.target_path) {
        Some(node) => node,
        None => {
            return TexasSolverSpotGrade {
                spot_id: spot.id.clone(),
                street: spot.street(),
                status: TexasSolverGradeStatus::MissingNode,
                our_action: None,
                reference_best_action: None,
                action_matches: None,
                chosen_action_ev: None,
                best_action_ev: None,
                ev_gap: None,
                note: Some("reference tree did not contain the requested node".to_string()),
            };
        }
    };
    let Some(strategy) = strategy_node.strategy.as_ref() else {
        return TexasSolverSpotGrade {
            spot_id: spot.id.clone(),
            street: spot.street(),
            status: TexasSolverGradeStatus::MissingStrategy,
            our_action: None,
            reference_best_action: None,
            action_matches: None,
            chosen_action_ev: None,
            best_action_ev: None,
            ev_gap: None,
            note: Some("reference node did not contain strategy data".to_string()),
        };
    };

    let combo_key = export.actor_combo.clone();
    let Some(probabilities) = strategy.strategy.get(&combo_key) else {
        return TexasSolverSpotGrade {
            spot_id: spot.id.clone(),
            street: spot.street(),
            status: TexasSolverGradeStatus::MissingCombo,
            our_action: None,
            reference_best_action: None,
            action_matches: None,
            chosen_action_ev: None,
            best_action_ev: None,
            ev_gap: None,
            note: Some(format!("reference strategy did not contain combo {combo_key}")),
        };
    };

    let our_action = match spot.our_action() {
        Ok(action) => action,
        Err(error) => {
            return TexasSolverSpotGrade {
                spot_id: spot.id.clone(),
                street: spot.street(),
                status: TexasSolverGradeStatus::SolveFailed,
                our_action: None,
                reference_best_action: None,
                action_matches: None,
                chosen_action_ev: None,
                best_action_ev: None,
                ev_gap: None,
                note: Some(error.to_string()),
            };
        }
    };
    let our_label = normalize_abstract_action(our_action, &spot.build_state().ok());

    let reference_actions = strategy
        .actions
        .iter()
        .filter_map(|action| NormalizedTexasAction::parse(action).ok())
        .collect::<Vec<_>>();
    let best_index = probabilities
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index);
    let reference_best_action = best_index
        .and_then(|index| reference_actions.get(index))
        .copied();

    let chosen_index = reference_actions.iter().position(|action| *action == our_label);
    let Some(chosen_index) = chosen_index else {
        return TexasSolverSpotGrade {
            spot_id: spot.id.clone(),
            street: spot.street(),
            status: TexasSolverGradeStatus::ActionUnavailable,
            our_action: Some(our_label.to_string()),
            reference_best_action: reference_best_action.map(|action| action.to_string()),
            action_matches: None,
            chosen_action_ev: None,
            best_action_ev: None,
            ev_gap: None,
            note: Some("our action was not present in the reference node".to_string()),
        };
    };

    let ev_node = reference
        .ev_root
        .as_ref()
        .and_then(|root| ev_node_at_path(root, &export.target_path));
    let (chosen_action_ev, best_action_ev, ev_gap) =
        extract_evs(ev_node, &combo_key, chosen_index, best_index);

    TexasSolverSpotGrade {
        spot_id: spot.id.clone(),
        street: spot.street(),
        status: TexasSolverGradeStatus::Graded,
        our_action: Some(our_label.to_string()),
        reference_best_action: reference_best_action.map(|action| action.to_string()),
        action_matches: reference_best_action.map(|action| action == our_label),
        chosen_action_ev,
        best_action_ev,
        ev_gap,
        note: None,
    }
}

fn extract_evs(
    node: Option<&TexasSolverEvNode>,
    combo_key: &str,
    chosen_index: usize,
    best_index: Option<usize>,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let Some(node) = node else {
        return (None, None, None);
    };
    let Some(evs) = node.evs.get(combo_key) else {
        return (None, None, None);
    };
    let chosen = evs.get(chosen_index).copied();
    let best = best_index.and_then(|index| evs.get(index).copied());
    let gap = match (chosen, best) {
        (Some(chosen), Some(best)) => Some(best - chosen),
        _ => None,
    };
    (chosen, best, gap)
}

fn action_node_at_path<'a>(
    mut node: &'a TexasSolverActionNode,
    path: &[String],
) -> Option<&'a TexasSolverActionNode> {
    for action in path {
        let next = node
            .childrens
            .iter()
            .find(|(label, _)| {
                NormalizedTexasAction::parse(label)
                    .is_ok_and(|normalized| normalized.to_string() == *action)
            })
            .map(|(_, child)| child)?;
        node = next;
    }
    Some(node)
}

fn ev_node_at_path<'a>(mut node: &'a TexasSolverEvNode, path: &[String]) -> Option<&'a TexasSolverEvNode> {
    for action in path {
        let next = node
            .childrens
            .iter()
            .find(|(label, _)| {
                NormalizedTexasAction::parse(label)
                    .is_ok_and(|normalized| normalized.to_string() == *action)
            })
            .map(|(_, child)| child)?;
        node = next;
    }
    Some(node)
}

fn validate_texassolver_state(
    spot: &TexasSolverEvalSpot,
    state: &HoldemHandState,
) -> Result<(), TexasSolverEvalError> {
    if state.player(Player::Button).stack != state.player(Player::BigBlind).stack {
        return Err(TexasSolverEvalError::UnequalEffectiveStacks(spot.id.clone()));
    }

    for street in relevant_streets(spot.street()) {
        for raise_size in &spot.profile.for_street(*street).raise_sizes {
            if matches!(raise_size, RaiseSize::CurrentBetMultipleBps(_)) {
                return Err(TexasSolverEvalError::UnsupportedRaiseSize {
                    spot_id: spot.id.clone(),
                    street: *street,
                });
            }
        }
    }

    Ok(())
}

fn texassolver_script(
    spot: &TexasSolverEvalSpot,
    start_state: &HoldemHandState,
) -> Result<String, TexasSolverEvalError> {
    let board = spot
        .script
        .board_cards()
        .into_iter()
        .map(|card| card.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let pot_bb = chips_to_bb_string(start_state.pot());
    let effective_stack_bb = chips_to_bb_string(start_state.player(Player::Button).stack);
    let mut lines = vec![
        format!("set_pot {pot_bb}"),
        format!("set_effective_stack {effective_stack_bb}"),
        format!("set_board {board}"),
        format!("set_range_oop {}", spot.big_blind_range),
        format!("set_range_ip {}", spot.button_range),
    ];

    for street in relevant_streets(spot.street()) {
        lines.extend(texassolver_bet_size_commands(
            "oop",
            *street,
            spot.profile.for_street(*street),
            *street != Street::Flop,
        )?);
        lines.extend(texassolver_bet_size_commands(
            "ip",
            *street,
            spot.profile.for_street(*street),
            false,
        )?);
    }

    lines.extend([
        "set_allin_threshold 1.0".to_string(),
        "build_tree".to_string(),
        "set_thread_num 1".to_string(),
        "set_accuracy 0.1".to_string(),
        format!("set_max_iteration {}", spot.iterations),
        "set_print_interval 25".to_string(),
        "set_use_isomorphism 1".to_string(),
        "start_solve".to_string(),
        "set_dump_rounds 1".to_string(),
        "dump_result output_result.json".to_string(),
    ]);

    Ok(lines.join("\n"))
}

fn texassolver_bet_size_commands(
    position: &str,
    street: Street,
    profile: &crate::StreetProfile,
    include_donk: bool,
) -> Result<Vec<String>, TexasSolverEvalError> {
    let street_name = street.to_string();
    let opening_sizes = profile
        .opening_sizes
        .iter()
        .map(texassolver_opening_size)
        .collect::<Result<Vec<_>, _>>()?;
    let raise_sizes = profile
        .raise_sizes
        .iter()
        .map(texassolver_raise_size)
        .collect::<Result<Vec<_>, _>>()?;
    let mut lines = Vec::new();

    if !opening_sizes.is_empty() {
        lines.push(format!(
            "set_bet_sizes {position},{street_name},bet,{}",
            opening_sizes.join(",")
        ));
        if include_donk {
            lines.push(format!(
                "set_bet_sizes {position},{street_name},donk,{}",
                opening_sizes.join(",")
            ));
        }
    }
    if !raise_sizes.is_empty() {
        lines.push(format!(
            "set_bet_sizes {position},{street_name},raise,{}",
            raise_sizes.join(",")
        ));
    }
    if profile.include_all_in {
        lines.push(format!("set_bet_sizes {position},{street_name},allin"));
    }

    Ok(lines)
}

fn texassolver_opening_size(size: &OpeningSize) -> Result<String, TexasSolverEvalError> {
    match size {
        OpeningSize::PotFractionBps(bps) => Ok(format_percentage_bps(*bps)),
        OpeningSize::BigBlindMultipleBps(_) => Err(TexasSolverEvalError::UnsupportedOpeningSize),
    }
}

fn texassolver_raise_size(size: &RaiseSize) -> Result<String, TexasSolverEvalError> {
    match size {
        RaiseSize::PotFractionAfterCallBps(bps) => Ok(format_percentage_bps(*bps)),
        RaiseSize::CurrentBetMultipleBps(_) => Err(TexasSolverEvalError::UnsupportedRaiseSizeType),
    }
}

fn build_signature(
    spot: &TexasSolverEvalSpot,
    start_state: &HoldemHandState,
    target_path: &[String],
    actor_combo: &str,
    script: &str,
) -> String {
    let target = if target_path.is_empty() {
        "-".to_string()
    } else {
        target_path.join(">")
    };
    format!(
        "{}|{}|pot:{}|eff:{}|oop:{}|ip:{}|combo:{}|path:{}|script:{}",
        spot.street(),
        spot
            .script
            .board_cards()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(""),
        start_state.pot(),
        start_state.player(Player::Button).stack,
        spot.big_blind_range,
        spot.button_range,
        actor_combo,
        target,
        script.replace('\n', ";"),
    )
}

fn relevant_streets(start: Street) -> &'static [Street] {
    match start {
        Street::Flop => &[Street::Flop, Street::Turn, Street::River],
        Street::Turn => &[Street::Turn, Street::River],
        Street::River => &[Street::River],
        Street::Preflop => &[],
    }
}

fn normalize_action_for_texassolver(
    state: &HoldemHandState,
    action: PlayerAction,
) -> Result<NormalizedTexasAction, TexasSolverEvalError> {
    match action {
        PlayerAction::Fold => Ok(NormalizedTexasAction::Fold),
        PlayerAction::Check => Ok(NormalizedTexasAction::Check),
        PlayerAction::Call => Ok(NormalizedTexasAction::Call),
        PlayerAction::BetTo(total) => Ok(NormalizedTexasAction::Bet(total)),
        PlayerAction::RaiseTo(total) => Ok(NormalizedTexasAction::Raise(total)),
        PlayerAction::AllIn => {
            let actor = state
                .current_actor()
                .ok_or_else(|| TexasSolverEvalError::NoActor("all-in normalization".to_string()))?;
            let total = state.player(actor).street_contribution + state.player(actor).stack;
            let legal = state.legal_actions().map_err(TexasSolverEvalError::State)?;
            if legal.call_amount.is_some() {
                Ok(NormalizedTexasAction::Raise(total))
            } else {
                Ok(NormalizedTexasAction::Bet(total))
            }
        }
    }
}

fn normalize_abstract_action(
    action: AbstractAction,
    state: &Option<HoldemHandState>,
) -> NormalizedTexasAction {
    match action {
        AbstractAction::Fold => NormalizedTexasAction::Fold,
        AbstractAction::Check => NormalizedTexasAction::Check,
        AbstractAction::Call => NormalizedTexasAction::Call,
        AbstractAction::BetTo(total) => NormalizedTexasAction::Bet(total),
        AbstractAction::RaiseTo(total) => NormalizedTexasAction::Raise(total),
        AbstractAction::AllIn(total) => {
            if state
                .as_ref()
                .and_then(|state| state.legal_actions().ok())
                .and_then(|legal| legal.call_amount)
                .is_some()
            {
                NormalizedTexasAction::Raise(total)
            } else {
                NormalizedTexasAction::Bet(total)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NormalizedTexasAction {
    Fold,
    Check,
    Call,
    Bet(Chips),
    Raise(Chips),
}

impl NormalizedTexasAction {
    fn parse(input: &str) -> Result<Self, TexasSolverEvalError> {
        let trimmed = input.trim().to_ascii_uppercase();
        if trimmed == "FOLD" {
            return Ok(Self::Fold);
        }
        if trimmed == "CHECK" {
            return Ok(Self::Check);
        }
        if trimmed == "CALL" {
            return Ok(Self::Call);
        }
        if let Some(amount) = trimmed.strip_prefix("BET ") {
            return Ok(Self::Bet(parse_bb_amount(amount)?));
        }
        if let Some(amount) = trimmed.strip_prefix("RAISE ") {
            return Ok(Self::Raise(parse_bb_amount(amount)?));
        }
        Err(TexasSolverEvalError::UnsupportedTexasSolverAction(input.to_string()))
    }
}

impl Display for NormalizedTexasAction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fold => formatter.write_str("FOLD"),
            Self::Check => formatter.write_str("CHECK"),
            Self::Call => formatter.write_str("CALL"),
            Self::Bet(total) => write!(formatter, "BET {}", chips_to_bb_string(*total)),
            Self::Raise(total) => write!(formatter, "RAISE {}", chips_to_bb_string(*total)),
        }
    }
}

fn parse_bb_amount(input: &str) -> Result<Chips, TexasSolverEvalError> {
    let trimmed = input.trim();
    let mut parts = trimmed.split('.');
    let whole = parts
        .next()
        .unwrap_or_default()
        .parse::<u64>()
        .map_err(|_| TexasSolverEvalError::UnsupportedTexasSolverAction(input.to_string()))?;
    let fractional = match parts.next() {
        Some(fraction) => {
            if parts.next().is_some() {
                return Err(TexasSolverEvalError::UnsupportedTexasSolverAction(input.to_string()));
            }
            let mut digits = fraction.chars().take(2).collect::<String>();
            while digits.len() < 2 {
                digits.push('0');
            }
            digits
                .parse::<u64>()
                .map_err(|_| TexasSolverEvalError::UnsupportedTexasSolverAction(input.to_string()))?
        }
        None => 0,
    };
    Ok(whole.saturating_mul(100) + fractional)
}

fn chips_to_bb_string(chips: Chips) -> String {
    let whole = chips / 100;
    let fractional = chips % 100;
    if fractional == 0 {
        format!("{whole}.0")
    } else if fractional % 10 == 0 {
        format!("{whole}.{}", fractional / 10)
    } else {
        format!("{whole}.{fractional:02}")
    }
}

fn format_percentage_bps(bps: u32) -> String {
    let whole = bps / 100;
    let fractional = bps % 100;
    if fractional == 0 {
        whole.to_string()
    } else if fractional % 10 == 0 {
        format!("{whole}.{}", fractional / 10)
    } else {
        format!("{whole}.{fractional:02}")
    }
}

#[derive(Debug)]
pub enum TexasSolverEvalError {
    UnsupportedSuiteFormatVersion { expected: u32, actual: u32 },
    UnsupportedReferenceFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
    FlopSolve(crate::FlopSolveError),
    TurnSolve(crate::TurnSolveError),
    RiverSolve(crate::RiverSolveError),
    State(gto_core::HoldemStateError),
    MissingOurAction(String),
    NoActor(String),
    UnequalEffectiveStacks(String),
    UnsupportedRaiseSize { spot_id: String, street: Street },
    UnsupportedOpeningSize,
    UnsupportedRaiseSizeType,
    UnsupportedTexasSolverAction(String),
    ActionNotInAbstraction { spot_id: String, action: PlayerAction },
}

impl Display for TexasSolverEvalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSuiteFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported TexasSolver spot suite format version {actual}; expected {expected}"
            ),
            Self::UnsupportedReferenceFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported TexasSolver reference format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode TexasSolver data: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode TexasSolver data: {error}"),
            Self::FlopSolve(error) => write!(formatter, "{error}"),
            Self::TurnSolve(error) => write!(formatter, "{error}"),
            Self::RiverSolve(error) => write!(formatter, "{error}"),
            Self::State(error) => write!(formatter, "{error}"),
            Self::MissingOurAction(spot_id) => write!(
                formatter,
                "solver did not return an action for evaluation spot `{spot_id}`"
            ),
            Self::NoActor(context) => write!(formatter, "no current actor available for {context}"),
            Self::UnequalEffectiveStacks(spot_id) => write!(
                formatter,
                "TexasSolver export only supports equal effective stacks for spot `{spot_id}`"
            ),
            Self::UnsupportedRaiseSize { spot_id, street } => write!(
                formatter,
                "TexasSolver export only supports pot-fraction-after-call raise sizes for spot `{spot_id}` on {street}"
            ),
            Self::UnsupportedOpeningSize => {
                formatter.write_str("TexasSolver export only supports postflop pot-fraction opening sizes")
            }
            Self::UnsupportedRaiseSizeType => formatter.write_str(
                "TexasSolver export only supports pot-fraction-after-call raise sizes",
            ),
            Self::UnsupportedTexasSolverAction(action) => {
                write!(formatter, "unsupported TexasSolver action label `{action}`")
            }
            Self::ActionNotInAbstraction { spot_id, action } => write!(
                formatter,
                "public script action `{action}` was not present in the abstraction for spot `{spot_id}`"
            ),
        }
    }
}

impl Error for TexasSolverEvalError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gto_core::{HoldemConfig, Range};

    use crate::{OpeningSize, RaiseSize, StreetProfile, smoke_blueprint_profile};

    use super::{
        NormalizedTexasAction, PostflopScriptedSpot, TexasSolverActionNode, TexasSolverEvalSpot,
        TexasSolverGradeStatus, TexasSolverReferenceSuite, TexasSolverSpotReference,
        TexasSolverSpotSuite, TexasSolverStrategyNode, chips_to_bb_string, format_percentage_bps,
        grade_texassolver_suite,
    };

    fn texassolver_profile() -> crate::AbstractionProfile {
        let preflop = StreetProfile {
            opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
            include_all_in: true,
        };
        let postflop = StreetProfile {
            opening_sizes: vec![
                OpeningSize::PotFractionBps(3_300),
                OpeningSize::PotFractionBps(6_600),
                OpeningSize::PotFractionBps(10_000),
            ],
            raise_sizes: vec![RaiseSize::PotFractionAfterCallBps(10_000)],
            include_all_in: true,
        };
        crate::AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop)
    }

    fn sample_spots() -> Vec<TexasSolverEvalSpot> {
        let config = HoldemConfig::default();
        let profile = texassolver_profile();
        vec![
            TexasSolverEvalSpot {
                id: "flop_oop_root".to_string(),
                description: "flop root with oop to act".to_string(),
                button_hole_cards: "AsKd".parse().unwrap(),
                big_blind_hole_cards: "QcJh".parse().unwrap(),
                button_range: "AsKd".parse::<Range>().unwrap(),
                big_blind_range: "QcJh".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 1,
                script: PostflopScriptedSpot::Flop(crate::ScriptedFlopSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::RaiseTo(250), gto_core::PlayerAction::Call],
                    flop: ["7c".parse().unwrap(), "4d".parse().unwrap(), "2s".parse().unwrap()],
                    flop_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "flop_ip_after_check".to_string(),
                description: "flop after oop checks".to_string(),
                button_hole_cards: "AcKc".parse().unwrap(),
                big_blind_hole_cards: "QhJh".parse().unwrap(),
                button_range: "AcKc".parse::<Range>().unwrap(),
                big_blind_range: "QhJh".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 1,
                script: PostflopScriptedSpot::Flop(crate::ScriptedFlopSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::Call, gto_core::PlayerAction::Check],
                    flop: ["Kh".parse().unwrap(), "9d".parse().unwrap(), "3c".parse().unwrap()],
                    flop_prefix_actions: vec![gto_core::PlayerAction::Check],
                }),
            },
            TexasSolverEvalSpot {
                id: "turn_oop_root".to_string(),
                description: "turn root with oop to act".to_string(),
                button_hole_cards: "AdQc".parse().unwrap(),
                big_blind_hole_cards: "JsTd".parse().unwrap(),
                button_range: "AdQc".parse::<Range>().unwrap(),
                big_blind_range: "JsTd".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 1,
                script: PostflopScriptedSpot::Turn(crate::ScriptedTurnSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::Call, gto_core::PlayerAction::Check],
                    flop: ["Ks".parse().unwrap(), "8c".parse().unwrap(), "4d".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2h".parse().unwrap(),
                    turn_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "turn_ip_after_check".to_string(),
                description: "turn after oop checks".to_string(),
                button_hole_cards: "AhQd".parse().unwrap(),
                big_blind_hole_cards: "JcTc".parse().unwrap(),
                button_range: "AhQd".parse::<Range>().unwrap(),
                big_blind_range: "JcTc".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 1,
                script: PostflopScriptedSpot::Turn(crate::ScriptedTurnSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::RaiseTo(250), gto_core::PlayerAction::Call],
                    flop: ["Qc".parse().unwrap(), "8h".parse().unwrap(), "5d".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2s".parse().unwrap(),
                    turn_prefix_actions: vec![gto_core::PlayerAction::Check],
                }),
            },
            TexasSolverEvalSpot {
                id: "river_ip_facing_bet".to_string(),
                description: "river facing an oop probe".to_string(),
                button_hole_cards: "AsQh".parse().unwrap(),
                big_blind_hole_cards: "JdTc".parse().unwrap(),
                button_range: "AsQh".parse::<Range>().unwrap(),
                big_blind_range: "JdTc".parse::<Range>().unwrap(),
                profile,
                iterations: 2,
                script: PostflopScriptedSpot::River(crate::ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::RaiseTo(250), gto_core::PlayerAction::Call],
                    flop: ["Qs".parse().unwrap(), "8d".parse().unwrap(), "4c".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2h".parse().unwrap(),
                    turn_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    river: "7s".parse().unwrap(),
                    river_prefix_actions: vec![gto_core::PlayerAction::BetTo(165)],
                }),
            },
        ]
    }

    fn sample_river_grading_spots() -> Vec<TexasSolverEvalSpot> {
        let config = HoldemConfig::default();
        let profile = texassolver_profile();
        vec![
            TexasSolverEvalSpot {
                id: "river_oop_root".to_string(),
                description: "river root with oop to act".to_string(),
                button_hole_cards: "AsQh".parse().unwrap(),
                big_blind_hole_cards: "JdTc".parse().unwrap(),
                button_range: "AsQh".parse::<Range>().unwrap(),
                big_blind_range: "JdTc".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 8,
                script: PostflopScriptedSpot::River(crate::ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::RaiseTo(250), gto_core::PlayerAction::Call],
                    flop: ["Qs".parse().unwrap(), "8d".parse().unwrap(), "4c".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2h".parse().unwrap(),
                    turn_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    river: "7s".parse().unwrap(),
                    river_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "river_oop_root_alt".to_string(),
                description: "second river root spot".to_string(),
                button_hole_cards: "AcKd".parse().unwrap(),
                big_blind_hole_cards: "QhJh".parse().unwrap(),
                button_range: "AcKd".parse::<Range>().unwrap(),
                big_blind_range: "QhJh".parse::<Range>().unwrap(),
                profile: profile.clone(),
                iterations: 8,
                script: PostflopScriptedSpot::River(crate::ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::Call, gto_core::PlayerAction::Check],
                    flop: ["Kh".parse().unwrap(), "9d".parse().unwrap(), "3c".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2s".parse().unwrap(),
                    turn_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    river: "7d".parse().unwrap(),
                    river_prefix_actions: vec![],
                }),
            },
            TexasSolverEvalSpot {
                id: "river_ip_facing_bet".to_string(),
                description: "river facing an oop probe".to_string(),
                button_hole_cards: "AsQh".parse().unwrap(),
                big_blind_hole_cards: "JdTc".parse().unwrap(),
                button_range: "AsQh".parse::<Range>().unwrap(),
                big_blind_range: "JdTc".parse::<Range>().unwrap(),
                profile,
                iterations: 8,
                script: PostflopScriptedSpot::River(crate::ScriptedRiverSpot {
                    config,
                    button_starting_stack: None,
                    big_blind_starting_stack: None,
                    preflop_actions: vec![gto_core::PlayerAction::RaiseTo(250), gto_core::PlayerAction::Call],
                    flop: ["Qs".parse().unwrap(), "8d".parse().unwrap(), "4c".parse().unwrap()],
                    flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    turn: "2h".parse().unwrap(),
                    turn_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                    river: "7s".parse().unwrap(),
                    river_prefix_actions: vec![gto_core::PlayerAction::BetTo(165)],
                }),
            },
        ]
    }

    fn strategy_node_for(
        combo: &str,
        actions: &[&str],
        probabilities: &[f64],
        children: &[(&str, TexasSolverActionNode)],
    ) -> TexasSolverActionNode {
        TexasSolverActionNode {
            node_type: Some("action_node".to_string()),
            actions: actions.iter().map(|action| (*action).to_string()).collect(),
            player: Some(0),
            strategy: Some(TexasSolverStrategyNode {
                actions: actions.iter().map(|action| (*action).to_string()).collect(),
                strategy: std::iter::once((
                    combo.to_string(),
                    probabilities.to_vec(),
                ))
                .collect(),
            }),
            childrens: children
                .iter()
                .map(|(label, node)| ((*label).to_string(), node.clone()))
                .collect(),
        }
    }

    fn live_action_labels(spot: &TexasSolverEvalSpot) -> Vec<String> {
        let state = spot.build_state().unwrap();
        crate::abstract_actions(&state, &spot.profile)
            .unwrap()
            .into_iter()
            .map(|action| super::normalize_abstract_action(action, &Some(state.clone())).to_string())
            .collect()
    }

    #[test]
    fn normalization_helpers_render_expected_strings() {
        assert_eq!(chips_to_bb_string(800), "8.0");
        assert_eq!(chips_to_bb_string(264), "2.64");
        assert_eq!(format_percentage_bps(3_300), "33");
        assert_eq!(format_percentage_bps(6_650), "66.5");
        assert_eq!(NormalizedTexasAction::parse("raise 8.0").unwrap().to_string(), "RAISE 8.0");
        assert_eq!(NormalizedTexasAction::parse("BET 2.64").unwrap().to_string(), "BET 2.64");
    }

    #[test]
    fn exporter_supports_varied_flop_turn_and_river_spots() {
        let exports = sample_spots()
            .into_iter()
            .map(|spot| spot.export_for_texassolver().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(exports.len(), 5);
        assert!(exports[0].script.contains("set_board 7c,4d,2s"));
        assert!(exports[0].script.contains("set_bet_sizes oop,flop,bet,33,66,100"));
        assert!(exports[1].target_path.iter().any(|action| action == "CHECK"));
        assert!(exports[2].script.contains("set_board Ks,8c,4d,2h"));
        assert!(exports[2].script.contains("set_bet_sizes oop,turn,donk,33,66,100"));
        assert!(exports[3].target_path.iter().any(|action| action == "CHECK"));
        assert!(exports[4].target_path.iter().any(|action| action == "BET 1.65"));
    }

    #[test]
    fn exporter_rejects_unsupported_raise_profiles_and_unequal_stacks() {
        let mut spots = sample_spots();
        let mut unsupported = spots.remove(0);
        unsupported.profile = smoke_blueprint_profile();
        assert!(unsupported.export_for_texassolver().is_err());

        let unequal = TexasSolverEvalSpot {
            id: "unequal_turn".to_string(),
            description: "unequal stacks".to_string(),
            button_hole_cards: "AsKd".parse().unwrap(),
            big_blind_hole_cards: "QcJh".parse().unwrap(),
            button_range: "AsKd".parse().unwrap(),
            big_blind_range: "QcJh".parse().unwrap(),
            profile: texassolver_profile(),
            iterations: 4,
            script: PostflopScriptedSpot::Turn(crate::ScriptedTurnSpot {
                config: HoldemConfig::default(),
                button_starting_stack: Some(9_000),
                big_blind_starting_stack: Some(10_000),
                preflop_actions: vec![gto_core::PlayerAction::Call, gto_core::PlayerAction::Check],
                flop: ["Kh".parse().unwrap(), "8d".parse().unwrap(), "4s".parse().unwrap()],
                flop_actions: vec![gto_core::PlayerAction::Check, gto_core::PlayerAction::Check],
                turn: "2c".parse().unwrap(),
                turn_prefix_actions: vec![],
            }),
        };
        assert!(unequal.export_for_texassolver().is_err());
    }

    #[test]
    fn grading_handles_matches_mismatches_and_missing_data() {
        let spots = sample_river_grading_spots();
        let exports = spots
            .iter()
            .map(|spot| spot.export_for_texassolver().unwrap())
            .collect::<Vec<_>>();
        let actions0 = live_action_labels(&spots[0]);
        let our0 = super::normalize_abstract_action(spots[0].our_action().unwrap(), &Some(spots[0].build_state().unwrap())).to_string();
        let probabilities0 = actions0
            .iter()
            .map(|action| if *action == our0 { 1.0 } else { 0.0 })
            .collect::<Vec<_>>();
        let actions1 = live_action_labels(&spots[1]);
        let our1 = super::normalize_abstract_action(spots[1].our_action().unwrap(), &Some(spots[1].build_state().unwrap())).to_string();
        let mismatch_best = actions1
            .iter()
            .find(|action| **action != our1)
            .cloned()
            .unwrap_or_else(|| our1.clone());
        let probabilities1 = actions1
            .iter()
            .map(|action| if *action == mismatch_best { 1.0 } else { 0.0 })
            .collect::<Vec<_>>();

        let references = TexasSolverReferenceSuite {
            format_version: TexasSolverReferenceSuite::FORMAT_VERSION,
            suite_name: "smoke".to_string(),
            references: vec![
                TexasSolverSpotReference {
                    spot_id: spots[0].id.clone(),
                    signature: exports[0].signature.clone(),
                    root: strategy_node_for(
                        &exports[0].actor_combo,
                        &actions0.iter().map(|action| action.as_str()).collect::<Vec<_>>(),
                        &probabilities0,
                        &[],
                    ),
                    ev_root: Some(super::TexasSolverEvNode {
                        actions: actions0.clone(),
                        evs: std::iter::once((
                            exports[0].actor_combo.clone(),
                            (0..actions0.len()).map(|index| 1.25 - index as f64).collect(),
                        ))
                        .collect(),
                        childrens: BTreeMap::new(),
                    }),
                },
                TexasSolverSpotReference {
                    spot_id: spots[1].id.clone(),
                    signature: exports[1].signature.clone(),
                    root: strategy_node_for(
                        &exports[1].actor_combo,
                        &actions1.iter().map(|action| action.as_str()).collect::<Vec<_>>(),
                        &probabilities1,
                        &[],
                    ),
                    ev_root: None,
                },
                TexasSolverSpotReference {
                    spot_id: spots[2].id.clone(),
                    signature: "mismatch".to_string(),
                    root: strategy_node_for(&exports[2].actor_combo, &["CHECK"], &[1.0], &[]),
                    ev_root: None,
                },
            ],
        };

        let suite = TexasSolverSpotSuite {
            format_version: TexasSolverSpotSuite::FORMAT_VERSION,
            suite_name: "smoke".to_string(),
            spots,
        };
        let report = grade_texassolver_suite(&suite, &references);

        assert_eq!(report.results.len(), 3);
        assert_eq!(report.results[0].status, TexasSolverGradeStatus::Graded);
        assert_eq!(report.results[0].action_matches, Some(true));
        assert!(report.results[0].ev_gap.is_some());
        assert_eq!(report.results[1].status, TexasSolverGradeStatus::Graded);
        assert_eq!(report.results[1].action_matches, Some(false));
        assert_eq!(report.results[2].status, TexasSolverGradeStatus::SignatureMismatch);
        assert_eq!(report.summary().total_spots, 3);
    }

    #[test]
    fn grading_reports_missing_combo_and_missing_reference() {
        let spots = sample_river_grading_spots();
        let exports = spots
            .iter()
            .map(|spot| spot.export_for_texassolver().unwrap())
            .collect::<Vec<_>>();
        let references = TexasSolverReferenceSuite {
            format_version: TexasSolverReferenceSuite::FORMAT_VERSION,
            suite_name: "smoke".to_string(),
            references: vec![TexasSolverSpotReference {
                spot_id: spots[0].id.clone(),
                signature: exports[0].signature.clone(),
                root: strategy_node_for("AhKh", &["CHECK", "BET 1.65"], &[0.5, 0.5], &[]),
                ev_root: None,
            }],
        };
        let suite = TexasSolverSpotSuite {
            format_version: TexasSolverSpotSuite::FORMAT_VERSION,
            suite_name: "smoke".to_string(),
            spots: vec![spots[0].clone(), spots[1].clone()],
        };

        let report = grade_texassolver_suite(&suite, &references);
        assert_eq!(report.results[0].status, TexasSolverGradeStatus::MissingCombo);
        assert_eq!(report.results[1].status, TexasSolverGradeStatus::MissingReference);
    }

    #[test]
    fn committed_smoke_spot_fixture_round_trips_and_exports() {
        let fixture = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/eval/spots/texassolver_smoke.json"
        ));
        let suite = TexasSolverSpotSuite::from_json_str(fixture).unwrap();
        assert_eq!(suite.suite_name, "smoke");
        assert_eq!(suite.spots.len(), 3);
        assert_eq!(TexasSolverSpotSuite::from_json_str(&suite.to_json_string().unwrap()).unwrap(), suite);
        assert!(suite
            .spots
            .iter()
            .all(|spot| spot.export_for_texassolver().is_ok()));
    }

    #[test]
    fn committed_smoke_reference_fixture_grades_cleanly() {
        let suite = TexasSolverSpotSuite::from_json_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/eval/spots/texassolver_smoke.json"
        )))
        .unwrap();
        let references = TexasSolverReferenceSuite::from_json_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/eval/texassolver/smoke_reference.json"
        )))
        .unwrap();
        let report = grade_texassolver_suite(&suite, &references);

        assert_eq!(report.results.len(), 3);
        assert!(report
            .results
            .iter()
            .all(|result| result.status == TexasSolverGradeStatus::Graded));
        assert!(report
            .results
            .iter()
            .all(|result| result.action_matches == Some(true)));
    }
}
