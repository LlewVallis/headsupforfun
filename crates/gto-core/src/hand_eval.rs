use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{Board, Card, CardMask, HoleCards, Rank};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandCategory {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

impl Display for HandCategory {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::HighCard => "high-card",
            Self::OnePair => "one-pair",
            Self::TwoPair => "two-pair",
            Self::ThreeOfAKind => "three-of-a-kind",
            Self::Straight => "straight",
            Self::Flush => "flush",
            Self::FullHouse => "full-house",
            Self::FourOfAKind => "four-of-a-kind",
            Self::StraightFlush => "straight-flush",
        };

        formatter.write_str(label)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandRank {
    category: HandCategory,
    tiebreakers: [Rank; 5],
}

impl HandRank {
    pub const fn category(self) -> HandCategory {
        self.category
    }

    pub const fn tiebreakers(self) -> [Rank; 5] {
        self.tiebreakers
    }
}

impl Display for HandRank {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}(", self.category)?;

        let mut first = true;
        for rank in self.tiebreakers {
            if !first {
                formatter.write_str(",")?;
            }
            write!(formatter, "{rank}")?;
            first = false;
        }

        formatter.write_str(")")
    }
}

pub fn evaluate_five(cards: [Card; 5]) -> Result<HandRank, EvaluateHandError> {
    validate_unique_cards(&cards)?;
    Ok(evaluate_five_unchecked(cards))
}

pub fn evaluate_seven(cards: [Card; 7]) -> Result<HandRank, EvaluateHandError> {
    validate_unique_cards(&cards)?;
    Ok(evaluate_seven_unchecked(cards))
}

pub fn resolve_holdem_showdown(
    board: &Board,
    player_one: HoleCards,
    player_two: HoleCards,
) -> Result<ShowdownResult, ShowdownError> {
    if board.len() != 5 {
        return Err(ShowdownError::BoardMustContainFiveCards {
            actual_len: board.len(),
        });
    }

    let mut seen_cards = board.mask();
    for card in player_one.cards() {
        if !seen_cards.insert(card) {
            return Err(ShowdownError::DuplicateCard { card });
        }
    }

    for card in player_two.cards() {
        if !seen_cards.insert(card) {
            return Err(ShowdownError::DuplicateCard { card });
        }
    }

    let board_cards = board.cards();
    let player_one_rank = evaluate_seven_unchecked([
        player_one.first(),
        player_one.second(),
        board_cards[0],
        board_cards[1],
        board_cards[2],
        board_cards[3],
        board_cards[4],
    ]);
    let player_two_rank = evaluate_seven_unchecked([
        player_two.first(),
        player_two.second(),
        board_cards[0],
        board_cards[1],
        board_cards[2],
        board_cards[3],
        board_cards[4],
    ]);

    Ok(ShowdownResult {
        player_one_rank,
        player_two_rank,
    })
}

pub fn award_pot_heads_up(
    total_pot: u64,
    ordering: Ordering,
    odd_chip_recipient: OddChipRecipient,
) -> HeadsUpPayout {
    match ordering {
        Ordering::Greater => HeadsUpPayout {
            player_one: total_pot,
            player_two: 0,
        },
        Ordering::Less => HeadsUpPayout {
            player_one: 0,
            player_two: total_pot,
        },
        Ordering::Equal => {
            let split = total_pot / 2;
            let odd_chip = total_pot % 2;
            match odd_chip_recipient {
                OddChipRecipient::PlayerOne => HeadsUpPayout {
                    player_one: split + odd_chip,
                    player_two: split,
                },
                OddChipRecipient::PlayerTwo => HeadsUpPayout {
                    player_one: split,
                    player_two: split + odd_chip,
                },
            }
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowdownResult {
    pub player_one_rank: HandRank,
    pub player_two_rank: HandRank,
}

impl ShowdownResult {
    pub fn ordering(self) -> Ordering {
        self.player_one_rank.cmp(&self.player_two_rank)
    }

    pub fn payout(self, total_pot: u64, odd_chip_recipient: OddChipRecipient) -> HeadsUpPayout {
        award_pot_heads_up(total_pot, self.ordering(), odd_chip_recipient)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadsUpPayout {
    pub player_one: u64,
    pub player_two: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OddChipRecipient {
    PlayerOne,
    PlayerTwo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluateHandError {
    DuplicateCard { card: Card },
}

impl Display for EvaluateHandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCard { card } => write!(formatter, "hand contains duplicate card {card}"),
        }
    }
}

impl Error for EvaluateHandError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowdownError {
    BoardMustContainFiveCards { actual_len: usize },
    DuplicateCard { card: Card },
}

impl Display for ShowdownError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardMustContainFiveCards { actual_len } => {
                write!(formatter, "holdem showdown requires a 5-card board, got {actual_len}")
            }
            Self::DuplicateCard { card } => write!(formatter, "showdown contains duplicate card {card}"),
        }
    }
}

impl Error for ShowdownError {}

fn validate_unique_cards(cards: &[Card]) -> Result<(), EvaluateHandError> {
    let mut seen = CardMask::empty();
    for card in cards {
        if !seen.insert(*card) {
            return Err(EvaluateHandError::DuplicateCard { card: *card });
        }
    }
    Ok(())
}

fn evaluate_seven_unchecked(cards: [Card; 7]) -> HandRank {
    let mut best_rank: Option<HandRank> = None;

    for first in 0..3 {
        for second in (first + 1)..4 {
            for third in (second + 1)..5 {
                for fourth in (third + 1)..6 {
                    for fifth in (fourth + 1)..7 {
                        let rank = evaluate_five_unchecked([
                            cards[first],
                            cards[second],
                            cards[third],
                            cards[fourth],
                            cards[fifth],
                        ]);

                        if best_rank.is_none_or(|best| rank > best) {
                            best_rank = Some(rank);
                        }
                    }
                }
            }
        }
    }

    best_rank.expect("seven-card evaluation must inspect at least one combination")
}

fn evaluate_five_unchecked(cards: [Card; 5]) -> HandRank {
    let mut rank_counts = [0u8; 13];
    let mut suit_counts = [0u8; 4];

    for card in cards {
        rank_counts[card.rank().index()] += 1;
        suit_counts[card.suit().index()] += 1;
    }

    let is_flush = suit_counts.iter().any(|count| *count == 5);
    let straight_high_rank = find_straight_high_rank(rank_counts);

    if is_flush && let Some(high_rank) = straight_high_rank {
        return HandRank {
            category: HandCategory::StraightFlush,
            tiebreakers: fill_tiebreakers(&[high_rank]),
        };
    }

    let mut groups = rank_groups(rank_counts);
    groups.sort_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));

    if groups[0].0 == 4 {
        return HandRank {
            category: HandCategory::FourOfAKind,
            tiebreakers: fill_tiebreakers(&[groups[0].1, groups[1].1]),
        };
    }

    if groups[0].0 == 3 && groups[1].0 == 2 {
        return HandRank {
            category: HandCategory::FullHouse,
            tiebreakers: fill_tiebreakers(&[groups[0].1, groups[1].1]),
        };
    }

    if is_flush {
        return HandRank {
            category: HandCategory::Flush,
            tiebreakers: fill_tiebreakers(&descending_ranks(rank_counts)),
        };
    }

    if let Some(high_rank) = straight_high_rank {
        return HandRank {
            category: HandCategory::Straight,
            tiebreakers: fill_tiebreakers(&[high_rank]),
        };
    }

    if groups[0].0 == 3 {
        return HandRank {
            category: HandCategory::ThreeOfAKind,
            tiebreakers: fill_tiebreakers(&[groups[0].1, groups[1].1, groups[2].1]),
        };
    }

    if groups[0].0 == 2 && groups[1].0 == 2 {
        return HandRank {
            category: HandCategory::TwoPair,
            tiebreakers: fill_tiebreakers(&[groups[0].1, groups[1].1, groups[2].1]),
        };
    }

    if groups[0].0 == 2 {
        return HandRank {
            category: HandCategory::OnePair,
            tiebreakers: fill_tiebreakers(&[groups[0].1, groups[1].1, groups[2].1, groups[3].1]),
        };
    }

    HandRank {
        category: HandCategory::HighCard,
        tiebreakers: fill_tiebreakers(&descending_ranks(rank_counts)),
    }
}

fn rank_groups(rank_counts: [u8; 13]) -> Vec<(u8, Rank)> {
    rank_counts
        .iter()
        .enumerate()
        .filter_map(|(index, count)| {
            if *count == 0 {
                None
            } else {
                Some((
                    *count,
                    Rank::from_index(index).expect("rank group index must be valid"),
                ))
            }
        })
        .collect()
}

fn descending_ranks(rank_counts: [u8; 13]) -> Vec<Rank> {
    let mut ranks = Vec::new();
    for index in (0..13).rev() {
        let rank = Rank::from_index(index).expect("descending rank index must be valid");
        for _ in 0..rank_counts[index] {
            ranks.push(rank);
        }
    }
    ranks
}

fn find_straight_high_rank(rank_counts: [u8; 13]) -> Option<Rank> {
    if rank_counts[Rank::Ace.index()] > 0
        && rank_counts[Rank::Two.index()] > 0
        && rank_counts[Rank::Three.index()] > 0
        && rank_counts[Rank::Four.index()] > 0
        && rank_counts[Rank::Five.index()] > 0
    {
        return Some(Rank::Five);
    }

    let mut consecutive = 0;
    for index in 0..13 {
        if rank_counts[index] > 0 {
            consecutive += 1;
            if consecutive == 5 {
                return Rank::from_index(index);
            }
        } else {
            consecutive = 0;
        }
    }

    None
}

fn fill_tiebreakers(ranks: &[Rank]) -> [Rank; 5] {
    let mut tiebreakers = [Rank::Two; 5];
    for (index, rank) in ranks.iter().enumerate() {
        tiebreakers[index] = *rank;
    }
    tiebreakers
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use proptest::collection::btree_set;
    use proptest::prelude::*;

    use crate::{
        Board, Card, HandCategory, HeadsUpPayout, OddChipRecipient, ShowdownError,
        award_pot_heads_up, evaluate_five, evaluate_seven, resolve_holdem_showdown,
    };

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
            .expect("test input should contain the expected card count")
    }

    fn slow_evaluate_seven(cards: [Card; 7]) -> crate::HandRank {
        let cards = cards.to_vec();
        let mut best = None;
        let mut current = Vec::with_capacity(5);
        choose_five(&cards, 0, &mut current, &mut best);
        best.expect("slow evaluator should produce a result")
    }

    fn choose_five(
        cards: &[Card],
        start: usize,
        current: &mut Vec<Card>,
        best: &mut Option<crate::HandRank>,
    ) {
        if current.len() == 5 {
            let rank = evaluate_five(current.clone().try_into().unwrap())
                .expect("slow evaluator combinations should be valid");
            if best.is_none_or(|candidate| rank > candidate) {
                *best = Some(rank);
            }
            return;
        }

        for index in start..cards.len() {
            current.push(cards[index]);
            choose_five(cards, index + 1, current, best);
            current.pop();
        }
    }

    #[test]
    fn five_card_categories_have_the_expected_strength_order() {
        let high_card = evaluate_five(parse_cards::<5>("AsKd9c7h3s")).unwrap();
        let one_pair = evaluate_five(parse_cards::<5>("AsAd9c7h3s")).unwrap();
        let two_pair = evaluate_five(parse_cards::<5>("AsAd9c9h3s")).unwrap();
        let trips = evaluate_five(parse_cards::<5>("AsAdAh7h3s")).unwrap();
        let straight = evaluate_five(parse_cards::<5>("9s8d7c6h5s")).unwrap();
        let flush = evaluate_five(parse_cards::<5>("AsTs8s5s2s")).unwrap();
        let full_house = evaluate_five(parse_cards::<5>("AsAdAh7h7s")).unwrap();
        let quads = evaluate_five(parse_cards::<5>("AsAdAhAc7s")).unwrap();
        let straight_flush = evaluate_five(parse_cards::<5>("9s8s7s6s5s")).unwrap();

        assert_eq!(high_card.category(), HandCategory::HighCard);
        assert_eq!(straight_flush.category(), HandCategory::StraightFlush);
        assert!(
            high_card < one_pair
                && one_pair < two_pair
                && two_pair < trips
                && trips < straight
                && straight < flush
                && flush < full_house
                && full_house < quads
                && quads < straight_flush
        );
    }

    #[test]
    fn five_card_tie_breakers_are_ranked_correctly() {
        let ace_high_straight = evaluate_five(parse_cards::<5>("AsKdQhJcTs")).unwrap();
        let wheel = evaluate_five(parse_cards::<5>("As2d3h4c5s")).unwrap();
        let aces_full = evaluate_five(parse_cards::<5>("AsAdAhKcKd")).unwrap();
        let kings_full = evaluate_five(parse_cards::<5>("KsKdKhAcAd")).unwrap();

        assert!(ace_high_straight > wheel);
        assert!(aces_full > kings_full);
    }

    #[test]
    fn showdown_detects_ties_and_splits_the_pot() {
        let board: Board = "AsKsQsJsTs".parse().unwrap();
        let showdown = resolve_holdem_showdown(&board, "2c3d".parse().unwrap(), "4h5c".parse().unwrap())
            .expect("showdown should resolve");

        assert_eq!(showdown.ordering(), Ordering::Equal);
        assert_eq!(
            showdown.payout(100, OddChipRecipient::PlayerOne),
            HeadsUpPayout {
                player_one: 50,
                player_two: 50,
            }
        );
        assert_eq!(
            showdown.payout(101, OddChipRecipient::PlayerTwo),
            HeadsUpPayout {
                player_one: 50,
                player_two: 51,
            }
        );
    }

    #[test]
    fn showdown_detects_duplicate_cards() {
        let board: Board = "AhKhQh2c2d".parse().unwrap();
        let error = resolve_holdem_showdown(&board, "JhTh".parse().unwrap(), "AcAh".parse().unwrap())
            .expect_err("showdown should reject duplicate cards");

        assert_eq!(error, ShowdownError::DuplicateCard { card: "Ah".parse().unwrap() });
    }

    #[test]
    fn showdown_rejects_incomplete_boards() {
        let board: Board = "AhKhQh".parse().unwrap();
        let error = resolve_holdem_showdown(&board, "JhTh".parse().unwrap(), "AcAd".parse().unwrap())
            .expect_err("showdown should reject boards with fewer than five cards");

        assert_eq!(error, ShowdownError::BoardMustContainFiveCards { actual_len: 3 });
    }

    #[test]
    fn award_pot_heads_up_pays_the_winner() {
        assert_eq!(
            award_pot_heads_up(64, Ordering::Greater, OddChipRecipient::PlayerOne),
            HeadsUpPayout {
                player_one: 64,
                player_two: 0,
            }
        );
        assert_eq!(
            award_pot_heads_up(64, Ordering::Less, OddChipRecipient::PlayerOne),
            HeadsUpPayout {
                player_one: 0,
                player_two: 64,
            }
        );
    }

    #[test]
    fn award_pot_heads_up_splits_odd_chips_by_configuration() {
        assert_eq!(
            award_pot_heads_up(101, Ordering::Equal, OddChipRecipient::PlayerOne),
            HeadsUpPayout {
                player_one: 51,
                player_two: 50,
            }
        );
        assert_eq!(
            award_pot_heads_up(101, Ordering::Equal, OddChipRecipient::PlayerTwo),
            HeadsUpPayout {
                player_one: 50,
                player_two: 51,
            }
        );
    }

    proptest! {
        #[test]
        fn seven_card_evaluator_matches_independent_combination_search(
            indices in btree_set(0usize..52, 7),
        ) {
            let cards: [Card; 7] = indices
                .into_iter()
                .map(|index| Card::from_index(index).expect("generated card should be valid"))
                .collect::<Vec<_>>()
                .try_into()
                .expect("generated set should contain exactly seven cards");

            let fast = evaluate_seven(cards).expect("generated seven-card hand should be valid");
            let slow = slow_evaluate_seven(cards);

            prop_assert_eq!(fast, slow);
        }
    }
}
