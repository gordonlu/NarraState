# NarraState

NarraState 是一个状态驱动的 AI 互动叙事运行时。Rust 决定事实、证据影响、角色阶段、披露与结案；LLM 只负责把受约束的行动解释和对话计划渲染成自然语言。

当前实现完成到 **Phase 5：存储与服务 API**：

- 强类型领域模型、案件语义校验与逐级审讯状态机；
- `DisclosureGraph` 驱动的 D1–D6 自然披露路径；
- 无需模型即可运行的雨夜画廊 Demo；
- OpenAI-compatible interpreter/renderer、ID allow-list、一次修复与模板降级；
- SQLite 事件日志、周期快照、乐观并发、原子回合提交、请求幂等和确定性恢复；
- Axum REST + SSE、Problem Details、玩家视角脱敏和证据要件指认。

Phase 6 Web UI 尚未实现；`web/` 当前只保留通过 typecheck/test 的 Vue 3 基线工程。

## 快速开始

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# 校验内置案件
cargo run -p narrastate-server -- validate-case cases/rain-gallery/case.json

# 无模型 CLI 演示
cargo run -p narrastate-server -- play --case rain-gallery --mock

# 本地 API（默认 127.0.0.1:3000）
cargo run -p narrastate-server -- serve --db narrastate.db --cases cases
```

前端基线验证：

```bash
npm --prefix web ci
npm --prefix web run typecheck
npm --prefix web test -- --run
```

## 模型配置

复制 `.env.example` 中的值到启动环境。API Key 只从 `NARRASTATE_API_KEY` 或 `OPENAI_API_KEY` 读取，不写入 SQLite、事件或日志。没有 Key 时使用 `mock` session；`llm` session 会安全降级，不会修改关键状态。

## API

统一前缀为 `/api/v1`。主要接口：

- `GET /health`、`GET /config/public`、`POST /config/test-provider`
- `GET /cases`、`GET /cases/{case_id}`、`POST /cases/validate`
- `POST /sessions`、`GET /sessions/{session_id}`、`GET /sessions/{session_id}/events`
- `POST /sessions/{session_id}/actions`（SSE）
- `POST /sessions/{session_id}/accusations`、`POST /sessions/{session_id}/restart`

普通 API 不返回隐藏事实、责任人标记、内部数值、未解锁披露图或防御资源。

## 架构边界

```text
narrastate-core      领域类型、不变量、案件校验
narrastate-runtime   证据评估、状态转换、对话计划、ports
narrastate-provider  OpenAI-compatible provider 与输出防护
narrastate-storage   SQLite、事件、快照、恢复、幂等事务
narrastate-server    Axum API、SSE、DTO 脱敏、组合根
web                  Phase 6 前端基线
```

详细产品和架构合同见 `NarraState_PRD_Architecture.md`；本轮审查记录见 `REVIEW_PHASE_0_5.md`。

## License

MIT
