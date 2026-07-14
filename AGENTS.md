# NarraState Agent Guidelines

## Architecture Baseline

This project follows `NarraState_PRD_Architecture.md` as its authoritative architecture baseline. All implementation decisions must conform to that document.

## Hard Rules

1. **LLM may not modify authoritative state.** Rust domain logic alone determines state transitions. The model is only for semantic interpretation and natural language rendering.

2. **No auto-confession by contradiction count.** Never implement "if contradictions >= N then confess". Confession must flow through the DisclosureGraph: evidence pressure → phase advancement → disclosure prerequisites met → natural disclosure.

3. **All new invariants require a failing test.** Every constraint added to the domain model must have at least one test that verifies the constraint is enforced.

4. **No silent error swallowing.** Every error path must be explicit, observable, and have a defined fallback strategy.

5. **No scope expansion beyond v0.1.** Do not implement features listed in the "v0.1 explicit not doing" section or later phases.

6. **World truth, character knowledge, and player knowledge are three separate layers.** API responses must be redacted to the player's perspective.

7. **DisclosureGraph is the only path to confession.** A single `is_confessed` boolean must never replace the graph.

8. **narrastate-core must not depend on Axum, SQLx, Reqwest, or any model SDK.** Its only dependencies are serde, schemars, uuid, and thiserror.

9. **All numeric ranges use domain methods with saturating arithmetic.** No raw field mutation scattered across business code.

10. **All ID references must be semantically validated at case load time.** Error messages must include the field path.
