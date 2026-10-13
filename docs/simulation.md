# 自动通关模拟

模拟器使用有界、确定性的广度优先搜索证明每个启用变体至少存在一条合法通关路径。它只执行领域允许的动作：进入场景、发现或出示证据、提出矛盾、推进满足前置条件的披露节点、解锁场景和提交结案。

当结案证据已经齐全且角色已披露部分或完整行为时，模拟器可以执行“决定性追问”。该动作只能让角色沿合法阶段逐级从 `Defensive` 推进到 `Pressured`、`Cornered` 和 `ConfessionEligible`，不能跳级，也不能绕过 DisclosureGraph 自动认罪。

## 可玩性契约

难度通过 `TransitionTuning` 与案件数值调节，放宽门槛时不得触碰认罪入口：

- 一回合出示多条线索必须比只出示一条产生更大压力（`turn_impact_ceiling_step`），否则“把线索一次性全扔出去”会让审讯直接卡死；
- 已出示过的线索再次出示仍保留一小部分压力（`novelty_multiplier_repeat` > 0），玩家不会因为线索用完而走进死路；
- 同一回合内可以连续解锁多个披露节点：每个节点仍必须满足自身前置条件、阶段下限、证据与矛盾要求，认罪节点还必须满足 `ConfessionEligible`、要件齐全和玩家决定性行动；
- 阶段仍然一次只推进一级，认罪资格仍要求 `has_confession_path && elements_complete && action_disclosed`；
- 矛盾数量、压力值或模型文本永远不会直接触发认罪。

`crates/narrastate-case/tests/playability_tests.rs` 用 Mock 解释器完整重放金案，锁定上述契约：逐条出示、批量出示、先听后问都能在有限回合内走到认罪节点，而没有 FullAction 前置线索时即使全部陈述被矛盾、压力拉满也不会认罪。

搜索状态有稳定哈希，并限制最大状态数、回合数和分支数。结果记录访问状态数、回合数、获得的证据、达到的披露节点、结局和可读 trace。模拟不会调用模型，也不会把模型文本当成证据或状态转换。

```bash
cargo run -p narrastate-server -- case simulate cases/rain-gallery-variants --json
cargo run -p narrastate-server -- case simulate cases/rain-gallery-variants --variant variant-lin
```
