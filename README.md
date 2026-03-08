# Heads up for fun

- Visit https://headsupforfun.com

## About

Pure Rust heads-up no-limit hold'em project with three main surfaces:

- reusable exact game logic in `gto-core`
- reusable abstraction and solving code in `gto-solver`
- playable frontends in the CLI and the browser

The current repository already includes:

- a playable CLI opponent
- a playable single-page web app
- artifact-backed and hybrid bot paths
- smoke training commands and bounded benchmarks
- strong automated test coverage, including browser tests and screenshot capture for UI work

`PLAN.md` is the source of truth for scope, constraints, milestone history, and testing policy. Read that first if you want the full product and engineering context.

## Workspace

The Rust workspace is split into small crates with explicit responsibilities:

- `crates/gto-core`: cards, hand evaluation, exact HU NLHE rules, legal action generation, stack/pot accounting
- `crates/gto-solver`: abstractions, tree building, strategy artifacts, training, runtime bot queries
- `crates/gto-cli`: interactive terminal app and training/build commands
- `crates/gto-web`: `wasm-bindgen` bridge used by the browser worker
- `xtask`: fast, time-bounded developer workflows
- `web/`: Vite + React + TypeScript frontend for playing against the bot in the browser

## Prerequisites

Rust:

```bash
rustup toolchain install stable
rustup target add wasm32-unknown-unknown
```

Web tooling:

- `node` / `npm`
- `wasm-pack`

## Quick Start

Run the fast Rust checks:

```bash
cargo xtask test-fast
cargo xtask check-wasm
```

Play in the CLI:

```bash
cargo run -p gto-cli -- play
```

Useful CLI variants:

```bash
cargo run -p gto-cli -- play --stub-bot
cargo run -p gto-cli -- play --blueprint-bot
cargo run -p gto-cli -- play --hybrid-bot --postflop-profile play
```

Build a blueprint artifact:

```bash
cargo run -p gto-cli -- build-blueprint-artifact
```

Run smoke training:

```bash
cargo xtask train-smoke
```

## Web App

From `web/`:

```bash
npm install
npm run dev
```

Important web commands:

```bash
npm test
npm run test:e2e
npm run build
npm run screenshots
```

The web app uses the `gto-web` WASM crate through a worker-backed browser client. Normal player-facing sessions use the fixed `hybrid-play` bot path.

## Developer Commands

`xtask` wraps the common Rust workflows:

```bash
cargo xtask test-fast
cargo xtask test-slow --timeout-secs 300
cargo xtask test-solver-slow --timeout-secs 300
cargo xtask check-all
cargo xtask train-smoke
cargo xtask train-dev
cargo xtask bench-smoke
```

## Testing

This repo treats tests as a first-class deliverable.

- Rust logic is covered with unit and regression tests
- solver changes are expected to come with correctness or usefulness evidence
- web work is covered with component tests, Playwright browser tests, and screenshot capture
- visual changes are not considered done until the fresh screenshots have been manually reviewed

## Repository Entry Points

- Planning and constraints: [PLAN.md](./PLAN.md)
- Agent-specific repo instructions: [AGENTS.md](./AGENTS.md)
- Web app: [web](./web)
- Rust workspace manifest: [Cargo.toml](./Cargo.toml)
