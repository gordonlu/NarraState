# v0.2 案件格式

正式案件是目录包，不是 LLM 响应。最小结构如下：

```text
my-case/
├── manifest.json
└── case.json
```

`manifest.json` 声明稳定案件 ID、语义版本、Schema 版本、默认真相变体、入口文件和资源哈希；其 Schema 位于 [`schemas/narrastate-case-manifest-v0.2.schema.json`](../schemas/narrastate-case-manifest-v0.2.schema.json)。`case.json` 保存共享模板和全部真相变体，Schema 位于 [`schemas/narrastate-case-template-v0.2.schema.json`](../schemas/narrastate-case-template-v0.2.schema.json)。生成型案件还必须包含 `generation-report.json`。

加载器拒绝绝对路径、父目录跳转、符号链接、缺失资源、资源哈希不匹配、manifest 与模板身份不一致，以及任何编译、校验或模拟失败。语义内容使用规范化 JSON 计算 SHA-256；安装路径、空白和对象字段顺序不影响内容哈希。

```bash
cargo run -p narrastate-server -- case validate cases/rain-gallery-variants --json
cargo run -p narrastate-server -- case inspect cases/rain-gallery-variants --json
cargo run -p narrastate-server -- case compile cases/rain-gallery-variants --variant variant-shen --json
```

旧 v0.1 单真相文件可迁移为只含 `classic` 默认变体的 v0.2 包：

```bash
cargo run -p narrastate-server -- case migrate cases/rain-gallery/case.json --output /tmp/rain-gallery-v02
```
