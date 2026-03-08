use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{Chips, HoldemConfig, HoldemHandState, HoldemStateError, HoleCards, Player};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchPlayer {
    PlayerOne,
    PlayerTwo,
}

impl MatchPlayer {
    pub const ALL: [Self; 2] = [Self::PlayerOne, Self::PlayerTwo];

    pub const fn opponent(self) -> Self {
        match self {
            Self::PlayerOne => Self::PlayerTwo,
            Self::PlayerTwo => Self::PlayerOne,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::PlayerOne => 0,
            Self::PlayerTwo => 1,
        }
    }
}

impl Display for MatchPlayer {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlayerOne => formatter.write_str("player-one"),
            Self::PlayerTwo => formatter.write_str("player-two"),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchConfig {
    pub initial_stack: Chips,
    pub small_blind: Chips,
    pub big_blind: Chips,
    pub first_button: MatchPlayer,
}

impl MatchConfig {
    pub fn new(
        initial_stack: Chips,
        small_blind: Chips,
        big_blind: Chips,
        first_button: MatchPlayer,
    ) -> Result<Self, MatchConfigError> {
        let config = Self {
            initial_stack,
            small_blind,
            big_blind,
            first_button,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(self) -> Result<(), MatchConfigError> {
        HoldemConfig::new(self.initial_stack, self.small_blind, self.big_blind)
            .map(|_| ())
            .map_err(MatchConfigError::InvalidHandConfig)
    }

    pub fn hand_config(self) -> HoldemConfig {
        HoldemConfig {
            starting_stack: self.initial_stack,
            small_blind: self.small_blind,
            big_blind: self.big_blind,
        }
    }
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            initial_stack: 10_000,
            small_blind: 50,
            big_blind: 100,
            first_button: MatchPlayer::PlayerOne,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchConfigError {
    InvalidHandConfig(crate::HoldemConfigError),
}

impl Display for MatchConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandConfig(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for MatchConfigError {}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchSeating {
    pub button: MatchPlayer,
    pub big_blind: MatchPlayer,
}

impl MatchSeating {
    pub const fn player_in_seat(self, seat: Player) -> MatchPlayer {
        match seat {
            Player::Button => self.button,
            Player::BigBlind => self.big_blind,
        }
    }

    pub fn seat_for_player(self, player: MatchPlayer) -> Player {
        if self.button == player {
            Player::Button
        } else {
            Player::BigBlind
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchSnapshot {
    pub hand_number: u64,
    pub player_one_stack: Chips,
    pub player_two_stack: Chips,
    pub seating: MatchSeating,
    pub hand_in_progress: bool,
    pub match_over: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadsUpMatchState {
    config: MatchConfig,
    bankrolls: [Chips; 2],
    seating: MatchSeating,
    hand_number: u64,
    hand_in_progress: bool,
}

impl HeadsUpMatchState {
    pub fn new(config: MatchConfig) -> Result<Self, MatchConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            bankrolls: [config.initial_stack, config.initial_stack],
            seating: MatchSeating {
                button: config.first_button,
                big_blind: config.first_button.opponent(),
            },
            hand_number: 0,
            hand_in_progress: false,
        })
    }

    pub const fn config(&self) -> MatchConfig {
        self.config
    }

    pub const fn hand_number(&self) -> u64 {
        self.hand_number
    }

    pub const fn hand_in_progress(&self) -> bool {
        self.hand_in_progress
    }

    pub const fn seating(&self) -> MatchSeating {
        self.seating
    }

    pub const fn bankroll(&self, player: MatchPlayer) -> Chips {
        self.bankrolls[player.index()]
    }

    pub fn seat_for_player(&self, player: MatchPlayer) -> Player {
        self.seating.seat_for_player(player)
    }

    pub const fn player_in_seat(&self, seat: Player) -> MatchPlayer {
        self.seating.player_in_seat(seat)
    }

    pub const fn match_over(&self) -> bool {
        self.bankrolls[0] == 0 || self.bankrolls[1] == 0
    }

    pub fn snapshot(&self) -> MatchSnapshot {
        MatchSnapshot {
            hand_number: self.hand_number,
            player_one_stack: self.bankroll(MatchPlayer::PlayerOne),
            player_two_stack: self.bankroll(MatchPlayer::PlayerTwo),
            seating: self.seating,
            hand_in_progress: self.hand_in_progress,
            match_over: self.match_over(),
        }
    }

    pub fn start_next_hand(
        &mut self,
        button_hole_cards: HoleCards,
        big_blind_hole_cards: HoleCards,
    ) -> Result<HoldemHandState, MatchStateError> {
        if self.hand_in_progress {
            return Err(MatchStateError::HandAlreadyInProgress);
        }
        if self.match_over() {
            return Err(MatchStateError::MatchAlreadyOver);
        }
        if self.hand_number > 0 {
            self.seating = MatchSeating {
                button: self.seating.big_blind,
                big_blind: self.seating.button,
            };
        }

        self.hand_number += 1;
        self.hand_in_progress = true;

        let button_starting_stack = self.bankroll(self.seating.button);
        let big_blind_starting_stack = self.bankroll(self.seating.big_blind);

        HoldemHandState::new_with_starting_stacks(
            self.config.hand_config(),
            button_hole_cards,
            big_blind_hole_cards,
            button_starting_stack,
            big_blind_starting_stack,
        )
        .map_err(MatchStateError::State)
    }

    pub fn complete_hand(&mut self, state: &HoldemHandState) -> Result<(), MatchStateError> {
        if !self.hand_in_progress {
            return Err(MatchStateError::NoHandInProgress);
        }

        let outcome = state
            .current_outcome()
            .ok_or(MatchStateError::HandNotTerminal)?;
        let payout = outcome.payout();
        let button_final_stack = state.player(Player::Button).stack + payout.player_one;
        let big_blind_final_stack = state.player(Player::BigBlind).stack + payout.player_two;

        self.bankrolls[self.seating.button.index()] = button_final_stack;
        self.bankrolls[self.seating.big_blind.index()] = big_blind_final_stack;
        self.hand_in_progress = false;

        Ok(())
    }

    pub fn display_stack_for_seat(&self, state: &HoldemHandState, seat: Player) -> Chips {
        if state.current_outcome().is_some() {
            self.bankroll(self.player_in_seat(seat))
        } else {
            state.player(seat).stack
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStateError {
    HandAlreadyInProgress,
    NoHandInProgress,
    HandNotTerminal,
    MatchAlreadyOver,
    State(HoldemStateError),
}

impl Display for MatchStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandAlreadyInProgress => formatter.write_str("a match hand is already in progress"),
            Self::NoHandInProgress => formatter.write_str("no match hand is currently in progress"),
            Self::HandNotTerminal => formatter.write_str("cannot settle a match hand before it is terminal"),
            Self::MatchAlreadyOver => formatter.write_str("the match is already over"),
            Self::State(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for MatchStateError {}

#[cfg(test)]
mod tests {
    use super::{HeadsUpMatchState, MatchConfig, MatchPlayer, MatchStateError};
    use crate::{HandOutcome, HoldemConfig, HoldemHandState, HoleCards, Player, PlayerAction};

    fn sample_hole_cards() -> (HoleCards, HoleCards) {
        ("AsKd".parse().unwrap(), "QcJh".parse().unwrap())
    }

    #[test]
    fn match_starts_with_player_one_on_the_button_by_default() {
        let match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let snapshot = match_state.snapshot();

        assert_eq!(snapshot.hand_number, 0);
        assert_eq!(snapshot.seating.button, MatchPlayer::PlayerOne);
        assert_eq!(snapshot.player_one_stack, 10_000);
        assert!(!snapshot.hand_in_progress);
    }

    #[test]
    fn start_next_hand_uses_current_bankrolls_and_sets_progress() {
        let mut match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let (button, big_blind) = sample_hole_cards();

        let hand = match_state.start_next_hand(button, big_blind).unwrap();

        assert_eq!(match_state.hand_number(), 1);
        assert!(match_state.hand_in_progress());
        assert_eq!(hand.starting_stack(Player::Button), 10_000);
        assert_eq!(hand.starting_stack(Player::BigBlind), 10_000);
    }

    #[test]
    fn next_hand_rotates_button_and_updates_bankrolls() {
        let mut match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let (button, big_blind) = sample_hole_cards();
        let mut hand = match_state.start_next_hand(button, big_blind).unwrap();

        hand.apply_action(PlayerAction::Fold).unwrap();
        match_state.complete_hand(&hand).unwrap();
        let next_hand = match_state.start_next_hand(button, big_blind).unwrap();

        assert_eq!(match_state.bankroll(MatchPlayer::PlayerOne), 9_950);
        assert_eq!(match_state.bankroll(MatchPlayer::PlayerTwo), 10_050);
        assert_eq!(match_state.seating().button, MatchPlayer::PlayerTwo);
        assert_eq!(next_hand.starting_stack(Player::Button), 10_050);
        assert_eq!(next_hand.starting_stack(Player::BigBlind), 9_950);
        assert!(match_state.hand_in_progress());
    }

    #[test]
    fn alternating_hands_preserve_total_bankroll_and_rotate_the_button() {
        let mut match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let (button, big_blind) = sample_hole_cards();

        let mut first_hand = match_state.start_next_hand(button, big_blind).unwrap();
        first_hand.apply_action(PlayerAction::Fold).unwrap();
        match_state.complete_hand(&first_hand).unwrap();
        assert_eq!(match_state.seating().button, MatchPlayer::PlayerOne);

        let mut second_hand = match_state.start_next_hand(button, big_blind).unwrap();
        assert_eq!(match_state.seating().button, MatchPlayer::PlayerTwo);
        second_hand.apply_action(PlayerAction::Fold).unwrap();
        match_state.complete_hand(&second_hand).unwrap();

        assert_eq!(
            match_state.bankroll(MatchPlayer::PlayerOne) + match_state.bankroll(MatchPlayer::PlayerTwo),
            20_000
        );
        assert_eq!(match_state.bankroll(MatchPlayer::PlayerOne), 10_000);
        assert_eq!(match_state.bankroll(MatchPlayer::PlayerTwo), 10_000);
    }

    #[test]
    fn complete_hand_rejects_non_terminal_states() {
        let mut match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let (button, big_blind) = sample_hole_cards();
        let hand = match_state.start_next_hand(button, big_blind).unwrap();

        assert_eq!(
            match_state.complete_hand(&hand).unwrap_err(),
            MatchStateError::HandNotTerminal
        );
    }

    #[test]
    fn match_over_blocks_new_hands_after_bankroll_reaches_zero() {
        let mut match_state = HeadsUpMatchState::new(
            MatchConfig::new(100, 50, 100, MatchPlayer::PlayerOne).unwrap(),
        )
        .unwrap();
        let button = "2c3d".parse().unwrap();
        let big_blind = "AsAh".parse().unwrap();
        let mut hand = match_state.start_next_hand(button, big_blind).unwrap();

        hand.apply_action(PlayerAction::Call).unwrap();
        hand.deal_flop(["Kc".parse().unwrap(), "Qd".parse().unwrap(), "8h".parse().unwrap()])
            .unwrap();
        hand.deal_turn("5s".parse().unwrap()).unwrap();
        hand.deal_river("4c".parse().unwrap()).unwrap();
        assert!(matches!(hand.current_outcome(), Some(HandOutcome::Showdown { .. })));
        match_state.complete_hand(&hand).unwrap();

        assert!(match_state.match_over());
        assert_eq!(
            match_state.start_next_hand(button, big_blind).unwrap_err(),
            MatchStateError::MatchAlreadyOver
        );
    }

    #[test]
    fn display_stack_uses_settled_bankroll_after_terminal_hand() {
        let mut match_state = HeadsUpMatchState::new(MatchConfig::default()).unwrap();
        let (button, big_blind) = sample_hole_cards();
        let mut hand = match_state.start_next_hand(button, big_blind).unwrap();

        hand.apply_action(PlayerAction::Fold).unwrap();
        match_state.complete_hand(&hand).unwrap();

        assert_eq!(match_state.display_stack_for_seat(&hand, Player::Button), 9_950);
        assert_eq!(match_state.display_stack_for_seat(&hand, Player::BigBlind), 10_050);
    }

    #[test]
    fn unequal_stack_constructor_supports_short_blinds() {
        let state = HoldemHandState::new_with_starting_stacks(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
            30,
            80,
        )
        .unwrap();

        assert_eq!(state.player(Player::Button).stack, 0);
        assert_eq!(state.player(Player::Button).street_contribution, 30);
        assert_eq!(state.player(Player::BigBlind).stack, 0);
        assert_eq!(state.player(Player::BigBlind).street_contribution, 80);
        assert_eq!(state.pot(), 110);
    }
}
