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
- UI-facing milestones are not done until automated screenshot capture has passed and the newly generated screenshots have been manually opened and inspected on the affected states.

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

#### Visual Validation

- Deterministic screenshot capture for important UI states
- Manual inspection of those screenshots before closing visual/product-facing milestones
- Running the screenshot command alone is insufficient; the captured images must actually be opened and reviewed
- Manual release-preview inspection for layout, hierarchy, readability, and obvious animation issues

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

## Web Follow-On Plan

The next phase extends the existing reusable Rust crates into a static browser experience. This is a follow-on delivery after the CLI milestones above, not a rewrite.

### Web Product Goal

Build a single-page static website where a human can play heads-up no-limit hold'em against the existing bot using the same abstract action menu already supported by the solver stack.

Constraints for this phase:

- No backend service
- Desktop-first, with mobile usability as a secondary concern
- Frontend built with `Vite`, `TypeScript`, and `Tailwind CSS`
- Browser integration through a dedicated Rust WASM adapter crate
- Heavy WASM solver work must run off the main browser thread via a Web Worker
- Production web builds use optimized release-mode WASM artifacts
- Production web play uses the stronger fixed `hybrid-play` bot path in the worker
- Normal player-facing web sessions use internal random seeds; explicit seeding is reserved for tests and debug workflows
- Normal web play uses the shipped persistent-stack match model rather than resetting both players to fresh `100bb` every hand

### Web Architecture Strategy

1. Keep `gto-core` and `gto-solver` free of browser framework concerns.
2. Add a thin `gto-web` Rust crate that exposes a browser-safe session API through `wasm-bindgen`.
3. Run the WASM-backed poker session inside a dedicated Web Worker so heavy bot actions do not block rendering or input.
4. Keep the web UI client-only and statically hosted.
5. Reuse the existing abstract action menu instead of introducing arbitrary bet sizing.
6. Treat runtime postflop solving in the browser as the shipped product path for normal play, with worker-based fallback and error handling kept internal rather than exposed as a player-facing bot selector.
7. Keep deterministic seeding available in the adapter and worker client for tests, screenshots, and debug tooling, but do not expose seed controls in the normal player UI.

### Frontend Testing Strategy

Testing for the web phase should mirror the repo's existing emphasis on deterministic, heavily automated validation.

Recommended layers:

- `Vitest` plus `React Testing Library` for component, hook, and client-state tests
- Worker-focused tests for command/response flow, initialization errors, and typed message handling
- `Playwright` for browser integration tests that play seeded hands end to end
- `Playwright` screenshot capture for stable visual states such as opening hand, hero turn, bot thinking, terminal hand, and recoverable error states
- Deterministic seeded fixtures and debug states so UI and browser tests can reproduce the same poker situations reliably

Principles:

- Keep browser tests deterministic by controlling RNG seeds and using stable bot/action modes
- Test the typed frontend client separately from the React component tree
- Prefer browser integration coverage for real user flows instead of relying only on DOM snapshots
- Add regression coverage for worker startup failures, artifact-load failures, and slow/failed bot action fallbacks
- Require manual visual inspection of captured screenshots and a local browser run for product-facing UI changes
- Treat screenshot review as an explicit test step, not an optional spot check
- Keep the fast frontend test loop small enough for frequent use; reserve heavier Playwright scenarios for a slower explicit lane

Screenshot workflow:

- Provide a dedicated command such as `npm run screenshots`
- Write captured images to a predictable local path such as `web/artifacts/screenshots/`
- Treat screenshot artifacts as a review tool, not a committed golden source by default
- Re-capture and review key states whenever layout, visual hierarchy, card rendering, animation, or typography changes
- The agent should open and inspect the fresh screenshot files after capture so visual regressions are caught before the milestone is considered complete

### Recommended Workspace Layout Update

```text
/
  PLAN.md
  AGENTS.md
  Cargo.toml
  crates/
    gto-core/
    gto-solver/
    gto-cli/
    gto-web/
    xtask/
  fixtures/
    strategies/
    transcripts/
    toy_games/
  web/
```

### `gto-web`

Responsibilities:

- WASM-facing wrapper around `gto-core` and `gto-solver`
- Browser-safe game session API
- JSON-friendly state and action types for the frontend
- Deterministic seeded session creation
- Artifact loading from bytes bundled into or loaded by the frontend
- API shape suitable for message-based use from a Web Worker

Rules:

- No poker logic that duplicates `gto-core` or `gto-solver`
- No `unsafe`
- No direct DOM logic
- Keep the exported API narrow and stable

### Web Milestones

### M14. WASM Adapter Crate And Browser Session API

Deliverables:

- Add `crates/gto-web`
- Expose a session-oriented API through `wasm-bindgen`
- Support creating a seeded game, querying state, listing legal actions, applying human actions, and advancing bot actions
- Provide a browser-safe way to load the default strategy artifact
- Define the API boundary so it can be owned by a dedicated Web Worker
- Keep `gto-core` and `gto-solver` compiling for `wasm32-unknown-unknown`

Validation:

- `gto-web` builds via `wasm-pack`
- Rust tests cover session creation, state progression, and action legality through the WASM adapter boundary
- Existing WASM compile checks still pass for `gto-core` and `gto-solver`

### M15. Vite Frontend Skeleton

Deliverables:

- Add `web/` with `Vite`, `TypeScript`, and `Tailwind CSS`
- Load and initialize the generated WASM package
- Add a dedicated poker Web Worker that owns the WASM session and handles game commands
- Add a thin typed frontend client that communicates with the worker via messages / Promises
- Build a single-page app shell with:
  - table state
  - hero and villain cards where appropriate
  - board cards
  - stacks and pot
  - legal action buttons
  - hand history / action log
- Support local development with no backend

Validation:

- Frontend builds successfully
- Browser app initializes the WASM package and renders the initial game state
- Worker round-trip tests cover command/response flow and initialization failures
- TypeScript component tests or DOM tests cover initial render and basic loading/error states

### M16. Playable Web Vertical Slice

Deliverables:

- Start, reset, and play complete hands in the browser
- Human actions come only from the legal abstract action menu
- Bot actions come from the Rust solver stack through WASM
- Heavy bot actions execute in the dedicated worker rather than on the UI thread
- Seeded sessions are reproducible
- Friendly handling for WASM initialization failures and missing artifacts

Validation:

- Browser integration tests cover at least one complete seeded hand
- Regression tests verify that the web adapter always returns legal actions
- Manual smoke run confirms the site is playable end to end on desktop

### M17. Production Web UX And Stronger Bot Mode

Deliverables:

- Refine the single-page layout for a desktop-first polished experience
- Add the stronger production bot mode equivalent to CLI hybrid `--postflop-profile play`
- Keep any explicit bot/play-mode configuration limited to development, testing, or transitional UX while the game-first product surface is still being shaped
- Build the production site with optimized release-mode WASM artifacts
- Keep the stronger production bot mode behind the worker-based execution path
- Preserve a fallback mode if the stronger browser solver path proves too slow on some environments

Validation:

- Production build completes from a clean checkout
- Manual desktop smoke run confirms that the stronger production bot mode is functional
- Browser-facing tests cover mode selection and fallback behavior

### M18. Game-First Web Product Redesign

Deliverables:

- Rework the web UI from a dashboard-style tool into a game-first poker table experience
- Add a better page title, browser metadata, favicon, and in-product iconography aligned with the poker theme
- Remove player-facing seed controls, solver terminology, worker terminology, and bot-mode selection from the normal game screen
- Fix the shipped web experience to the stronger `hybrid-play` bot path
- Generate random seeds internally for normal play while preserving deterministic seed hooks for tests and screenshots
- Center each player's identity, stack, and cards into compact player panels instead of splitting them across the table edges
- Remove the hand-status panel from the main game screen
- Reduce action history to a secondary surface instead of a primary dashboard panel
- Keep the current per-hand fresh `100bb` reset model for now; persistent cross-hand stack carry remains deferred

Validation:

- Browser tests confirm the normal player UI no longer exposes seed or bot-mode controls
- Screenshot capture covers opening-hand, player-turn, terminal-hand, and error states after the redesign
- Manual visual inspection confirms table-first hierarchy, centered player presentation, and no obvious layout collisions or overlap bugs

### M19. Visual Cards, Typography, And Bot Presence

Deliverables:

- Replace string-based card rendering with open-source visual card assets
- Record the source and license for any adopted card-face and card-back assets
- Replace the current serif typography with a cleaner game-appropriate font system
- Improve spatial layout so player information reads as one centered unit near each seat
- Add visible bot thinking feedback near the bot panel
- Add a visible bot action reveal near the bot panel before control returns to the player
- Add restrained animation for thinking, action reveal, and street/card transitions without turning the experience into a noisy casino effect

Validation:

- Component and browser tests cover bot-thinking, bot-action, and return-to-player state transitions
- Screenshot capture covers visual-card states, bot-thinking state, and bot-action state
- Manual visual inspection confirms card readability, motion restraint, and a more game-like presentation

### M20. Compact Table Polish And Bot Feedback Cleanup

Deliverables:

- Tighten the overall desktop layout so the full table experience fits more comfortably on common laptop screens without losing the game-first hierarchy
- Make the hand-recap disclosure affordance more legible, including a larger expand/collapse icon
- Fix card-face rendering so card art is not visibly clipped by container radius or inner sizing
- Simplify the action tray by removing redundant descriptive copy and presenting the available actions as a cleaner horizontal control row
- Keep the bot action bubble visible until the human makes the next move instead of auto-dismissing it on a short timer
- Reuse the same bubble treatment for bot thinking feedback so the bot panel does not shift when the thinking state appears
- Reduce layout shift and visual jumping during bot response transitions

Validation:

- Component and browser tests cover persistent bot feedback from bot action through the next human decision
- Screenshot capture covers opening-hand, bot-thinking, bot-action, and terminal states after the compact pass
- Manual visual inspection confirms:
  - the layout fits more comfortably on a typical desktop viewport
  - the hand-recap toggle is clearly legible
  - cards are no longer visibly cropped
  - the action tray reads as a concise play control, not an information panel
  - bot feedback no longer introduces obvious seat-panel layout shift

Non-goals for the web phase:

- Adding a backend or online multiplayer
- Introducing arbitrary chip-size input
- Replacing the reusable Rust solver core with frontend logic
- A separate web-only performance milestone; responsiveness should instead be validated inside the playable web milestones above
- Persisting match-result history across page reloads, devices, or accounts; page-level match statistics may remain in-memory only unless a later milestone says otherwise

## Post-M20 Follow-On Plan

These milestones extend the playable product in the most practical order:

1. Widen the abstract action menu first, because it improves play quality and user agency with relatively low architectural risk.
2. Add proper alternating blind/button flow across repeated hands next, because it is mostly session work and should be shared across CLI and web.
3. Treat persistent cross-hand stacks as a separate multi-milestone effort, because it changes both the exact engine/session model and the solver assumptions around fixed `100bb` starting states.

### M21. Planned Action Menu Expansion And Consistent All-In Availability

Goal:

- Bring the exposed action menu closer to the originally planned abstraction so the human and bot both have a more credible no-limit menu.

Deliverables:

- Expand the shipped abstraction profiles so the supported actions match the intended menu more closely:
  - Preflop unopened: `fold`, `call`, `2.5bb`, `4bb`, `7bb`, `all-in`
  - Postflop unopened: `check`, `33% pot`, `66% pot`, `100% pot`, `all-in`
  - Facing a bet: `fold`, `call`, `2.5x raise`, `all-in`
- Ensure the CLI and web UI both surface `all-in` whenever it is legal under the exact engine and included by the active abstraction profile
- Align blueprint and hybrid bot menu exposure so the human-facing action set is consistent with the bot/query layer
- Regenerate or update the bundled small default strategy artifact to match the widened abstraction

Validation:

- Exact-engine tests prove all-in remains legal exactly when expected for short-stack, full-raise, and capped-raise spots
- Abstraction tests prove the widened menu only emits legal actions and does not duplicate all-in lines
- CLI transcript tests cover the expanded preflop and postflop menus
- Browser tests cover widened action menus and explicit all-in availability in representative states
- Manual smoke run in CLI and web confirms the player can actually choose the richer menu without broken labels or missing actions

Risk notes:

- This is primarily abstraction/profile/artifact work, not a fundamental solver rewrite
- It may modestly increase browser and runtime-solver latency because the decision tree is wider

### M22. Shared Match Session With Alternating Button And Big Blind

Goal:

- Make repeated play feel like a real heads-up match by alternating who is on the button and who is in the big blind each hand.

Deliverables:

- Add an explicit reusable match/session layer above a single `HoldemHandState`
- Alternate button and big blind assignment every hand in both CLI and web
- Keep fresh per-hand `100bb` stacks for this milestone; stack carry remains out of scope here
- Unify hand-start role assignment so CLI and web do not drift in seating behavior
- Update transcript/history rendering so player labels remain clear as seats alternate

Validation:

- Unit and integration tests prove seat assignment alternates deterministically across hands
- CLI transcript tests cover at least two hands with role rotation
- Web session tests cover at least two hands with role rotation
- Browser tests confirm the visible player/dealer indicators update correctly between hands

Risk notes:

- Low solver risk by itself
- This milestone should land before persistent stacks so the match/session boundary is established first

### M23. Persistent Match Bankrolls With Unequal Starting Stacks

Goal:

- Carry chips across hands so wins and losses change the next hand's starting stacks instead of resetting to fresh `100bb` each hand.

Deliverables:

- Extend the exact heads-up engine so a new hand can start from unequal per-player stacks rather than a single symmetric `starting_stack`
- Add match-level bankroll accounting across hands for both CLI and web
- Define heads-up match rules for:
  - blind posting under short stacks
  - all-in blind posting when necessary
  - match termination when one player is bust
- Preserve correctness for unequal-stack hands from blind posting through showdown

Validation:

- Exact-engine tests for unequal-stack hand starts, short blind posts, forced all-in blind posts, bust-out detection, and bankroll conservation across hands
- CLI integration tests cover multi-hand bankroll carry
- Web session tests cover bankroll carry and match-end states
- Soak tests confirm no invalid states emerge from repeated uneven-stack hands

Risk notes:

- This is the first milestone that materially changes the exact engine model, not just the UI/session layer
- It should be completed before attempting to claim stack-aware solver behavior

### M24. Stack-Aware Strategy Compatibility And Solver Follow-Through

Goal:

- Make the bot architecture honest under persistent stacks instead of merely reusing the old fixed-`100bb` strategy everywhere.
- Lock in the intended long-term product architecture:
  - stack-aware offline preflop blueprinting
  - runtime postflop solving

Deliverables:

- Review and update the blueprint and hybrid bot architecture so strategy selection accounts for uneven effective stacks
- Keep preflop artifact/blueprint-first rather than introducing true runtime preflop solving
- Extend preflop policy/context modeling to include coarse effective-stack-depth information instead of assuming one fixed-stack regime
- Introduce explicit effective-stack buckets for preflop strategy selection, for example:
  - `<=15bb`
  - `16-25bb`
  - `26-40bb`
  - `41-75bb`
  - `76bb+`
- Update postflop runtime-solver spot construction so scripted spots and rebuilt states remain valid under unequal starting stacks
- Define the stack-aware hybrid architecture explicitly:
  - preflop decisions come from offline generated blueprint artifacts keyed by coarse stack-depth context
  - postflop decisions continue to use the hybrid runtime solver path
- Decide and document the approximation policy for unsupported or thinly covered stack depths, such as mapping them into the nearest coarse depth band with explicit fallback behavior
- Version any affected strategy artifacts or policy schemas if compatibility changes are required

Validation:

- Regression tests prove stack-aware policy lookup works across multiple depth bands
- Runtime postflop solver tests cover unequal-stack spots reconstructed from live match state
- Artifact compatibility tests distinguish older fixed-stack artifacts from newer stack-aware ones
- Training/build tests prove stack-aware preflop blueprint artifacts can still be generated in bounded `smoke` form for agent workflows
- Manual smoke runs in CLI and web confirm the bot remains playable through a multi-hand uneven-stack match

Risk notes:

- This is the highest solver-architecture-risk milestone in the current roadmap
- Preflop is the hardest area because the current blueprint context is not stack-aware
- Runtime preflop solving remains out of scope unless a later milestone explicitly revisits that decision
- If a full stack-aware preflop blueprint is too expensive immediately, the depth-bucketing approximation and fallback policy must be explicit and tested rather than implicit

## Post-M24 Follow-On Plan

The milestones above are already reflected in the current shipped product state. The next planned work is:

1. Add restrained web audio next, because it improves feel without changing solver or match architecture.
2. Add a page-local match win/loss counter after that, because it improves repeated-play feedback while staying intentionally non-persistent across page reloads.

### M25. Web Table Audio And Action Sound Effects

Goal:

- Add restrained table audio that makes the browser game feel more responsive without turning it into a noisy casino effect.

Deliverables:

- Add browser-safe SFX playback for important game events, at minimum:
  - dealing the flop
  - the bot making or revealing an action
- Optionally add a few additional cues only if they stay restrained and clearly improve state readability.
- Source the initial audio set from the MIT-licensed `jacks-or-better` audio assets at `https://github.com/murbar/jacks-or-better/tree/master/src/audio`, or from clearly documented derivatives of those files.
- Record the adopted audio-file source, license, and any edits in a checked-in asset/source note so attribution is not implicit.
- Keep audio behavior in the web layer so Rust engine and solver crates remain free of presentation concerns.
- Ensure the page degrades gracefully when audio is unavailable, blocked by autoplay rules, or explicitly muted.

Validation:

- Component and browser tests cover the required event-to-sound mappings for flop dealing and bot action reveal.
- Regression tests prove sounds do not double-fire on rerender, replay, or worker/client state resync.
- Manual browser smoke run confirms the cues are audible, restrained, and correctly timed.
- Manual review confirms the repo contains the promised source and license reference for the adopted audio assets.

### M26. Page-Local Match Win/Loss Counter

Goal:

- Give repeated web play a lightweight match record that reflects completed matches without changing the already shipped persistent-stack match model.

Deliverables:

- Add a visible page-local counter that tracks how many full matches the human player has won versus lost during the current page load.
- Increment the counter only when the existing match-end rules declare a winner, rather than after every hand.
- Keep the counter in browser memory only; a full page reload clears it and no local-storage persistence is required.
- Preserve the displayed totals across successive in-app match restarts during the same page session.
- Display the counter somewhere in the page chrome where it is easy to find but does not compete with the table itself.

Validation:

- Component and browser tests cover counter updates after a completed player match win and player match loss.
- Regression tests prove ordinary hand results inside an ongoing match do not change the match totals.
- Regression tests prove the totals persist across in-app new-match flows but reset on a fresh page load.
- Manual smoke run confirms the displayed totals remain correct across multiple completed matches in one browser session.

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
- `jacks-or-better` audio assets: https://github.com/murbar/jacks-or-better/tree/master/src/audio
- OpenSpiel algorithms docs: https://openspiel.readthedocs.io/en/latest/algorithms.html
- OpenSpiel contributing/design docs: https://openspiel.readthedocs.io/en/latest/contributing.html
- OpenSpiel Windows docs: https://openspiel.readthedocs.io/en/stable/windows.html
