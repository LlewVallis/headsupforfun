use std::io::{self, BufRead, Write};

use gto_core::{
    DEFAULT_RNG_SEED, Deck, DeterministicRng, HandOutcome, HandPhase, HoldemConfig,
    HoldemHandState, Player, PlayerAction, rng_from_seed,
};
use gto_solver::{SolverProfile, StubBot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliConfig {
    pub seed: u64,
    pub max_hands: Option<usize>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            seed: DEFAULT_RNG_SEED,
            max_hands: None,
        }
    }
}

pub fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    run_session(&mut input, &mut output, CliConfig::default())
}

pub fn startup_banner() -> String {
    let build = gto_solver::build_info();
    let profile = SolverProfile::placeholder();

    format!(
        "{name} {version}\nstatus: milestone M4 interactive CLI vertical slice\nsolver-profile: {profile}\nwasm-safe-core: {wasm_safe}",
        name = build.crate_name,
        version = build.crate_version,
        profile = profile.name(),
        wasm_safe = build.wasm_safe && build.core.wasm_safe,
    )
}

pub fn run_session<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    config: CliConfig,
) -> io::Result<()> {
    writeln!(output, "{}", startup_banner())?;

    let mut rng = rng_from_seed(config.seed);
    let bot = StubBot;
    let mut hand_number = 1usize;

    loop {
        let human_role = if hand_number % 2 == 1 {
            Player::Button
        } else {
            Player::BigBlind
        };
        let completed = play_single_hand(input, output, &bot, &mut rng, hand_number, human_role)?;
        if !completed {
            break;
        }

        if let Some(max_hands) = config.max_hands {
            if hand_number >= max_hands {
                break;
            }
        } else if !prompt_play_again(input, output)? {
            break;
        }

        hand_number += 1;
    }

    Ok(())
}

fn play_single_hand<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    bot: &StubBot,
    rng: &mut DeterministicRng,
    hand_number: usize,
    human_role: Player,
) -> io::Result<bool> {
    let deal = DealtHand::deal(rng)?;
    let mut state = HoldemHandState::new(HoldemConfig::default(), deal.button, deal.big_blind)
        .map_err(io::Error::other)?;
    let bot_role = human_role.opponent();

    writeln!(output, "\nHand {hand_number}")?;
    writeln!(output, "You are the {human_role}.")?;
    writeln!(output, "Your cards: {}", format_hole_cards(state.player(human_role).hole_cards))?;

    loop {
        match state.phase() {
            HandPhase::BettingRound { actor, .. } => {
                if actor == human_role {
                    if !handle_human_turn(input, output, &mut state, human_role, bot_role)? {
                        return Ok(false);
                    }
                } else {
                    let action = bot.choose_action(&state).map_err(io::Error::other)?;
                    writeln!(
                        output,
                        "Bot ({bot_role}) -> {}",
                        describe_action(action, &state)
                    )?;
                    state.apply_action(action).map_err(io::Error::other)?;
                }
            }
            HandPhase::AwaitingBoard { next_street } => {
                reveal_next_board(output, &mut state, &deal, next_street)?;
            }
            HandPhase::Terminal { outcome } => {
                render_outcome(output, &state, human_role, outcome)?;
                return Ok(true);
            }
        }
    }
}

fn handle_human_turn<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut HoldemHandState,
    human_role: Player,
    bot_role: Player,
) -> io::Result<bool> {
    loop {
        render_decision_prompt(output, state, human_role, bot_role)?;
        let Some(line) = read_line(input, output)? else {
            return Ok(false);
        };
        match parse_user_action(&line) {
            Ok(Some(action)) => match state.apply_action(action) {
                Ok(()) => return Ok(true),
                Err(error) => writeln!(output, "Invalid action: {error}")?,
            },
            Ok(None) => {
                writeln!(output, "Exiting.")?;
                return Ok(false);
            }
            Err(error) => writeln!(output, "Invalid action: {error}")?,
        }
    }
}

fn render_decision_prompt<W: Write>(
    output: &mut W,
    state: &HoldemHandState,
    human_role: Player,
    bot_role: Player,
) -> io::Result<()> {
    let legal = state.legal_actions().map_err(io::Error::other)?;
    writeln!(output, "\nStreet: {}", state.street())?;
    writeln!(output, "Board: {}", format_board(state))?;
    writeln!(output, "Pot: {}", state.pot())?;
    writeln!(
        output,
        "You ({human_role}): stack={} hand={}",
        state.player(human_role).stack,
        format_hole_cards(state.player(human_role).hole_cards),
    )?;
    writeln!(
        output,
        "Bot ({bot_role}): stack={}",
        state.player(bot_role).stack,
    )?;
    writeln!(output, "Options: {}", format_legal_actions(legal))?;
    write!(output, "> ")?;
    output.flush()
}

fn reveal_next_board<W: Write>(
    output: &mut W,
    state: &mut HoldemHandState,
    deal: &DealtHand,
    next_street: gto_core::Street,
) -> io::Result<()> {
    match next_street {
        gto_core::Street::Flop => {
            state.deal_flop([deal.board[0], deal.board[1], deal.board[2]])
                .map_err(io::Error::other)?;
            writeln!(output, "Flop: {}", format_board(state))?;
        }
        gto_core::Street::Turn => {
            state.deal_turn(deal.board[3]).map_err(io::Error::other)?;
            writeln!(output, "Turn: {}", format_board(state))?;
        }
        gto_core::Street::River => {
            state.deal_river(deal.board[4]).map_err(io::Error::other)?;
            writeln!(output, "River: {}", format_board(state))?;
        }
        gto_core::Street::Preflop => {
            return Err(io::Error::other("cannot reveal preflop board cards"));
        }
    }
    Ok(())
}

fn render_outcome<W: Write>(
    output: &mut W,
    state: &HoldemHandState,
    human_role: Player,
    outcome: HandOutcome,
) -> io::Result<()> {
    writeln!(output, "\nFinal board: {}", format_board(state))?;
    writeln!(
        output,
        "Your cards: {}",
        format_hole_cards(state.player(human_role).hole_cards)
    )?;
    writeln!(
        output,
        "Bot cards: {}",
        format_hole_cards(state.player(human_role.opponent()).hole_cards)
    )?;

    match outcome {
        HandOutcome::Uncontested {
            winner,
            pot,
            payout: _,
            street,
        } => {
            writeln!(
                output,
                "{} wins {pot} chips without showdown on {street}.",
                player_label(winner, human_role)
            )?;
        }
        HandOutcome::Showdown { result, pot, payout } => {
            writeln!(output, "Showdown for {pot} chips.")?;
            writeln!(output, "You: {}", result_for_player(result, human_role))?;
            writeln!(
                output,
                "Bot: {}",
                result_for_player(result, human_role.opponent())
            )?;
            let winner = if payout.player_one > payout.player_two {
                Some(Player::Button)
            } else if payout.player_two > payout.player_one {
                Some(Player::BigBlind)
            } else {
                None
            };
            match winner {
                Some(player) => writeln!(output, "{} wins the pot.", player_label(player, human_role))?,
                None => writeln!(output, "The pot is split.")?,
            }
        }
    }

    Ok(())
}

fn prompt_play_again<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> io::Result<bool> {
    loop {
        writeln!(output, "\nPlay another hand? [y/n]")?;
        write!(output, "> ")?;
        output.flush()?;

        let Some(line) = read_line(input, output)? else {
            return Ok(false);
        };
        match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => writeln!(output, "Please answer `y` or `n`.")?,
        }
    }
}

fn read_line<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> io::Result<Option<String>> {
    let mut line = String::new();
    let read = input.read_line(&mut line)?;
    if read == 0 {
        writeln!(output, "Input closed; exiting.")?;
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

fn parse_user_action(input: &str) -> Result<Option<PlayerAction>, &'static str> {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("enter an action such as `call`, `check`, `bet 100`, or `quit`");
    }
    match normalized.as_str() {
        "fold" | "f" => return Ok(Some(PlayerAction::Fold)),
        "check" | "k" => return Ok(Some(PlayerAction::Check)),
        "call" | "c" => return Ok(Some(PlayerAction::Call)),
        "allin" | "all-in" | "jam" => return Ok(Some(PlayerAction::AllIn)),
        "quit" | "q" | "exit" => return Ok(None),
        _ => {}
    }

    let mut parts = normalized.split_whitespace();
    let Some(keyword) = parts.next() else {
        return Err("enter an action");
    };
    let Some(amount_text) = parts.next() else {
        return Err("expected an amount after the action");
    };
    if parts.next().is_some() {
        return Err("too many tokens in action");
    }
    let amount = amount_text
        .parse::<u64>()
        .map_err(|_| "amount must be a positive integer")?;

    match keyword {
        "bet" | "b" => Ok(Some(PlayerAction::BetTo(amount))),
        "raise" | "r" | "raise-to" => Ok(Some(PlayerAction::RaiseTo(amount))),
        _ => Err("unknown action"),
    }
}

fn format_legal_actions(legal: gto_core::LegalActions) -> String {
    let mut parts = Vec::new();
    if legal.fold {
        parts.push("fold".to_string());
    }
    if legal.check {
        parts.push("check".to_string());
    }
    if let Some(call_amount) = legal.call_amount {
        parts.push(format!("call ({call_amount})"));
    }
    if let Some(range) = legal.bet_range {
        parts.push(format!("bet <{}-{}>", range.min_total, range.max_total));
    }
    if let Some(range) = legal.raise_range {
        parts.push(format!("raise <{}-{}>", range.min_total, range.max_total));
    }
    if let Some(total) = legal.all_in_to {
        parts.push(format!("all-in ({total})"));
    }
    parts.join(" | ")
}

fn format_board(state: &HoldemHandState) -> String {
    if state.board().is_empty() {
        "-".to_string()
    } else {
        state.board()
            .cards()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn format_hole_cards(cards: gto_core::HoleCards) -> String {
    let [left, right] = cards.cards();
    format!("{left} {right}")
}

fn describe_action(action: PlayerAction, state: &HoldemHandState) -> String {
    match action {
        PlayerAction::Fold => "fold".to_string(),
        PlayerAction::Check => "check".to_string(),
        PlayerAction::Call => "call".to_string(),
        PlayerAction::BetTo(total) => format!("bet to {total}"),
        PlayerAction::RaiseTo(total) => format!("raise to {total}"),
        PlayerAction::AllIn => {
            let current_total = state.player(state.current_actor().unwrap()).street_contribution;
            let target_total = current_total + state.player(state.current_actor().unwrap()).stack;
            format!("all-in to {target_total}")
        }
    }
}

fn player_label(player: Player, human_role: Player) -> &'static str {
    if player == human_role {
        "You"
    } else {
        "Bot"
    }
}

fn result_for_player(result: gto_core::ShowdownResult, player: Player) -> String {
    match player {
        Player::Button => result.player_one_rank.to_string(),
        Player::BigBlind => result.player_two_rank.to_string(),
    }
}

struct DealtHand {
    button: gto_core::HoleCards,
    big_blind: gto_core::HoleCards,
    board: [gto_core::Card; 5],
}

impl DealtHand {
    fn deal(rng: &mut DeterministicRng) -> io::Result<Self> {
        let mut deck = Deck::standard();
        deck.shuffle(rng);

        let button = gto_core::HoleCards::new(draw_card(&mut deck)?, draw_card(&mut deck)?)
            .map_err(io::Error::other)?;
        let big_blind =
            gto_core::HoleCards::new(draw_card(&mut deck)?, draw_card(&mut deck)?)
                .map_err(io::Error::other)?;

        Ok(Self {
            button,
            big_blind,
            board: [
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
                draw_card(&mut deck)?,
            ],
        })
    }
}

fn draw_card(deck: &mut Deck) -> io::Result<gto_core::Card> {
    deck.draw()
        .ok_or_else(|| io::Error::other("deck ran out of cards"))
}

#[cfg(test)]
mod tests {
    use super::{CliConfig, run_session, startup_banner};
    use std::io::Cursor;

    #[test]
    fn startup_banner_mentions_interactive_cli() {
        let banner = startup_banner();

        assert!(banner.contains("gto-solver"));
        assert!(banner.contains("milestone M4 interactive CLI vertical slice"));
    }

    #[test]
    fn scripted_session_can_play_a_complete_hand() {
        let input = b"call\ncheck\ncheck\ncheck\n";
        let mut output = Vec::new();

        run_session(
            &mut Cursor::new(&input[..]),
            &mut output,
            CliConfig {
                seed: 7,
                max_hands: Some(1),
            },
        )
        .unwrap();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("Hand 1"));
        assert!(transcript.contains("You are the button."));
        assert!(transcript.contains("Your cards:"));
        assert!(transcript.contains("Flop:"));
        assert!(transcript.contains("Showdown"));
    }

    #[test]
    fn invalid_input_is_reported_and_reprompted() {
        let input = b"banana\ncall\ncheck\ncheck\ncheck\n";
        let mut output = Vec::new();

        run_session(
            &mut Cursor::new(&input[..]),
            &mut output,
            CliConfig {
                seed: 7,
                max_hands: Some(1),
            },
        )
        .unwrap();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("Invalid action"));
        assert!(transcript.contains("Showdown"));
    }

    #[test]
    fn eof_exits_cleanly() {
        let mut output = Vec::new();

        run_session(
            &mut Cursor::new(&b""[..]),
            &mut output,
            CliConfig {
                seed: 7,
                max_hands: Some(1),
            },
        )
        .unwrap();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("Input closed; exiting."));
    }
}
