#![forbid(unsafe_code)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use gto_core::{Board, HoleCards, evaluate_five, evaluate_seven, resolve_holdem_showdown};

fn benchmark_hand_evaluation(criterion: &mut Criterion) {
    let five_card_samples = [
        ["Ah", "Kh", "Qh", "Jh", "Th"],
        ["As", "Ad", "Ac", "7h", "7d"],
        ["9c", "8d", "7s", "6h", "5c"],
        ["Kc", "Kd", "9s", "4h", "2d"],
    ]
    .map(parse_five_cards);

    let seven_card_samples = [
        ["Ah", "Kh", "Qh", "Jh", "Th", "2c", "3d"],
        ["As", "Ad", "Ac", "7h", "7d", "2c", "2h"],
        ["9c", "8d", "7s", "6h", "5c", "2d", "Ah"],
        ["Kc", "Kd", "9s", "4h", "2d", "Jc", "7s"],
    ]
    .map(parse_seven_cards);

    criterion.bench_function("evaluate_five_smoke", |bencher| {
        let mut index = 0usize;
        bencher.iter(|| {
            let cards = five_card_samples[index % five_card_samples.len()];
            index += 1;
            black_box(evaluate_five(black_box(cards)).unwrap())
        });
    });

    criterion.bench_function("evaluate_seven_smoke", |bencher| {
        let mut index = 0usize;
        bencher.iter(|| {
            let cards = seven_card_samples[index % seven_card_samples.len()];
            index += 1;
            black_box(evaluate_seven(black_box(cards)).unwrap())
        });
    });
}

fn benchmark_showdown_resolution(criterion: &mut Criterion) {
    let board = Board::try_from_cards(["Kh", "7d", "2c", "2h", "5d"].map(parse_card)).unwrap();
    let button = parse_hole_cards("AhQh");
    let big_blind = parse_hole_cards("Kd7h");

    criterion.bench_function("resolve_holdem_showdown_smoke", |bencher| {
        bencher.iter(|| {
            black_box(resolve_holdem_showdown(
                black_box(&board),
                black_box(button),
                black_box(big_blind),
            )
            .unwrap())
        });
    });
}

fn parse_card(value: &str) -> gto_core::Card {
    value.parse().unwrap()
}

fn parse_five_cards(value: [&str; 5]) -> [gto_core::Card; 5] {
    value.map(parse_card)
}

fn parse_seven_cards(value: [&str; 7]) -> [gto_core::Card; 7] {
    value.map(parse_card)
}

fn parse_hole_cards(value: &str) -> HoleCards {
    value.parse().unwrap()
}

criterion_group!(hand_eval_smoke, benchmark_hand_evaluation, benchmark_showdown_resolution);
criterion_main!(hand_eval_smoke);
