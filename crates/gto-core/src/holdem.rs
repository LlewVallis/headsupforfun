use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{
    Board, Card, HeadsUpPayout, HoleCards, OddChipRecipient, ShowdownError, ShowdownResult,
    resolve_holdem_showdown,
};

pub type Chips = u64;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Player {
    Button,
    BigBlind,
}

impl Player {
    pub const ALL: [Self; 2] = [Self::Button, Self::BigBlind];

    pub const fn opponent(self) -> Self {
        match self {
            Self::Button => Self::BigBlind,
            Self::BigBlind => Self::Button,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Button => 0,
            Self::BigBlind => 1,
        }
    }
}

impl Display for Player {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Button => "button",
            Self::BigBlind => "big-blind",
        };
        formatter.write_str(label)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

impl Street {
    const fn next(self) -> Option<Self> {
        match self {
            Self::Preflop => Some(Self::Flop),
            Self::Flop => Some(Self::Turn),
            Self::Turn => Some(Self::River),
            Self::River => None,
        }
    }
}

impl Display for Street {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Preflop => "preflop",
            Self::Flop => "flop",
            Self::Turn => "turn",
            Self::River => "river",
        };
        formatter.write_str(label)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldemConfig {
    pub starting_stack: Chips,
    pub small_blind: Chips,
    pub big_blind: Chips,
}

impl HoldemConfig {
    pub fn new(
        starting_stack: Chips,
        small_blind: Chips,
        big_blind: Chips,
    ) -> Result<Self, HoldemConfigError> {
        let config = Self {
            starting_stack,
            small_blind,
            big_blind,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(self) -> Result<(), HoldemConfigError> {
        if self.small_blind == 0 {
            return Err(HoldemConfigError::SmallBlindMustBePositive);
        }
        if self.big_blind == 0 {
            return Err(HoldemConfigError::BigBlindMustBePositive);
        }
        if self.small_blind >= self.big_blind {
            return Err(HoldemConfigError::SmallBlindMustBeLessThanBigBlind);
        }
        if self.starting_stack < self.big_blind {
            return Err(HoldemConfigError::StartingStackMustCoverBigBlind);
        }
        Ok(())
    }
}

impl Default for HoldemConfig {
    fn default() -> Self {
        Self {
            starting_stack: 10_000,
            small_blind: 50,
            big_blind: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldemConfigError {
    SmallBlindMustBePositive,
    BigBlindMustBePositive,
    SmallBlindMustBeLessThanBigBlind,
    StartingStackMustCoverBigBlind,
}

impl Display for HoldemConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::SmallBlindMustBePositive => formatter.write_str("small blind must be positive"),
            Self::BigBlindMustBePositive => formatter.write_str("big blind must be positive"),
            Self::SmallBlindMustBeLessThanBigBlind => {
                formatter.write_str("small blind must be less than big blind")
            }
            Self::StartingStackMustCoverBigBlind => {
                formatter.write_str("starting stack must be at least the big blind")
            }
        }
    }
}

impl Error for HoldemConfigError {}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    Fold,
    Check,
    Call,
    BetTo(Chips),
    RaiseTo(Chips),
    AllIn,
}

impl Display for PlayerAction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fold => formatter.write_str("fold"),
            Self::Check => formatter.write_str("check"),
            Self::Call => formatter.write_str("call"),
            Self::BetTo(total) => write!(formatter, "bet-to {total}"),
            Self::RaiseTo(total) => write!(formatter, "raise-to {total}"),
            Self::AllIn => formatter.write_str("all-in"),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WagerRange {
    pub min_total: Chips,
    pub max_total: Chips,
}

impl WagerRange {
    pub const fn contains(self, total: Chips) -> bool {
        total >= self.min_total && total <= self.max_total
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalActions {
    pub fold: bool,
    pub check: bool,
    pub call_amount: Option<Chips>,
    pub bet_range: Option<WagerRange>,
    pub raise_range: Option<WagerRange>,
    pub all_in_to: Option<Chips>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerSnapshot {
    pub hole_cards: HoleCards,
    pub stack: Chips,
    pub total_contribution: Chips,
    pub street_contribution: Chips,
    pub folded: bool,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEvent {
    BlindPosted {
        player: Player,
        amount: Chips,
    },
    ActionApplied {
        street: Street,
        player: Player,
        action: PlayerAction,
        pot_after: Chips,
    },
    BoardDealt {
        street: Street,
        cards: Vec<Card>,
    },
    HandCompleted {
        outcome: HandOutcome,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandPhase {
    BettingRound {
        street: Street,
        actor: Player,
    },
    AwaitingBoard {
        next_street: Street,
    },
    Terminal {
        outcome: HandOutcome,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOutcome {
    Uncontested {
        winner: Player,
        pot: Chips,
        payout: HeadsUpPayout,
        street: Street,
    },
    Showdown {
        result: ShowdownResult,
        pot: Chips,
        payout: HeadsUpPayout,
    },
}

impl HandOutcome {
    pub const fn payout(self) -> HeadsUpPayout {
        match self {
            Self::Uncontested { payout, .. } | Self::Showdown { payout, .. } => payout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldemHandState {
    config: HoldemConfig,
    starting_stacks: [Chips; 2],
    players: [InternalPlayerState; 2],
    board: Board,
    street: Street,
    phase: HandPhase,
    current_bet: Chips,
    last_full_raise_size: Chips,
    checks_in_round: u8,
    raise_reopened: [bool; 2],
    history: Vec<HistoryEvent>,
}

impl HoldemHandState {
    pub fn new(
        config: HoldemConfig,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<Self, HoldemStateError> {
        Self::new_with_starting_stacks(
            config,
            button_hole_cards,
            big_blind_hole_cards,
            config.starting_stack,
            config.starting_stack,
        )
    }

    pub fn new_with_starting_stacks(
        config: HoldemConfig,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
        button_starting_stack: Chips,
        big_blind_starting_stack: Chips,
    ) -> Result<Self, HoldemStateError> {
        config.validate()?;
        validate_unique_hole_cards(button_hole_cards, big_blind_hole_cards)?;

        let mut players = [
            InternalPlayerState::new(button_hole_cards, button_starting_stack),
            InternalPlayerState::new(big_blind_hole_cards, big_blind_starting_stack),
        ];
        let mut history = Vec::with_capacity(4);

        let button_blind = post_blind(&mut players, &mut history, Player::Button, config.small_blind);
        let big_blind = post_blind(&mut players, &mut history, Player::BigBlind, config.big_blind);
        let current_bet = button_blind.max(big_blind);

        Ok(Self {
            config,
            starting_stacks: [button_starting_stack, big_blind_starting_stack],
            players,
            board: Board::new(),
            street: Street::Preflop,
            phase: HandPhase::BettingRound {
                street: Street::Preflop,
                actor: Player::Button,
            },
            current_bet,
            last_full_raise_size: current_bet,
            checks_in_round: 0,
            raise_reopened: [true, true],
            history,
        })
    }

    pub const fn config(&self) -> HoldemConfig {
        self.config
    }

    pub const fn street(&self) -> Street {
        self.street
    }

    pub const fn phase(&self) -> HandPhase {
        self.phase
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn history(&self) -> &[HistoryEvent] {
        &self.history
    }

    pub fn current_actor(&self) -> Option<Player> {
        match self.phase {
            HandPhase::BettingRound { actor, .. } => Some(actor),
            HandPhase::AwaitingBoard { .. } | HandPhase::Terminal { .. } => None,
        }
    }

    pub fn current_outcome(&self) -> Option<HandOutcome> {
        match self.phase {
            HandPhase::Terminal { outcome } => Some(outcome),
            HandPhase::BettingRound { .. } | HandPhase::AwaitingBoard { .. } => None,
        }
    }

    pub fn pot(&self) -> Chips {
        self.players
            .iter()
            .map(|player| player.total_contribution)
            .sum()
    }

    pub fn player(&self, player: Player) -> PlayerSnapshot {
        let state = self.player_state(player);
        PlayerSnapshot {
            hole_cards: state.hole_cards,
            stack: state.stack,
            total_contribution: state.total_contribution,
            street_contribution: state.street_contribution,
            folded: state.folded,
        }
    }

    pub const fn starting_stack(&self, player: Player) -> Chips {
        self.starting_stacks[player.index()]
    }

    pub fn legal_actions(&self) -> Result<LegalActions, HoldemStateError> {
        let actor = self
            .current_actor()
            .ok_or(HoldemStateError::ActionNotAllowedInCurrentPhase)?;

        let state = self.player_state(actor);
        let opponent = self.player_state(actor.opponent());
        let to_call = self.current_bet.saturating_sub(state.street_contribution);
        let max_total = effective_max_total(state, opponent);

        if to_call == 0 {
            if self.big_blind_option_pending() {
                let mut raise_range = None;
                let mut all_in_to = None;
                if self.raise_reopened[actor.index()] && max_total > self.current_bet {
                    let minimum_raise_to = self.current_bet + self.last_full_raise_size;
                    if max_total >= minimum_raise_to {
                        raise_range = Some(WagerRange {
                            min_total: minimum_raise_to,
                            max_total,
                        });
                    } else {
                        all_in_to = Some(max_total);
                    }
                }

                return Ok(LegalActions {
                    fold: false,
                    check: true,
                    call_amount: None,
                    bet_range: None,
                    raise_range,
                    all_in_to,
                });
            }

            let bet_range = if self.current_bet == 0 && max_total >= self.config.big_blind {
                Some(WagerRange {
                    min_total: self.config.big_blind,
                    max_total,
                })
            } else {
                None
            };
            let all_in_to = if self.current_bet == 0
                && state.stack > 0
                && max_total > state.street_contribution
                && max_total < self.config.big_blind
            {
                Some(max_total)
            } else {
                None
            };

            return Ok(LegalActions {
                fold: false,
                check: true,
                call_amount: None,
                bet_range,
                raise_range: None,
                all_in_to,
            });
        }

        let mut raise_range = None;
        let mut all_in_to = None;
        if self.raise_reopened[actor.index()] && max_total > self.current_bet {
            let minimum_raise_to = self.current_bet + self.last_full_raise_size;
            if max_total >= minimum_raise_to {
                raise_range = Some(WagerRange {
                    min_total: minimum_raise_to,
                    max_total,
                });
            } else {
                all_in_to = Some(max_total);
            }
        }

        Ok(LegalActions {
            fold: true,
            check: false,
            call_amount: Some(to_call),
            bet_range: None,
            raise_range,
            all_in_to,
        })
    }

    pub fn apply_action(&mut self, action: PlayerAction) -> Result<(), HoldemStateError> {
        let actor = self
            .current_actor()
            .ok_or(HoldemStateError::ActionNotAllowedInCurrentPhase)?;
        let legal_actions = self.legal_actions()?;
        let current_street = self.street;

        match action {
            PlayerAction::Fold => {
                if !legal_actions.fold {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                }
                self.player_state_mut(actor).folded = true;
                self.raise_reopened[actor.index()] = false;
                self.record_action(current_street, actor, action);
                self.finish_uncontested(actor.opponent());
            }
            PlayerAction::Check => {
                let closes_big_blind_option = self.big_blind_option_pending();
                if !legal_actions.check {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                }
                self.raise_reopened[actor.index()] = false;
                self.checks_in_round += 1;
                self.record_action(current_street, actor, action);
                if closes_big_blind_option || self.checks_in_round >= 2 {
                    self.finish_betting_round()?;
                } else {
                    self.phase = HandPhase::BettingRound {
                        street: self.street,
                        actor: actor.opponent(),
                    };
                }
            }
            PlayerAction::Call => {
                let opens_big_blind_option = self.opens_big_blind_option(actor);
                let Some(call_amount) = legal_actions.call_amount else {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                };
                self.contribute(actor, call_amount);
                self.raise_reopened[actor.index()] = false;
                self.record_action(current_street, actor, action);
                if opens_big_blind_option {
                    self.phase = HandPhase::BettingRound {
                        street: self.street,
                        actor: actor.opponent(),
                    };
                } else {
                    self.finish_betting_round()?;
                }
            }
            PlayerAction::BetTo(total) => {
                let Some(range) = legal_actions.bet_range else {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                };
                if !range.contains(total) {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                }
                let contribution = total - self.player_state(actor).street_contribution;
                self.contribute(actor, contribution);
                self.current_bet = total;
                self.last_full_raise_size = total;
                self.checks_in_round = 0;
                self.raise_reopened[actor.index()] = false;
                self.raise_reopened[actor.opponent().index()] = true;
                self.record_action(current_street, actor, action);
                self.phase = HandPhase::BettingRound {
                    street: self.street,
                    actor: actor.opponent(),
                };
            }
            PlayerAction::RaiseTo(total) => {
                let raise_allowed = legal_actions
                    .raise_range
                    .is_some_and(|range| range.contains(total));
                if !raise_allowed {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                }
                let previous_bet = self.current_bet;
                let contribution = total - self.player_state(actor).street_contribution;
                self.contribute(actor, contribution);
                self.current_bet = total;
                self.last_full_raise_size = total - previous_bet;
                self.checks_in_round = 0;
                self.raise_reopened[actor.index()] = false;
                self.raise_reopened[actor.opponent().index()] = true;
                self.record_action(current_street, actor, action);
                self.phase = HandPhase::BettingRound {
                    street: self.street,
                    actor: actor.opponent(),
                };
            }
            PlayerAction::AllIn => {
                let current_total = self.player_state(actor).street_contribution;
                let all_in_total = current_total + self.player_state(actor).stack;
                if Some(all_in_total) == legal_actions.all_in_to {
                    let contribution = all_in_total - current_total;
                    self.contribute(actor, contribution);
                    self.current_bet = self.current_bet.max(all_in_total);
                    self.checks_in_round = 0;
                    self.raise_reopened[actor.index()] = false;
                    self.record_action(current_street, actor, action);
                    self.phase = HandPhase::BettingRound {
                        street: self.street,
                        actor: actor.opponent(),
                    };
                } else if legal_actions
                    .bet_range
                    .is_some_and(|range| range.max_total == all_in_total)
                {
                    self.apply_action(PlayerAction::BetTo(all_in_total))?;
                } else if legal_actions
                    .raise_range
                    .is_some_and(|range| range.max_total == all_in_total)
                {
                    self.apply_action(PlayerAction::RaiseTo(all_in_total))?;
                } else if legal_actions.call_amount == Some(self.player_state(actor).stack) {
                    self.apply_action(PlayerAction::Call)?;
                } else {
                    return Err(HoldemStateError::IllegalAction {
                        player: actor,
                        action,
                    });
                }
            }
        }

        Ok(())
    }

    pub fn deal_flop(&mut self, cards: [Card; 3]) -> Result<(), HoldemStateError> {
        self.deal_board_cards(Street::Flop, cards.to_vec())
    }

    pub fn deal_turn(&mut self, card: Card) -> Result<(), HoldemStateError> {
        self.deal_board_cards(Street::Turn, vec![card])
    }

    pub fn deal_river(&mut self, card: Card) -> Result<(), HoldemStateError> {
        self.deal_board_cards(Street::River, vec![card])
    }

    fn deal_board_cards(
        &mut self,
        dealt_street: Street,
        cards: Vec<Card>,
    ) -> Result<(), HoldemStateError> {
        let HandPhase::AwaitingBoard { next_street } = self.phase else {
            return Err(HoldemStateError::BoardNotExpected);
        };
        if next_street != dealt_street {
            return Err(HoldemStateError::UnexpectedStreet {
                expected: next_street,
                actual: dealt_street,
            });
        }

        let expected_card_count = match dealt_street {
            Street::Flop => 3,
            Street::Turn | Street::River => 1,
            Street::Preflop => 0,
        };
        if cards.len() != expected_card_count {
            return Err(HoldemStateError::WrongBoardCardCount {
                street: dealt_street,
                expected: expected_card_count,
                actual: cards.len(),
            });
        }

        for card in cards.iter().copied() {
            self.ensure_card_is_available(card)?;
            self.board
                .push(card)
                .map_err(|_| HoldemStateError::DuplicateBoardCard { card })?;
        }

        self.history.push(HistoryEvent::BoardDealt {
            street: dealt_street,
            cards,
        });
        self.street = dealt_street;

        if self.players_all_in() {
            if let Some(next_street) = self.street.next() {
                self.phase = HandPhase::AwaitingBoard { next_street };
            } else {
                self.finish_showdown()?;
            }
        } else {
            self.begin_betting_round(dealt_street);
        }

        Ok(())
    }

    fn begin_betting_round(&mut self, street: Street) {
        for player in &mut self.players {
            player.street_contribution = 0;
        }
        self.current_bet = 0;
        self.last_full_raise_size = self.config.big_blind;
        self.checks_in_round = 0;
        self.raise_reopened = [true, true];
        self.phase = HandPhase::BettingRound {
            street,
            actor: Player::BigBlind,
        };
    }

    fn finish_betting_round(&mut self) -> Result<(), HoldemStateError> {
        for player in &mut self.players {
            player.street_contribution = 0;
        }
        self.current_bet = 0;
        self.checks_in_round = 0;

        if let Some(next_street) = self.street.next() {
            self.phase = HandPhase::AwaitingBoard { next_street };
            Ok(())
        } else {
            self.finish_showdown()
        }
    }

    fn finish_uncontested(&mut self, winner: Player) {
        let pot = self.pot();
        let payout = match winner {
            Player::Button => HeadsUpPayout {
                player_one: pot,
                player_two: 0,
            },
            Player::BigBlind => HeadsUpPayout {
                player_one: 0,
                player_two: pot,
            },
        };
        let outcome = HandOutcome::Uncontested {
            winner,
            pot,
            payout,
            street: self.street,
        };
        self.phase = HandPhase::Terminal { outcome };
        self.history.push(HistoryEvent::HandCompleted { outcome });
    }

    fn finish_showdown(&mut self) -> Result<(), HoldemStateError> {
        let result = resolve_holdem_showdown(
            &self.board,
            self.player_state(Player::Button).hole_cards,
            self.player_state(Player::BigBlind).hole_cards,
        )
        .map_err(HoldemStateError::ShowdownFailed)?;
        let pot = self.pot();
        let payout = result.payout(pot, OddChipRecipient::PlayerTwo);
        let outcome = HandOutcome::Showdown {
            result,
            pot,
            payout,
        };
        self.phase = HandPhase::Terminal { outcome };
        self.history.push(HistoryEvent::HandCompleted { outcome });
        Ok(())
    }

    fn ensure_card_is_available(&self, card: Card) -> Result<(), HoldemStateError> {
        if self.board.cards().contains(&card) {
            return Err(HoldemStateError::DuplicateBoardCard { card });
        }
        if self.players.iter().any(|player| player.hole_cards.contains(card)) {
            return Err(HoldemStateError::CardAlreadyInUse { card });
        }
        Ok(())
    }

    fn big_blind_option_pending(&self) -> bool {
        let big_blind = self.player_state(Player::BigBlind);
        self.street == Street::Preflop
            && self.current_actor() == Some(Player::BigBlind)
            && big_blind.stack > 0
            && self.current_bet == big_blind.street_contribution
            && self.current_bet >= self.config.big_blind
            && self.player_state(Player::Button).street_contribution == self.current_bet
            && self.preflop_action_count() == 1
    }

    fn opens_big_blind_option(&self, actor: Player) -> bool {
        let big_blind = self.player_state(Player::BigBlind);
        actor == Player::Button
            && self.street == Street::Preflop
            && big_blind.stack > 0
            && self.current_bet == big_blind.street_contribution
            && self.current_bet >= self.config.big_blind
            && self.player_state(Player::Button).street_contribution == self.config.small_blind
            && self.preflop_action_count() == 0
    }

    fn preflop_action_count(&self) -> usize {
        self.history
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    HistoryEvent::ActionApplied {
                        street: Street::Preflop,
                        ..
                    }
                )
            })
            .count()
    }

    fn player_state(&self, player: Player) -> &InternalPlayerState {
        &self.players[player.index()]
    }

    fn player_state_mut(&mut self, player: Player) -> &mut InternalPlayerState {
        &mut self.players[player.index()]
    }

    fn players_all_in(&self) -> bool {
        self.players
            .iter()
            .filter(|player| !player.folded)
            .all(|player| player.stack == 0)
    }

    fn contribute(&mut self, player: Player, amount: Chips) {
        let state = self.player_state_mut(player);
        state.stack -= amount;
        state.street_contribution += amount;
        state.total_contribution += amount;
    }

    fn record_action(&mut self, street: Street, player: Player, action: PlayerAction) {
        self.history.push(HistoryEvent::ActionApplied {
            street,
            player,
            action,
            pot_after: self.pot(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldemStateError {
    DuplicateHoleCard { card: Card },
    IllegalAction { player: Player, action: PlayerAction },
    ActionNotAllowedInCurrentPhase,
    BoardNotExpected,
    UnexpectedStreet { expected: Street, actual: Street },
    WrongBoardCardCount {
        street: Street,
        expected: usize,
        actual: usize,
    },
    DuplicateBoardCard { card: Card },
    CardAlreadyInUse { card: Card },
    ShowdownFailed(ShowdownError),
    InvalidConfig(HoldemConfigError),
}

impl Display for HoldemStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateHoleCard { card } => write!(formatter, "duplicate hole card {card}"),
            Self::IllegalAction { player, action } => {
                write!(formatter, "illegal action `{action}` for {player}")
            }
            Self::ActionNotAllowedInCurrentPhase => {
                formatter.write_str("action is not allowed in the current phase")
            }
            Self::BoardNotExpected => formatter.write_str("board cards are not expected right now"),
            Self::UnexpectedStreet { expected, actual } => {
                write!(formatter, "expected to deal {expected}, got {actual}")
            }
            Self::WrongBoardCardCount {
                street,
                expected,
                actual,
            } => write!(formatter, "{street} requires {expected} board cards, got {actual}"),
            Self::DuplicateBoardCard { card } => write!(formatter, "duplicate board card {card}"),
            Self::CardAlreadyInUse { card } => write!(formatter, "card {card} is already in use"),
            Self::ShowdownFailed(error) => write!(formatter, "showdown failed: {error}"),
            Self::InvalidConfig(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for HoldemStateError {}

impl From<HoldemConfigError> for HoldemStateError {
    fn from(value: HoldemConfigError) -> Self {
        Self::InvalidConfig(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InternalPlayerState {
    hole_cards: HoleCards,
    stack: Chips,
    total_contribution: Chips,
    street_contribution: Chips,
    folded: bool,
}

impl InternalPlayerState {
    const fn new(hole_cards: HoleCards, stack: Chips) -> Self {
        Self {
            hole_cards,
            stack,
            total_contribution: 0,
            street_contribution: 0,
            folded: false,
        }
    }
}

fn validate_unique_hole_cards(
    button_hole_cards: HoleCards,
    big_blind_hole_cards: HoleCards,
) -> Result<(), HoldemStateError> {
    for card in button_hole_cards.cards() {
        if big_blind_hole_cards.contains(card) {
            return Err(HoldemStateError::DuplicateHoleCard { card });
        }
    }
    Ok(())
}

fn post_blind(
    players: &mut [InternalPlayerState; 2],
    history: &mut Vec<HistoryEvent>,
    player: Player,
    amount: Chips,
) -> Chips {
    let state = &mut players[player.index()];
    let posted = state.stack.min(amount);
    state.stack -= posted;
    state.street_contribution += posted;
    state.total_contribution += posted;
    history.push(HistoryEvent::BlindPosted {
        player,
        amount: posted,
    });
    posted
}

fn effective_max_total(actor: &InternalPlayerState, opponent: &InternalPlayerState) -> Chips {
    let actor_total = actor.street_contribution + actor.stack;
    let opponent_total = opponent.street_contribution + opponent.stack;
    actor_total.min(opponent_total)
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use crate::{
        Card, CardMask, Deck, HandOutcome, HandPhase, HeadsUpPayout, HistoryEvent,
        HoldemConfig, HoldemHandState, HoldemStateError, HoleCards, LegalActions, Player,
        PlayerAction, Street, WagerRange, rng_from_seed,
    };

    fn sample_state_with_default_config() -> HoldemHandState {
        HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn new_hand_posts_blinds_and_waits_for_button_action() {
        let state = sample_state_with_default_config();

        assert_eq!(
            state.phase(),
            HandPhase::BettingRound {
                street: Street::Preflop,
                actor: Player::Button,
            }
        );
        assert_eq!(state.pot(), 150);
        assert_eq!(state.player(Player::Button).stack, 9_950);
        assert_eq!(state.player(Player::Button).street_contribution, 50);
        assert_eq!(state.player(Player::BigBlind).stack, 9_900);
        assert_eq!(state.player(Player::BigBlind).street_contribution, 100);
        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: true,
                check: false,
                call_amount: Some(50),
                bet_range: None,
                raise_range: Some(WagerRange {
                    min_total: 200,
                    max_total: 10_000,
                }),
                all_in_to: None,
            }
        );
        assert_eq!(
            state.history(),
            &[
                HistoryEvent::BlindPosted {
                    player: Player::Button,
                    amount: 50,
                },
                HistoryEvent::BlindPosted {
                    player: Player::BigBlind,
                    amount: 100,
                },
            ]
        );
    }

    #[test]
    fn calling_preflop_gives_the_big_blind_their_option() {
        let mut state = sample_state_with_default_config();

        state.apply_action(PlayerAction::Call).unwrap();

        assert_eq!(
            state.phase(),
            HandPhase::BettingRound {
                street: Street::Preflop,
                actor: Player::BigBlind,
            }
        );
        assert_eq!(state.pot(), 200);
        assert_eq!(state.player(Player::Button).stack, 9_900);
        assert_eq!(state.player(Player::BigBlind).stack, 9_900);
        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: false,
                check: true,
                call_amount: None,
                bet_range: None,
                raise_range: Some(WagerRange {
                    min_total: 200,
                    max_total: 10_000,
                }),
                all_in_to: None,
            }
        );
    }

    #[test]
    fn dealing_flop_begins_a_new_betting_round_with_big_blind_first() {
        let mut state = sample_state_with_default_config();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();

        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();

        assert_eq!(
            state.phase(),
            HandPhase::BettingRound {
                street: Street::Flop,
                actor: Player::BigBlind,
            }
        );
        assert_eq!(state.board().to_string(), "2c3d4h");
        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: false,
                check: true,
                call_amount: None,
                bet_range: Some(WagerRange {
                    min_total: 100,
                    max_total: 9_900,
                }),
                raise_range: None,
                all_in_to: None,
            }
        );
    }

    #[test]
    fn check_check_advances_the_street() {
        let mut state = sample_state_with_default_config();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();

        state.apply_action(PlayerAction::Check).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();

        assert_eq!(
            state.phase(),
            HandPhase::AwaitingBoard {
                next_street: Street::Turn,
            }
        );
    }

    #[test]
    fn fold_ends_the_hand_immediately() {
        let mut state = sample_state_with_default_config();

        state.apply_action(PlayerAction::Fold).unwrap();

        let Some(HandOutcome::Uncontested {
            winner,
            pot,
            payout,
            street,
        }) = state.current_outcome()
        else {
            panic!("expected an uncontested outcome");
        };
        assert_eq!(winner, Player::BigBlind);
        assert_eq!(pot, 150);
        assert_eq!(
            payout,
            HeadsUpPayout {
                player_one: 0,
                player_two: 150,
            }
        );
        assert_eq!(street, Street::Preflop);
    }

    #[test]
    fn min_raise_is_enforced_preflop() {
        let mut state = sample_state_with_default_config();

        let error = state
            .apply_action(PlayerAction::RaiseTo(150))
            .expect_err("raise below the minimum should be rejected");

        assert_eq!(error.to_string(), "illegal action `raise-to 150` for button");
    }

    #[test]
    fn incomplete_all_in_raise_does_not_offer_a_re_raise() {
        let config = HoldemConfig::new(250, 50, 100).unwrap();
        let mut state =
            HoldemHandState::new(config, "AsKd".parse().unwrap(), "QcJh".parse().unwrap())
                .unwrap();

        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: true,
                check: false,
                call_amount: Some(100),
                bet_range: None,
                raise_range: None,
                all_in_to: Some(150),
            }
        );

        state.apply_action(PlayerAction::AllIn).unwrap();

        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: true,
                check: false,
                call_amount: Some(50),
                bet_range: None,
                raise_range: None,
                all_in_to: None,
            }
        );

        state.apply_action(PlayerAction::Call).unwrap();

        assert_eq!(
            state.phase(),
            HandPhase::AwaitingBoard {
                next_street: Street::Turn,
            }
        );
        assert_eq!(state.player(Player::Button).stack, 0);
        assert_eq!(state.player(Player::BigBlind).stack, 0);
        assert_eq!(state.pot(), 500);
    }

    #[test]
    fn all_in_before_the_river_forces_a_runout_then_showdown() {
        let config = HoldemConfig::new(250, 50, 100).unwrap();
        let mut state =
            HoldemHandState::new(config, "AhAd".parse().unwrap(), "KhKd".parse().unwrap())
                .unwrap();

        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();
        state.apply_action(PlayerAction::AllIn).unwrap();
        state.apply_action(PlayerAction::Call).unwrap();

        state.deal_turn("5s".parse().unwrap()).unwrap();
        assert_eq!(
            state.phase(),
            HandPhase::AwaitingBoard {
                next_street: Street::River,
            }
        );
        state.deal_river("7c".parse().unwrap()).unwrap();

        let Some(HandOutcome::Showdown {
            result,
            pot,
            payout,
        }) = state.current_outcome()
        else {
            panic!("expected showdown");
        };
        assert!(result.player_one_rank > result.player_two_rank);
        assert_eq!(pot, 500);
        assert_eq!(
            payout,
            HeadsUpPayout {
                player_one: 500,
                player_two: 0,
            }
        );
    }

    #[test]
    fn duplicate_board_cards_are_rejected() {
        let mut state = sample_state_with_default_config();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();

        let error = state
            .deal_flop(parse_cards::<3>("As3d4h"))
            .expect_err("flop should reject hole-card duplicates");

        assert_eq!(error.to_string(), "card As is already in use");
    }

    #[test]
    fn action_history_records_board_and_terminal_events() {
        let mut state = sample_state_with_default_config();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_turn("5s".parse().unwrap()).unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();
        state.apply_action(PlayerAction::Fold).unwrap();

        assert!(matches!(
            state.history().last(),
            Some(HistoryEvent::HandCompleted {
                outcome: HandOutcome::Uncontested { .. }
            })
        ));
    }

    #[test]
    fn invalid_configs_and_duplicate_hole_cards_are_rejected() {
        assert_eq!(
            HoldemConfig::new(10_000, 0, 100).unwrap_err().to_string(),
            "small blind must be positive"
        );
        assert_eq!(
            HoldemConfig::new(10_000, 50, 0).unwrap_err().to_string(),
            "big blind must be positive"
        );
        assert_eq!(
            HoldemConfig::new(10_000, 100, 100).unwrap_err().to_string(),
            "small blind must be less than big blind"
        );
        assert_eq!(
            HoldemConfig::new(99, 50, 100).unwrap_err().to_string(),
            "starting stack must be at least the big blind"
        );
        assert_eq!(
            HoldemHandState::new(
                HoldemConfig::default(),
                "AsKd".parse().unwrap(),
                "AsQc".parse().unwrap(),
            )
            .unwrap_err(),
            HoldemStateError::DuplicateHoleCard {
                card: "As".parse().unwrap(),
            }
        );
    }

    #[test]
    fn legal_actions_and_board_deals_are_rejected_in_invalid_phases() {
        let mut state = sample_state_with_default_config();

        assert_eq!(
            state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap_err(),
            HoldemStateError::BoardNotExpected
        );

        state.apply_action(PlayerAction::Fold).unwrap();
        assert_eq!(
            state.legal_actions().unwrap_err(),
            HoldemStateError::ActionNotAllowedInCurrentPhase
        );
        assert_eq!(
            state.apply_action(PlayerAction::Call).unwrap_err(),
            HoldemStateError::ActionNotAllowedInCurrentPhase
        );
        assert_eq!(
            state.deal_turn("5s".parse().unwrap()).unwrap_err(),
            HoldemStateError::BoardNotExpected
        );
    }

    #[test]
    fn unexpected_street_is_rejected_when_board_cards_are_due() {
        let mut state = sample_state_with_default_config();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();

        let error = state.deal_turn("5s".parse().unwrap()).unwrap_err();
        assert_eq!(
            error,
            HoldemStateError::UnexpectedStreet {
                expected: Street::Flop,
                actual: Street::Turn,
            }
        );
    }

    #[test]
    fn short_stack_postflop_can_only_open_jam_or_check() {
        let config = HoldemConfig::new(150, 50, 100).unwrap();
        let mut state =
            HoldemHandState::new(config, "AsKd".parse().unwrap(), "QcJh".parse().unwrap())
                .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();

        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: false,
                check: true,
                call_amount: None,
                bet_range: None,
                raise_range: None,
                all_in_to: Some(50),
            }
        );

        state.apply_action(PlayerAction::AllIn).unwrap();
        assert_eq!(state.pot(), 250);
        assert_eq!(state.player(Player::BigBlind).stack, 0);
        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: true,
                check: false,
                call_amount: Some(50),
                bet_range: None,
                raise_range: None,
                all_in_to: None,
            }
        );
    }

    #[test]
    fn all_in_can_take_the_exact_full_raise_line() {
        let config = HoldemConfig::new(300, 50, 100).unwrap();
        let mut state =
            HoldemHandState::new(config, "AsKd".parse().unwrap(), "QcJh".parse().unwrap())
                .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_flop(parse_cards::<3>("2c3d4h")).unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        state.apply_action(PlayerAction::AllIn).unwrap();

        assert_eq!(state.pot(), 500);
        assert_eq!(state.player(Player::Button).stack, 0);
        assert_eq!(
            state.legal_actions().unwrap(),
            LegalActions {
                fold: true,
                check: false,
                call_amount: Some(100),
                bet_range: None,
                raise_range: None,
                all_in_to: None,
            }
        );
    }

    #[test]
    fn random_legal_play_preserves_invariants() {
        for seed in 0..64u64 {
            play_random_hand_and_assert_invariants(seed);
        }
    }

    #[test]
    #[ignore]
    fn random_legal_play_soak_preserves_invariants() {
        for seed in 0..1_024u64 {
            play_random_hand_and_assert_invariants(seed);
        }
    }

    fn play_random_hand_and_assert_invariants(seed: u64) {
        let config = HoldemConfig::default();
        let mut rng = rng_from_seed(seed);
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
        let mut state = HoldemHandState::new(config, button, big_blind).unwrap();
        assert_state_invariants(&state);

        loop {
            match state.phase() {
                HandPhase::BettingRound { .. } => {
                    let action = choose_random_legal_action(&state, &mut rng);
                    state.apply_action(action).unwrap();
                }
                HandPhase::AwaitingBoard { next_street } => match next_street {
                    Street::Flop => state.deal_flop([board[0], board[1], board[2]]).unwrap(),
                    Street::Turn => state.deal_turn(board[3]).unwrap(),
                    Street::River => state.deal_river(board[4]).unwrap(),
                    Street::Preflop => panic!("cannot await preflop board cards"),
                },
                HandPhase::Terminal { .. } => break,
            }
            assert_state_invariants(&state);
        }

        assert_state_invariants(&state);
    }

    fn choose_random_legal_action(
        state: &HoldemHandState,
        rng: &mut crate::DeterministicRng,
    ) -> PlayerAction {
        let legal = state.legal_actions().unwrap();
        let mut options = Vec::new();
        if legal.fold {
            options.push(PlayerAction::Fold);
        }
        if legal.check {
            options.push(PlayerAction::Check);
        }
        if legal.call_amount.is_some() {
            options.push(PlayerAction::Call);
        }
        if let Some(range) = legal.bet_range {
            options.push(PlayerAction::BetTo(range.min_total));
            if range.max_total != range.min_total {
                options.push(PlayerAction::BetTo(range.max_total));
            }
        }
        if let Some(range) = legal.raise_range {
            options.push(PlayerAction::RaiseTo(range.min_total));
            if range.max_total != range.min_total {
                options.push(PlayerAction::RaiseTo(range.max_total));
            }
        }
        if legal.all_in_to.is_some() {
            options.push(PlayerAction::AllIn);
        }

        let index = (rng.next_u64() as usize) % options.len();
        options[index]
    }

    fn assert_state_invariants(state: &HoldemHandState) {
        let button = state.player(Player::Button);
        let big_blind = state.player(Player::BigBlind);

        assert_eq!(
            button.stack + button.total_contribution,
            state.starting_stack(Player::Button),
            "button stack conservation failed"
        );
        assert_eq!(
            big_blind.stack + big_blind.total_contribution,
            state.starting_stack(Player::BigBlind),
            "big blind stack conservation failed"
        );
        assert_eq!(
            state.pot(),
            button.total_contribution + big_blind.total_contribution,
            "pot should equal total contributions"
        );
        assert!(!(button.folded && big_blind.folded), "both players cannot fold");

        let mut seen = CardMask::empty();
        for card in [button.hole_cards.first(), button.hole_cards.second()] {
            assert!(seen.insert(card), "duplicate button hole card {card}");
        }
        for card in [big_blind.hole_cards.first(), big_blind.hole_cards.second()] {
            assert!(seen.insert(card), "duplicate big blind hole card {card}");
        }
        for &card in state.board().cards() {
            assert!(seen.insert(card), "duplicate board card {card}");
        }

        let expected_board_len = match state.street() {
            Street::Preflop => 0,
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        };
        assert_eq!(state.board().len(), expected_board_len);

        match state.phase() {
            HandPhase::BettingRound { actor, .. } => {
                assert_eq!(state.current_actor(), Some(actor));
                assert!(!state.player(actor).folded);
                let legal = state.legal_actions().unwrap();
                if let Some(range) = legal.bet_range {
                    assert!(range.min_total <= range.max_total);
                }
                if let Some(range) = legal.raise_range {
                    assert!(range.min_total <= range.max_total);
                }
            }
            HandPhase::AwaitingBoard { .. } => {
                assert_eq!(state.current_actor(), None);
                assert_eq!(
                    state.legal_actions().unwrap_err(),
                    HoldemStateError::ActionNotAllowedInCurrentPhase
                );
            }
            HandPhase::Terminal { outcome } => {
                assert_eq!(state.current_actor(), None);
                assert_eq!(
                    state.legal_actions().unwrap_err(),
                    HoldemStateError::ActionNotAllowedInCurrentPhase
                );
                match outcome {
                    HandOutcome::Uncontested { payout, .. }
                    | HandOutcome::Showdown { payout, .. } => {
                        assert_eq!(payout.player_one + payout.player_two, state.pot());
                    }
                }
            }
        }
    }

    fn parse_cards<const N: usize>(input: &str) -> [Card; N] {
        assert_eq!(input.len(), N * 2, "input should contain exactly {N} cards");
        (0..N)
            .map(|index| {
                let start = index * 2;
                let end = start + 2;
                input[start..end]
                    .parse::<Card>()
                    .expect("test card text should parse")
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("test input should contain the expected number of cards")
    }
}
