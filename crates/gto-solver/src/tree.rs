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
    use gto_core::{HoldemConfig, HoldemHandState, HandPhase};

    use crate::abstraction::{AbstractionProfile, OpeningSize, RaiseSize, StreetProfile};

    use super::{PublicTreeNodeKind, build_public_tree};

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
}
