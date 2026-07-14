# NarraState

**State-driven AI narrative simulation engine** for interactive fiction, detective games, and AI character interrogation.

## Architecture

```
narrastate-core      — Domain model, state machine, case validation
narrastate-runtime   — Turn transaction, action interpreter, evaluator, planner
narrastate-provider  — LLM provider interface (OpenAI-compatible + Mock)
narrastate-storage   — SQLite persistence, event log, snapshots
narrastate-server    — Axum API + SSE, static file hosting
web                  — Vue 3 + Vite + TypeScript frontend
```

The engine follows a strict principle: **Rust determines what happens, the LLM determines how a character expresses it.**

## Current Status

**v0.1 — Phase 1 complete.** The domain core is implemented with:
- Strongly-typed IDs for all entities
- World Fact, Claim, Evidence types
- Character definitions and runtime state with safe range arithmetic
- Interrogation phase machine with legal transition rules
- Disclosure graph with cycle detection and prerequisite validation
- Case definition JSON loading with semantic validation
- JSON Schema generation via schemars
- 67 unit tests + 6 property tests (all passing)

## Quick Start

```bash
# Rust toolchain
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# Frontend
npm --prefix web install
npm --prefix web run typecheck
npm --prefix web test -- --run
```

## License

MIT
