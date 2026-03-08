# AGENTS.md

Start with [PLAN.md](./PLAN.md). It is the source of truth for scope, constraints, architecture, testing, performance policy, and milestone order.

## Golden Rule

Assume that you write a lot of buggy code. You should always write strong tests that fully cover all edge-cases. You should have a testing strategy and consider TDD where appropriate. Write a lot of tests.

## Repo Context

- Goal: pure-Rust heads-up no-limit hold'em solver plus interactive CLI
- Architecture: keep the exact engine and solver reusable as library crates; keep the CLI separate
- Portability: `gto-core` and `gto-solver` must stay WASM compatible
- Dependencies: no native libraries and no poker-specific crates
- Safety: `unsafe` Rust is forbidden everywhere in the repository
- Priorities: correctness first, then maintainability, then reasonable performance

## Working Rules

- Tests are mandatory. New logic needs tests, and bug fixes should add regression tests.
- Default development commands must stay fast. Long training and slow tests must be opt-in and time-bounded.
- Do not require multi-hour offline training for ordinary repo use.
- Keep abstractions explicit and avoid hiding solver assumptions inside the exact rules engine.
- Commit frequently as tests pass; prefer small commits that leave the repo in a working state.
- CI and formatting automation are intentionally out of scope for the first delivery.
