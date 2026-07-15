# Case authoring guide

> 本页说明旧 v0.1 固定真相格式。v0.2 多真相案件请同时阅读 [案件格式](case-format.md)、[真相变体设计](solution-variants.md)、[确定性校验](validation.md) 和 [自动通关模拟](simulation.md)。

NarraState v0.1 accepts one canonical JSON format. The generated schema is [`schemas/narrastate-case.schema.json`](../schemas/narrastate-case.schema.json), and the complete reference case is [`cases/rain-gallery/case.json`](../cases/rain-gallery/case.json).

## Workflow

1. Copy the reference case into a new directory under `cases/`.
2. Give every entity, fact, evidence item, character, claim, defense, and disclosure a stable, descriptive ID.
3. Define world facts independently of what the player initially knows.
4. Define claims and the evidence that invalidates them.
5. Build each responsible character's disclosure graph from peripheral admissions through `PartialAction` or `FullAction` to at most one `Confession` node.
6. Ensure discoverable evidence covers every `required_case_elements` entry.
7. Validate the file and run the test suite.

```bash
cargo run -p narrastate-server -- validate-case cases/my-case/case.json
cargo run -p narrastate-server -- generate-schema
cargo test --workspace
```

## Top-level fields

- `schema_version`: must be `"0.1"` for v0.1 content.
- `id`, `title`, `summary`, `locale`: stable identity and public metadata.
- `required_case_elements`: any subset of `Identity`, `Opportunity`, `Means`, `Action`, `Intent`, and `Concealment` needed to prove the case.
- `entities`: referenced people, locations, and objects.
- `facts`: objective propositions with truth and visibility.
- `evidence`: player-presentable records and their deterministic relationships.
- `characters`: public profile, limited knowledge, claims, defenses, and disclosure graph.
- `initial_player_knowledge`: only public-at-start fact and evidence IDs.
- `ending`: text revealed after resolution.

## Evidence and claims

Evidence strength fields `reliability`, `directness`, and `exclusivity` must be finite values from 0 to 1. An effective contradiction requires an evidence item whose `contradicts` list contains a claim owned by the target character. Merely sharing words or topics is not enough.

Discovery rules are tagged JSON objects such as `{"type":"StartingEvidence"}`. Use the generated schema for every supported variant and exact serialization shape.

## Disclosure graph

Every disclosure lists facts it reveals, its minimum phase, response intent, and explicit prerequisites. The graph must be acyclic. A confession must depend on a prior `PartialAction` or `FullAction`-equivalent path, and non-responsible characters must not contain a main-crime confession.

Design the graph so that one valid turn unlocks one meaningful admission. Strong evidence presented out of order may create pressure, but the prerequisite chain must still prevent skipped disclosures.

## Validation failures

Validation reports semantic field paths, for example:

```text
characters[1].disclosure_graph.nodes[4]: prerequisite disclosure "admit_access" does not exist
```

Fix all structural and reachability errors. Do not weaken validation or hide required evidence merely to make a case load.
