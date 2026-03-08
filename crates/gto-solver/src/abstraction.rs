use gto_core::{Card, Chips, HoldemHandState, HoldemStateError, HoleCards, Player, PlayerAction, Street};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractionProfile {
    preflop: StreetProfile,
    flop: StreetProfile,
    turn: StreetProfile,
    river: StreetProfile,
}

impl AbstractionProfile {
    pub fn new(
        preflop: StreetProfile,
        flop: StreetProfile,
        turn: StreetProfile,
        river: StreetProfile,
    ) -> Self {
        Self {
            preflop,
            flop,
            turn,
            river,
        }
    }

    pub fn for_street(&self, street: Street) -> &StreetProfile {
        match street {
            Street::Preflop => &self.preflop,
            Street::Flop => &self.flop,
            Street::Turn => &self.turn,
            Street::River => &self.river,
        }
    }

    pub fn river_smoke() -> Self {
        let postflop = StreetProfile {
            opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
            raise_sizes: vec![],
            include_all_in: false,
        };
        Self::new(
            StreetProfile {
                opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
                raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
                include_all_in: true,
            },
            postflop.clone(),
            postflop.clone(),
            postflop,
        )
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetProfile {
    pub opening_sizes: Vec<OpeningSize>,
    pub raise_sizes: Vec<RaiseSize>,
    pub include_all_in: bool,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningSize {
    BigBlindMultipleBps(u32),
    PotFractionBps(u32),
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseSize {
    CurrentBetMultipleBps(u32),
    PotFractionAfterCallBps(u32),
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbstractAction {
    Fold,
    Check,
    Call,
    BetTo(Chips),
    RaiseTo(Chips),
    AllIn(Chips),
}

impl AbstractAction {
    pub fn to_player_action(self) -> PlayerAction {
        match self {
            Self::Fold => PlayerAction::Fold,
            Self::Check => PlayerAction::Check,
            Self::Call => PlayerAction::Call,
            Self::BetTo(total) => PlayerAction::BetTo(total),
            Self::RaiseTo(total) => PlayerAction::RaiseTo(total),
            Self::AllIn(_) => PlayerAction::AllIn,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicStateKey {
    pub street: Street,
    pub board: Vec<Card>,
    pub actor: Option<Player>,
    pub pot: Chips,
    pub button_stack: Chips,
    pub big_blind_stack: Chips,
    pub button_total_contribution: Chips,
    pub big_blind_total_contribution: Chips,
    pub button_street_contribution: Chips,
    pub big_blind_street_contribution: Chips,
}

impl PublicStateKey {
    pub fn from_state(state: &HoldemHandState) -> Self {
        let button = state.player(Player::Button);
        let big_blind = state.player(Player::BigBlind);

        Self {
            street: state.street(),
            board: state.board().cards().to_vec(),
            actor: state.current_actor(),
            pot: state.pot(),
            button_stack: button.stack,
            big_blind_stack: big_blind.stack,
            button_total_contribution: button.total_contribution,
            big_blind_total_contribution: big_blind.total_contribution,
            button_street_contribution: button.street_contribution,
            big_blind_street_contribution: big_blind.street_contribution,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HoldemInfoSetKey {
    pub player: Player,
    pub hole_cards: HoleCards,
    pub public_state: PublicStateKey,
    pub public_history: Vec<AbstractAction>,
}

impl HoldemInfoSetKey {
    pub fn from_state(
        player: Player,
        hole_cards: HoleCards,
        state: &HoldemHandState,
        public_history: Vec<AbstractAction>,
    ) -> Self {
        Self {
            player,
            hole_cards,
            public_state: PublicStateKey::from_state(state),
            public_history,
        }
    }
}

pub fn abstract_actions(
    state: &HoldemHandState,
    profile: &AbstractionProfile,
) -> Result<Vec<AbstractAction>, HoldemStateError> {
    let legal = state.legal_actions()?;
    let street_profile = profile.for_street(state.street());
    let mut actions = Vec::new();

    if legal.fold {
        actions.push(AbstractAction::Fold);
    }
    if legal.check {
        actions.push(AbstractAction::Check);
    }
    if legal.call_amount.is_some() {
        actions.push(AbstractAction::Call);
    }

    let actor = match state.current_actor() {
        Some(actor) => actor,
        None => return Ok(actions),
    };
    let actor_snapshot = state.player(actor);
    let opponent_snapshot = state.player(actor.opponent());

    if let Some(range) = legal.bet_range {
        let mut candidates = street_profile
            .opening_sizes
            .iter()
            .filter_map(|size| opening_total(state, *size))
            .filter(|total| range.contains(*total))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.dedup();

        let all_in_total = state.player(actor).street_contribution + state.player(actor).stack;
        for total in candidates {
            if street_profile.include_all_in && total == all_in_total {
                actions.push(AbstractAction::AllIn(total));
            } else {
                actions.push(AbstractAction::BetTo(total));
            }
        }

        if street_profile.include_all_in
            && legal.all_in_to == Some(all_in_total)
            && !actions.contains(&AbstractAction::AllIn(all_in_total))
        {
            actions.push(AbstractAction::AllIn(all_in_total));
        }
    }

    if let Some(range) = legal.raise_range {
        let mut candidates = Vec::new();
        if is_preflop_opening_raise_spot(state, actor_snapshot, opponent_snapshot) {
            candidates.extend(
                street_profile
                    .opening_sizes
                    .iter()
                    .filter_map(|size| opening_total(state, *size)),
            );
        }
        candidates.extend(
            street_profile
                .raise_sizes
                .iter()
                .filter_map(|size| raise_total(state, *size)),
        );
        let mut candidates = candidates
            .into_iter()
            .filter(|total| range.contains(*total))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.dedup();

        let all_in_total = actor_snapshot.street_contribution + actor_snapshot.stack;
        for total in candidates {
            if street_profile.include_all_in && total == all_in_total {
                actions.push(AbstractAction::AllIn(total));
            } else {
                actions.push(AbstractAction::RaiseTo(total));
            }
        }
    }

    if let Some(all_in_total) = legal.all_in_to {
        if street_profile.include_all_in && !actions.contains(&AbstractAction::AllIn(all_in_total)) {
            actions.push(AbstractAction::AllIn(all_in_total));
        }
    }

    Ok(actions)
}

fn opening_total(state: &HoldemHandState, size: OpeningSize) -> Option<Chips> {
    match size {
        OpeningSize::BigBlindMultipleBps(bps) => Some(scale_bps(state.config().big_blind, bps)),
        OpeningSize::PotFractionBps(bps) => Some(scale_bps(state.pot(), bps)),
    }
}

fn raise_total(state: &HoldemHandState, size: RaiseSize) -> Option<Chips> {
    let actor = state.current_actor()?;
    let actor_snapshot = state.player(actor);
    let to_call = state
        .legal_actions()
        .ok()?
        .call_amount?;
    let current_bet = actor_snapshot.street_contribution + to_call;

    match size {
        RaiseSize::CurrentBetMultipleBps(bps) => Some(scale_bps(current_bet, bps)),
        RaiseSize::PotFractionAfterCallBps(bps) => {
            let pot_after_call = state.pot() + to_call;
            Some(current_bet + scale_bps(pot_after_call, bps))
        }
    }
}

fn scale_bps(base: Chips, bps: u32) -> Chips {
    if base == 0 || bps == 0 {
        return 0;
    }

    (base.saturating_mul(bps as u64)).div_ceil(10_000)
}

fn is_preflop_opening_raise_spot(
    state: &HoldemHandState,
    actor: gto_core::PlayerSnapshot,
    opponent: gto_core::PlayerSnapshot,
) -> bool {
    state.street() == Street::Preflop
        && state.board().is_empty()
        && state.current_actor() == Some(Player::Button)
        && actor.street_contribution == state.config().small_blind
        && opponent.street_contribution == state.config().big_blind
        && state.pot() == state.config().small_blind + state.config().big_blind
}

#[cfg(test)]
mod tests {
    use gto_core::{HoldemConfig, HoldemHandState};

    use super::{
        AbstractionProfile, AbstractAction, OpeningSize, RaiseSize, StreetProfile, abstract_actions,
    };

    #[test]
    fn abstraction_filters_to_legal_preflop_actions() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let profile = AbstractionProfile::new(
            StreetProfile {
                opening_sizes: vec![
                    OpeningSize::BigBlindMultipleBps(15_000),
                    OpeningSize::BigBlindMultipleBps(25_000),
                    OpeningSize::BigBlindMultipleBps(40_000),
                ],
                raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
                include_all_in: false,
            },
            StreetProfile {
                opening_sizes: vec![],
                raise_sizes: vec![],
                include_all_in: false,
            },
            StreetProfile {
                opening_sizes: vec![],
                raise_sizes: vec![],
                include_all_in: false,
            },
            StreetProfile {
                opening_sizes: vec![],
                raise_sizes: vec![],
                include_all_in: false,
            },
        );

        let actions = abstract_actions(&state, &profile).unwrap();
        assert_eq!(
            actions,
            vec![
                AbstractAction::Fold,
                AbstractAction::Call,
                AbstractAction::RaiseTo(250),
                AbstractAction::RaiseTo(400),
            ]
        );
    }
}
