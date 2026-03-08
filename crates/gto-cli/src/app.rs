use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use gto_core::{
    DEFAULT_RNG_SEED, Deck, DeterministicRng, HandOutcome, HandPhase, HoldemConfig,
    HoldemHandState, HoleCards, Player, PlayerAction, Range, rng_from_seed,
};
use gto_solver::{
    AbstractionProfile, AbstractAction, HoldemInfoSetKey, OpeningSize, RaiseSize,
    RiverStrategyArtifact, ScriptedRiverSpot, SolverProfile, StreetProfile, StubBot,
    abstract_actions, solve_river_spot,
};

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

pub fn run_stdio_with_args(args: &[String]) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    match args.first().map(String::as_str) {
        Some("river-demo") => {
            let options = parse_river_demo_args(&args[1..]).map_err(io::Error::other)?;
            run_river_demo(&mut input, &mut output, &options)
        }
        _ => run_session(&mut input, &mut output, CliConfig::default()),
    }
}

pub fn startup_banner() -> String {
    let build = gto_solver::build_info();
    let profile = SolverProfile::placeholder();

    format!(
        "{name} {version}\nstatus: milestones M4-M7 CLI demos and river solver integration\nsolver-profile: {profile}\nwasm-safe-core: {wasm_safe}",
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RiverDemoOptions {
    artifact_path: PathBuf,
    solve_if_missing: bool,
    write_artifact_path: Option<PathBuf>,
    no_play: bool,
}

impl Default for RiverDemoOptions {
    fn default() -> Self {
        Self {
            artifact_path: default_river_demo_artifact_path(),
            solve_if_missing: true,
            write_artifact_path: None,
            no_play: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RiverDemoScenario {
    spot: ScriptedRiverSpot,
    button_range: Range,
    big_blind_range: Range,
    profile: AbstractionProfile,
    button_hole_cards: HoleCards,
    big_blind_hole_cards: HoleCards,
    artifact_iterations: u64,
}

impl RiverDemoScenario {
    fn default() -> Self {
        let preflop = StreetProfile {
            opening_sizes: vec![OpeningSize::BigBlindMultipleBps(25_000)],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(25_000)],
            include_all_in: false,
        };
        let postflop = StreetProfile {
            opening_sizes: vec![OpeningSize::PotFractionBps(10_000)],
            raise_sizes: vec![RaiseSize::CurrentBetMultipleBps(30_000)],
            include_all_in: false,
        };

        Self {
            spot: ScriptedRiverSpot {
                config: HoldemConfig::default(),
                preflop_actions: vec![PlayerAction::Call],
                flop: [
                    "Kc".parse().unwrap(),
                    "8d".parse().unwrap(),
                    "4s".parse().unwrap(),
                ],
                flop_actions: vec![PlayerAction::Check, PlayerAction::Check],
                turn: "3h".parse().unwrap(),
                turn_actions: vec![PlayerAction::Check, PlayerAction::Check],
                river: "2d".parse().unwrap(),
                river_prefix_actions: vec![PlayerAction::BetTo(100)],
            },
            button_range: "8c7c,QhJc,2c2h".parse().unwrap(),
            big_blind_range: "AhQh,KhKd,8s8h".parse().unwrap(),
            profile: AbstractionProfile::new(
                preflop,
                postflop.clone(),
                postflop.clone(),
                postflop,
            ),
            button_hole_cards: "8c7c".parse().unwrap(),
            big_blind_hole_cards: "AhQh".parse().unwrap(),
            artifact_iterations: 8_000,
        }
    }

    fn build_artifact(&self, iterations: u64) -> io::Result<RiverStrategyArtifact> {
        let result = solve_river_spot(
            self.spot.clone(),
            self.button_range.clone(),
            self.big_blind_range.clone(),
            self.profile.clone(),
            iterations,
        )
        .map_err(io::Error::other)?;

        Ok(result.into_artifact(
            self.spot.clone(),
            self.button_range.clone(),
            self.big_blind_range.clone(),
            self.profile.clone(),
        ))
    }

    fn validate_artifact(&self, artifact: &RiverStrategyArtifact) -> io::Result<()> {
        if artifact.spot != self.spot
            || artifact.button_range != self.button_range
            || artifact.big_blind_range != self.big_blind_range
            || artifact.profile != self.profile
        {
            return Err(io::Error::other(
                "river demo artifact does not match the built-in scenario",
            ));
        }
        Ok(())
    }
}

fn run_river_demo<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    options: &RiverDemoOptions,
) -> io::Result<()> {
    writeln!(output, "{}", startup_banner())?;

    let scenario = RiverDemoScenario::default();
    let (artifact, artifact_source) = load_or_build_river_demo_artifact(&scenario, options)?;
    scenario.validate_artifact(&artifact)?;

    if let Some(path) = &options.write_artifact_path {
        write_river_artifact(path, &artifact)?;
        writeln!(output, "Wrote river artifact to {}", path.display())?;
    }

    if options.no_play {
        writeln!(
            output,
            "River demo artifact ready ({artifact_source}, {} infosets, {} iterations).",
            artifact.entries.len(),
            artifact.iterations,
        )?;
        return Ok(());
    }

    let strategy = artifact.to_solver_result().map_err(io::Error::other)?;
    let mut state = scenario
        .spot
        .build_state(scenario.button_hole_cards, scenario.big_blind_hole_cards)
        .map_err(io::Error::other)?;
    let human_role = Player::Button;
    let bot_role = Player::BigBlind;
    let mut public_history = Vec::new();

    writeln!(output, "\nRiver Demo")?;
    writeln!(output, "source: {artifact_source}")?;
    writeln!(output, "board: {}", format_board(&state))?;
    writeln!(
        output,
        "you ({human_role}): {}",
        format_hole_cards(scenario.button_hole_cards),
    )?;
    writeln!(output, "bot ({bot_role}): stack={}", state.player(bot_role).stack)?;
    writeln!(
        output,
        "pot: {} | scripted history: call / check-check / check-check / bot bet 100",
        state.pot()
    )?;

    loop {
        match state.phase() {
            HandPhase::BettingRound { actor, .. } => {
                let actions = abstract_actions(&state, &scenario.profile).map_err(io::Error::other)?;
                if actor == human_role {
                    if !handle_human_abstract_turn(
                        input,
                        output,
                        &mut state,
                        &actions,
                        &mut public_history,
                    )? {
                        return Ok(());
                    }
                } else {
                    let infoset = HoldemInfoSetKey::from_state(
                        bot_role,
                        scenario.big_blind_hole_cards,
                        &state,
                        public_history.clone(),
                    );
                    let action = strategy.choose_action_max(&infoset).ok_or_else(|| {
                        io::Error::other("river strategy artifact had no action for the bot infoset")
                    })?;
                    writeln!(output, "Bot ({bot_role}) -> {}", describe_abstract_action(action))?;
                    state
                        .apply_action(action.to_player_action())
                        .map_err(io::Error::other)?;
                    public_history.push(action);
                }
            }
            HandPhase::Terminal { outcome } => {
                render_outcome(output, &state, human_role, outcome)?;
                return Ok(());
            }
            HandPhase::AwaitingBoard { .. } => {
                return Err(io::Error::other(
                    "river demo unexpectedly requested more board cards",
                ));
            }
        }
    }
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

fn parse_river_demo_args(args: &[String]) -> Result<RiverDemoOptions, &'static str> {
    let mut options = RiverDemoOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--artifact" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err("expected a path after `--artifact`");
                };
                options.artifact_path = PathBuf::from(path);
            }
            "--write-artifact" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err("expected a path after `--write-artifact`");
                };
                options.write_artifact_path = Some(PathBuf::from(path));
            }
            "--solve" => {
                options.solve_if_missing = true;
            }
            "--artifact-only" => {
                options.solve_if_missing = false;
            }
            "--no-play" => {
                options.no_play = true;
            }
            _ => return Err("unknown river-demo option"),
        }
        index += 1;
    }

    Ok(options)
}

fn load_or_build_river_demo_artifact(
    scenario: &RiverDemoScenario,
    options: &RiverDemoOptions,
) -> io::Result<(RiverStrategyArtifact, String)> {
    match read_river_artifact(&options.artifact_path) {
        Ok(artifact) => Ok((
            artifact,
            format!("loaded {}", options.artifact_path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound && options.solve_if_missing => Ok((
            scenario.build_artifact(scenario.artifact_iterations)?,
            format!(
                "generated inline (default artifact missing at {})",
                options.artifact_path.display()
            ),
        )),
        Err(error) => Err(error),
    }
}

fn read_river_artifact(path: &Path) -> io::Result<RiverStrategyArtifact> {
    let encoded = fs::read_to_string(path)?;
    RiverStrategyArtifact::from_json_str(&encoded).map_err(io::Error::other)
}

fn write_river_artifact(path: &Path, artifact: &RiverStrategyArtifact) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = artifact.to_json_string().map_err(io::Error::other)?;
    fs::write(path, encoded)
}

fn default_river_demo_artifact_path() -> PathBuf {
    workspace_root().join("fixtures").join("strategies").join("river_demo.json")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("gto-cli manifest dir should live under the workspace root")
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

fn handle_human_abstract_turn<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut HoldemHandState,
    actions: &[AbstractAction],
    public_history: &mut Vec<AbstractAction>,
) -> io::Result<bool> {
    loop {
        render_abstract_decision_prompt(output, state, actions)?;
        let Some(line) = read_line(input, output)? else {
            return Ok(false);
        };
        match parse_human_abstract_action(&line, actions) {
            Ok(Some(action)) => {
                state
                    .apply_action(action.to_player_action())
                    .map_err(io::Error::other)?;
                public_history.push(action);
                return Ok(true);
            }
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

fn render_abstract_decision_prompt<W: Write>(
    output: &mut W,
    state: &HoldemHandState,
    actions: &[AbstractAction],
) -> io::Result<()> {
    writeln!(output, "\nStreet: {}", state.street())?;
    writeln!(output, "Board: {}", format_board(state))?;
    writeln!(output, "Pot: {}", state.pot())?;
    writeln!(
        output,
        "Options: {}",
        actions
            .iter()
            .enumerate()
            .map(|(index, action)| format!("{}: {}", index + 1, describe_abstract_action(*action)))
            .collect::<Vec<_>>()
            .join(" | ")
    )?;
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

fn parse_human_abstract_action(
    input: &str,
    actions: &[AbstractAction],
) -> Result<Option<AbstractAction>, &'static str> {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("enter a listed action, its number, or `quit`");
    }
    if matches!(normalized.as_str(), "quit" | "q" | "exit") {
        return Ok(None);
    }
    if let Ok(index) = normalized.parse::<usize>() {
        let action = actions
            .get(index.saturating_sub(1))
            .copied()
            .ok_or("option number is out of range")?;
        return Ok(Some(action));
    }

    let exact_action = parse_user_action(&normalized)?;
    match exact_action {
        Some(action) => actions
            .iter()
            .copied()
            .find(|candidate| candidate.to_player_action() == action)
            .map(Some)
            .ok_or("that action is not in the solver menu for this spot"),
        None => Ok(None),
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

fn describe_abstract_action(action: AbstractAction) -> String {
    match action {
        AbstractAction::Fold => "fold".to_string(),
        AbstractAction::Check => "check".to_string(),
        AbstractAction::Call => "call".to_string(),
        AbstractAction::BetTo(total) => format!("bet to {total}"),
        AbstractAction::RaiseTo(total) => format!("raise to {total}"),
        AbstractAction::AllIn(total) => format!("all-in to {total}"),
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
    use super::{
        CliConfig, RiverDemoOptions, RiverDemoScenario, default_river_demo_artifact_path,
        run_river_demo, run_session, startup_banner, write_river_artifact,
    };
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn startup_banner_mentions_interactive_cli() {
        let banner = startup_banner();

        assert!(banner.contains("gto-solver"));
        assert!(banner.contains("milestones M4-M7 CLI demos and river solver integration"));
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

    #[test]
    fn river_demo_can_play_from_a_saved_artifact() {
        let scenario = RiverDemoScenario::default();
        let artifact_path = unique_test_path("river-demo-artifact.json");
        let artifact = scenario.build_artifact(2_000).unwrap();
        write_river_artifact(&artifact_path, &artifact).unwrap();

        let mut output = Vec::new();
        run_river_demo(
            &mut Cursor::new(&b"call\n"[..]),
            &mut output,
            &RiverDemoOptions {
                artifact_path: artifact_path.clone(),
                solve_if_missing: false,
                write_artifact_path: None,
                no_play: false,
            },
        )
        .unwrap();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("River Demo"));
        assert!(transcript.contains("source: loaded"));
        assert!(transcript.contains("board: Kc 8d 4s 3h 2d"));
        assert!(transcript.contains("Showdown"));

        let _ = fs::remove_file(artifact_path);
    }

    #[test]
    fn river_demo_can_generate_an_artifact_without_playing() {
        let artifact_path = unique_test_path("generated-river-demo.json");
        let mut output = Vec::new();

        run_river_demo(
            &mut Cursor::new(&b""[..]),
            &mut output,
            &RiverDemoOptions {
                artifact_path: default_river_demo_artifact_path(),
                solve_if_missing: true,
                write_artifact_path: Some(artifact_path.clone()),
                no_play: true,
            },
        )
        .unwrap();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("Wrote river artifact"));
        assert!(artifact_path.exists());

        let _ = fs::remove_file(artifact_path);
    }

    fn unique_test_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("gto-{nanos}-{name}"))
    }
}
