use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::collections::HashMap;

use gto_core::{
    Card, HandCategory, HandRank, HoldemHandState, HoldemStateError, HoleCards, Player,
    PlayerAction, Range, Rank, Street, evaluate_five, evaluate_seven,
};

use crate::{AbstractionProfile, AbstractAction, OpeningSize, RaiseSize, StreetProfile, abstract_actions};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlueprintActionKind {
    Fold,
    Check,
    Call,
    Aggression1,
    Aggression2,
    Aggression3,
    AllIn,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintActionProbability {
    pub action: BlueprintActionKind,
    pub probability: f64,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartingRangeName {
    ButtonOpenRaiseLarge,
    ButtonOpenRaiseSmall,
    ButtonOpenLimp,
    BigBlindIsoRaiseVsLimp,
    BigBlindDefendVsOpen,
    BigBlindThreeBetVsOpen,
    ButtonContinueVsIso,
    ButtonRaiseVsIso,
    ButtonContinueVsThreeBet,
    ButtonFourBetVsThreeBet,
    BigBlindContinueVsFourBet,
    BigBlindFiveBetVsFourBet,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingRanges {
    pub button_open_raise_large: Range,
    pub button_open_raise_small: Range,
    pub button_open_limp: Range,
    pub big_blind_iso_raise_vs_limp: Range,
    pub big_blind_defend_vs_open: Range,
    pub big_blind_three_bet_vs_open: Range,
    pub button_continue_vs_iso: Range,
    pub button_raise_vs_iso: Range,
    pub button_continue_vs_three_bet: Range,
    pub button_four_bet_vs_three_bet: Range,
    pub big_blind_continue_vs_four_bet: Range,
    pub big_blind_five_bet_vs_four_bet: Range,
}

impl StartingRanges {
    pub fn smoke_default() -> Self {
        Self {
            button_open_raise_large: "JJ+,AQs+,AKo".parse().unwrap(),
            button_open_raise_small: "55+,A2s+,K9s+,Q9s+,J9s+,T8s+,98s,87s,76s,A9o+,KTo+,QTo+,JTo".parse().unwrap(),
            button_open_limp: "22+,A2s+,K2s+,Q4s+,J6s+,T6s+,96s+,86s+,75s+,65s,54s,A2o+,K8o+,Q9o+,J9o+,T9o".parse().unwrap(),
            big_blind_iso_raise_vs_limp: "99+,AJs+,AQo+,KQs".parse().unwrap(),
            big_blind_defend_vs_open: "22+,A2s+,K7s+,Q8s+,J8s+,T8s+,97s+,86s+,76s,65s,A7o+,KTo+,QTo+,JTo".parse().unwrap(),
            big_blind_three_bet_vs_open: "TT+,AQs+,AKo,KQs,A5s,A4s".parse().unwrap(),
            button_continue_vs_iso: "22+,A2s+,KTs+,QTs+,JTs,T9s,98s,87s,AJo+,KQo".parse().unwrap(),
            button_raise_vs_iso: "QQ+,AKs,AKo,AQs".parse().unwrap(),
            button_continue_vs_three_bet: "88+,AQs+,AKo,AJs,KQs".parse().unwrap(),
            button_four_bet_vs_three_bet: "QQ+,AKs,AKo".parse().unwrap(),
            big_blind_continue_vs_four_bet: "JJ+,AQs+,AKo".parse().unwrap(),
            big_blind_five_bet_vs_four_bet: "KK+,AKs".parse().unwrap(),
        }
    }

    pub fn get(&self, name: StartingRangeName) -> &Range {
        match name {
            StartingRangeName::ButtonOpenRaiseLarge => &self.button_open_raise_large,
            StartingRangeName::ButtonOpenRaiseSmall => &self.button_open_raise_small,
            StartingRangeName::ButtonOpenLimp => &self.button_open_limp,
            StartingRangeName::BigBlindIsoRaiseVsLimp => &self.big_blind_iso_raise_vs_limp,
            StartingRangeName::BigBlindDefendVsOpen => &self.big_blind_defend_vs_open,
            StartingRangeName::BigBlindThreeBetVsOpen => &self.big_blind_three_bet_vs_open,
            StartingRangeName::ButtonContinueVsIso => &self.button_continue_vs_iso,
            StartingRangeName::ButtonRaiseVsIso => &self.button_raise_vs_iso,
            StartingRangeName::ButtonContinueVsThreeBet => &self.button_continue_vs_three_bet,
            StartingRangeName::ButtonFourBetVsThreeBet => &self.button_four_bet_vs_three_bet,
            StartingRangeName::BigBlindContinueVsFourBet => &self.big_blind_continue_vs_four_bet,
            StartingRangeName::BigBlindFiveBetVsFourBet => &self.big_blind_five_bet_vs_four_bet,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PreflopContextKey {
    pub actor: Player,
    pub prior_limp: bool,
    pub aggressive_actions: u8,
    pub effective_stack_bucket: EffectiveStackBucket,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflopRangeRule {
    pub range: StartingRangeName,
    pub action: BlueprintActionKind,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflopPolicyEntry {
    pub context: PreflopContextKey,
    pub default_action: BlueprintActionKind,
    pub rules: Vec<PreflopRangeRule>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectiveStackBucket {
    UpTo15Bb,
    Bb16To25,
    Bb26To40,
    Bb41To75,
    Bb76Plus,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StackPressureBucket {
    Low,
    Medium,
    High,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MadeHandBucket {
    Monster,
    Strong,
    Medium,
    Weak,
    Air,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawBucket {
    None,
    Straight,
    Flush,
    Combo,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PostflopPolicyKey {
    pub street: Street,
    pub actor: Player,
    pub facing_bet: bool,
    pub aggressive_actions: u8,
    pub stack_pressure: StackPressureBucket,
    pub made_hand: MadeHandBucket,
    pub draw: DrawBucket,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PostflopPolicyEntry {
    pub key: PostflopPolicyKey,
    pub actions: Vec<BlueprintActionProbability>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct FullHandBlueprintArtifact {
    pub format_version: u32,
    pub profile: AbstractionProfile,
    pub starting_ranges: StartingRanges,
    pub preflop_policies: Vec<PreflopPolicyEntry>,
    pub postflop_policies: Vec<PostflopPolicyEntry>,
}

impl FullHandBlueprintArtifact {
    pub const FORMAT_VERSION: u32 = 2;

    pub fn smoke_default() -> Self {
        let starting_ranges = StartingRanges::smoke_default();
        let profile = smoke_blueprint_profile();
        Self {
            format_version: Self::FORMAT_VERSION,
            profile,
            starting_ranges,
            preflop_policies: default_preflop_policies(),
            postflop_policies: default_postflop_policies(),
        }
    }

    fn validate_version(&self) -> Result<(), BlueprintArtifactError> {
        if self.format_version == Self::FORMAT_VERSION {
            Ok(())
        } else {
            Err(BlueprintArtifactError::UnsupportedFormatVersion {
                expected: Self::FORMAT_VERSION,
                actual: self.format_version,
            })
        }
    }

    pub fn preflop_policy(&self, context: PreflopContextKey) -> Option<&PreflopPolicyEntry> {
        self.preflop_policies.iter().find(|entry| entry.context == context)
    }

    pub fn postflop_policy(&self, key: PostflopPolicyKey) -> Option<&PostflopPolicyEntry> {
        self.postflop_policies.iter().find(|entry| entry.key == key)
    }

    #[cfg(feature = "serde")]
    pub fn to_json_string(&self) -> Result<String, BlueprintArtifactError> {
        self.validate_version()?;
        serde_json::to_string(self)
            .map_err(|error| BlueprintArtifactError::Encode(error.to_string()))
    }

    #[cfg(feature = "serde")]
    pub fn from_json_str(input: &str) -> Result<Self, BlueprintArtifactError> {
        let artifact = serde_json::from_str::<Self>(input)
            .map_err(|error| BlueprintArtifactError::Decode(error.to_string()))?;
        artifact.validate_version()?;
        Ok(artifact)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintBot {
    artifact: FullHandBlueprintArtifact,
    preflop_index: HashMap<PreflopContextKey, usize>,
    postflop_index: HashMap<PostflopPolicyKey, usize>,
}

impl Default for BlueprintBot {
    fn default() -> Self {
        Self::new(FullHandBlueprintArtifact::smoke_default())
    }
}

impl BlueprintBot {
    pub fn new(artifact: FullHandBlueprintArtifact) -> Self {
        let preflop_index = artifact
            .preflop_policies
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.context, index))
            .collect();
        let postflop_index = artifact
            .postflop_policies
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.key, index))
            .collect();

        Self {
            artifact,
            preflop_index,
            postflop_index,
        }
    }

    pub fn artifact(&self) -> &FullHandBlueprintArtifact {
        &self.artifact
    }

    pub fn profile(&self) -> &AbstractionProfile {
        &self.artifact.profile
    }

    pub fn choose_action(
        &self,
        bot_player: Player,
        state: &HoldemHandState,
    ) -> Result<PlayerAction, BlueprintBotError> {
        if state.current_actor() != Some(bot_player) {
            return Err(BlueprintBotError::NotActorsTurn {
                expected: state.current_actor(),
                actual: bot_player,
            });
        }

        let legal = abstract_actions(state, &self.artifact.profile).map_err(BlueprintBotError::State)?;
        if legal.is_empty() {
            return Err(BlueprintBotError::NoLegalAbstractActions);
        }

        let choice = if state.street() == Street::Preflop {
            let context = preflop_context_from_state(state)?;
            let policy = self
                .preflop_policy(context)
                .ok_or(BlueprintBotError::MissingPreflopPolicy(context))?;
            self.choose_preflop_action(policy, state.player(bot_player).hole_cards, &legal)
                .ok_or(BlueprintBotError::NoMatchingPolicyAction)?
        } else {
            let key = postflop_policy_key(bot_player, state)?;
            let policy = self
                .postflop_policy(key)
                .ok_or(BlueprintBotError::MissingPostflopPolicy(key))?;
            choose_policy_action(&policy.actions, &legal)
                .ok_or(BlueprintBotError::NoMatchingPolicyAction)?
        };

        Ok(choice.to_player_action())
    }

    fn choose_preflop_action(
        &self,
        policy: &PreflopPolicyEntry,
        hole_cards: HoleCards,
        legal: &[AbstractAction],
    ) -> Option<AbstractAction> {
        for rule in &policy.rules {
            if self.artifact.starting_ranges.get(rule.range).contains(hole_cards) {
                if let Some(action) = resolve_action_kind(rule.action, legal) {
                    return Some(action);
                }
            }
        }

        resolve_action_kind(policy.default_action, legal).or_else(|| safe_fallback_action(legal))
    }

    fn preflop_policy(&self, context: PreflopContextKey) -> Option<&PreflopPolicyEntry> {
        self.preflop_index
            .get(&context)
            .and_then(|index| self.artifact.preflop_policies.get(*index))
    }

    fn postflop_policy(&self, key: PostflopPolicyKey) -> Option<&PostflopPolicyEntry> {
        self.postflop_index
            .get(&key)
            .and_then(|index| self.artifact.postflop_policies.get(*index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueprintArtifactError {
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    Encode(String),
    Decode(String),
}

impl Display for BlueprintArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { expected, actual } => write!(
                formatter,
                "unsupported blueprint artifact format version {actual}; expected {expected}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode blueprint artifact: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode blueprint artifact: {error}"),
        }
    }
}

impl Error for BlueprintArtifactError {}

#[derive(Debug)]
pub enum BlueprintBotError {
    NoLegalAbstractActions,
    NoMatchingPolicyAction,
    MissingPreflopPolicy(PreflopContextKey),
    MissingPostflopPolicy(PostflopPolicyKey),
    UnsupportedStreetState(Street),
    NotActorsTurn {
        expected: Option<Player>,
        actual: Player,
    },
    State(HoldemStateError),
}

impl Display for BlueprintBotError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLegalAbstractActions => formatter.write_str("blueprint bot found no legal abstract actions"),
            Self::NoMatchingPolicyAction => formatter.write_str("blueprint bot found no policy action that matched the legal menu"),
            Self::MissingPreflopPolicy(context) => {
                write!(formatter, "blueprint bot is missing a preflop policy for {:?}", context)
            }
            Self::MissingPostflopPolicy(key) => {
                write!(formatter, "blueprint bot is missing a postflop policy for {:?}", key)
            }
            Self::UnsupportedStreetState(street) => {
                write!(formatter, "blueprint bot cannot build a policy key for {street}")
            }
            Self::NotActorsTurn { expected, actual } => write!(
                formatter,
                "blueprint bot expected actor {:?}, got {actual}",
                expected
            ),
            Self::State(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for BlueprintBotError {}

pub fn smoke_blueprint_profile() -> AbstractionProfile {
    let preflop = StreetProfile {
        opening_sizes: vec![
            OpeningSize::BigBlindMultipleBps(25_000),
            OpeningSize::BigBlindMultipleBps(40_000),
            OpeningSize::BigBlindMultipleBps(70_000),
        ],
        raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
        include_all_in: true,
    };
    let postflop = StreetProfile {
        opening_sizes: vec![
            OpeningSize::PotFractionBps(3_300),
            OpeningSize::PotFractionBps(6_600),
            OpeningSize::PotFractionBps(10_000),
        ],
        raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
        include_all_in: true,
    };
    AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop)
}

pub fn preflop_context_from_state(
    state: &HoldemHandState,
) -> Result<PreflopContextKey, BlueprintBotError> {
    if state.street() != Street::Preflop {
        return Err(BlueprintBotError::UnsupportedStreetState(state.street()));
    }

    let actor = state.current_actor().ok_or(BlueprintBotError::NoLegalAbstractActions)?;
    let actions = actions_for_street(state, Street::Preflop);
    let prior_limp = matches!(actions.first(), Some(PlayerAction::Call));
    let aggressive_actions = actions
        .iter()
        .filter(|action| is_aggressive_action(**action))
        .count()
        .min(4) as u8;

    Ok(PreflopContextKey {
        actor,
        prior_limp,
        aggressive_actions,
        effective_stack_bucket: effective_stack_bucket_from_state(state),
    })
}
fn default_preflop_policies() -> Vec<PreflopPolicyEntry> {
    let mut entries = Vec::new();
    for bucket in [
        EffectiveStackBucket::UpTo15Bb,
        EffectiveStackBucket::Bb16To25,
        EffectiveStackBucket::Bb26To40,
        EffectiveStackBucket::Bb41To75,
        EffectiveStackBucket::Bb76Plus,
    ] {
        entries.extend(default_preflop_policies_for_bucket(bucket));
    }
    entries
}

fn default_preflop_policies_for_bucket(
    bucket: EffectiveStackBucket,
) -> Vec<PreflopPolicyEntry> {
    let button_open_large = match bucket {
        EffectiveStackBucket::UpTo15Bb => BlueprintActionKind::AllIn,
        EffectiveStackBucket::Bb16To25 | EffectiveStackBucket::Bb26To40 => {
            BlueprintActionKind::Aggression3
        }
        EffectiveStackBucket::Bb41To75 | EffectiveStackBucket::Bb76Plus => {
            BlueprintActionKind::Aggression2
        }
    };
    let button_open_small = match bucket {
        EffectiveStackBucket::UpTo15Bb => BlueprintActionKind::AllIn,
        EffectiveStackBucket::Bb16To25 => BlueprintActionKind::Aggression2,
        EffectiveStackBucket::Bb26To40
        | EffectiveStackBucket::Bb41To75
        | EffectiveStackBucket::Bb76Plus => BlueprintActionKind::Aggression1,
    };
    let big_blind_iso = match bucket {
        EffectiveStackBucket::UpTo15Bb => BlueprintActionKind::AllIn,
        EffectiveStackBucket::Bb16To25 => BlueprintActionKind::Aggression2,
        EffectiveStackBucket::Bb26To40
        | EffectiveStackBucket::Bb41To75
        | EffectiveStackBucket::Bb76Plus => BlueprintActionKind::Aggression1,
    };
    let big_blind_three_bet = match bucket {
        EffectiveStackBucket::UpTo15Bb | EffectiveStackBucket::Bb16To25 => {
            BlueprintActionKind::AllIn
        }
        EffectiveStackBucket::Bb26To40 => BlueprintActionKind::Aggression2,
        EffectiveStackBucket::Bb41To75 | EffectiveStackBucket::Bb76Plus => {
            BlueprintActionKind::Aggression1
        }
    };
    let button_raise_vs_iso = match bucket {
        EffectiveStackBucket::UpTo15Bb | EffectiveStackBucket::Bb16To25 => {
            BlueprintActionKind::AllIn
        }
        EffectiveStackBucket::Bb26To40
        | EffectiveStackBucket::Bb41To75
        | EffectiveStackBucket::Bb76Plus => BlueprintActionKind::Aggression1,
    };
    let button_four_bet = match bucket {
        EffectiveStackBucket::UpTo15Bb
        | EffectiveStackBucket::Bb16To25
        | EffectiveStackBucket::Bb26To40 => BlueprintActionKind::AllIn,
        EffectiveStackBucket::Bb41To75 | EffectiveStackBucket::Bb76Plus => {
            BlueprintActionKind::Aggression1
        }
    };

    vec![
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::Button,
                prior_limp: false,
                aggressive_actions: 0,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![
                PreflopRangeRule {
                    range: StartingRangeName::ButtonOpenRaiseLarge,
                    action: button_open_large,
                },
                PreflopRangeRule {
                    range: StartingRangeName::ButtonOpenRaiseSmall,
                    action: button_open_small,
                },
                PreflopRangeRule {
                    range: StartingRangeName::ButtonOpenLimp,
                    action: BlueprintActionKind::Call,
                },
            ],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::BigBlind,
                prior_limp: true,
                aggressive_actions: 0,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Check,
            rules: vec![PreflopRangeRule {
                range: StartingRangeName::BigBlindIsoRaiseVsLimp,
                action: big_blind_iso,
            }],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::BigBlind,
                prior_limp: false,
                aggressive_actions: 1,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![
                PreflopRangeRule {
                    range: StartingRangeName::BigBlindThreeBetVsOpen,
                    action: big_blind_three_bet,
                },
                PreflopRangeRule {
                    range: StartingRangeName::BigBlindDefendVsOpen,
                    action: BlueprintActionKind::Call,
                },
            ],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::Button,
                prior_limp: true,
                aggressive_actions: 1,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![
                PreflopRangeRule {
                    range: StartingRangeName::ButtonRaiseVsIso,
                    action: button_raise_vs_iso,
                },
                PreflopRangeRule {
                    range: StartingRangeName::ButtonContinueVsIso,
                    action: BlueprintActionKind::Call,
                },
            ],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::Button,
                prior_limp: false,
                aggressive_actions: 2,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![
                PreflopRangeRule {
                    range: StartingRangeName::ButtonFourBetVsThreeBet,
                    action: button_four_bet,
                },
                PreflopRangeRule {
                    range: StartingRangeName::ButtonContinueVsThreeBet,
                    action: BlueprintActionKind::Call,
                },
            ],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::BigBlind,
                prior_limp: false,
                aggressive_actions: 3,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![
                PreflopRangeRule {
                    range: StartingRangeName::BigBlindFiveBetVsFourBet,
                    action: BlueprintActionKind::AllIn,
                },
                PreflopRangeRule {
                    range: StartingRangeName::BigBlindContinueVsFourBet,
                    action: BlueprintActionKind::Call,
                },
            ],
        },
        PreflopPolicyEntry {
            context: PreflopContextKey {
                actor: Player::Button,
                prior_limp: false,
                aggressive_actions: 4,
                effective_stack_bucket: bucket,
            },
            default_action: BlueprintActionKind::Fold,
            rules: vec![PreflopRangeRule {
                range: StartingRangeName::ButtonFourBetVsThreeBet,
                action: BlueprintActionKind::Call,
            }],
        },
    ]
}

fn default_postflop_policies() -> Vec<PostflopPolicyEntry> {
    let mut entries = Vec::new();
    for street in [Street::Flop, Street::Turn, Street::River] {
        for actor in Player::ALL {
            for facing_bet in [false, true] {
                for aggressive_actions in 0..=3 {
                    for stack_pressure in [
                        StackPressureBucket::Low,
                        StackPressureBucket::Medium,
                        StackPressureBucket::High,
                    ] {
                        for made_hand in [
                            MadeHandBucket::Monster,
                            MadeHandBucket::Strong,
                            MadeHandBucket::Medium,
                            MadeHandBucket::Weak,
                            MadeHandBucket::Air,
                        ] {
                            for draw in [
                                DrawBucket::None,
                                DrawBucket::Straight,
                                DrawBucket::Flush,
                                DrawBucket::Combo,
                            ] {
                                let key = PostflopPolicyKey {
                                    street,
                                    actor,
                                    facing_bet,
                                    aggressive_actions,
                                    stack_pressure,
                                    made_hand,
                                    draw,
                                };
                                entries.push(PostflopPolicyEntry {
                                    key,
                                    actions: default_postflop_action_mix(key),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    entries
}

fn default_postflop_action_mix(key: PostflopPolicyKey) -> Vec<BlueprintActionProbability> {
    use BlueprintActionKind::{Aggression1, Aggression2, Aggression3, AllIn, Call, Check, Fold};
    use DrawBucket::{Combo, Flush, None, Straight};
    use MadeHandBucket::{Air, Medium, Monster, Strong, Weak};
    use StackPressureBucket::{High, Low};

    let actions = if key.facing_bet {
        match (key.made_hand, key.draw, key.stack_pressure) {
            (Monster, _, Low) => vec![(AllIn, 0.65), (Aggression2, 0.20), (Call, 0.15)],
            (Monster, _, _) => vec![(Aggression2, 0.45), (Aggression1, 0.25), (Call, 0.20), (AllIn, 0.10)],
            (Strong, _, Low) => vec![(Call, 0.55), (AllIn, 0.30), (Aggression1, 0.15)],
            (Strong, _, _) => vec![(Call, 0.50), (Aggression1, 0.30), (Aggression2, 0.10), (Fold, 0.10)],
            (Medium, Combo | Flush, _) => vec![(Call, 0.55), (Aggression1, 0.20), (Aggression2, 0.05), (Fold, 0.20)],
            (Medium, _, _) => vec![(Call, 0.70), (Fold, 0.30)],
            (Weak, Combo, _) => vec![(Call, 0.50), (Aggression1, 0.20), (Fold, 0.30)],
            (Weak, Flush | Straight, High) => vec![(Call, 0.45), (Aggression1, 0.15), (Fold, 0.40)],
            (Weak, Flush | Straight, _) => vec![(Call, 0.35), (Fold, 0.65)],
            (Weak, None, _) => vec![(Fold, 0.85), (Call, 0.15)],
            (Air, Combo, _) => vec![(Aggression1, 0.35), (Fold, 0.65)],
            (Air, Flush | Straight, High) => vec![(Call, 0.35), (Fold, 0.65)],
            (Air, _, _) => vec![(Fold, 0.90), (Call, 0.10)],
        }
    } else {
        match (key.made_hand, key.draw, key.stack_pressure, key.street) {
            (Monster, _, Low, _) => vec![(AllIn, 0.50), (Aggression3, 0.20), (Aggression2, 0.20), (Check, 0.10)],
            (Monster, _, _, _) => vec![(Aggression3, 0.35), (Aggression2, 0.30), (Aggression1, 0.20), (Check, 0.15)],
            (Strong, _, _, Street::River) => vec![(Aggression3, 0.20), (Aggression2, 0.30), (Aggression1, 0.25), (Check, 0.25)],
            (Strong, _, _, _) => vec![(Aggression2, 0.30), (Aggression1, 0.35), (Check, 0.35)],
            (Medium, Combo | Flush, _, _) => vec![(Aggression1, 0.30), (Aggression2, 0.10), (Check, 0.60)],
            (Medium, _, _, Street::River) => vec![(Check, 0.65), (Aggression1, 0.35)],
            (Medium, _, _, _) => vec![(Check, 0.75), (Aggression1, 0.25)],
            (Weak, Combo, _, _) => vec![(Aggression1, 0.30), (Check, 0.70)],
            (Weak, Flush | Straight, High, _) => vec![(Aggression1, 0.25), (Check, 0.75)],
            (Weak, _, _, _) => vec![(Check, 0.90), (Aggression1, 0.10)],
            (Air, Combo, _, _) => vec![(Aggression1, 0.25), (Check, 0.75)],
            (Air, Flush | Straight, _, _) => vec![(Check, 0.85), (Aggression1, 0.15)],
            (Air, None, _, _) => vec![(Check, 0.95), (Aggression1, 0.05)],
        }
    };

    normalized_action_probabilities(actions)
}

fn normalized_action_probabilities(
    actions: Vec<(BlueprintActionKind, f64)>,
) -> Vec<BlueprintActionProbability> {
    let sum = actions.iter().map(|(_, weight)| *weight).sum::<f64>();
    let normalizer = if sum > 0.0 { sum } else { 1.0 };
    actions
        .into_iter()
        .map(|(action, weight)| BlueprintActionProbability {
            action,
            probability: weight / normalizer,
        })
        .collect()
}

pub fn postflop_policy_key(
    actor: Player,
    state: &HoldemHandState,
) -> Result<PostflopPolicyKey, BlueprintBotError> {
    let hole_cards = state.player(actor).hole_cards;
    let board = state.board().cards();
    if board.len() < 3 {
        return Err(BlueprintBotError::UnsupportedStreetState(state.street()));
    }

    let legal = state.legal_actions().map_err(BlueprintBotError::State)?;
    Ok(PostflopPolicyKey {
        street: state.street(),
        actor,
        facing_bet: legal.call_amount.is_some(),
        aggressive_actions: actions_for_street(state, state.street())
            .iter()
            .filter(|action| is_aggressive_action(**action))
            .count()
            .min(3) as u8,
        stack_pressure: classify_stack_pressure(state, actor),
        made_hand: classify_made_hand(hole_cards, board),
        draw: classify_draw_bucket(hole_cards, board),
    })
}

fn classify_stack_pressure(state: &HoldemHandState, actor: Player) -> StackPressureBucket {
    let effective_stack = state
        .player(actor)
        .stack
        .min(state.player(actor.opponent()).stack);
    let pot = state.pot().max(1);
    let spr = effective_stack as f64 / pot as f64;

    if spr <= 1.5 {
        StackPressureBucket::Low
    } else if spr <= 4.0 {
        StackPressureBucket::Medium
    } else {
        StackPressureBucket::High
    }
}

fn effective_stack_bucket_from_state(state: &HoldemHandState) -> EffectiveStackBucket {
    let effective_stack = state
        .starting_stack(Player::Button)
        .min(state.starting_stack(Player::BigBlind));
    let big_blinds = effective_stack as f64 / state.config().big_blind as f64;

    if big_blinds <= 15.0 {
        EffectiveStackBucket::UpTo15Bb
    } else if big_blinds <= 25.0 {
        EffectiveStackBucket::Bb16To25
    } else if big_blinds <= 40.0 {
        EffectiveStackBucket::Bb26To40
    } else if big_blinds <= 75.0 {
        EffectiveStackBucket::Bb41To75
    } else {
        EffectiveStackBucket::Bb76Plus
    }
}

fn classify_made_hand(hole_cards: HoleCards, board: &[Card]) -> MadeHandBucket {
    let rank = current_best_rank(hole_cards, board);
    match rank.category() {
        HandCategory::StraightFlush
        | HandCategory::FourOfAKind
        | HandCategory::FullHouse
        | HandCategory::Flush => MadeHandBucket::Monster,
        HandCategory::Straight | HandCategory::ThreeOfAKind | HandCategory::TwoPair => {
            MadeHandBucket::Strong
        }
        HandCategory::OnePair => classify_one_pair_bucket(hole_cards, board, rank),
        HandCategory::HighCard => MadeHandBucket::Air,
    }
}

fn classify_one_pair_bucket(
    hole_cards: HoleCards,
    board: &[Card],
    rank: HandRank,
) -> MadeHandBucket {
    let board_high = board.iter().map(|card| card.rank()).max().unwrap_or(Rank::Two);
    let [left, right] = hole_cards.cards();
    let pair_rank = rank.tiebreakers()[0];

    if left.rank() == right.rank() && left.rank() > board_high {
        return MadeHandBucket::Medium;
    }
    if left.rank() == board_high || right.rank() == board_high {
        return MadeHandBucket::Medium;
    }
    if pair_rank >= Rank::Ten {
        return MadeHandBucket::Medium;
    }
    MadeHandBucket::Weak
}

fn classify_draw_bucket(hole_cards: HoleCards, board: &[Card]) -> DrawBucket {
    if board.len() >= 5 {
        return DrawBucket::None;
    }

    let rank = current_best_rank(hole_cards, board);
    let flush_draw = has_flush_draw(hole_cards, board) && rank.category() < HandCategory::Flush;
    let straight_draw =
        has_straight_draw(hole_cards, board) && rank.category() < HandCategory::Straight;

    match (straight_draw, flush_draw) {
        (true, true) => DrawBucket::Combo,
        (true, false) => DrawBucket::Straight,
        (false, true) => DrawBucket::Flush,
        (false, false) => DrawBucket::None,
    }
}

fn has_flush_draw(hole_cards: HoleCards, board: &[Card]) -> bool {
    let mut counts = [0u8; 4];
    for card in hole_cards.cards().into_iter().chain(board.iter().copied()) {
        counts[card.suit().index()] += 1;
    }
    counts.into_iter().any(|count| count >= 4)
}

fn has_straight_draw(hole_cards: HoleCards, board: &[Card]) -> bool {
    let mut present = [false; 15];
    for card in hole_cards.cards().into_iter().chain(board.iter().copied()) {
        let value = rank_value(card.rank());
        present[value as usize] = true;
        if value == 14 {
            present[1] = true;
        }
    }

    (1..=10).any(|start| {
        let mut count = 0;
        for value in start..=start + 4 {
            if present[value as usize] {
                count += 1;
            }
        }
        count >= 4
    })
}

fn rank_value(rank: Rank) -> u8 {
    match rank {
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Ace => 14,
    }
}
fn current_best_rank(hole_cards: HoleCards, board: &[Card]) -> HandRank {
    match board.len() {
        3 => evaluate_five([
            hole_cards.first(),
            hole_cards.second(),
            board[0],
            board[1],
            board[2],
        ])
        .expect("board and hole cards should be unique"),
        4 => {
            let cards = [
                hole_cards.first(),
                hole_cards.second(),
                board[0],
                board[1],
                board[2],
                board[3],
            ];
            let mut best = None;
            for omitted in 0..cards.len() {
                let mut five = [cards[0]; 5];
                let mut index = 0usize;
                for (card_index, card) in cards.iter().enumerate() {
                    if card_index == omitted {
                        continue;
                    }
                    five[index] = *card;
                    index += 1;
                }
                let rank = evaluate_five(five).expect("board and hole cards should be unique");
                if best.map_or(true, |current| rank > current) {
                    best = Some(rank);
                }
            }
            best.expect("six cards should yield a best five-card hand")
        }
        5 => evaluate_seven([
            hole_cards.first(),
            hole_cards.second(),
            board[0],
            board[1],
            board[2],
            board[3],
            board[4],
        ])
        .expect("board and hole cards should be unique"),
        actual => panic!("unsupported board length for current_best_rank: {actual}"),
    }
}

fn choose_policy_action(
    policy: &[BlueprintActionProbability],
    legal: &[AbstractAction],
) -> Option<AbstractAction> {
    let mut ordered = policy.to_vec();
    ordered.sort_by(|left, right| right.probability.total_cmp(&left.probability));
    for candidate in ordered {
        if let Some(action) = resolve_action_kind(candidate.action, legal) {
            return Some(action);
        }
    }
    safe_fallback_action(legal)
}

fn resolve_action_kind(
    kind: BlueprintActionKind,
    legal: &[AbstractAction],
) -> Option<AbstractAction> {
    let aggressive = legal
        .iter()
        .copied()
        .filter(|action| matches!(action, AbstractAction::BetTo(_) | AbstractAction::RaiseTo(_)))
        .collect::<Vec<_>>();

    match kind {
        BlueprintActionKind::Fold => legal.iter().copied().find(|action| *action == AbstractAction::Fold),
        BlueprintActionKind::Check => legal.iter().copied().find(|action| *action == AbstractAction::Check),
        BlueprintActionKind::Call => legal.iter().copied().find(|action| *action == AbstractAction::Call),
        BlueprintActionKind::Aggression1 => aggressive.first().copied(),
        BlueprintActionKind::Aggression2 => aggressive
            .get(1)
            .copied()
            .or_else(|| aggressive.last().copied()),
        BlueprintActionKind::Aggression3 => aggressive
            .get(2)
            .copied()
            .or_else(|| aggressive.last().copied()),
        BlueprintActionKind::AllIn => legal
            .iter()
            .copied()
            .find(|action| matches!(action, AbstractAction::AllIn(_))),
    }
}

fn safe_fallback_action(legal: &[AbstractAction]) -> Option<AbstractAction> {
    resolve_action_kind(BlueprintActionKind::Check, legal)
        .or_else(|| resolve_action_kind(BlueprintActionKind::Call, legal))
        .or_else(|| resolve_action_kind(BlueprintActionKind::Fold, legal))
        .or_else(|| resolve_action_kind(BlueprintActionKind::Aggression1, legal))
        .or_else(|| resolve_action_kind(BlueprintActionKind::AllIn, legal))
}

fn actions_for_street(state: &HoldemHandState, street: Street) -> Vec<PlayerAction> {
    state
        .history()
        .iter()
        .filter_map(|event| match event {
            gto_core::HistoryEvent::ActionApplied {
                street: event_street,
                action,
                ..
            } if *event_street == street => Some(*action),
            _ => None,
        })
        .collect()
}

fn is_aggressive_action(action: PlayerAction) -> bool {
    matches!(
        action,
        PlayerAction::BetTo(_) | PlayerAction::RaiseTo(_) | PlayerAction::AllIn
    )
}

#[cfg(test)]
mod tests {
    use gto_core::{
        Deck, HandPhase, HoldemConfig, HoldemHandState, HoleCards, Player, PlayerAction, Street,
        default_rng,
    };

    use super::{
        BlueprintActionKind, BlueprintActionProbability, BlueprintBot, DrawBucket,
        EffectiveStackBucket, FullHandBlueprintArtifact, MadeHandBucket, PreflopContextKey,
        StartingRanges, choose_policy_action, classify_draw_bucket, classify_made_hand,
        postflop_policy_key, preflop_context_from_state, resolve_action_kind,
        smoke_blueprint_profile,
    };
    use crate::abstract_actions;

    #[test]
    fn blueprint_artifact_json_round_trips() {
        let artifact = FullHandBlueprintArtifact::smoke_default();
        let encoded = artifact.to_json_string().unwrap();
        let decoded = FullHandBlueprintArtifact::from_json_str(&encoded).unwrap();

        assert_eq!(decoded, artifact);
    }

    #[test]
    fn blueprint_artifact_rejects_unknown_format_versions() {
        let mut artifact = FullHandBlueprintArtifact::smoke_default();
        artifact.format_version += 1;
        let encoded = serde_json::to_string(&artifact).unwrap();

        let error = FullHandBlueprintArtifact::from_json_str(&encoded).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "unsupported blueprint artifact format version {}; expected {}",
                FullHandBlueprintArtifact::FORMAT_VERSION + 1,
                FullHandBlueprintArtifact::FORMAT_VERSION
            )
        );
    }

    #[test]
    fn limp_context_is_detected_for_the_big_blind() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();

        let context = preflop_context_from_state(&state).unwrap();
        assert_eq!(
            context,
            PreflopContextKey {
                actor: Player::BigBlind,
                prior_limp: true,
                aggressive_actions: 0,
                effective_stack_bucket: EffectiveStackBucket::Bb76Plus,
            }
        );
    }

    #[test]
    fn blueprint_profile_exposes_raise_options_after_a_limp() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();

        let actions = abstract_actions(&state, &smoke_blueprint_profile()).unwrap();
        assert!(actions.contains(&crate::AbstractAction::Check));
        assert!(actions.iter().any(|action| matches!(action, crate::AbstractAction::RaiseTo(_))));
    }

    #[test]
    fn preflop_context_rejects_postflop_states() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();

        let error = preflop_context_from_state(&state).unwrap_err();
        assert!(matches!(
            error,
            super::BlueprintBotError::UnsupportedStreetState(Street::Flop)
        ));
    }

    #[test]
    fn preflop_context_caps_aggression_to_the_last_bucket() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::RaiseTo(200)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(300)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(400)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(500)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(600)).unwrap();

        let context = preflop_context_from_state(&state).unwrap();
        assert_eq!(context.actor, Player::BigBlind);
        assert_eq!(context.aggressive_actions, 4);
    }

    #[test]
    fn made_hand_and_draw_buckets_cover_common_cases() {
        let hole_cards = "AhQh".parse().unwrap();
        let flop = ["2h".parse().unwrap(), "7h".parse().unwrap(), "Kd".parse().unwrap()];
        assert_eq!(classify_draw_bucket(hole_cards, &flop), DrawBucket::Flush);

        let monster = classify_made_hand(
            "KhKc".parse().unwrap(),
            &["Kd".parse().unwrap(), "7h".parse().unwrap(), "7d".parse().unwrap()],
        );
        assert_eq!(monster, MadeHandBucket::Monster);
    }

    #[test]
    fn postflop_policy_key_rejects_preflop_states() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();

        let error = postflop_policy_key(Player::Button, &state).unwrap_err();
        assert!(matches!(
            error,
            super::BlueprintBotError::UnsupportedStreetState(Street::Preflop)
        ));
    }

    #[test]
    fn postflop_policy_key_caps_aggression_to_the_last_bucket() {
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(200)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(300)).unwrap();
        state.apply_action(PlayerAction::RaiseTo(400)).unwrap();

        let key = postflop_policy_key(Player::BigBlind, &state).unwrap();
        assert_eq!(key.aggressive_actions, 3);
        assert!(BlueprintBot::default().artifact().postflop_policy(key).is_some());
    }

    #[test]
    fn effective_stack_buckets_cover_multiple_depth_bands() {
        let cases = [
            (1_500, EffectiveStackBucket::UpTo15Bb),
            (2_500, EffectiveStackBucket::Bb16To25),
            (4_000, EffectiveStackBucket::Bb26To40),
            (7_500, EffectiveStackBucket::Bb41To75),
            (10_000, EffectiveStackBucket::Bb76Plus),
        ];

        for (stack, bucket) in cases {
            let state = HoldemHandState::new_with_starting_stacks(
                HoldemConfig::default(),
                "AsKd".parse().unwrap(),
                "QcJh".parse().unwrap(),
                stack,
                stack,
            )
            .unwrap();
            let context = preflop_context_from_state(&state).unwrap();
            assert_eq!(context.effective_stack_bucket, bucket);
        }
    }

    #[test]
    fn effective_stack_bucket_uses_the_shorter_uneven_stack() {
        let state = HoldemHandState::new_with_starting_stacks(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
            7_500,
            2_500,
        )
        .unwrap();

        let context = preflop_context_from_state(&state).unwrap();
        assert_eq!(context.effective_stack_bucket, EffectiveStackBucket::Bb16To25);
    }

    #[test]
    fn stack_aware_artifact_rejects_older_fixed_stack_payloads() {
        let legacy_json = serde_json::json!({
            "format_version": 1,
            "profile": smoke_blueprint_profile(),
            "starting_ranges": StartingRanges::smoke_default(),
            "preflop_policies": [],
            "postflop_policies": [],
        })
        .to_string();

        let error = FullHandBlueprintArtifact::from_json_str(&legacy_json).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unsupported blueprint artifact format version 1; expected 2"
        );
    }

    #[test]
    fn blueprint_bot_chooses_legal_preflop_and_postflop_actions() {
        let bot = BlueprintBot::default();
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKs".parse().unwrap(),
            "QhJh".parse().unwrap(),
        )
        .unwrap();

        let preflop_action = bot.choose_action(Player::Button, &state).unwrap();
        state.apply_action(preflop_action).unwrap();
        while let HandPhase::BettingRound { actor, .. } = state.phase() {
            let action = bot.choose_action(actor, &state).unwrap();
            state.apply_action(action).unwrap();
            if !matches!(state.phase(), HandPhase::BettingRound { .. }) {
                break;
            }
        }

        if let HandPhase::AwaitingBoard { .. } = state.phase() {
            state
                .deal_flop(["2c".parse().unwrap(), "7d".parse().unwrap(), "Th".parse().unwrap()])
                .unwrap();
        }
        let postflop_action = bot.choose_action(state.current_actor().unwrap(), &state).unwrap();
        state.apply_action(postflop_action).unwrap();
    }

    #[test]
    fn blueprint_bot_rejects_out_of_turn_queries() {
        let bot = BlueprintBot::default();
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();

        let error = bot.choose_action(Player::BigBlind, &state).unwrap_err();
        assert!(matches!(
            error,
            super::BlueprintBotError::NotActorsTurn {
                expected: Some(Player::Button),
                actual: Player::BigBlind,
            }
        ));
    }

    #[test]
    fn blueprint_bot_can_self_play_many_complete_hands_without_invalid_actions() {
        let bot = BlueprintBot::default();
        let mut rng = default_rng();

        for _ in 0..64 {
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
            let mut state = HoldemHandState::new(HoldemConfig::default(), button, big_blind).unwrap();

            loop {
                match state.phase() {
                    HandPhase::BettingRound { actor, .. } => {
                        let action = bot.choose_action(actor, &state).unwrap();
                        state.apply_action(action).unwrap();
                    }
                    HandPhase::AwaitingBoard { next_street } => match next_street {
                        Street::Flop => state.deal_flop([board[0], board[1], board[2]]).unwrap(),
                        Street::Turn => state.deal_turn(board[3]).unwrap(),
                        Street::River => state.deal_river(board[4]).unwrap(),
                        Street::Preflop => panic!("cannot await preflop cards"),
                    },
                    HandPhase::Terminal { .. } => break,
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn blueprint_bot_self_play_soak_remains_stable() {
        let bot = BlueprintBot::default();
        let mut rng = default_rng();

        for _ in 0..1_024 {
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
            let mut state = HoldemHandState::new(HoldemConfig::default(), button, big_blind).unwrap();

            loop {
                match state.phase() {
                    HandPhase::BettingRound { actor, .. } => {
                        let action = bot.choose_action(actor, &state).unwrap();
                        state.apply_action(action).unwrap();
                    }
                    HandPhase::AwaitingBoard { next_street } => match next_street {
                        Street::Flop => state.deal_flop([board[0], board[1], board[2]]).unwrap(),
                        Street::Turn => state.deal_turn(board[3]).unwrap(),
                        Street::River => state.deal_river(board[4]).unwrap(),
                        Street::Preflop => panic!("cannot await preflop cards"),
                    },
                    HandPhase::Terminal { .. } => break,
                }
            }
        }
    }

    #[test]
    fn postflop_key_detects_facing_bet_and_policy_coverage() {
        let bot = BlueprintBot::default();
        let mut state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::BetTo(100)).unwrap();

        let key = postflop_policy_key(Player::Button, &state).unwrap();
        assert!(key.facing_bet);
        assert!(bot.artifact().postflop_policy(key).is_some());
    }

    #[test]
    fn choose_policy_action_uses_next_legal_probability_then_safe_fallback() {
        let legal = vec![crate::AbstractAction::Check, crate::AbstractAction::Call];
        let policy = vec![
            BlueprintActionProbability {
                action: BlueprintActionKind::AllIn,
                probability: 0.9,
            },
            BlueprintActionProbability {
                action: BlueprintActionKind::Call,
                probability: 0.1,
            },
        ];
        assert_eq!(choose_policy_action(&policy, &legal), Some(crate::AbstractAction::Call));

        let unreachable_policy = vec![BlueprintActionProbability {
            action: BlueprintActionKind::Aggression3,
            probability: 1.0,
        }];
        assert_eq!(
            choose_policy_action(&unreachable_policy, &[crate::AbstractAction::Check]),
            Some(crate::AbstractAction::Check)
        );
    }

    #[test]
    fn resolve_action_kind_uses_last_available_aggression_when_needed() {
        let legal = vec![crate::AbstractAction::RaiseTo(300)];
        assert_eq!(
            resolve_action_kind(BlueprintActionKind::Aggression2, &legal),
            Some(crate::AbstractAction::RaiseTo(300))
        );
        assert_eq!(
            resolve_action_kind(BlueprintActionKind::Aggression3, &legal),
            Some(crate::AbstractAction::RaiseTo(300))
        );
    }

    #[test]
    fn blueprint_bot_indexes_match_artifact_policy_scans() {
        let bot = BlueprintBot::default();

        for entry in &bot.artifact().preflop_policies {
            assert_eq!(bot.preflop_policy(entry.context), Some(entry));
        }

        for entry in &bot.artifact().postflop_policies {
            assert_eq!(bot.postflop_policy(entry.key), Some(entry));
        }
    }

    #[test]
    fn all_postflop_policy_rows_are_normalized() {
        let artifact = FullHandBlueprintArtifact::smoke_default();
        for entry in artifact.postflop_policies {
            let sum = entry.actions.iter().map(|action| action.probability).sum::<f64>();
            assert!((sum - 1.0).abs() < 1e-9);
            assert!(entry.actions.iter().all(|action| action.probability.is_finite()));
        }
    }

    #[test]
    fn default_preflop_policy_can_open_raise_premium_hands() {
        let bot = BlueprintBot::default();
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsAh".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let action = bot.choose_action(Player::Button, &state).unwrap();
        assert!(matches!(action, PlayerAction::RaiseTo(_) | PlayerAction::AllIn));
    }
}
