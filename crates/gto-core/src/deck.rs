use rand::Rng;

use crate::{Card, CardMask, DeterministicRng};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn standard() -> Self {
        let cards = (0..52)
            .map(|index| Card::from_index(index).expect("standard deck indices must be valid"))
            .collect();

        Self { cards }
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn shuffle(&mut self, rng: &mut DeterministicRng) {
        for index in (1..self.cards.len()).rev() {
            let swap_index = rng.random_range(0..=index);
            self.cards.swap(index, swap_index);
        }
    }

    pub fn remove_dead_cards(&mut self, dead_cards: CardMask) {
        self.cards.retain(|card| !dead_cards.contains(*card));
    }

    pub fn draw(&mut self) -> Option<Card> {
        self.cards.pop()
    }
}

#[cfg(test)]
mod tests {
    use crate::{CardMask, Deck, rng_from_seed};

    #[test]
    fn standard_deck_contains_52_unique_cards() {
        let deck = Deck::standard();
        let unique_cards = CardMask::from_cards(deck.cards().iter().copied());

        assert_eq!(deck.len(), 52);
        assert_eq!(unique_cards.len(), 52);
    }

    #[test]
    fn dead_cards_are_removed_from_deck() {
        let mut deck = Deck::standard();
        let dead_cards = CardMask::from_cards([
            "As".parse().unwrap(),
            "Kd".parse().unwrap(),
            "Tc".parse().unwrap(),
        ]);

        deck.remove_dead_cards(dead_cards);

        assert_eq!(deck.len(), 49);
        assert!(!deck.cards().contains(&"As".parse().unwrap()));
        assert!(!deck.cards().contains(&"Kd".parse().unwrap()));
        assert!(!deck.cards().contains(&"Tc".parse().unwrap()));
    }

    #[test]
    fn shuffle_is_deterministic_for_the_same_seed() {
        let mut left = Deck::standard();
        let mut right = Deck::standard();
        let mut left_rng = rng_from_seed(7);
        let mut right_rng = rng_from_seed(7);

        left.shuffle(&mut left_rng);
        right.shuffle(&mut right_rng);

        assert_eq!(left, right);
    }
}
