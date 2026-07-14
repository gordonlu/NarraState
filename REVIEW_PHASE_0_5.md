# NarraState Phase 0–5 架构与实现审查

审查基线：`NarraState_PRD_Architecture.md`、`AGENTS.md`，代码提交 `37eb1ba`。

状态：**初审完成，Phase 0–5 阻断项与高风险项已修复并复验。**

## 初审结论

当前仓库具备 Phase 0–4 的主要类型和演示骨架，也有数量可观的单元/属性/Golden 测试，但尚不能视为 Phase 5 完成。现有测试主要验证局部类型行为，未覆盖服务真实启动、回合事务原子性、请求幂等、SSE、玩家视角脱敏、指认结案和事件回放。实际执行 `serve` 会因 SQLite repository 在 Tokio runtime 内再次 `block_on` 而立即崩溃。

## 阻断问题（P0）

1. **服务不可启动**：`SqliteRepository` 自建 Tokio runtime，并在 Axum runtime 中同步 `block_on`，触发 `Cannot start a runtime from within a runtime`。
2. **回合提交不原子**：session 更新与 event append 是两个事务；event 写入失败只告警，API 仍返回成功，违反失败原子性和“不得吞数据库错误”。
3. **请求协议缺失并且不幂等**：行动请求没有 `client_action_id`、`expected_revision`、`target_character_id`，也没有持久化幂等结果；重试会产生双回合。
4. **API 泄露内部状态**：普通 `GET session` 返回 phase、stress、composure、trust、defense budget；`GET case` 返回完整隐藏 facts、角色 knowledge、披露图和防御策略。
5. **指认逻辑是假实现**：所有指认结果固定为 `WrongSuspect`，不检查目标、证据要件、玩家已发现证据、认罪状态或结案状态。
6. **无 SSE**：行动接口返回普通 JSON，不存在 PRD 要求的 `turn.accepted → turn.progress → dialogue.delta → state.public_changed → turn.completed`。
7. **无恢复/回放**：snapshot 仅能保存/读取；服务启动和 session 读取不从 snapshot + events 重建状态，事件 payload 也不足以确定性重放。

## 高风险问题（P1）

### 领域与案件校验

- 阶段 API 允许 `Calm → Cornered` 等跨级跳转，与连续阶段和非法跳跃不变量冲突。
- `ClaimInvalidated` prerequisite 错用 `DisclosureId`，并被当成披露图边；运行时从未计算“某 Claim 已失效”。
- 案件校验遗漏多个语义引用和范围：claim owner/fallback、defense IDs 与 fallback、disclosure reveals、discovery rules、entity refs、证据浮点范围/NaN、resilience/belief confidence、初始知识可见性等。
- “可达性”只检查 confession 节点是否有 reveals 或 evidence prerequisite，不是真正从初始知识到结案的规则模拟。
- Demo 的 D1–D6 主要依赖 phase + 前一披露，没有按文档要求为每层声明明确 evidence/claim prerequisite。

### 确定性运行时

- 有效矛盾不要求 action 映射到具体 Claim，也不执行置信度门槛；只要附加 evidence 就会自动推翻它声明的所有 Claim。
- 多证据时用总影响预测 phase，却只把“最大单项影响”写入 state，导致 phase 与数值状态不一致。
- 重复 evidence 仍可能通过 chain bonus 持续生效；已失效 SpokenClaim 也没有被标记。
- `ConfessionEligible` 由 stress/矛盾数量阈值触发，没有检查 required case elements 和 confession 前置披露，违背最核心的认罪合同。
- 披露可解锁性在状态更新前计算，`EvidencePresented`/`ClaimInvalidated` prerequisite 被硬编码为 false。
- Planner 的“本回合新披露”函数是占位实现，永远返回 `None`，因此真实管线不会按新节点生成局部承认。
- defense strategy 没有使用次数状态，也不会消耗/耗尽；active strategy 基本不生效。

### Provider

- 服务组合根始终注入 Mock Interpreter/Renderer，session 也没有 `mode`，真实 provider 没有进入应用管线。
- LLM interpreter 不在解析后执行 ID allow-list，不强制保留附件 evidence，低置信度也不降级。
- renderer 校验失败没有“修复一次再模板”的管线；provider 元数据/token usage/llm_calls 未落库。
- provider crate 没有 G8 级失败/越权测试。

### 存储与 API

- migrations 缺少 `settings`、`llm_calls`，也缺幂等 action result 表。
- schema 无外键；`append_events` 忽略传入的 session ID。
- 默认数据库参数不是可靠的 SQLx SQLite URL；cases loader 只扫一层，默认 `cases/rain-gallery/case.json` 不会被加载。
- 创建 session 只初始化一个角色，无法按 API 在任意嫌疑人间切换。
- 行动接口不校验文本长度、target、已发现 evidence，且内部生成两个不同 TurnId。
- restart 把 revision 重置为 0 后调用 optimistic update，失败只记录 warning，却向客户端返回伪成功。
- 错误格式不是 RFC 9457 Problem Details；缺少 config public/test-provider 接口。

## 中风险问题（P2）

- README 状态仍写 Phase 1；仓库缺 PRD 声明的 MIT `LICENSE`。
- `narrastate-core` 直接依赖 `serde_json`，与 `AGENTS.md` 的 core 依赖白名单不一致；事件 payload 也是无类型 JSON。
- 模板响应只有英文，与内置 `zh-CN` Demo 不一致。
- 现有 Golden G6/G7 注释明确承认没有真正测试 accusation subsystem；G9/G10 未实现。
- 前端仍是 Phase 0 模板；这属于 Phase 6，当前审查不提前实现。

## 修复边界

本轮只修复 Phase 0–5 合同：领域不变量、确定性回合、Provider 安全门、SQLite 原子事务/幂等/快照回放、REST+SSE、脱敏 DTO、指认和恢复，以及相应测试与文档。Phase 6 Web UI 和 Phase 7 发布项不在本轮实现范围。

## 修复复审

### 已关闭

- **领域层**：阶段只能逐级前进；`ClaimInvalidated` 使用 `ClaimId`；披露支持 evidence/claim/phase/disclosure 真实前置；案件校验补齐范围、有限浮点、引用、知识边界与静态可达性；core 的事件 payload 改为强类型，`serde_json` 仅保留为测试依赖。
- **确定性运行时**：统一聚合多证据影响，重复证据不再刷收益；失效 Claim、required case elements、认罪路径、单回合单披露和 defense 使用次数进入状态转移；Planner 能看到本回合新披露并只使用允许事实。
- **Demo**：D1–D6 增加明确 evidence/claim/disclosure 前置，验证命令可自然到达 `ConfessionEligible`，不会跳阶段。
- **Provider**：改为异步调用；附件优先、ID allow-list、低置信度安全降级、renderer 一次修复后模板降级；解析 token usage，并把 prompt hash、延迟、token、状态和错误码记录到 `llm_calls`（不保存 API key 或原始 prompt）。
- **存储**：SQLite repository 全异步；session、事件、幂等结果和周期快照在同一事务提交；乐观并发、G9 请求重试、G10 snapshot + events 回放、外键/约束、settings 与 llm_calls 已实现。
- **服务 API**：真实服务可启动；公开 Case/Session DTO 脱敏；行动请求包含 client action、revision、target 和 attachments；提供有序 SSE；指认区分错误嫌疑人、证据不足、无认罪证明和有认罪证明；错误使用 Problem Details；restart 创建新 session，不伪造 revision 回退。
- **仓库文档**：README 更新到 Phase 5，补充 MIT `LICENSE` 和新的环境变量示例。

### 架构判断

PRD 的总体分层是合适的，不建议在 Phase 5 末尾改成微服务或让 LLM 直接修改状态。当前实现继续保持 `core → runtime ports → provider/storage adapters → server composition root`，事实与结案由 Rust 决定，模型只解释输入和渲染受限计划。后续 Phase 6 应只消费公开 DTO/SSE，不读取内部角色状态或完整案件定义。

### 本轮验证

- `cargo test --workspace`：通过（领域回归、属性测试、Golden、Provider、API、G9/G10 与存储事务测试）。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo run -p narrastate-server -- validate-case cases/rain-gallery/case.json`：通过，可到达 `ConfessionEligible`。
- 真实 `serve` 进程 + `/api/v1/health`：通过。
- `web` 的 `npm run typecheck` 与 `vitest run`：通过。

### 尚未执行但不阻断 Phase 0–5 代码验收

- 未使用真实第三方模型做在线 smoke test，因为本轮没有注入 API key；已用脚本 Provider 覆盖越权 ID、附件权威性、修复与模板降级。
- Phase 6 Web UI 仍是基线模板，按本轮边界不提前实现。
