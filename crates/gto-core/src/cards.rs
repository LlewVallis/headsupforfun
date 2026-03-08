use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Self; 4] = [Self::Clubs, Self::Diamonds, Self::Hearts, Self::Spades];

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Clubs),
            1 => Some(Self::Diamonds),
            2 => Some(Self::Hearts),
            3 => Some(Self::Spades),
            _ => None,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Clubs => 0,
            Self::Diamonds => 1,
            Self::Hearts => 2,
            Self::Spades => 3,
        }
    }

    pub const fn symbol(self) -> char {
        match self {
            Self::Clubs => 'c',
            Self::Diamonds => 'd',
            Self::Hearts => 'h',
            Self::Spades => 's',
        }
    }
}

impl Display for Suit {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.symbol().to_string())
    }
}

impl FromStr for Suit {
    type Err = ParseCardError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut characters = input.chars();
        let suit = characters
            .next()
            .ok_or_else(|| ParseCardError::new("suit must contain exactly one character"))?;

        if characters.next().is_some() {
            return Err(ParseCardError::new(
                "suit must contain exactly one character",
            ));
        }

        match suit.to_ascii_lowercase() {
            'c' => Ok(Self::Clubs),
            'd' => Ok(Self::Diamonds),
            'h' => Ok(Self::Hearts),
            's' => Ok(Self::Spades),
            _ => Err(ParseCardError::new("invalid suit character")),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    pub const ALL: [Self; 13] = [
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
        Self::Ace,
    ];

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Two),
            1 => Some(Self::Three),
            2 => Some(Self::Four),
            3 => Some(Self::Five),
            4 => Some(Self::Six),
            5 => Some(Self::Seven),
            6 => Some(Self::Eight),
            7 => Some(Self::Nine),
            8 => Some(Self::Ten),
            9 => Some(Self::Jack),
            10 => Some(Self::Queen),
            11 => Some(Self::King),
            12 => Some(Self::Ace),
            _ => None,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Two => 0,
            Self::Three => 1,
            Self::Four => 2,
            Self::Five => 3,
            Self::Six => 4,
            Self::Seven => 5,
            Self::Eight => 6,
            Self::Nine => 7,
            Self::Ten => 8,
            Self::Jack => 9,
            Self::Queen => 10,
            Self::King => 11,
            Self::Ace => 12,
        }
    }

    pub const fn symbol(self) -> char {
        match self {
            Self::Two => '2',
            Self::Three => '3',
            Self::Four => '4',
            Self::Five => '5',
            Self::Six => '6',
            Self::Seven => '7',
            Self::Eight => '8',
            Self::Nine => '9',
            Self::Ten => 'T',
            Self::Jack => 'J',
            Self::Queen => 'Q',
            Self::King => 'K',
            Self::Ace => 'A',
        }
    }
}

impl Display for Rank {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.symbol().to_string())
    }
}

impl FromStr for Rank {
    type Err = ParseCardError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut characters = input.chars();
        let rank = characters
            .next()
            .ok_or_else(|| ParseCardError::new("rank must contain exactly one character"))?;

        if characters.next().is_some() {
            return Err(ParseCardError::new(
                "rank must contain exactly one character",
            ));
        }

        match rank.to_ascii_uppercase() {
            '2' => Ok(Self::Two),
            '3' => Ok(Self::Three),
            '4' => Ok(Self::Four),
            '5' => Ok(Self::Five),
            '6' => Ok(Self::Six),
            '7' => Ok(Self::Seven),
            '8' => Ok(Self::Eight),
            '9' => Ok(Self::Nine),
            'T' => Ok(Self::Ten),
            'J' => Ok(Self::Jack),
            'Q' => Ok(Self::Queen),
            'K' => Ok(Self::King),
            'A' => Ok(Self::Ace),
            _ => Err(ParseCardError::new("invalid rank character")),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Card {
    rank: Rank,
    suit: Suit,
}

impl Card {
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    pub const fn rank(self) -> Rank {
        self.rank
    }

    pub const fn suit(self) -> Suit {
        self.suit
    }

    pub const fn index(self) -> usize {
        self.rank.index() * 4 + self.suit.index()
    }

    pub const fn from_index(index: usize) -> Option<Self> {
        let rank_index = index / 4;
        let suit_index = index % 4;

        let Some(rank) = Rank::from_index(rank_index) else {
            return None;
        };

        let Some(suit) = Suit::from_index(suit_index) else {
            return None;
        };

        Some(Self::new(rank, suit))
    }
}

impl Display for Card {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.rank, self.suit)
    }
}

impl FromStr for Card {
    type Err = ParseCardError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() != 2 {
            return Err(ParseCardError::new("card must contain exactly two characters"));
        }

        let rank = Rank::from_str(&input[0..1])?;
        let suit = Suit::from_str(&input[1..2])?;
        Ok(Self::new(rank, suit))
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CardMask {
    bits: u64,
}

impl CardMask {
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn from_card(card: Card) -> Self {
        Self {
            bits: 1u64 << card.index(),
        }
    }

    pub fn from_cards(cards: impl IntoIterator<Item = Card>) -> Self {
        let mut mask = Self::empty();
        for card in cards {
            mask.insert(card);
        }
        mask
    }

    pub const fn bits(self) -> u64 {
        self.bits
    }

    pub const fn contains(self, card: Card) -> bool {
        self.bits & (1u64 << card.index()) != 0
    }

    pub fn insert(&mut self, card: Card) -> bool {
        let had_card = self.contains(card);
        self.bits |= 1u64 << card.index();
        !had_card
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.bits & other.bits != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub const fn len(self) -> usize {
        self.bits.count_ones() as usize
    }

    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HoleCards {
    cards: [Card; 2],
}

impl HoleCards {
    pub fn new(first: Card, second: Card) -> Result<Self, DuplicateCardError> {
        if first == second {
            return Err(DuplicateCardError { card: first });
        }

        let cards = if first >= second {
            [first, second]
        } else {
            [second, first]
        };

        Ok(Self { cards })
    }

    pub const fn cards(self) -> [Card; 2] {
        self.cards
    }

    pub const fn first(self) -> Card {
        self.cards[0]
    }

    pub const fn second(self) -> Card {
        self.cards[1]
    }

    pub fn contains(self, card: Card) -> bool {
        self.cards[0] == card || self.cards[1] == card
    }

    pub const fn mask(self) -> CardMask {
        CardMask::from_card(self.cards[0]).union(CardMask::from_card(self.cards[1]))
    }
}

impl Display for HoleCards {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.cards[0], self.cards[1])
    }
}

impl FromStr for HoleCards {
    type Err = ParseCardError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() != 4 {
            return Err(ParseCardError::new(
                "hole cards must contain exactly four characters",
            ));
        }

        let first = Card::from_str(&input[0..2])?;
        let second = Card::from_str(&input[2..4])?;
        Self::new(first, second).map_err(|_| ParseCardError::new("hole cards cannot repeat a card"))
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Board {
    cards: Vec<Card>,
}

impl Board {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn try_from_cards(cards: impl IntoIterator<Item = Card>) -> Result<Self, BoardError> {
        let mut board = Self::new();
        for card in cards {
            board.push(card)?;
        }
        Ok(board)
    }

    pub fn push(&mut self, card: Card) -> Result<(), BoardError> {
        if self.cards.len() >= 5 {
            return Err(BoardError::TooManyCards {
                attempted_len: self.cards.len() + 1,
            });
        }

        if self.cards.contains(&card) {
            return Err(BoardError::DuplicateCard { card });
        }

        self.cards.push(card);
        Ok(())
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn mask(&self) -> CardMask {
        CardMask::from_cards(self.cards.iter().copied())
    }
}

impl Display for Board {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for card in &self.cards {
            write!(formatter, "{card}")?;
        }
        Ok(())
    }
}

impl FromStr for Board {
    type Err = ParseCardError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Ok(Self::new());
        }

        if input.len() % 2 != 0 {
            return Err(ParseCardError::new(
                "board text must contain an even number of characters",
            ));
        }

        let mut board = Self::new();
        let mut start = 0;
        while start < input.len() {
            let end = start + 2;
            let card = Card::from_str(&input[start..end])?;
            board
                .push(card)
                .map_err(|error| ParseCardError::new(error.to_string()))?;
            start = end;
        }

        Ok(board)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCardError {
    message: String,
}

impl ParseCardError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ParseCardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ParseCardError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateCardError {
    card: Card,
}

impl DuplicateCardError {
    pub const fn card(self) -> Card {
        self.card
    }
}

impl Display for DuplicateCardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate card {}", self.card)
    }
}

impl Error for DuplicateCardError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardError {
    DuplicateCard { card: Card },
    TooManyCards { attempted_len: usize },
}

impl Display for BoardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCard { card } => write!(formatter, "duplicate board card {card}"),
            Self::TooManyCards { attempted_len } => {
                write!(formatter, "board cannot contain {attempted_len} cards")
            }
        }
    }
}

impl Error for BoardError {}

#[cfg(test)]
mod tests {
    use super::{Board, Card, CardMask, HoleCards, Rank, Suit};

    #[test]
    fn card_parser_round_trips() {
        let card: Card = "As".parse().expect("card should parse");

        assert_eq!(card.rank(), Rank::Ace);
        assert_eq!(card.suit(), Suit::Spades);
        assert_eq!(card.to_string(), "As");
    }

    #[test]
    fn hole_cards_are_canonicalized() {
        let cards: HoleCards = "KdAs".parse().expect("hole cards should parse");

        assert_eq!(cards.to_string(), "AsKd");
        assert!(cards.contains("As".parse().unwrap()));
        assert!(cards.contains("Kd".parse().unwrap()));
    }

    #[test]
    fn board_parser_round_trips() {
        let board: Board = "AsKdTc".parse().expect("board should parse");

        assert_eq!(board.len(), 3);
        assert_eq!(board.to_string(), "AsKdTc");
        assert_eq!(board.mask().len(), 3);
    }

    #[test]
    fn board_rejects_duplicate_cards() {
        let error = "AsKdAs"
            .parse::<Board>()
            .expect_err("board should reject duplicates");

        assert_eq!(error.to_string(), "duplicate board card As");
    }

    #[test]
    fn empty_board_round_trips() {
        let board: Board = "".parse().expect("empty board should parse");

        assert!(board.is_empty());
        assert_eq!(board.to_string(), "");
    }

    #[test]
    fn board_try_from_cards_rejects_more_than_five_cards() {
        let error = Board::try_from_cards([
            "As".parse().unwrap(),
            "Kd".parse().unwrap(),
            "Qc".parse().unwrap(),
            "Jh".parse().unwrap(),
            "Ts".parse().unwrap(),
            "9d".parse().unwrap(),
        ])
        .expect_err("board should reject a sixth card");

        assert_eq!(error.to_string(), "board cannot contain 6 cards");
    }

    #[test]
    fn card_mask_tracks_insertions() {
        let ace_spades: Card = "As".parse().unwrap();
        let king_diamonds: Card = "Kd".parse().unwrap();
        let mut mask = CardMask::empty();

        assert!(mask.insert(ace_spades));
        assert!(!mask.insert(ace_spades));
        assert!(mask.insert(king_diamonds));
        assert!(mask.contains(ace_spades));
        assert!(mask.contains(king_diamonds));
        assert_eq!(mask.len(), 2);
    }
}
