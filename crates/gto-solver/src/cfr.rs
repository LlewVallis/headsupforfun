use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone, PartialEq)]
pub enum GameNode<A, I, S> {
    Terminal { utilities: [f64; 2] },
    Chance { outcomes: Vec<(S, f64)> },
    Decision {
        player: usize,
        infoset: I,
        actions: Vec<A>,
    },
}

pub trait ExtensiveGameState: Clone {
    type Action: Clone + Eq;
    type InfoSet: Clone + Eq + Hash;

    fn node(&self) -> GameNode<Self::Action, Self::InfoSet, Self>;
    fn next_state(&self, action: &Self::Action) -> Self;
}

#[derive(Debug, Clone)]
pub struct CfrPlusSolver<G: ExtensiveGameState> {
    root: G,
    entries: HashMap<G::InfoSet, InfoSetEntry<G::Action>>,
    iterations: u64,
}

impl<G: ExtensiveGameState> CfrPlusSolver<G> {
    pub fn new(root: G) -> Self {
        Self {
            root,
            entries: HashMap::new(),
            iterations: 0,
        }
    }

    pub const fn iterations(&self) -> u64 {
        self.iterations
    }

    pub fn infoset_count(&self) -> usize {
        self.entries.len()
    }

    pub fn train_iterations(&mut self, iterations: u64) {
        for _ in 0..iterations {
            self.cfr(self.root.clone(), [1.0, 1.0]);
            self.iterations += 1;
        }
    }

    pub fn average_strategy(
        &self,
        infoset: &G::InfoSet,
    ) -> Option<Vec<(G::Action, f64)>> {
        self.entries.get(infoset).map(|entry| {
            let strategy = normalized(&entry.strategy_sum);
            entry
                .actions
                .iter()
                .cloned()
                .zip(strategy)
                .collect::<Vec<_>>()
        })
    }

    pub fn average_strategy_snapshot(&self) -> HashMap<G::InfoSet, Vec<(G::Action, f64)>> {
        self.entries
            .iter()
            .map(|(infoset, entry)| {
                let strategy = normalized(&entry.strategy_sum);
                (
                    infoset.clone(),
                    entry
                        .actions
                        .iter()
                        .cloned()
                        .zip(strategy)
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    pub fn expected_value(&self) -> [f64; 2] {
        self.expected_value_from(self.root.clone())
    }

    fn cfr(&mut self, state: G, reach: [f64; 2]) -> [f64; 2] {
        match state.node() {
            GameNode::Terminal { utilities } => utilities,
            GameNode::Chance { outcomes } => outcomes.into_iter().fold([0.0, 0.0], |acc, (next, probability)| {
                let utility = self.cfr(next, reach);
                [
                    acc[0] + probability * utility[0],
                    acc[1] + probability * utility[1],
                ]
            }),
            GameNode::Decision {
                player,
                infoset,
                actions,
            } => {
                self.ensure_entry(&infoset, &actions);
                let strategy = self.current_strategy(&infoset);
                let mut node_utility = [0.0, 0.0];
                let mut action_utilities = Vec::with_capacity(actions.len());

                for (index, action) in actions.iter().enumerate() {
                    let mut next_reach = reach;
                    next_reach[player] *= strategy[index];
                    let utility = self.cfr(state.next_state(action), next_reach);
                    action_utilities.push(utility);
                    node_utility[0] += strategy[index] * utility[0];
                    node_utility[1] += strategy[index] * utility[1];
                }

                let opponent = 1 - player;
                let entry = self
                    .entries
                    .get_mut(&infoset)
                    .expect("infoset entry should exist after initialization");
                for (index, action_utility) in action_utilities.iter().enumerate() {
                    let regret = action_utility[player] - node_utility[player];
                    entry.cumulative_regrets[index] =
                        (entry.cumulative_regrets[index] + reach[opponent] * regret).max(0.0);
                    entry.strategy_sum[index] += reach[player] * strategy[index];
                }

                node_utility
            }
        }
    }

    fn expected_value_from(&self, state: G) -> [f64; 2] {
        match state.node() {
            GameNode::Terminal { utilities } => utilities,
            GameNode::Chance { outcomes } => outcomes.into_iter().fold([0.0, 0.0], |acc, (next, probability)| {
                let utility = self.expected_value_from(next);
                [
                    acc[0] + probability * utility[0],
                    acc[1] + probability * utility[1],
                ]
            }),
            GameNode::Decision {
                infoset, actions, ..
            } => {
                let strategy = self
                    .entries
                    .get(&infoset)
                    .map(|entry| normalized(&entry.strategy_sum))
                    .unwrap_or_else(|| uniform(actions.len()));

                actions
                    .iter()
                    .enumerate()
                    .fold([0.0, 0.0], |acc, (index, action)| {
                        let utility = self.expected_value_from(state.next_state(action));
                        [
                            acc[0] + strategy[index] * utility[0],
                            acc[1] + strategy[index] * utility[1],
                        ]
                    })
            }
        }
    }

    fn ensure_entry(&mut self, infoset: &G::InfoSet, actions: &[G::Action]) {
        self.entries
            .entry(infoset.clone())
            .and_modify(|entry| assert!(entry.actions == actions))
            .or_insert_with(|| InfoSetEntry::new(actions.to_vec()));
    }

    fn current_strategy(&self, infoset: &G::InfoSet) -> Vec<f64> {
        let entry = self
            .entries
            .get(infoset)
            .expect("infoset entry should exist before strategy lookup");
        let positive_regrets = entry
            .cumulative_regrets
            .iter()
            .map(|regret| regret.max(0.0))
            .collect::<Vec<_>>();
        normalized(&positive_regrets)
    }
}

#[derive(Debug, Clone)]
struct InfoSetEntry<A> {
    actions: Vec<A>,
    cumulative_regrets: Vec<f64>,
    strategy_sum: Vec<f64>,
}

impl<A> InfoSetEntry<A> {
    fn new(actions: Vec<A>) -> Self {
        let action_count = actions.len();
        Self {
            actions,
            cumulative_regrets: vec![0.0; action_count],
            strategy_sum: vec![0.0; action_count],
        }
    }
}

fn normalized(weights: &[f64]) -> Vec<f64> {
    let sum = weights.iter().sum::<f64>();
    if sum <= 0.0 {
        return uniform(weights.len());
    }

    weights.iter().map(|weight| weight / sum).collect()
}

fn uniform(count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }

    vec![1.0 / count as f64; count]
}

#[cfg(test)]
mod tests {
    use super::{CfrPlusSolver, ExtensiveGameState, GameNode};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum ToyAction {
        Left,
        Right,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct ToyInfoSet;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct ToyState;

    impl ExtensiveGameState for ToyState {
        type Action = ToyAction;
        type InfoSet = ToyInfoSet;

        fn node(&self) -> GameNode<Self::Action, Self::InfoSet, Self> {
            GameNode::Decision {
                player: 0,
                infoset: ToyInfoSet,
                actions: vec![ToyAction::Left, ToyAction::Right],
            }
        }

        fn next_state(&self, action: &Self::Action) -> Self {
            match action {
                ToyAction::Left | ToyAction::Right => *self,
            }
        }
    }

    #[test]
    fn solver_can_be_constructed() {
        let solver = CfrPlusSolver::new(ToyState);

        assert_eq!(solver.iterations(), 0);
        assert_eq!(solver.infoset_count(), 0);
    }
}
