use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::{Card, CardMask, HoleCards, Rank, Suit};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Range {
    combos: BTreeSet<HoleCards>,
}

impl Range {
    pub fn empty() -> Self {
        Self {
            combos: BTreeSet::new(),
        }
    }

    pub fn from_hole_cards(cards: impl IntoIterator<Item = HoleCards>) -> Self {
        let mut range = Self::empty();
        for cards in cards {
            range.insert(cards);
        }
        range
    }

    pub fn insert(&mut self, cards: HoleCards) -> bool {
        self.combos.insert(cards)
    }

    pub fn len(&self) -> usize {
        self.combos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.combos.is_empty()
    }

    pub fn contains(&self, cards: HoleCards) -> bool {
        self.combos.contains(&cards)
    }

    pub fn iter(&self) -> impl Iterator<Item = &HoleCards> {
        self.combos.iter()
    }

    pub fn without_dead_cards(&self, dead_cards: CardMask) -> Self {
        Self::from_hole_cards(
            self.combos
                .iter()
                .copied()
                .filter(|cards| !cards.mask().intersects(dead_cards)),
        )
    }
}

impl Display for Range {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for combo in &self.combos {
            if !first {
                formatter.write_str(",")?;
            }
            write!(formatter, "{combo}")?;
            first = false;
        }
        Ok(())
    }
}

impl FromStr for Range {
    type Err = ParseRangeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut range = Self::empty();
        for token in input
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|token| !token.is_empty())
        {
            for combo in parse_range_token(token)? {
                range.insert(combo);
            }
        }

        Ok(range)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suitedness {
    Any,
    Suited,
    Offsuit,
}

fn parse_range_token(token: &str) -> Result<Vec<HoleCards>, ParseRangeError> {
    if let Ok(cards) = HoleCards::from_str(token) {
        return Ok(vec![cards]);
    }

    let (base_token, plus) = if let Some(base) = token.strip_suffix('+') {
        (base, true)
    } else {
        (token, false)
    };

    let characters: Vec<char> = base_token.chars().collect();
    let (first_rank, second_rank, suitedness) = match characters.as_slice() {
        [first, second] => (
            Rank::from_str(&first.to_string()).map_err(ParseRangeError::from)?,
            Rank::from_str(&second.to_string()).map_err(ParseRangeError::from)?,
            Suitedness::Any,
        ),
        [first, second, suitedness] => {
            let suitedness = match suitedness.to_ascii_lowercase() {
                's' => Suitedness::Suited,
                'o' => Suitedness::Offsuit,
                _ => return Err(ParseRangeError::new("invalid suitedness marker")),
            };

            (
                Rank::from_str(&first.to_string()).map_err(ParseRangeError::from)?,
                Rank::from_str(&second.to_string()).map_err(ParseRangeError::from)?,
                suitedness,
            )
        }
        _ => {
            return Err(ParseRangeError::new(
                "range token must be a combo like AsKd or a class like AKs",
            ))
        }
    };

    if first_rank == second_rank {
        if !matches!(suitedness, Suitedness::Any) {
            return Err(ParseRangeError::new(
                "pair range tokens cannot use suited or offsuit markers",
            ));
        }

        return expand_pairs(first_rank, plus);
    }

    if first_rank < second_rank {
        return Err(ParseRangeError::new(
            "range class tokens must use descending rank order, for example AKs",
        ));
    }

    expand_non_pair(first_rank, second_rank, suitedness, plus)
}

fn expand_pairs(rank: Rank, plus: bool) -> Result<Vec<HoleCards>, ParseRangeError> {
    let mut combos = Vec::new();
    let start = rank.index();
    let end = Rank::Ace.index();

    for rank_index in start..=end {
        let pair_rank = Rank::from_index(rank_index).expect("pair rank index must be valid");
        combos.extend(generate_pair_combos(pair_rank)?);
        if !plus {
            break;
        }
    }

    Ok(combos)
}

fn expand_non_pair(
    first_rank: Rank,
    second_rank: Rank,
    suitedness: Suitedness,
    plus: bool,
) -> Result<Vec<HoleCards>, ParseRangeError> {
    let mut combos = Vec::new();
    let start = second_rank.index();
    let end = first_rank.index() - 1;

    for second_index in start..=end {
        let next_rank = Rank::from_index(second_index).expect("second rank index must be valid");
        combos.extend(generate_non_pair_combos(first_rank, next_rank, suitedness)?);
        if !plus {
            break;
        }
    }

    Ok(combos)
}

fn generate_pair_combos(rank: Rank) -> Result<Vec<HoleCards>, ParseRangeError> {
    let mut combos = Vec::with_capacity(6);
    for (left_index, left_suit) in Suit::ALL.iter().enumerate() {
        for right_suit in Suit::ALL.iter().skip(left_index + 1) {
            combos.push(
                HoleCards::new(Card::new(rank, *left_suit), Card::new(rank, *right_suit))
                    .map_err(|_| ParseRangeError::new("pair combos must not repeat cards"))?,
            );
        }
    }
    Ok(combos)
}

fn generate_non_pair_combos(
    high_rank: Rank,
    low_rank: Rank,
    suitedness: Suitedness,
) -> Result<Vec<HoleCards>, ParseRangeError> {
    let mut combos = Vec::new();

    for high_suit in Suit::ALL {
        for low_suit in Suit::ALL {
            let is_suited = high_suit == low_suit;
            if matches!(suitedness, Suitedness::Suited) && !is_suited {
                continue;
            }
            if matches!(suitedness, Suitedness::Offsuit) && is_suited {
                continue;
            }

            combos.push(
                HoleCards::new(Card::new(high_rank, high_suit), Card::new(low_rank, low_suit))
                    .map_err(|_| ParseRangeError::new("non-pair combos must not repeat cards"))?,
            );
        }
    }

    Ok(combos)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseRangeError {
    message: String,
}

impl ParseRangeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ParseRangeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ParseRangeError {}

impl From<crate::ParseCardError> for ParseRangeError {
    fn from(value: crate::ParseCardError) -> Self {
        Self::new(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use crate::{Card, CardMask, HoleCards, Range};

    #[test]
    fn common_range_notation_expands_to_expected_combo_counts() {
        assert_eq!("AA".parse::<Range>().unwrap().len(), 6);
        assert_eq!("AKs".parse::<Range>().unwrap().len(), 4);
        assert_eq!("AKo".parse::<Range>().unwrap().len(), 12);
        assert_eq!("AK".parse::<Range>().unwrap().len(), 16);
        assert_eq!("TT+".parse::<Range>().unwrap().len(), 30);
        assert_eq!("ATs+".parse::<Range>().unwrap().len(), 16);
    }

    #[test]
    fn range_round_trips_through_canonical_display() {
        let range: Range = "AKs,AA,AsKd".parse().expect("range should parse");
        let reparsed: Range = range.to_string().parse().expect("range should reparse");

        assert_eq!(range, reparsed);
    }

    #[test]
    fn dead_card_filtering_removes_conflicting_combos() {
        let range: Range = "AK".parse().expect("range should parse");
        let dead_cards = CardMask::from_cards([
            "As".parse().unwrap(),
            "Kh".parse().unwrap(),
        ]);

        let filtered = range.without_dead_cards(dead_cards);

        assert_eq!(filtered.len(), 9);
        assert!(filtered.iter().all(|combo| !combo.mask().intersects(dead_cards)));
    }

    #[test]
    fn explicit_combo_parsing_is_supported() {
        let range: Range = "AsKd,AcKh".parse().expect("range should parse");

        assert_eq!(range.len(), 2);
        assert!(range.contains("AsKd".parse().unwrap()));
        assert!(range.contains("AcKh".parse().unwrap()));
    }

    proptest! {
        #[test]
        fn range_filtering_never_returns_dead_cards(
            raw_pairs in vec((0usize..52, 0usize..52), 0..40),
            dead_indices in vec(0usize..52, 0..8),
        ) {
            let combos = raw_pairs
                .into_iter()
                .filter_map(|(left, right)| {
                    let left = Card::from_index(left).expect("generated left card must be valid");
                    let right = Card::from_index(right).expect("generated right card must be valid");
                    HoleCards::new(left, right).ok()
                })
                .collect::<Vec<_>>();
            let range = Range::from_hole_cards(combos);
            let dead_cards = CardMask::from_cards(dead_indices.into_iter().map(|index| {
                Card::from_index(index).expect("generated dead card must be valid")
            }));

            let filtered = range.without_dead_cards(dead_cards);
            prop_assert!(filtered
                .iter()
                .all(|combo| !combo.mask().intersects(dead_cards)));
        }
    }
}
