# NarraState Agent Guidelines

## Architecture Baseline

The implemented architecture and dependency boundaries are documented in `docs/architecture.md`. New development must preserve that document's current-state invariants and the hard rules below.

## Hard Rules

1. **LLM may not modify authoritative state.** Rust domain logic alone determines state transitions. The model is only for semantic interpretation and natural language rendering.

2. **No auto-confession by contradiction count.** Never implement "if contradictions >= N then confess". Confession must flow through the DisclosureGraph: evidence pressure → phase advancement → disclosure prerequisites met → natural disclosure.

3. **All new invariants require a failing test.** Every constraint added to the domain model must have at least one test that verifies the constraint is enforced.

4. **No silent error swallowing.** Every error path must be explicit, observable, and have a defined fallback strategy.

5. **World truth, character knowledge, and player knowledge are three separate layers.** API responses must be redacted to the player's perspective.

6. **DisclosureGraph is the only path to confession.** A single `is_confessed` boolean must never replace the graph.

7. **narrastate-core must not depend on Axum, SQLx, Reqwest, or any model SDK.** Its only dependencies are serde, schemars, uuid, and thiserror.

8. **All numeric ranges use domain methods with saturating arithmetic.** No raw field mutation scattered across business code.

9. **All ID references must be semantically validated at case load time.** Error messages must include the field path.

10. **Generated drafts are never playable content.** Every draft must pass Rust normalization, compilation, validation, and deterministic simulation before package installation.

11. **Sessions load immutable case instances.** New sessions must reference a frozen compiled instance; they may not reload the latest case file by `case_id` during play or recovery.

12. **Variant selection is deterministic and private.** The selector algorithm is versioned, a fixed seed must reproduce the same result, and normal player DTOs must not reveal the variant or responsible character.

13. **Golden cases are static.** They may be authored with AI assistance, but CI and compiler/simulator tests must consume committed files without calling an LLM.

## Helping a User Start NarraState

When a user asks an agent to start, run, or try the application:

1. From the repository root, use `./start.sh` on Linux/macOS or `.\start.ps1` on Windows. Do not invent a different startup sequence unless this entry point fails.
2. Wait for the server to report that it is listening, then give the user `http://127.0.0.1:3000`.
3. Never ask the user to paste an API Key into chat, a command argument, source code, or a tracked file.
4. Tell the user to open **设置** in the Web UI and enter the OpenAI-compatible Base URL, model, and API Key there. The server persists it locally in `data/provider.env` and never returns the plaintext value.
5. Text and image providers are configured independently. Do not reuse one provider's key for the other unless the user explicitly chooses to do so in the UI.
6. If no provider is configured, start the app normally and explain that Mock mode remains available.
7. If a prerequisite is missing, report the exact missing command (`npm` or `cargo`) and the official installation requirement. Do not silently switch to Docker or install system software without permission.
