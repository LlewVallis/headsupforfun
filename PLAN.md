# PLAN

## Accepted Constraints

- Pure Rust only. No native C/C++ libraries and no poker-specific crates.
- Non-poker Rust crates are allowed when they improve correctness, ergonomics, or maintainability.
- `gto-core` and `gto-solver` must stay compatible with `wasm32-unknown-unknown`.
- The interactive CLI must be separate from the core solver so the solver can be reused as a library later.
- `unsafe` Rust is completely forbidden in this repository.
- CI and formatting automation are intentionally deferred for now.
- Tests are a first-class deliverable. Edge-case coverage matters more than raw feature count.
- The default developer loop must stay fast. Slow offline training must never be a prerequisite for ordinary edits.

## Product Goal

Build a Rust codebase that can:

1. Model heads-up no-limit hold'em correctly.
2. Train and load approximate-GTO strategies under a deliberately small initial abstraction.
3. Let a human play against a bot through a plain console interface.
4. Expose the poker engine and solver as reusable library crates.

Primary priorities, in order:

1. Correctness
2. Maintainability
3. Reasonable performance

## Scope

### In Scope

- Heads-up no-limit Texas hold'em only
- Standard `100bb` effective stacks, `0.5 / 1.0` blinds, no ante
- Approximate equilibrium via explicit action abstraction
- Offline training and saved strategy artifacts for v1
- Deterministic CLI play against a cached strategy bot
- Extensive tests, smoke benchmarks, and time-bounded dev workflows
- Strategy artifact persistence and replay/transcript support

### Out of Scope for the First Delivery

- Multiplayer poker
- Pot-limit / fixed-limit / PLO / short deck
- Arbitrary continuous bet sizing
- Neural methods, Deep CFR, NFSP, value networks
- Real-time endgame re-solving during normal CLI play
- GUI / web frontend
- Database-backed analysis pipeline
- CI setup, formatter/linter automation

Note:

- After the first playable artifact-backed CLI is complete, a limited hybrid bot that keeps artifact-backed preflop and adds bounded runtime postflop solving is an acceptable follow-on milestone.
- This is not the same as full real-time re-solving on every street, which remains out of scope until there is a stronger reason to take on that complexity.

## Research Summary And Planning Implications

The plan should be informed by what existing open-source solvers actually do well and where they become expensive or fragile.

### `postflop-solver`

- Rust library focused on postflop solving, not full-game HUNL.
- Uses Discounted CFR, aggressive performance work, multithreading, and isomorphism reductions.
- Has separate WASM and desktop frontends.

Implications:

- Keep the solver core as a library, not fused to the UI.
- Isolate expensive optimization work until correctness is proven.
- Treat persistence and reusable artifacts as core design concerns.

### `TexasSolver`

- Focuses on efficient solving under constrained betting trees.
- Exports strategy artifacts and benchmarks against commercial solvers.
- Release history and issue history show that tree-building and action-legality bugs matter a lot.

Implications:

- Tree generation and action legality need unusually strong tests.
- Start with a narrow, explicit betting abstraction instead of pretending to support the full continuous action space.

### `slumbot2019`

- Supports CFR+, MCCFR, endgame resolving, card abstractions, betting abstractions, and best-response style evaluation.
- Treats game params, card abstraction, betting abstraction, and solver params as distinct inputs.

Implications:

- Keep abstractions explicit and versioned.
- Support objective evaluation early on, especially exploitability or best-response style checks on small games.
- Avoid hard-coding abstraction assumptions deep inside the rules engine.

### `robopoker`

- Shows how quickly a full NLHE pipeline expands into clustering, metric computation, offline training, checkpoints, and later search.
- The offline training pipeline is intentionally resource intensive.

Implications:

- Do not make heavyweight abstraction generation or long training the default path.
- Use tiny deterministic training profiles and checkpoints from the start.
- Delay learned clustering and large imperfect-recall machinery until a simpler system is working.

### `OpenSpiel`

- Excellent reference framework for game abstractions, solver ideas, and test philosophy.
- Prioritizes clarity and reference-quality implementations over maximum performance.
- Windows and Rust support are not first-class.

Implications:

- Use toy games and exact reference checks for solver validation.
- Do not depend on OpenSpiel directly.
- Keep our own architecture simple and auditable instead of prematurely clever.

## High-Level Technical Strategy

### Core Architectural Decisions

1. Separate exact game rules from abstractions and from the CLI.
2. Keep the library crates portable and WASM-safe.
3. Use explicit action abstraction from day one.
4. Start with exact or near-exact small games for solver correctness before touching larger HUNL trees.
5. Prefer saved strategy artifacts over online solving in the first playable CLI.
6. Prefer deterministic, explicit, debuggable algorithms over highly compressed or learned approaches.

### Recommended Workspace Layout

```text
/
  PLAN.md
  AGENTS.md
  Cargo.toml
  crates/
    gto-core/
    gto-solver/
    gto-cli/
    xtask/
  fixtures/
    strategies/
    transcripts/
    toy_games/
```

### Crate Responsibilities

#### `gto-core`

- Card, suit, rank, deck, hand, board, combo, and range types
- Heads-up NLHE rules engine
- Stack / pot accounting
- Legal action generation
- Street transitions and terminal detection
- Showdown logic and hand evaluation
- Hand history / transcript domain types

Rules:

- No CLI logic
- No filesystem access
- No native-only dependencies
- No thread requirements
- No `unsafe` under any circumstances

#### `gto-solver`

- Abstraction profiles
- Public tree builder
- Information-state encoding
- CFR+/DCFR engine
- Optional MCCFR later if needed
- Strategy tables and policy sampling
- Strategy serialization to and from bytes
- Training profiles: `smoke`, `dev`, `full`
- Bot query interface used by CLI and later by WASM

Rules:

- No console I/O
- No filesystem assumptions in the core API
- Native-only acceleration, if ever added, must be feature-gated and optional

#### `gto-cli`

- Plain text interactive play loop
- Prompt rendering and input parsing
- Hand history display
- Strategy artifact loading from disk
- Deterministic seed handling for reproducible sessions
- Graceful failure when artifacts are missing or incompatible

#### `xtask`

- Cross-platform developer commands
- Time-bounded wrappers for tests, smoke training, and benchmark runs
- Keeps agent workflows consistent without relying on shell-specific scripts

## Solver Approach

### Initial Action Abstraction

The first playable bot should use a narrow, explicit action set. The human should be offered the same supported actions instead of entering arbitrary chip amounts.

Recommended starting profile:

- Preflop unopened: `fold`, `call`, `2.5bb`, `4bb`, `7bb`, `all-in`
- Postflop unopened: `check`, `33% pot`, `66% pot`, `100% pot`, `all-in`
- Facing a bet: `fold`, `call`, `2.5x raise`, `all-in`

This profile can be shrunk if training cost is still too high. The first priority is a complete, stable, playable system.

### Card Abstraction

Default stance:

- Delay card abstraction as long as possible in early postflop milestones.
- If full-game training becomes too slow, introduce a simple deterministic bucketing scheme first.
- Do not start with hierarchical k-means, optimal transport metrics, or other heavy offline clustering pipelines.

Recommended order:

1. Exact toy games
2. Exact or lightly abstracted river / turn subgames
3. Postflop with small action abstraction
4. Only then consider simple deterministic card buckets if needed

### Algorithm Progression

1. Generic CFR+ on toy games for correctness
2. DCFR or CFR+ on river / postflop abstractions
3. External-sampling MCCFR only if the larger training problem becomes too slow
4. No neural approximators in the initial repository delivery

## Testing Strategy

### Testing Philosophy

- Use TDD or test-first development for rules, evaluator, serialization, and bug fixes.
- For solver work, require a test or benchmark that proves the new code is correct or useful.
- Every bug fix adds a regression test before or alongside the fix.
- Default test commands must stay fast enough for constant use by an autonomous agent.
- Slow tests and long training runs must be explicitly opt-in.

### Test Layers

#### Unit Tests

- Card parsing and formatting
- Deck and dead-card handling
- Range parsing and normalization
- Hand evaluator correctness
- Pot accounting
- Min-raise and all-in legality
- Street transition logic
- Serialization round-trips

#### Property Tests

- No duplicate cards can appear in a legal state
- Pot plus remaining stacks stays conserved
- Legal actions are never negative or impossible
- Terminal states have no legal actions
- Strategy distributions sum to `1.0` within tolerance
- Regret / policy tables never produce `NaN` or `inf`
- Strategy artifact round-trips preserve semantics

#### Exhaustive Or Brute-Force Cross-Checks

- Exhaustive 5-card ranking tests
- Randomized 7-card cross-checks against a slower choose-best-5 reference implementation
- Reduced-deck exhaustive solver checks on toy games
- Exact exploitability checks on Kuhn / Leduc or similarly tiny domains

#### Integration Tests

- Full hand simulations from blind posting to showdown
- Fold, call, raise, re-raise, and all-in branches on every street
- End-to-end training smoke run on a tiny abstraction
- Strategy load -> bot action query -> CLI transcript flow

#### Regression And Snapshot Tests

- Golden transcripts for complete CLI sessions under fixed seeds
- Strategy artifact compatibility tests
- Reproductions of previously fixed solver or tree-construction bugs

#### Soak Tests

- Long random self-play sessions to catch panics, invalid states, or probability drift
- Time-bounded, opt-in only

### Edge-Case Inventory That Must Be Covered

- Posting blinds and entering the first preflop decision
- Check-check street advancement
- Min-bet, min-raise, and all-in interactions
- All-in before the river and forced runout
- Fold on each street
- Exact stack exhaustion
- Split pots and tied hands
- Board-pair, flush, straight, full house, and straight flush tie logic
- Dead cards removed from ranges and chance outcomes
- Illegal human input in the CLI
- EOF / interrupted input in the CLI
- Missing or incompatible strategy artifacts
- Zero-probability action handling in the solver

### Test Speed Policy

Fast path:

- Must be runnable constantly during development
- Target: under `60s` total on a normal development machine
- Includes library unit tests, most integration tests, and WASM compile checks

Slow path:

- Opt-in only
- Target: under `5m` for slow tests, `1m` for smoke training
- Includes solver convergence checks on bigger abstractions and soak tests

Long path:

- Manual only
- Includes fuller training runs and deeper benchmarks

### Recommended Rust Test/Bench Dependencies

- `proptest` for invariants
- `criterion` for benchmarks
- `insta` or plain golden files for transcript snapshots
- `serde` plus a stable binary format for artifact tests
- `rand_chacha` for deterministic RNG

## Performance Strategy

### General Rules

- Do not optimize blind.
- Do not trade away testability for speculative speed.
- Treat hand evaluation, tree building, info-set lookup, and regret updates as likely hot paths.
- Avoid platform-specific tricks unless benchmarks show a clear need, and keep `unsafe` fully banned.

### Planned Performance Work

1. Build correctness first.
2. Add small benchmarks as soon as hot code exists.
3. Measure memory and latency before changing data layout aggressively.
4. Keep native-only acceleration behind optional features so WASM compatibility is not broken.

### Performance Budgets

- CLI action selection from cached strategy: under `250ms` typical, under `1s` worst case
- Strategy artifact load for demo-sized assets: under `2s`
- Smoke training profile: under `60s`
- Dev training profile: under `10m`
- Full training profile: allowed to be long, but must be checkpointed and resumable

### DX Guardrails

- The repository must be usable without a long training run.
- Commit at least one tiny playable strategy artifact or a tiny deterministic generator that completes quickly.
- Long training profiles must checkpoint frequently.
- No default command should launch a long offline run without an explicit choice.
- `xtask` should expose time-bounded commands such as:
  - `cargo xtask test-fast`
  - `cargo xtask test-slow --timeout-secs 300`
  - `cargo xtask train-smoke --timeout-secs 60`
  - `cargo xtask bench-smoke --timeout-secs 120`

## Milestones

Each milestone is deliverable, testable, and should leave the repo in a usable state.

### M0. Workspace Split And Developer Harness

Deliverables:

- Convert the repo into a workspace with `gto-core`, `gto-solver`, `gto-cli`, and `xtask`
- Shared dependency policy
- Feature flags for WASM-safe builds and optional native helpers
- Basic timed developer commands

Validation:

- Workspace builds
- `gto-core` and `gto-solver` compile for `wasm32-unknown-unknown`
- Fast test harness exists even if tests are minimal at this point

### M1. Cards, Ranges, And Core Domain Types

Deliverables:

- Card / suit / rank / deck / hole cards / board types
- Range and combo representations
- Parsing and formatting utilities
- Deterministic RNG utilities

Validation:

- Exhaustive uniqueness tests
- Parser round-trips
- Dead-card filtering tests
- Property tests for range integrity

### M2. Hand Evaluation And Showdown Resolution

Deliverables:

- 5-card and 7-card hand evaluation
- Heads-up showdown comparison
- Pot award logic for heads-up play

Validation:

- Known hand-ranking regression suite
- Randomized cross-check against slower reference evaluator
- Tie / split-pot tests
- Benchmark for evaluator latency

### M3. Exact Heads-Up NLHE State Machine

Deliverables:

- Blind posting
- Betting rounds
- Legal action generation
- Min-raise and all-in handling
- Street progression and terminal state detection
- Hand history model

Validation:

- Pot-conservation property tests
- Rule edge-case tests across every street
- Random-play integration tests with no panics or invalid states

### M4. CLI Vertical Slice With Stub Bot

Deliverables:

- Plain console game loop
- Human input parsing and validation
- Deterministic stub bot using legal actions from `gto-core`
- Hand transcript rendering

Validation:

- Snapshot tests for seeded sessions
- Invalid input and EOF handling tests
- End-to-end playthrough using the stub bot

Note:

- This milestone exists to validate UX and crate boundaries early, before solver complexity lands.

### M5. Generic Extensive-Form Framework On Toy Games

Deliverables:

- Information-state interfaces
- Tabular regret / strategy storage
- CFR+ implementation for toy games such as Kuhn and Leduc
- Exact exploitability or known-solution checks where feasible

Validation:

- Convergence tests against known toy-game strategies
- Serialization round-trip tests
- No-`NaN` / finite-value invariant tests

### M6. HUNL Abstraction Layer And Public Tree Builder

Deliverables:

- `AbstractionProfile`
- Deterministic public-tree construction
- Information-state encoding for the abstracted HUNL game
- Strategy indexing scheme

Validation:

- Tree determinism tests
- Legal action equivalence checks against the exact rules engine
- Public-tree integrity tests with no unreachable or malformed nodes

### M7. River-Only Solver And Bot Query API

Deliverables:

- River-only solving over the chosen abstraction
- Strategy query API usable by the CLI
- Artifact save/load support for river strategies

Validation:

- Brute-force or reduced-game best-response checks where feasible
- River scenario regression suite
- CLI can play river-only spots against the solver bot

### M8. Turn Plus River Solver, Checkpointing, And Smoke Training

Deliverables:

- Chance handling for turn-to-river subgames
- Checkpointed training
- `smoke`, `dev`, and `full` training profiles
- Time-bounded `xtask` commands

Validation:

- Resume-from-checkpoint tests
- Deterministic smoke training within the stated timeout
- Artifact compatibility tests

### M9. Postflop Solver From Flop To River

Deliverables:

- Flop chance expansion
- End-to-end postflop strategy loading and querying
- Optional simple deterministic card buckets only if exact handling is too slow

Validation:

- Postflop scenario regression suite
- Benchmarks for tree build and action lookup
- CLI can play a full postflop hand against the solver bot

Decision gate:

- If performance is not acceptable, reduce abstraction size before adding major algorithmic complexity.

### M10. Preflop Blueprint And Full-Hand Integration

Deliverables:

- Preflop abstraction and starting ranges
- Full hand flow from blind posting to river using one consistent strategy system
- Cached strategy artifact that supports complete hand play

Validation:

- Full-hand simulation invariants
- Soak test for repeated self-play without crashes
- CLI can play complete heads-up hands from preflop to showdown

### M11. Playable CLI Release Candidate

Deliverables:

- Clear prompts and action menus
- Bot backed by cached solver strategy
- Replayable seeded sessions
- Friendly error handling around missing artifacts and invalid input
- Bundled tiny default strategy artifact or tiny generator for immediate use

Validation:

- Transcript snapshots for complete hands
- Reproducibility tests with fixed seeds
- Manual smoke run shows a human can play a complete hand with no extra setup beyond build and run

### M12. Performance Hardening And Optional Solver Extensions

Deliverables:

- Criterion benchmarks for hot paths
- Profiling-driven data layout improvements where justified
- Optional native-only parallel training behind a feature flag, only if it does not contaminate WASM-safe crates
- Re-evaluation of whether MCCFR or limited re-solving is worth the added complexity

Validation:

- Action latency and smoke training budgets met
- No regression in fast test loop
- WASM compilation still passes for `gto-core` and `gto-solver`

### M13. Hybrid CLI Bot With Runtime Postflop Solving

Goal:

- Improve the default play experience quickly by keeping cached artifact-backed preflop, but using the existing runtime solver infrastructure for selected postflop decisions.
- Produce a bot that is more fun and more credible to play against without waiting for a much stronger full-hand artifact pipeline.

Deliverables:

- Add a hybrid bot mode in `gto-cli`
- Keep preflop on the cached blueprint artifact
- Use `PostflopSolverBot` for `turn` and `river` by default in hybrid mode
- Optionally enable runtime `flop` solving behind an explicit stronger profile
- Add CLI configuration for hybrid solver profiles such as `fast` and `play`
- Add a per-decision timeout or bounded-work fallback so the CLI remains responsive
- Fall back to the cached blueprint action when runtime solving is unavailable, too slow, or produces no action
- Keep the default developer loop fast and avoid introducing mandatory long training for ordinary play

Validation:

- Transcript and replay tests for hybrid sessions under fixed seeds
- Integration tests proving the hybrid bot always returns legal actions on every street
- Regression tests for fallback behavior when the runtime solver cannot build a usable strategy
- Benchmarks or timed smoke checks for hybrid action latency on `flop`, `turn`, and `river`
- Manual smoke run confirms the bot feels responsive enough for CLI play

Success criteria:

- The hybrid bot is measurably stronger postflop than the current smoke blueprint bot in practical play
- `turn` and `river` runtime solving fit within the interactive CLI latency budget
- `flop` runtime solving is only enabled in profiles where the measured latency is acceptable

Non-goals:

- Replacing the full-hand artifact pipeline
- Building a true runtime solver for preflop
- Solving every decision online regardless of latency cost

## Risk Register

### Risk: Tree Size Explosion

Mitigation:

- Keep the first action abstraction small
- Add checkpoints and smoke profiles early
- Prefer smaller abstractions over fancy algorithm changes

### Risk: Hidden Rules Bugs

Mitigation:

- Heavy unit, property, and integration coverage on the exact engine
- Regression tests for every discovered rules bug

### Risk: Solver Appears To Improve But Is Wrong

Mitigation:

- Exact toy-game convergence checks
- Best-response or exploitability-style checks on reduced domains
- Deterministic regression baselines for larger abstractions

### Risk: Slow Developer Iteration

Mitigation:

- `xtask` wrappers with explicit timeouts
- Bundled tiny artifacts
- Smoke profiles that finish quickly
- Long training always opt-in

## Commit Strategy

- Commit frequently as milestones progress, not only at the end.
- A commit should usually follow a green, relevant test run.
- Prefer small commits that preserve a working tree and isolate one coherent change.
- Bug fixes should include the regression test in the same commit.
- Do not batch unrelated refactors and feature work into one commit.
- Long-running solver work should checkpoint in files and in git history so progress is not fragile.

### Risk: WASM Drift

Mitigation:

- Compile-check `gto-core` and `gto-solver` for WASM throughout development
- Keep filesystem, threads, and CLI concerns outside the library crates

## Definition Of Done For The Repository

The repository is successful when all of the following are true:

1. A user can run the CLI and play a complete heads-up NLHE hand against a bot.
2. The bot is backed by a real cached strategy produced by the solver stack, not a placeholder policy.
3. The exact rules engine and solver core are exposed as reusable library crates.
4. `gto-core` and `gto-solver` compile for WASM.
5. The default test loop is fast and extensive, with slow work clearly separated.
6. The repo contains enough strategy data or fast-generation tooling that an autonomous agent is not blocked by multi-hour training.

## References Reviewed

- `postflop-solver`: https://github.com/b-inary/postflop-solver
- `TexasSolver`: https://github.com/bupticybee/TexasSolver
- `slumbot2019`: https://github.com/ericgjackson/slumbot2019
- `robopoker`: https://github.com/krukah/robopoker
- OpenSpiel algorithms docs: https://openspiel.readthedocs.io/en/latest/algorithms.html
- OpenSpiel contributing/design docs: https://openspiel.readthedocs.io/en/latest/contributing.html
- OpenSpiel Windows docs: https://openspiel.readthedocs.io/en/stable/windows.html
