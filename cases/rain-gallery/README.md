# 雨夜画廊失窃案

NarraState v0.1 的内置完整示例。它包含三名角色、17 项事实、7 项证据，以及罗成从承认离开控制室到完整认罪的 D1–D6 披露路径。

```bash
cargo run -p narrastate-server -- validate-case cases/rain-gallery/case.json
cargo run -p narrastate-server -- play --case rain-gallery --mock
```

案件文件包含完整真相和披露条件，不应直接提供给普通玩家。服务端只通过脱敏 DTO 返回玩家已经获知的内容。编写新案件请阅读 [`docs/case-authoring.md`](../../docs/case-authoring.md)。
