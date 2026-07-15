# 雨夜画廊：多重真相

这是 v0.2 确定性案件系统的三真相 Golden Package。三个变体共享地点、角色和主要审讯流程，但分别由罗成、沈岸和林岳承担主责任；每个变体会替换责任人角色图、动机/行为事实、决定性证据描述和结局。

```bash
cargo run -p narrastate-server -- case validate cases/rain-gallery-variants
cargo run -p narrastate-server -- case simulate cases/rain-gallery-variants
```

`case.json` 由 `narrastate-case` 的 `generate_three_variant` example 确定性生成。修改生成逻辑后可重新运行：

```bash
cargo run -p narrastate-case --example generate_three_variant -- cases/rain-gallery-variants
```
