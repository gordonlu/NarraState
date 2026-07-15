# AI 案件生成

生成模型只产生 `GeneratedCaseDraft`，不能直接发布或游玩。Rust 会依次执行请求限制、严格解析、规范化、编译、全部变体校验和确定性模拟；失败最多 Repair 两次，仍失败则保存结构化错误，不安装案件。

配置页支持任意 OpenAI-compatible Base URL、模型和 API Key。Key 可来自 `NARRASTATE_API_KEY`，或保存到服务端 `data/provider.env`；不会进入 SQLite、响应、日志或生成报告。自动测试只使用 `MockCaseGenerationProvider`。

```bash
cargo run -p narrastate-server -- case generate request.json --output cases/generated
```

HTTP 接口：

- `POST /api/v1/case-generation/jobs`
- `GET /api/v1/case-generation/jobs/{job_id}`
- `GET /api/v1/case-generation/jobs/{job_id}/report`

成功案件以普通目录包原子安装。生成报告只保存请求、次数和确定性校验结果，不保存 Key、Authorization Header 或完整系统提示词。
