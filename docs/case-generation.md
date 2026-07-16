# AI 案件生成

生成模型只产生非权威草案，不能直接发布或游玩。为了避免一次输出完整多真相案件造成截断，OpenAI-compatible Provider 使用分段生成：

```text
案件蓝图（稳定 ID、角色和真相计划）
        ↓
共享人物、事实与公共线索
        ↓
每个真相变体独立生成（最多并发 3 个）
        ↓
Rust 确定性组装 GeneratedCaseDraft
        ↓
规范化、编译、全部变体校验和确定性模拟
```

通常文本模型调用次数为 `2 + 真相变体数量`。任一分段发生 JSON 截断或结构错误时，只重试该分段；校验发现单个真相有问题时，也只修复对应变体。共享内容改变后会重新生成受其影响的变体。只有冻结元数据本身损坏时才回退到整包修复。修复总轮数仍受 Rust `GenerationLimits` 限制，超过限制不会安装案件。

生成请求限制为 2–4 名主要角色，短篇时长会进一步降低上限。已知的机械性 Schema 泄漏（例如模型将披露阶段输出为字面值 `"enum"`）会先由 Rust 做确定性本地规范化，之后仍须通过完整编译、校验和模拟；这类错误不再额外消耗两次模型调用。

蓝图确定后，Rust 会拒绝后续分段修改角色 ID、公开身份、真相变体 ID 或责任人绑定。协议 Schema 版本、初始案件版本和语言不由模型输出，Rust 分别固定为 `0.2`、`1.0.0` 和原始请求语言。最终完整草案仍必须通过请求限制、严格解析、规范化、编译、全部变体校验和确定性模拟。

配置页支持任意 OpenAI-compatible Base URL、模型和 API Key。Key 可来自 `NARRASTATE_API_KEY`，或保存到服务端 `data/provider.env`；不会进入 SQLite、响应、日志或生成报告。自动测试只使用 `MockCaseGenerationProvider`。

案件生成的每个分段模型请求默认超时为 180 秒，与普通对话的短请求分开。较慢的兼容服务可以在启动服务前通过 `NARRASTATE_GENERATION_TIMEOUT_SECS` 调整为 30–900 秒。

每个分段的结构化输出预算默认为 65536 token。可通过 `NARRASTATE_GENERATION_MAX_TOKENS` 在 4096–65536 之间调低；普通对话继续使用较小预算。模型或兼容服务仍可能应用自身更低的输出上限；发生截断时 Provider 会自动再请求一次更紧凑的当前分段。

生成任务会持久化蓝图、共享内容、变体完成数量、组装、局部修复和可选配图阶段。刷新页面后仍可从任务接口读取已保存的阶段；Web UI 同时显示已等待秒数。

可选图片只在文本案件完成编译、校验和模拟后调用独立图片 Provider。默认图片集合为 5 张公共氛围图、最多 6 张地点图和每名角色 1 张头像，最多并发 3 个请求。图片失败只记录警告，不会改变案件是否可安装。

```bash
cargo run -p narrastate-server -- case generate request.json --output cases/generated
```

HTTP 接口：

- `POST /api/v1/case-generation/jobs`
- `GET /api/v1/case-generation/jobs/{job_id}`
- `GET /api/v1/case-generation/jobs/{job_id}/report`

成功案件以普通目录包原子安装。生成报告只保存请求、次数和确定性校验结果，不保存 Key、Authorization Header 或完整系统提示词。
