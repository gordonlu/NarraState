# 自动通关模拟

模拟器使用有界、确定性的广度优先搜索证明每个启用变体至少存在一条合法通关路径。它只执行领域允许的动作：进入场景、发现或出示证据、提出矛盾、推进满足前置条件的披露节点、解锁场景和提交结案。

搜索状态有稳定哈希，并限制最大状态数、回合数和分支数。结果记录访问状态数、回合数、获得的证据、达到的披露节点、结局和可读 trace。模拟不会调用模型，也不会把模型文本当成证据或状态转换。

```bash
cargo run -p narrastate-server -- case simulate cases/rain-gallery-variants --json
cargo run -p narrastate-server -- case simulate cases/rain-gallery-variants --variant variant-lin
```
