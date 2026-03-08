use gto_core::{HandOutcome, HandPhase, HoldemHandState, HoldemStateError, Street};

use crate::abstraction::{AbstractAction, AbstractionProfile, PublicStateKey, abstract_actions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTree {
    pub root: usize,
    pub nodes: Vec<PublicTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTreeNode {
    pub state: PublicStateKey,
    pub kind: PublicTreeNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicTreeNodeKind {
    Decision { actions: Vec<PublicTreeEdge> },
    AwaitingBoard { next_street: Street },
    Terminal { outcome: HandOutcome },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTreeEdge {
    pub action: AbstractAction,
    pub child: usize,
}

pub fn build_public_tree(
    initial_state: &HoldemHandState,
    profile: &AbstractionProfile,
) -> Result<PublicTree, HoldemStateError> {
    let mut nodes = Vec::new();
    let root = build_node(initial_state.clone(), profile, &mut nodes)?;
    Ok(PublicTree { root, nodes })
}

fn build_node(
    state: HoldemHandState,
    profile: &AbstractionProfile,
    nodes: &mut Vec<PublicTreeNode>,
) -> Result<usize, HoldemStateError> {
    let node_index = nodes.len();
    let state_key = PublicStateKey::from_state(&state);
    nodes.push(PublicTreeNode {
        state: state_key,
        kind: PublicTreeNodeKind::AwaitingBoard {
            next_street: state.street(),
        },
    });

    let kind = match state.phase() {
        HandPhase::Terminal { outcome } => PublicTreeNodeKind::Terminal { outcome },
        HandPhase::AwaitingBoard { next_street } => PublicTreeNodeKind::AwaitingBoard { next_street },
        HandPhase::BettingRound { .. } => {
            let actions = abstract_actions(&state, profile)?
                .into_iter()
                .map(|action| {
                    let mut next_state = state.clone();
                    next_state.apply_action(action.to_player_action())?;
                    let child = build_node(next_state, profile, nodes)?;
                    Ok(PublicTreeEdge { action, child })
                })
                .collect::<Result<Vec<_>, HoldemStateError>>()?;
            PublicTreeNodeKind::Decision { actions }
        }
    };

    nodes[node_index].kind = kind;
    Ok(node_index)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use gto_core::{HandPhase, HoldemConfig, HoldemHandState, PlayerAction, Street};

    use crate::abstraction::{AbstractionProfile, OpeningSize, RaiseSize, StreetProfile};

    use super::{PublicTree, PublicTreeNodeKind, build_public_tree};

    fn sample_profile() -> AbstractionProfile {
        let preflop = StreetProfile {
            opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
            include_all_in: false,
        };
        let postflop = StreetProfile {
            opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
            raise_sizes: vec![RaiseSize::PotFractionAfterCallBps(10_000)],
            include_all_in: false,
        };
        AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop)
    }

    fn larger_profile() -> AbstractionProfile {
        let preflop = StreetProfile {
            opening_sizes: vec![
                OpeningSize::BigBlindMultipleBps(25_000),
                OpeningSize::BigBlindMultipleBps(40_000),
                OpeningSize::BigBlindMultipleBps(70_000),
            ],
            raise_sizes: vec![
                RaiseSize::CurrentBetMultipleBps(25_000),
                RaiseSize::PotFractionAfterCallBps(10_000),
            ],
            include_all_in: true,
        };
        let postflop = StreetProfile {
            opening_sizes: vec![
                OpeningSize::PotFractionBps(3_300),
                OpeningSize::PotFractionBps(6_600),
                OpeningSize::PotFractionBps(10_000),
            ],
            raise_sizes: vec![
                RaiseSize::CurrentBetMultipleBps(25_000),
                RaiseSize::PotFractionAfterCallBps(10_000),
            ],
            include_all_in: true,
        };
        AbstractionProfile::new(preflop, postflop.clone(), postflop.clone(), postflop)
    }

    #[test]
    fn public_tree_is_deterministic() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let profile = sample_profile();

        let left = build_public_tree(&state, &profile).unwrap();
        let right = build_public_tree(&state, &profile).unwrap();

        assert_eq!(left, right);
    }

    #[test]
    fn public_tree_stops_at_board_chance_boundaries() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &sample_profile()).unwrap();

        let root = &tree.nodes[tree.root];
        let PublicTreeNodeKind::Decision { actions } = &root.kind else {
            panic!("expected a decision root");
        };
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|edge| matches!(
            tree.nodes[edge.child].kind,
            PublicTreeNodeKind::AwaitingBoard { .. } | PublicTreeNodeKind::Terminal { .. }
        )));
    }

    #[test]
    fn every_tree_edge_corresponds_to_a_legal_exact_action() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &sample_profile()).unwrap();

        fn visit(
            state: HoldemHandState,
            node_index: usize,
            tree: &super::PublicTree,
        ) {
            match &tree.nodes[node_index].kind {
                PublicTreeNodeKind::Decision { actions } => {
                    assert!(matches!(state.phase(), HandPhase::BettingRound { .. }));
                    for edge in actions {
                        let mut next_state = state.clone();
                        next_state
                            .apply_action(edge.action.to_player_action())
                            .expect("tree edge should be legal");
                        visit(next_state, edge.child, tree);
                    }
                }
                PublicTreeNodeKind::AwaitingBoard { .. } => {
                    assert!(matches!(state.phase(), HandPhase::AwaitingBoard { .. }));
                }
                PublicTreeNodeKind::Terminal { .. } => {
                    assert!(matches!(state.phase(), HandPhase::Terminal { .. }));
                }
            }
        }

        visit(state, tree.root, &tree);
    }

    #[test]
    fn every_decision_node_has_unique_actions_and_valid_children() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &sample_profile()).unwrap();

        for node in &tree.nodes {
            if let PublicTreeNodeKind::Decision { actions } = &node.kind {
                assert!(!actions.is_empty());
                let unique = actions.iter().map(|edge| edge.action).collect::<HashSet<_>>();
                assert_eq!(unique.len(), actions.len());
                assert!(actions.iter().all(|edge| edge.child < tree.nodes.len()));
            }
        }
    }

    #[test]
    fn replayed_public_states_match_tree_node_keys() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &sample_profile()).unwrap();

        fn visit(state: HoldemHandState, node_index: usize, tree: &PublicTree) {
            assert_eq!(tree.nodes[node_index].state, crate::PublicStateKey::from_state(&state));

            if let PublicTreeNodeKind::Decision { actions } = &tree.nodes[node_index].kind {
                for edge in actions {
                    let mut next = state.clone();
                    next.apply_action(edge.action.to_player_action()).unwrap();
                    visit(next, edge.child, tree);
                }
            }
        }

        visit(state, tree.root, &tree);
    }

    #[test]
    fn tree_edges_always_change_the_public_state() {
        let state = HoldemHandState::new(
            HoldemConfig::default(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &sample_profile()).unwrap();

        for node in &tree.nodes {
            if let PublicTreeNodeKind::Decision { actions } = &node.kind {
                for edge in actions {
                    assert_ne!(node.state, tree.nodes[edge.child].state);
                }
            }
        }
    }

    #[test]
    fn larger_preflop_abstraction_tree_builds_and_is_well_formed() {
        let state = HoldemHandState::new(
            HoldemConfig::new(600, 50, 100).unwrap(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        let tree = build_public_tree(&state, &larger_profile()).unwrap();

        assert!(tree.nodes.len() > 10);
        assert!(matches!(
            tree.nodes[tree.root].kind,
            PublicTreeNodeKind::Decision { .. }
        ));
        assert!(tree.nodes.iter().any(|node| matches!(
            node.kind,
            PublicTreeNodeKind::Terminal { .. }
        )));
        assert!(tree.nodes.iter().any(|node| matches!(
            node.kind,
            PublicTreeNodeKind::AwaitingBoard {
                next_street: Street::Flop
            }
        )));
    }

    #[test]
    fn larger_flop_abstraction_tree_builds_and_stays_legal() {
        let mut state = HoldemHandState::new(
            HoldemConfig::new(600, 50, 100).unwrap(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();

        let tree = build_public_tree(&state, &larger_profile()).unwrap();

        fn visit(state: HoldemHandState, node_index: usize, tree: &PublicTree) {
            match &tree.nodes[node_index].kind {
                PublicTreeNodeKind::Decision { actions } => {
                    assert!(!actions.is_empty());
                    for edge in actions {
                        let mut next_state = state.clone();
                        next_state.apply_action(edge.action.to_player_action()).unwrap();
                        visit(next_state, edge.child, tree);
                    }
                }
                PublicTreeNodeKind::AwaitingBoard {
                    next_street: Street::Turn,
                }
                | PublicTreeNodeKind::Terminal { .. } => {}
                PublicTreeNodeKind::AwaitingBoard { next_street } => {
                    panic!("unexpected street boundary {next_street:?}");
                }
            }
        }

        visit(state, tree.root, &tree);
    }

    #[test]
    #[ignore]
    fn larger_turn_abstraction_tree_stress_builds_without_invalid_edges() {
        let mut state = HoldemHandState::new(
            HoldemConfig::new(600, 50, 100).unwrap(),
            "AsKd".parse().unwrap(),
            "QcJh".parse().unwrap(),
        )
        .unwrap();
        state.apply_action(PlayerAction::Call).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state
            .deal_flop(["2c".parse().unwrap(), "3d".parse().unwrap(), "4h".parse().unwrap()])
            .unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.apply_action(PlayerAction::Check).unwrap();
        state.deal_turn("5s".parse().unwrap()).unwrap();

        let tree = build_public_tree(&state, &larger_profile()).unwrap();
        assert!(tree.nodes.len() > 20);
        assert!(tree.nodes.iter().all(|node| match &node.kind {
            PublicTreeNodeKind::Decision { actions } => !actions.is_empty(),
            _ => true,
        }));
    }
}
