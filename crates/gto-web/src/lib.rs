#![forbid(unsafe_code)]
#![doc = "Browser-facing WASM adapter for the reusable poker engine and solver crates."]

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gto_core::{
    DEFAULT_RNG_SEED, Deck, DeterministicRng, DuplicateCardError, HandOutcome, HandPhase,
    HeadsUpMatchState, HistoryEvent, HoldemConfig, HoldemHandState, HoldemStateError, MatchConfig,
    MatchPlayer, MatchStateError, Player, PlayerAction, Street, rng_from_seed,
};
use gto_solver::{
    AbstractAction, BlueprintArtifactError, BlueprintBot, FullHandBlueprintArtifact, HybridBot,
    HybridBotConfig, HybridBotError, HybridPostflopProfile, abstract_actions,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebSeat {
    Button,
    BigBlind,
}

impl From<WebSeat> for Player {
    fn from(value: WebSeat) -> Self {
        match value {
            WebSeat::Button => Self::Button,
            WebSeat::BigBlind => Self::BigBlind,
        }
    }
}

impl From<Player> for WebSeat {
    fn from(value: Player) -> Self {
        match value {
            Player::Button => Self::Button,
            Player::BigBlind => Self::BigBlind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebBotMode {
    Blueprint,
    HybridFast,
    HybridPlay,
}

impl WebBotMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blueprint => "blueprint",
            Self::HybridFast => "hybrid-fast",
            Self::HybridPlay => "hybrid-play",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSessionConfig {
    pub seed: u64,
    pub human_seat: WebSeat,
    pub bot_mode: WebBotMode,
    pub blueprint_artifact_json: Option<String>,
}

impl Default for WebSessionConfig {
    fn default() -> Self {
        Self {
            seed: DEFAULT_RNG_SEED,
            human_seat: WebSeat::Button,
            bot_mode: WebBotMode::HybridFast,
            blueprint_artifact_json: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebActionChoice {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPlayerSnapshot {
    pub seat: WebSeat,
    pub stack: u64,
    pub total_contribution: u64,
    pub street_contribution: u64,
    pub folded: bool,
    pub hole_cards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSessionSnapshot {
    pub hand_number: u64,
    pub human_seat: WebSeat,
    pub bot_seat: WebSeat,
    pub bot_mode: WebBotMode,
    pub match_over: bool,
    pub street: String,
    pub phase: String,
    pub current_actor: Option<WebSeat>,
    pub pot: u64,
    pub board_cards: Vec<String>,
    pub button: WebPlayerSnapshot,
    pub big_blind: WebPlayerSnapshot,
    pub legal_actions: Vec<WebActionChoice>,
    pub history: Vec<String>,
    pub status: String,
    pub terminal_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrowserSession {
    config: WebSessionConfig,
    rng: DeterministicRng,
    match_state: HeadsUpMatchState,
    human_player: MatchPlayer,
    bot_player: MatchPlayer,
    bot: BotController,
    deal: DealtHand,
    state: HoldemHandState,
}

impl BrowserSession {
    pub fn new(config: WebSessionConfig) -> Result<Self, WebSessionError> {
        let human_player = MatchPlayer::PlayerOne;
        let bot_player = MatchPlayer::PlayerTwo;
        let match_config = MatchConfig::new(
            HoldemConfig::default().starting_stack,
            HoldemConfig::default().small_blind,
            HoldemConfig::default().big_blind,
            if config.human_seat == WebSeat::Button {
                human_player
            } else {
                bot_player
            },
        )
        .map_err(WebSessionError::MatchConfig)?;
        let mut match_state = HeadsUpMatchState::new(match_config).map_err(WebSessionError::MatchConfig)?;
        let artifact = load_blueprint_artifact(config.blueprint_artifact_json.as_deref())?;
        let bot = BotController::new(config.bot_mode, artifact);
        let mut rng = rng_from_seed(config.seed);
        let deal = DealtHand::deal(&mut rng)?;
        let state = match_state
            .start_next_hand(deal.button, deal.big_blind)
            .map_err(WebSessionError::MatchState)?;

        let mut session = Self {
            config,
            rng,
            match_state,
            human_player,
            bot_player,
            bot,
            deal,
            state,
        };
        session.advance_until_human_turn_or_terminal()?;
        session.settle_terminal_hand_if_needed()?;
        Ok(session)
    }

    pub fn snapshot(&self) -> Result<WebSessionSnapshot, WebSessionError> {
        let terminal_visible = self.state.current_outcome().is_some();
        let button = self.player_snapshot(Player::Button, terminal_visible);
        let big_blind = self.player_snapshot(Player::BigBlind, terminal_visible);
        let terminal_summary = self.state.current_outcome().map(format_outcome_summary);

        Ok(WebSessionSnapshot {
            hand_number: self.match_state.hand_number(),
            human_seat: WebSeat::from(self.human_role()),
            bot_seat: WebSeat::from(self.bot_role()),
            bot_mode: self.config.bot_mode,
            match_over: self.match_state.match_over(),
            street: self.state.street().to_string(),
            phase: phase_label(self.state.phase()).to_string(),
            current_actor: self.state.current_actor().map(WebSeat::from),
            pot: self.state.pot(),
            board_cards: self
                .state
                .board()
                .cards()
                .iter()
                .map(ToString::to_string)
                .collect(),
            button,
            big_blind,
            legal_actions: self.legal_action_choices()?,
            history: self.state.history().iter().map(format_history_event).collect(),
            status: self.status_line(),
            terminal_summary,
        })
    }

    pub fn apply_human_action(
        &mut self,
        action_id: &str,
    ) -> Result<WebSessionSnapshot, WebSessionError> {
        let Some(actor) = self.state.current_actor() else {
            return Err(WebSessionError::HumanActionUnavailable);
        };
        if actor != self.human_role() {
            return Err(WebSessionError::HumanActionUnavailable);
        }

        let actions = self.human_actions()?;
        let Some(action) = actions.into_iter().find(|action| action_id_for_abstract(*action) == action_id)
        else {
            return Err(WebSessionError::UnknownActionId(action_id.to_owned()));
        };

        self.state.apply_action(action.to_player_action())?;
        self.advance_until_next_decision_or_terminal()?;
        self.settle_terminal_hand_if_needed()?;
        self.snapshot()
    }

    pub fn advance_bot(&mut self) -> Result<WebSessionSnapshot, WebSessionError> {
        self.advance_bot_once_then_reveal_until_next_decision()?;
        self.settle_terminal_hand_if_needed()?;
        self.snapshot()
    }

    pub fn reset_hand(&mut self) -> Result<WebSessionSnapshot, WebSessionError> {
        if self.match_state.match_over() {
            return Err(WebSessionError::MatchState(MatchStateError::MatchAlreadyOver));
        }
        self.deal = DealtHand::deal(&mut self.rng)?;
        self.state = self
            .match_state
            .start_next_hand(self.deal.button, self.deal.big_blind)
            .map_err(WebSessionError::MatchState)?;
        self.advance_until_human_turn_or_terminal()?;
        self.settle_terminal_hand_if_needed()?;
        self.snapshot()
    }

    fn advance_until_human_turn_or_terminal(&mut self) -> Result<(), WebSessionError> {
        loop {
            match self.state.phase() {
                HandPhase::AwaitingBoard { next_street } => self.reveal_board(next_street)?,
                HandPhase::BettingRound { actor, .. } if actor == self.bot_role() => {
                    let action = self.bot.choose_action(self.bot_role(), &self.state)?;
                    self.state.apply_action(action)?;
                }
                HandPhase::BettingRound { .. } | HandPhase::Terminal { .. } => break,
            }
        }
        Ok(())
    }

    fn advance_bot_once_then_reveal_until_next_decision(&mut self) -> Result<(), WebSessionError> {
        let mut bot_acted = false;

        loop {
            match self.state.phase() {
                HandPhase::AwaitingBoard { next_street } => self.reveal_board(next_street)?,
                HandPhase::BettingRound { actor, .. } if actor == self.bot_role() && !bot_acted => {
                    let action = self.bot.choose_action(self.bot_role(), &self.state)?;
                    self.state.apply_action(action)?;
                    bot_acted = true;
                }
                HandPhase::BettingRound { .. } | HandPhase::Terminal { .. } => break,
            }
        }

        Ok(())
    }

    fn advance_until_next_decision_or_terminal(&mut self) -> Result<(), WebSessionError> {
        loop {
            match self.state.phase() {
                HandPhase::AwaitingBoard { next_street } => self.reveal_board(next_street)?,
                HandPhase::BettingRound { .. } | HandPhase::Terminal { .. } => break,
            }
        }
        Ok(())
    }

    fn reveal_board(&mut self, street: Street) -> Result<(), WebSessionError> {
        match street {
            Street::Flop => self
                .state
                .deal_flop([self.deal.board[0], self.deal.board[1], self.deal.board[2]])?,
            Street::Turn => self.state.deal_turn(self.deal.board[3])?,
            Street::River => self.state.deal_river(self.deal.board[4])?,
            Street::Preflop => return Err(WebSessionError::UnexpectedPreflopBoardDeal),
        }
        Ok(())
    }

    fn human_actions(&self) -> Result<Vec<AbstractAction>, WebSessionError> {
        if self.state.current_actor() != Some(self.human_role()) {
            return Ok(Vec::new());
        }
        abstract_actions(&self.state, self.bot.human_profile()).map_err(WebSessionError::State)
    }

    fn legal_action_choices(&self) -> Result<Vec<WebActionChoice>, WebSessionError> {
        let actions = self.human_actions()?;
        Ok(actions
            .into_iter()
            .map(|action| WebActionChoice {
                id: action_id_for_abstract(action),
                label: action_label(action, HoldemConfig::default()),
            })
            .collect())
    }

    fn player_snapshot(&self, player: Player, terminal_visible: bool) -> WebPlayerSnapshot {
        let snapshot = self.state.player(player);
        let cards_visible = player == self.human_role() || terminal_visible;

        WebPlayerSnapshot {
            seat: WebSeat::from(player),
            stack: self.match_state.display_stack_for_seat(&self.state, player),
            total_contribution: snapshot.total_contribution,
            street_contribution: snapshot.street_contribution,
            folded: snapshot.folded,
            hole_cards: if cards_visible {
                vec![
                    snapshot.hole_cards.first().to_string(),
                    snapshot.hole_cards.second().to_string(),
                ]
            } else {
                Vec::new()
            },
        }
    }

    fn status_line(&self) -> String {
        match self.state.phase() {
            HandPhase::BettingRound { actor, street } if actor == self.human_role() => {
                format!("Your turn on {street}.")
            }
            HandPhase::BettingRound { actor, street } => {
                format!("Bot to act on {street} ({actor}).")
            }
            HandPhase::AwaitingBoard { next_street } => format!("Dealing {next_street}."),
            HandPhase::Terminal { outcome } if self.match_state.match_over() => {
                format!("Match over. {}", format_outcome_summary(outcome))
            }
            HandPhase::Terminal { outcome } => format_outcome_summary(outcome),
        }
    }

    fn human_role(&self) -> Player {
        self.match_state.seat_for_player(self.human_player)
    }

    fn bot_role(&self) -> Player {
        self.match_state.seat_for_player(self.bot_player)
    }

    fn settle_terminal_hand_if_needed(&mut self) -> Result<(), WebSessionError> {
        if self.state.current_outcome().is_some() && self.match_state.hand_in_progress() {
            self.match_state
                .complete_hand(&self.state)
                .map_err(WebSessionError::MatchState)?;
        }
        Ok(())
    }
}

#[wasm_bindgen(js_name = PokerSession)]
pub struct WasmPokerSession {
    inner: BrowserSession,
}

#[wasm_bindgen(js_class = PokerSession)]
impl WasmPokerSession {
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> Result<Self, JsValue> {
        let config = serde_wasm_bindgen::from_value::<WebSessionConfig>(config)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let inner = BrowserSession::new(config).map_err(js_error)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name = snapshot)]
    pub fn snapshot_js(&self) -> Result<JsValue, JsValue> {
        to_js_value(&self.inner.snapshot().map_err(js_error)?)
    }

    #[wasm_bindgen(js_name = applyHumanAction)]
    pub fn apply_human_action_js(&mut self, action_id: String) -> Result<JsValue, JsValue> {
        to_js_value(&self.inner.apply_human_action(&action_id).map_err(js_error)?)
    }

    #[wasm_bindgen(js_name = advanceBot)]
    pub fn advance_bot_js(&mut self) -> Result<JsValue, JsValue> {
        to_js_value(&self.inner.advance_bot().map_err(js_error)?)
    }

    #[wasm_bindgen(js_name = resetHand)]
    pub fn reset_hand_js(&mut self) -> Result<JsValue, JsValue> {
        to_js_value(&self.inner.reset_hand().map_err(js_error)?)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum BotController {
    Blueprint(BlueprintBot),
    Hybrid(HybridBot),
}

impl BotController {
    fn new(mode: WebBotMode, artifact: FullHandBlueprintArtifact) -> Self {
        match mode {
            WebBotMode::Blueprint => Self::Blueprint(BlueprintBot::new(artifact)),
            WebBotMode::HybridFast => Self::Hybrid(HybridBot::new(HybridBotConfig::new(
                artifact,
                HybridPostflopProfile::Fast,
            ))),
            WebBotMode::HybridPlay => Self::Hybrid(HybridBot::new(HybridBotConfig::new(
                artifact,
                HybridPostflopProfile::Play,
            ))),
        }
    }

    fn choose_action(&self, bot_role: Player, state: &HoldemHandState) -> Result<PlayerAction, WebSessionError> {
        match self {
            Self::Blueprint(bot) => bot
                .choose_action(bot_role, state)
                .map_err(WebSessionError::BlueprintBot),
            Self::Hybrid(bot) => bot
                .choose_action(bot_role, state)
                .map_err(WebSessionError::HybridBot),
        }
    }

    fn human_profile(&self) -> &gto_solver::AbstractionProfile {
        match self {
            Self::Blueprint(bot) => bot.profile(),
            Self::Hybrid(bot) => bot.blueprint_profile(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DealtHand {
    button: gto_core::HoleCards,
    big_blind: gto_core::HoleCards,
    board: [gto_core::Card; 5],
}

impl DealtHand {
    fn deal(rng: &mut DeterministicRng) -> Result<Self, WebSessionError> {
        let mut deck = Deck::standard();
        deck.shuffle(rng);

        let button = gto_core::HoleCards::new(draw_card(&mut deck)?, draw_card(&mut deck)?)
            .map_err(WebSessionError::DuplicateCard)?;
        let big_blind = gto_core::HoleCards::new(draw_card(&mut deck)?, draw_card(&mut deck)?)
            .map_err(WebSessionError::DuplicateCard)?;

        Ok(Self {
            button,
            big_blind,
            board: [
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
            ],
        })
    }
}

fn load_blueprint_artifact(
    artifact_json: Option<&str>,
) -> Result<FullHandBlueprintArtifact, WebSessionError> {
    match artifact_json {
        Some(json) => FullHandBlueprintArtifact::from_json_str(json)
            .map_err(WebSessionError::BlueprintArtifact),
        None => Ok(FullHandBlueprintArtifact::smoke_default()),
    }
}

fn draw_card(deck: &mut Deck) -> Result<gto_core::Card, WebSessionError> {
    deck.draw().ok_or(WebSessionError::DeckExhausted)
}

fn phase_label(phase: HandPhase) -> &'static str {
    match phase {
        HandPhase::BettingRound { .. } => "bettingRound",
        HandPhase::AwaitingBoard { .. } => "awaitingBoard",
        HandPhase::Terminal { .. } => "terminal",
    }
}

fn action_id_for_abstract(action: AbstractAction) -> String {
    match action {
        AbstractAction::Fold => "fold".to_owned(),
        AbstractAction::Check => "check".to_owned(),
        AbstractAction::Call => "call".to_owned(),
        AbstractAction::BetTo(total) => format!("betTo:{total}"),
        AbstractAction::RaiseTo(total) => format!("raiseTo:{total}"),
        AbstractAction::AllIn(total) => format!("allIn:{total}"),
    }
}

fn action_label(action: AbstractAction, config: HoldemConfig) -> String {
    match action {
        AbstractAction::Fold => "Fold".to_owned(),
        AbstractAction::Check => "Check".to_owned(),
        AbstractAction::Call => "Call".to_owned(),
        AbstractAction::BetTo(total) => format!("Bet to {}", chips_to_big_blinds(total, config)),
        AbstractAction::RaiseTo(total) => {
            format!("Raise to {}", chips_to_big_blinds(total, config))
        }
        AbstractAction::AllIn(total) => format!("All-in to {}", chips_to_big_blinds(total, config)),
    }
}

fn chips_to_big_blinds(chips: u64, config: HoldemConfig) -> String {
    let tenths = chips.saturating_mul(10) / config.big_blind;
    format!("{}.{} bb", tenths / 10, tenths % 10)
}

fn format_history_event(event: &HistoryEvent) -> String {
    match event {
        HistoryEvent::BlindPosted { player, amount } => {
            format!("{player} posts {}", chips_to_big_blinds(*amount, HoldemConfig::default()))
        }
        HistoryEvent::ActionApplied {
            street,
            player,
            action,
            ..
        } => format!("{street}: {player} {}", format_player_action(*action)),
        HistoryEvent::BoardDealt { street, cards } => format!(
            "{street}: {}",
            cards
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        HistoryEvent::HandCompleted { outcome } => format_outcome_summary(*outcome),
    }
}

fn format_player_action(action: PlayerAction) -> String {
    match action {
        PlayerAction::Fold => "folds".to_owned(),
        PlayerAction::Check => "checks".to_owned(),
        PlayerAction::Call => "calls".to_owned(),
        PlayerAction::BetTo(total) => {
            format!("bets to {}", chips_to_big_blinds(total, HoldemConfig::default()))
        }
        PlayerAction::RaiseTo(total) => {
            format!("raises to {}", chips_to_big_blinds(total, HoldemConfig::default()))
        }
        PlayerAction::AllIn => "moves all-in".to_owned(),
    }
}

fn format_outcome_summary(outcome: HandOutcome) -> String {
    match outcome {
        HandOutcome::Uncontested { winner, pot, .. } => {
            format!("{winner} wins uncontested for {}", chips_to_big_blinds(pot, HoldemConfig::default()))
        }
        HandOutcome::Showdown { result, pot, .. } => {
            let outcome_label = match result.ordering() {
                std::cmp::Ordering::Less => "big-blind wins at showdown",
                std::cmp::Ordering::Greater => "button wins at showdown",
                std::cmp::Ordering::Equal => "showdown split pot",
            };
            format!("{outcome_label} for {}", chips_to_big_blinds(pot, HoldemConfig::default()))
        }
    }
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn js_error(error: impl Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[derive(Debug)]
pub enum WebSessionError {
    DeckExhausted,
    UnexpectedPreflopBoardDeal,
    UnknownActionId(String),
    HumanActionUnavailable,
    DuplicateCard(DuplicateCardError),
    State(HoldemStateError),
    BlueprintArtifact(BlueprintArtifactError),
    BlueprintBot(gto_solver::BlueprintBotError),
    HybridBot(HybridBotError),
    MatchConfig(gto_core::MatchConfigError),
    MatchState(MatchStateError),
}

impl Display for WebSessionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeckExhausted => formatter.write_str("deck ran out of cards while dealing a hand"),
            Self::UnexpectedPreflopBoardDeal => {
                formatter.write_str("attempted to deal preflop board cards")
            }
            Self::UnknownActionId(action_id) => {
                write!(formatter, "unknown or unavailable action id `{action_id}`")
            }
            Self::HumanActionUnavailable => formatter.write_str("human action is not available right now"),
            Self::DuplicateCard(error) => write!(formatter, "{error}"),
            Self::State(error) => write!(formatter, "{error}"),
            Self::BlueprintArtifact(error) => write!(formatter, "{error}"),
            Self::BlueprintBot(error) => write!(formatter, "{error}"),
            Self::HybridBot(error) => write!(formatter, "{error}"),
            Self::MatchConfig(error) => write!(formatter, "{error}"),
            Self::MatchState(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for WebSessionError {}

impl From<HoldemStateError> for WebSessionError {
    fn from(value: HoldemStateError) -> Self {
        Self::State(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserSession, WebBotMode, WebSeat, WebSessionConfig};
    use gto_solver::FullHandBlueprintArtifact;
    use std::collections::BTreeSet;

    #[test]
    fn new_session_exposes_hero_decision_or_terminal_state() {
        let session = BrowserSession::new(WebSessionConfig::default()).unwrap();
        let snapshot = session.snapshot().unwrap();

        assert_eq!(snapshot.human_seat, WebSeat::Button);
        assert!(
            snapshot.current_actor == Some(WebSeat::Button) || snapshot.terminal_summary.is_some()
        );
        assert_eq!(snapshot.big_blind.hole_cards.len(), 0);
    }

    #[test]
    fn terminal_snapshots_reveal_both_players_hole_cards_even_without_showdown() {
        let mut session = BrowserSession::new(WebSessionConfig {
            seed: 5,
            bot_mode: WebBotMode::Blueprint,
            ..WebSessionConfig::default()
        })
        .unwrap();

        for _ in 0..32 {
            let snapshot = session.snapshot().unwrap();
            if snapshot.terminal_summary.is_some() {
                assert_eq!(snapshot.button.hole_cards.len(), 2);
                assert_eq!(snapshot.big_blind.hole_cards.len(), 2);
                return;
            }

            let action_id = snapshot
                .legal_actions
                .iter()
                .find(|action| action.id == "fold")
                .map(|action| action.id.clone())
                .unwrap_or_else(|| preferred_action_id(&snapshot));
            session.apply_human_action(&action_id).unwrap();
        }

        panic!("expected test hand to reach a terminal state within the action budget");
    }

    #[test]
    fn new_session_auto_advances_when_bot_acts_first() {
        let session = BrowserSession::new(WebSessionConfig {
            human_seat: WebSeat::BigBlind,
            ..WebSessionConfig::default()
        })
        .unwrap();
        let snapshot = session.snapshot().unwrap();

        assert!(
            snapshot.current_actor == Some(WebSeat::BigBlind) || snapshot.terminal_summary.is_some()
        );
    }

    #[test]
    fn applying_human_action_progresses_history() {
        let mut session = BrowserSession::new(WebSessionConfig::default()).unwrap();
        let before = session.snapshot().unwrap();
        let action = before.legal_actions.first().unwrap().id.clone();

        let after = session.apply_human_action(&action).unwrap();

        assert!(after.history.len() > before.history.len());
        assert!(
            after.terminal_summary.is_some()
                || after.current_actor == Some(WebSeat::Button)
                || after.current_actor == Some(WebSeat::BigBlind)
        );
    }

    #[test]
    fn reset_hand_advances_hand_counter() {
        let mut session = BrowserSession::new(WebSessionConfig::default()).unwrap();
        play_hand_to_terminal(&mut session);
        let before = session.snapshot().unwrap();

        let after = session.reset_hand().unwrap();

        assert_eq!(after.hand_number, before.hand_number + 1);
        assert_ne!(
            after.button.hole_cards,
            before.button.hole_cards,
            "reset should usually produce a fresh deal under deterministic rng progression"
        );
    }

    #[test]
    fn reset_hand_rotates_seats_and_carries_bankrolls() {
        let mut session = BrowserSession::new(WebSessionConfig::default()).unwrap();

        session.apply_human_action("fold").unwrap();
        let terminal = session.snapshot().unwrap();
        assert_eq!(terminal.human_seat, WebSeat::Button);
        assert_eq!(terminal.button.stack, 9_950);
        assert_eq!(terminal.big_blind.stack, 10_050);

        let next = session.reset_hand().unwrap();
        assert_eq!(next.hand_number, 2);
        assert_eq!(next.human_seat, WebSeat::BigBlind);
        assert_eq!(next.bot_seat, WebSeat::Button);
        assert_ne!(
            next.big_blind.stack, 9_900,
            "expected carried bankroll rather than a fresh 100bb reset"
        );
    }

    #[test]
    fn accepts_explicit_artifact_json() {
        let artifact_json = FullHandBlueprintArtifact::smoke_default()
            .to_json_string()
            .unwrap();
        let session = BrowserSession::new(WebSessionConfig {
            bot_mode: WebBotMode::Blueprint,
            blueprint_artifact_json: Some(artifact_json),
            ..WebSessionConfig::default()
        })
        .unwrap();

        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.bot_mode, WebBotMode::Blueprint);
    }

    #[test]
    fn seeded_blueprint_sessions_are_reproducible_under_same_human_policy() {
        let config = WebSessionConfig {
            seed: 17,
            bot_mode: WebBotMode::Blueprint,
            ..WebSessionConfig::default()
        };
        let mut left = BrowserSession::new(config.clone()).unwrap();
        let mut right = BrowserSession::new(config).unwrap();

        for _ in 0..24 {
            let left_snapshot = left.snapshot().unwrap();
            let right_snapshot = right.snapshot().unwrap();
            assert_eq!(left_snapshot, right_snapshot);

            if left_snapshot.terminal_summary.is_some() {
                return;
            }

            if left_snapshot.current_actor == Some(left_snapshot.bot_seat) {
                let next_left = left.advance_bot().unwrap();
                let next_right = right.advance_bot().unwrap();
                assert_eq!(next_left, next_right);
                continue;
            }

            let action_id = preferred_action_id(&left_snapshot);
            let next_left = left.apply_human_action(&action_id).unwrap();
            let next_right = right.apply_human_action(&action_id).unwrap();
            assert_eq!(next_left, next_right);
        }

        panic!("expected seeded sessions to reach a terminal state within the action budget");
    }

    #[test]
    fn browser_session_exposes_unique_legal_actions_and_reaches_terminal_state() {
        for bot_mode in [WebBotMode::Blueprint, WebBotMode::HybridFast] {
            for seed in [3_u64, 7, 19] {
                let mut session = BrowserSession::new(WebSessionConfig {
                    seed,
                    bot_mode,
                    ..WebSessionConfig::default()
                })
                .unwrap();

                play_hand_to_terminal(&mut session);
            }
        }
    }

    fn play_hand_to_terminal(session: &mut BrowserSession) {
        for _ in 0..32 {
            let snapshot = session.snapshot().unwrap();
            if snapshot.terminal_summary.is_some() {
                assert!(snapshot.legal_actions.is_empty());
                return;
            }

            if snapshot.current_actor == Some(snapshot.bot_seat) {
                session.advance_bot().unwrap();
                continue;
            }

            assert_eq!(snapshot.current_actor, Some(snapshot.human_seat));
            assert!(
                !snapshot.legal_actions.is_empty(),
                "human turn should expose legal actions"
            );

            let ids = snapshot
                .legal_actions
                .iter()
                .map(|action| action.id.clone())
                .collect::<Vec<_>>();
            let unique_ids = ids.iter().cloned().collect::<BTreeSet<_>>();
            assert_eq!(
                ids.len(),
                unique_ids.len(),
                "browser action ids should stay unique"
            );

            let action_id = preferred_action_id(&snapshot);
            assert!(
                ids.contains(&action_id),
                "chosen action `{action_id}` should be present in legal actions"
            );
            session.apply_human_action(&action_id).unwrap();
        }

        panic!("expected browser session to complete the hand within the action budget");
    }

    fn preferred_action_id(snapshot: &super::WebSessionSnapshot) -> String {
        for preferred in ["check", "call"] {
            if let Some(action) = snapshot
                .legal_actions
                .iter()
                .find(|action| action.id == preferred)
            {
                return action.id.clone();
            }
        }

        snapshot
            .legal_actions
            .first()
            .expect("non-terminal snapshot should expose at least one legal action")
            .id
            .clone()
    }
}
