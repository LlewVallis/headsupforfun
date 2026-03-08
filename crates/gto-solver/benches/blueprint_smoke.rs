#![forbid(unsafe_code)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use gto_core::{HoldemConfig, HoldemHandState, Player, PlayerAction};
use gto_solver::{BlueprintBot, FullHandBlueprintArtifact};

fn benchmark_blueprint_action_lookup(criterion: &mut Criterion) {
    let bot = BlueprintBot::default();
    let preflop_state = HoldemHandState::new(
        HoldemConfig::default(),
        "AsKs".parse().unwrap(),
        "QhJh".parse().unwrap(),
    )
    .unwrap();

    let mut facing_bet = HoldemHandState::new(
        HoldemConfig::default(),
        "AsKd".parse().unwrap(),
        "QcJh".parse().unwrap(),
    )
    .unwrap();
    facing_bet.apply_action(PlayerAction::Call).unwrap();
    facing_bet.apply_action(PlayerAction::Check).unwrap();
    facing_bet
        .deal_flop(["2c".parse().unwrap(), "7d".parse().unwrap(), "Th".parse().unwrap()])
        .unwrap();
    facing_bet.apply_action(PlayerAction::BetTo(200)).unwrap();

    criterion.bench_function("blueprint_choose_action_preflop_smoke", |bencher| {
        bencher.iter(|| black_box(bot.choose_action(Player::Button, black_box(&preflop_state)).unwrap()));
    });

    criterion.bench_function("blueprint_choose_action_postflop_smoke", |bencher| {
        bencher.iter(|| black_box(bot.choose_action(Player::Button, black_box(&facing_bet)).unwrap()));
    });
}

fn benchmark_blueprint_artifact_loading(criterion: &mut Criterion) {
    let encoded = FullHandBlueprintArtifact::smoke_default()
        .to_json_string()
        .unwrap();

    criterion.bench_function("blueprint_artifact_parse_smoke", |bencher| {
        bencher.iter(|| {
            black_box(FullHandBlueprintArtifact::from_json_str(black_box(encoded.as_str())).unwrap())
        });
    });
}

criterion_group!(
    blueprint_smoke,
    benchmark_blueprint_action_lookup,
    benchmark_blueprint_artifact_loading
);
criterion_main!(blueprint_smoke);
