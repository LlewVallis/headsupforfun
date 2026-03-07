use crate::cfr::{ExtensiveGameState, GameNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KuhnCard {
    Jack,
    Queen,
    King,
}

impl KuhnCard {
    const ALL: [Self; 3] = [Self::Jack, Self::Queen, Self::King];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KuhnAction {
    Check,
    Bet,
    Call,
    Fold,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KuhnInfoSet {
    pub player: usize,
    pub private_card: KuhnCard,
    pub history: Vec<KuhnAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KuhnState {
    private_cards: Option<[KuhnCard; 2]>,
    history: Vec<KuhnAction>,
}

impl KuhnState {
    pub fn new() -> Self {
        Self {
            private_cards: None,
            history: Vec::new(),
        }
    }
}

impl Default for KuhnState {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensiveGameState for KuhnState {
    type Action = KuhnAction;
    type InfoSet = KuhnInfoSet;

    fn node(&self) -> GameNode<Self::Action, Self::InfoSet, Self> {
        if self.private_cards.is_none() {
            return GameNode::Chance {
                outcomes: all_deals()
                    .into_iter()
                    .map(|cards| {
                        (
                            Self {
                                private_cards: Some(cards),
                                history: Vec::new(),
                            },
                            1.0 / 6.0,
                        )
                    })
                    .collect(),
            };
        }

        if let Some(utilities) = terminal_utilities(self.private_cards.expect("cards must be dealt"), &self.history) {
            return GameNode::Terminal { utilities };
        }

        let private_cards = self.private_cards.expect("cards must be dealt");
        match self.history.as_slice() {
            [] => GameNode::Decision {
                player: 0,
                infoset: KuhnInfoSet {
                    player: 0,
                    private_card: private_cards[0],
                    history: self.history.clone(),
                },
                actions: vec![KuhnAction::Check, KuhnAction::Bet],
            },
            [KuhnAction::Check] => GameNode::Decision {
                player: 1,
                infoset: KuhnInfoSet {
                    player: 1,
                    private_card: private_cards[1],
                    history: self.history.clone(),
                },
                actions: vec![KuhnAction::Check, KuhnAction::Bet],
            },
            [KuhnAction::Bet] => GameNode::Decision {
                player: 1,
                infoset: KuhnInfoSet {
                    player: 1,
                    private_card: private_cards[1],
                    history: self.history.clone(),
                },
                actions: vec![KuhnAction::Fold, KuhnAction::Call],
            },
            [KuhnAction::Check, KuhnAction::Bet] => GameNode::Decision {
                player: 0,
                infoset: KuhnInfoSet {
                    player: 0,
                    private_card: private_cards[0],
                    history: self.history.clone(),
                },
                actions: vec![KuhnAction::Fold, KuhnAction::Call],
            },
            history => panic!("unexpected Kuhn history {history:?}"),
        }
    }

    fn next_state(&self, action: &Self::Action) -> Self {
        let mut next = self.clone();
        next.history.push(*action);
        next
    }
}

fn all_deals() -> Vec<[KuhnCard; 2]> {
    let mut deals = Vec::new();
    for first in KuhnCard::ALL {
        for second in KuhnCard::ALL {
            if first != second {
                deals.push([first, second]);
            }
        }
    }
    deals
}

fn terminal_utilities(cards: [KuhnCard; 2], history: &[KuhnAction]) -> Option<[f64; 2]> {
    let winner = match cards[0].cmp(&cards[1]) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => unreachable!("Kuhn deals never duplicate cards"),
    };

    match history {
        [KuhnAction::Check, KuhnAction::Check] => Some(showdown_utility(winner, 1.0)),
        [KuhnAction::Bet, KuhnAction::Fold] => Some([1.0, -1.0]),
        [KuhnAction::Bet, KuhnAction::Call] => Some(showdown_utility(winner, 2.0)),
        [KuhnAction::Check, KuhnAction::Bet, KuhnAction::Fold] => Some([-1.0, 1.0]),
        [KuhnAction::Check, KuhnAction::Bet, KuhnAction::Call] => {
            Some(showdown_utility(winner, 2.0))
        }
        _ => None,
    }
}

fn showdown_utility(winner: usize, amount: f64) -> [f64; 2] {
    if winner == 0 {
        [amount, -amount]
    } else {
        [-amount, amount]
    }
}

#[cfg(test)]
mod tests {
    use crate::cfr::{ExtensiveGameState, GameNode};

    use super::{KuhnAction, KuhnCard, KuhnInfoSet, KuhnState};

    #[test]
    fn root_state_starts_with_a_chance_node() {
        let node = KuhnState::new().node();

        let GameNode::Chance { outcomes } = node else {
            panic!("expected a chance node");
        };
        assert_eq!(outcomes.len(), 6);
    }

    #[test]
    fn first_player_infoset_contains_private_card_and_history() {
        let state = KuhnState {
            private_cards: Some([KuhnCard::King, KuhnCard::Jack]),
            history: vec![],
        };

        let GameNode::Decision { infoset, .. } = state.node() else {
            panic!("expected a decision node");
        };
        assert_eq!(
            infoset,
            KuhnInfoSet {
                player: 0,
                private_card: KuhnCard::King,
                history: vec![],
            }
        );
    }

    #[test]
    fn facing_a_bet_offers_fold_or_call() {
        let state = KuhnState {
            private_cards: Some([KuhnCard::Queen, KuhnCard::Jack]),
            history: vec![KuhnAction::Bet],
        };

        let GameNode::Decision { actions, .. } = state.node() else {
            panic!("expected a decision node");
        };
        assert_eq!(actions, vec![KuhnAction::Fold, KuhnAction::Call]);
    }
}
