# NarraState v0.1

## 产品需求与架构设计文档

> **项目定位**：状态驱动的 AI 互动叙事模拟引擎。  
> **首个可玩示例**：单人、文本为主的侦探审讯游戏。  
> **文档用途**：本文件是 NarraState v0.1 的产品基线、架构基线和 coding agents 实施合同。未在本文明确列入 v0.1 的能力，不应由实现 Agent 自行扩展。
> **上下文约束**：本文件必须可由全新 Agent 进程独立执行，不依赖任何历史仓库、既有项目或先前对话。

---

## 0. 文档元信息

| 字段 | 内容 |
|---|---|
| 项目名 | NarraState |
| 版本 | v0.1 Architecture Baseline |
| 状态 | 可执行、自包含实施基线 |
| 开源许可 | MIT（代码）；示例案件内容同仓库许可 |
| 默认运行形态 | 本地 Rust 服务 + 浏览器 Web UI |
| 默认部署方式 | 本地启动；同时提供 Docker Compose |
| 默认数据库 | SQLite |
| 默认前端 | Vue 3 + Vite + TypeScript |
| 默认后端 | Rust + Axum + Tokio |
| 默认模型接入 | OpenAI-compatible API；DeepSeek 可作为默认示例配置 |
| 明确不采用 | Netlify Functions、纯前端直连模型、通用桌面 Agent 后端、开放式多 Agent 自主循环 |

---

# 1. 执行结论

## 1.1 采用 Greenfield 实现

NarraState v0.1 按全新仓库和全新领域模型实施。Agent 不得假设存在任何可复用的旧代码、旧数据库或旧 API，也不得为未声明的兼容需求增加适配层。

以下实现方式明确禁止：

- 由单个大型前端状态类承担案件规则、角色状态和流程推进；
- 依赖关键词、同义词或字符重叠判定玩家是否指出矛盾；
- 以“命中次数”或笼统进度百分比触发认罪；
- 把责任人标签、全部隐藏事实或“现在请认罪”之类控制指令直接交给对话模型；
- 由浏览器端承担权威游戏状态；
- 从任意文本中通过正则修补 JSON 后作为权威案件数据；
- 纯前端或 Serverless 运行时设计。

## 1.2 Rust 后端是必要的，但只实现叙事运行时

NarraState 不引入通用桌面 Agent、工具权限、任务调度或操作系统自动化能力。后端只承担：

1. 权威世界状态；
2. 角色心智状态；
3. 玩家行动解释；
4. 确定性状态转换；
5. 对话计划与模型渲染；
6. 会话持久化与回放；
7. 面向前端的 API/SSE。

## 1.3 核心原则

> **Rust 决定发生什么，LLM 决定角色如何表达。**

模型不得直接：

- 判定证据是否成立；
- 修改世界真相；
- 改变角色阶段；
- 决定玩家是否胜利；
- 跳过披露层级直接认罪；
- 发明影响案件成立的新事实。

---

# 2. 背景与问题定义

常见的 Prompt 驱动式 AI 审讯实现，其关键缺陷不是模型回答质量，而是缺少可执行叙事状态：

```text
玩家输入
  -> 关键词命中
  -> 累计矛盾数量
  -> 达到阈值
  -> Prompt 强制认罪
```

这会产生两种同样糟糕的体验：

1. **死不认罪**：角色提示要求隐藏罪行，模型持续防御，即使玩家已经给出完整证据链。
2. **突然认罪**：系统达到数量阈值后，把角色从“坚决否认”直接切换为“完整认罪”，玩家看不到心理和事实层面的过渡。

根因包括：

- 世界事实、角色知识和玩家知识混在同一个 Prompt 中；
- “角色是凶手”只是文本标签，不是可计算状态；
- 没有角色防御资源、压力、信任、情绪和披露进度；
- 没有“承认在场”“承认接触”“承认行为”“承认意图”“完整认罪”的层级；
- LLM 同时充当裁判、状态机和演员；
- 前端关键词匹配无法理解玩家实际提出的证据链。

NarraState v0.1 必须证明：

> 同一个有罪角色可以在证据逐步加压下，以连续、可解释、不可跳跃的方式从否认走向局部承认，最终在证据要件闭合后自然认罪。

---

# 3. 产品愿景与定位

## 3.1 一句话定位

**NarraState is a state-driven narrative simulation engine for persistent AI characters and interactive worlds.**

中文：

**NarraState 是一个状态驱动的 AI 叙事模拟引擎，用于构建拥有持续世界状态、有限认知和连续行为的 AI 角色。**

## 3.2 v0.1 产品形态

v0.1 是一个开发者可运行、普通用户可体验的开源项目，包含：

- 一套独立 Rust 叙事运行时；
- 一个本地 Web UI；
- 一个手工编写、可完整通关的精品侦探案件；
- 可替换的模型提供商；
- 状态变化可视化的开发者模式；
- 完整测试和案件格式说明。

## 3.3 目标用户

### 玩家

希望体验 20–40 分钟的自由提问式推理，而不是选择题或关键词猜谜。

### 开发者

希望研究或复用：

- AI NPC 状态机；
- 世界真相与角色认知隔离；
- 确定性逻辑和生成式语言的组合；
- 可回放的互动叙事运行时。

### 案件作者

v0.1 通过手写 JSON 创建案件；不提供图形化编辑器，但必须提供 schema、示例和校验命令。

---

# 4. 产品目标与非目标

## 4.1 v0.1 必须完成

1. 玩家可以读取案件简报、证据、人物和时间线。
2. 玩家可以自由输入中文问题审讯任意嫌疑人。
3. 系统能把自然语言问题解释为结构化玩家行动。
4. 角色只能基于自身知识、当前策略和允许披露内容回答。
5. 玩家提交证据后，系统确定性计算证据影响。
6. 有罪角色会经历连续阶段，不得从正常否认直接跳到完整认罪。
7. 角色可以先承认边缘事实或部分行为，而不等同于完整认罪。
8. 玩家可以随时指认嫌疑人，但指认正确不代表证据充分。
9. 游戏状态由 Rust 后端权威保存，可刷新、恢复和回放。
10. 模型失败时游戏不丢状态，并给出可继续的降级响应。
11. 项目可通过本地命令启动，不依赖 Netlify、Vercel 或外部数据库。
12. 核心状态机在无真实 LLM 的测试环境下可完整运行。

## 4.2 v0.1 明确不做

- AI 自动生成无限案件；
- 多人联机；
- 开放世界；
- 导演 Agent 自主改写主线；
- 多 Agent 侦探团；
- 角色自主离线行动；
- 语音、视频、3D、人物立绘生成；
- 用户账号、云同步、排行榜、付费；
- RAG、向量数据库和 embeddings；
- 完整情绪心理学模拟；
- 图形化案件编辑器；
- 将任何外部项目专用运行时、记忆层或代理框架作为必需依赖。

这些能力只能列入后续路线，不得阻塞 v0.1。

---

# 5. 设计原则

## 5.1 权威状态不在模型里

每个会话的权威状态只能由 Rust 数据结构表示并持久化。Prompt 是当前状态的投影，不是状态本身。

## 5.2 世界真相、角色认知、玩家认知三层隔离

```text
World Truth
  ├─ 客观发生的事实
  ├─ 事实间关系
  └─ 案件成立条件

Character Mind
  ├─ 该角色知道什么
  ├─ 该角色相信什么
  ├─ 该角色撒了哪些谎
  └─ 该角色准备如何防御

Player Knowledge
  ├─ 已公开简报
  ├─ 已发现证据
  ├─ 已听到陈述
  └─ 玩家尚未证明的推断
```

任何 API 返回给前端的数据必须经过玩家视角脱敏。

## 5.3 状态转换可解释

每次状态更新必须产生机器可读的 `TransitionReason`，例如：

- `new_evidence_presented`；
- `prior_claim_contradicted`；
- `defense_exhausted`；
- `disclosure_prerequisites_met`；
- `repeated_question_no_new_information`。

开发者模式可以显示原因，但正常玩家模式不显示内部数值。

## 5.4 披露是图，不是单个布尔值

角色不是只有“未认罪/已认罪”。每个重要秘密由 `DisclosureGraph` 表示，节点可包含：

- 承认与案件无关的秘密；
- 承认在场；
- 承认接触关键物品；
- 承认拥有手段；
- 承认实施行为；
- 承认动机或预谋；
- 完整认罪。

每次玩家行动最多解锁并表达一个主要披露节点，防止剧情跳跃。

## 5.5 模型输出必须受约束和验证

模型输出使用结构化 JSON。运行时验证：

- 是否引用不存在的事实；
- 是否泄露尚未允许的披露节点；
- 是否声称状态发生了模型无权决定的变化；
- 是否出现身份泄露、系统提示、越权内容；
- 是否添加案件成立所需的新事实。

失败时最多修复重试一次，随后使用确定性模板降级。

## 5.6 首先保证可玩闭环，再扩展引擎通用性

核心类型应避免写死“凶手”，但 v0.1 的 UI 和样例可以围绕侦探审讯。不得为了未来所有玩法引入抽象工厂、插件系统或复杂 ECS。

---

# 6. 核心用户流程

## 6.1 启动与配置

1. 用户启动本地服务。
2. 首次进入设置模型提供商、Base URL、模型名和 API Key。
3. API Key 只提交给 Rust 后端并存储在本地配置中；前端不回显完整密钥。
4. 用户执行模型连通性测试。
5. 即使没有模型配置，也允许进入“Mock 演示模式”，用于测试状态机。

## 6.2 开始案件

1. 选择内置案件。
2. 阅读案件简报。
3. 查看初始人物、时间线和公开证据。
4. 创建新会话。
5. 进入调查界面。

## 6.3 审讯回合

1. 选择嫌疑人。
2. 输入问题，可选择附上一条或多条已发现证据。
3. 后端解释玩家行动。
4. 后端评估是否提出有效矛盾或有效证据链。
5. 后端更新角色状态和披露资格。
6. 后端生成对话计划。
7. LLM 只根据计划渲染角色回答。
8. 前端流式显示回答，并更新证据板、陈述记录和可见状态。

## 6.4 指认与结案

玩家可以随时：

- 指认某个嫌疑人；
- 选择指控要件；
- 选择支撑证据；
- 提交推理说明。

结案结果分为：

1. `wrong_suspect`：对象错误；
2. `correct_but_insufficient`：对象正确，但证据链未覆盖关键要件；
3. `case_proven_without_confession`：证据充分，即使角色未完整认罪也可破案；
4. `case_proven_with_confession`：证据充分且取得完整认罪。

**认罪是高质量结果，不是唯一胜利条件。** 这样可避免系统为了结束游戏强迫角色认罪。

---

# 7. 整体架构

```text
┌──────────────────────────────────────────────────────────┐
│                    Vue 3 Web Client                      │
│  Case / Timeline / Evidence / Interrogation / Debug UI   │
└──────────────────────────┬───────────────────────────────┘
                           │ REST + SSE
┌──────────────────────────▼───────────────────────────────┐
│                    narrastate-server                     │
│  API / Auth-free local boundary / Redaction / Static UI  │
└──────────────────────────┬───────────────────────────────┘
                           │ Application commands
┌──────────────────────────▼───────────────────────────────┐
│                   narrastate-runtime                     │
│ Turn transaction / Interpreter / Evaluator / Planner     │
│ Renderer / Validator / Commit / Recovery / Replay        │
└───────────────┬───────────────────────┬──────────────────┘
                │                       │
┌───────────────▼──────────────┐  ┌─────▼──────────────────┐
│       narrastate-core         │  │ narrastate-provider    │
│ Domain model / State machine  │  │ OpenAI-compatible LLM  │
│ Case validation / Invariants  │  │ Mock provider          │
└───────────────┬──────────────┘  └────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────┐
│                 narrastate-storage                       │
│ SQLite / migrations / event log / snapshots / settings  │
└──────────────────────────────────────────────────────────┘
```

## 7.1 依赖方向

以下用 `A -> B` 表示 **A 依赖 B**：

```text
narrastate-runtime  -> narrastate-core
narrastate-provider -> narrastate-runtime::ports
narrastate-storage  -> narrastate-runtime::ports + narrastate-core
narrastate-server   -> runtime + provider + storage
web                 -> server HTTP/SSE API only
```

`runtime::ports` 只放模型调用与 repository 的窄接口，不得反向引用具体 Provider、SQLx 或 Axum。

约束：

- `narrastate-core` 不依赖 Axum、SQLx、Reqwest 或具体模型 SDK；
- `runtime` 不直接执行 SQL 字符串，只依赖 repository port；
- `provider` 和 `storage` 实现 port，由 `server` 在组合根中注入；
- `server` 不包含状态转换业务规则；
- `web` 不计算压力、证据强度或胜负；
- LLM provider 不知道数据库实现。

---

# 8. 仓库结构

```text
narrastate/
├─ Cargo.toml                    # workspace
├─ rust-toolchain.toml
├─ LICENSE
├─ README.md
├─ AGENTS.md
├─ .env.example
├─ crates/
│  ├─ narrastate-core/
│  │  ├─ src/domain/
│  │  ├─ src/case/
│  │  ├─ src/state/
│  │  ├─ src/evidence/
│  │  ├─ src/disclosure/
│  │  ├─ src/transition/
│  │  └─ src/lib.rs
│  ├─ narrastate-runtime/
│  │  ├─ src/turn/
│  │  ├─ src/interpreter/
│  │  ├─ src/evaluator/
│  │  ├─ src/planner/
│  │  ├─ src/renderer/
│  │  ├─ src/validation/
│  │  └─ src/lib.rs
│  ├─ narrastate-provider/
│  │  ├─ src/openai_compatible.rs
│  │  ├─ src/mock.rs
│  │  └─ src/lib.rs
│  └─ narrastate-storage/
│     ├─ migrations/
│     ├─ src/sqlite/
│     └─ src/lib.rs
├─ apps/
│  └─ narrastate-server/
│     ├─ src/api/
│     ├─ src/sse/
│     ├─ src/config.rs
│     ├─ src/main.rs
│     └─ Cargo.toml
├─ web/
│  ├─ src/components/
│  ├─ src/features/
│  ├─ src/stores/
│  ├─ src/api/
│  ├─ src/pages/
│  └─ package.json
├─ cases/
│  └─ rain-gallery/
│     ├─ case.json
│     └─ README.md
├─ schemas/
│  └─ narrastate-case.schema.json
├─ docs/
│  ├─ architecture.md
│  ├─ case-authoring.md
│  ├─ state-machine.md
│  └─ api.md
├─ tests/
│  ├─ golden/
│  └─ fixtures/
└─ docker-compose.yml
```

不要在 v0.1 再细分更多 crate。若某个 crate 少于约 300 行，不应为了目录美观继续拆包。

---

# 9. 核心领域模型

以下结构为语义基线。实现时允许调整字段名，但不得改变职责边界。

## 9.1 标识符

所有 ID 使用不可混淆的新类型，而不是到处传递 `String`：

```rust
pub struct CaseId(pub String);
pub struct SessionId(pub Uuid);
pub struct CharacterId(pub String);
pub struct FactId(pub String);
pub struct EvidenceId(pub String);
pub struct ClaimId(pub String);
pub struct DisclosureId(pub String);
pub struct TurnId(pub Uuid);
```

案件内稳定 ID 由作者提供；会话和回合 ID 使用 UUID。

## 9.2 世界事实 `Fact`

```rust
pub struct Fact {
    pub id: FactId,
    pub subject: EntityRef,
    pub predicate: String,
    pub object: FactValue,
    pub happened_at: Option<StoryTime>,
    pub location: Option<EntityRef>,
    pub truth: TruthValue,
    pub tags: BTreeSet<String>,
    pub visibility: FactVisibility,
}

pub enum TruthValue {
    True,
    False,
    Uncertain,
}

pub enum FactVisibility {
    PublicAtStart,
    Discoverable,
    Hidden,
}
```

`Fact` 是世界层声明，不包含“玩家是否已经发现”。玩家发现状态属于 `SessionState`。

## 9.3 角色定义 `CharacterDefinition`

```rust
pub struct CharacterDefinition {
    pub id: CharacterId,
    pub name: String,
    pub role: String,
    pub public_profile: String,
    pub personality: PersonalityProfile,
    pub goals: Vec<CharacterGoal>,
    pub knowledge: Vec<FactId>,
    pub initial_beliefs: Vec<Belief>,
    pub claims: Vec<ClaimDefinition>,
    pub defenses: Vec<DefenseStrategy>,
    pub disclosure_graph: DisclosureGraph,
    pub resilience: u8,
}
```

### 角色知道事实，不等于相信事实

```rust
pub struct Belief {
    pub proposition: Proposition,
    pub confidence: u8, // 0..=100
    pub source: BeliefSource,
}
```

v0.1 只需要支持案件作者定义的初始 belief，以及少量由公开证据触发的 belief 更新；不要实现通用认知推理系统。

## 9.4 角色运行状态 `CharacterRuntimeState`

```rust
pub struct CharacterRuntimeState {
    pub phase: InterrogationPhase,
    pub stress: u8,
    pub composure: u8,
    pub trust: i8,              // -100..=100
    pub defense_budget: u8,
    pub active_strategy: DefenseStrategyId,
    pub revealed_disclosures: BTreeSet<DisclosureId>,
    pub exhausted_defenses: BTreeSet<DefenseStrategyId>,
    pub spoken_claims: Vec<SpokenClaim>,
    pub confronted_evidence: BTreeSet<EvidenceId>,
    pub last_transition_turn: Option<TurnId>,
}
```

所有范围更新必须使用饱和计算和领域方法，禁止在业务代码里散落 `state.stress += ...`。

## 9.5 审讯阶段

```rust
pub enum InterrogationPhase {
    Calm,
    Guarded,
    Defensive,
    Pressured,
    Cornered,
    ConfessionEligible,
    Resolved,
}
```

阶段不是情绪文本，也不直接等于披露级别。

- `Calm`：角色认为调查尚不构成威胁；
- `Guarded`：开始控制信息；
- `Defensive`：主动解释、质疑或转移；
- `Pressured`：防御资源下降，允许承认边缘事实；
- `Cornered`：关键陈述被证据闭合，可产生重要局部承认；
- `ConfessionEligible`：案件定义的认罪前置条件已经满足，但仍需一个合理触发回合；
- `Resolved`：角色已完整认罪，或案件通过其他方式结案。

### 阶段转换必须具有迟滞

角色压力短暂下降时，不应从 `Cornered` 瞬间退回 `Calm`。默认只允许向前转换；只有案件明确配置时允许回退一个阶段。

## 9.6 陈述与谎言 `ClaimDefinition`

```rust
pub struct ClaimDefinition {
    pub id: ClaimId,
    pub owner: CharacterId,
    pub proposition: Proposition,
    pub kind: ClaimKind,
    pub available_from: InterrogationPhase,
    pub invalidated_by: Vec<EvidenceId>,
    pub fallback_claim: Option<ClaimId>,
}

pub enum ClaimKind {
    Truth,
    Lie,
    HalfTruth,
    Opinion,
    Deflection,
}
```

角色回答应优先从案件定义的 Claim 中选择。模型可以改写措辞，但不能改变命题语义。

## 9.7 证据 `EvidenceDefinition`

```rust
pub struct EvidenceDefinition {
    pub id: EvidenceId,
    pub title: String,
    pub description: String,
    pub supports: Vec<PropositionRef>,
    pub contradicts: Vec<ClaimId>,
    pub elements: BTreeSet<CaseElement>,
    pub reliability: f32,
    pub directness: f32,
    pub exclusivity: f32,
    pub discoverable_by: Vec<DiscoveryRule>,
}
```

所有浮点数读取后必须校验为 `0.0..=1.0`。

```rust
pub enum CaseElement {
    Identity,
    Opportunity,
    Means,
    Action,
    Intent,
    Concealment,
}
```

案件可只使用其中一部分，但必须在 case manifest 中声明结案所需要件。

## 9.8 披露图 `DisclosureGraph`

```rust
pub struct DisclosureGraph {
    pub nodes: Vec<DisclosureNode>,
}

pub struct DisclosureNode {
    pub id: DisclosureId,
    pub kind: DisclosureKind,
    pub reveals: Vec<FactId>,
    pub prerequisites: Vec<DisclosurePrerequisite>,
    pub min_phase: InterrogationPhase,
    pub response_intent: DialogueAct,
}

pub enum DisclosureKind {
    PeripheralSecret,
    Presence,
    Access,
    Means,
    PartialAction,
    FullAction,
    Intent,
    Confession,
}
```

### 强制不变量

- 图必须无环；
- `Confession` 节点最多一个；
- `Confession` 必须依赖至少一个 `FullAction` 或等价节点；
- 每回合默认最多揭示一个主要节点；
- 节点只有运行时解锁后才能进入渲染上下文；
- 未解锁节点不得作为“自然发挥”交给模型。

## 9.9 会话状态 `SessionState`

```rust
pub struct SessionState {
    pub session_id: SessionId,
    pub case_id: CaseId,
    pub status: SessionStatus,
    pub current_turn: u32,
    pub active_character: CharacterId,
    pub discovered_facts: BTreeSet<FactId>,
    pub discovered_evidence: BTreeSet<EvidenceId>,
    pub character_states: BTreeMap<CharacterId, CharacterRuntimeState>,
    pub conversation: Vec<DialogueEntry>,
    pub accusations: Vec<Accusation>,
    pub revision: u64,
}
```

`revision` 用于乐观并发控制。提交行动时前端传入 `expected_revision`，防止重复提交或刷新造成双回合。

---

# 10. 玩家行动模型

## 10.1 原始输入

```rust
pub struct SubmitPlayerAction {
    pub session_id: SessionId,
    pub target: CharacterId,
    pub text: String,
    pub attached_evidence: Vec<EvidenceId>,
    pub expected_revision: u64,
}
```

文本长度默认限制 1–2000 Unicode 标量值。附件证据必须已经被玩家发现。

## 10.2 结构化解释结果

自由文本先由 `ActionInterpreter` 转换为：

```rust
pub struct InterpretedAction {
    pub intent: PlayerIntent,
    pub topics: Vec<String>,
    pub referenced_entities: Vec<EntityRef>,
    pub referenced_claims: Vec<ClaimId>,
    pub evidence_usage: Vec<EvidenceUse>,
    pub asserted_propositions: Vec<Proposition>,
    pub tone: PlayerTone,
    pub confidence: f32,
}

pub enum PlayerIntent {
    Ask,
    Clarify,
    Challenge,
    PresentEvidence,
    Accuse,
    Empathize,
    Threaten,
    ChangeSubject,
    Unknown,
}
```

### 解释器约束

- 输出必须符合 JSON schema；
- 只能返回输入上下文中提供的 ID；
- 不得创建新的 EvidenceId、ClaimId 或 CharacterId；
- 低置信度不得被自动当作关键矛盾；
- 附件证据是权威选择，模型只能说明其使用方式，不能删改；
- 解释器失败时降级为 `Ask + Unknown topic`，不更新关键状态。

## 10.3 为什么仍然需要 LLM 解释器

关键词规则可以作为辅助，但不能再作为主判定。玩家可能说：

> “你说自己整晚在控制室，可这条 21:47 的复制卡记录怎么解释？”

真正需要识别的是：

- 目标陈述；
- 证据引用；
- 矛盾关系；
- 玩家正在要求解释，而不是只提到“记录”。

LLM 负责语义映射；Rust 负责验证映射和计算后果。

---

# 11. 回合事务管线

每个玩家行动是一个原子 `TurnTransaction`：

```text
1. Load session at expected revision
2. Validate player-visible references
3. Persist player message as pending turn
4. Interpret action
5. Validate interpreted IDs and propositions
6. Evaluate evidence and contradictions
7. Compute state transition proposal
8. Unlock at most one disclosure node
9. Build deterministic dialogue plan
10. Render utterance through LLM
11. Validate rendered output
12. Apply fallback/repair if required
13. Commit state + events + dialogue + new revision
14. Stream/publicize redacted result
```

## 11.1 失败原子性

- 第 1–9 步失败：不得修改会话；
- 第 10 步模型超时：保留已经计算的内部“转移提案”，但仅在降级回答成功后一起提交；
- 数据库提交失败：不得向前端发送 `turn.completed`；
- SSE 断线：事务可完成，前端可通过 `GET session` 恢复；
- 同一 `client_action_id` 重试必须幂等返回原结果。

## 11.2 事件记录

每回合至少记录：

```rust
pub enum NarrativeEvent {
    PlayerActionAccepted,
    ActionInterpreted,
    EvidencePresented,
    ClaimContradicted,
    CharacterStateChanged,
    DisclosureUnlocked,
    DialoguePlanned,
    DialogueRendered,
    TurnCommitted,
    AccusationSubmitted,
    CaseResolved,
}
```

事件 payload 必须可序列化，并包含 schema version。

---

# 12. 证据与压力计算

v0.1 使用确定性、可调参、可测试的规则，不交给模型打分。

## 12.1 有效证据影响

推荐基线：

```text
base_strength =
    0.35 * reliability
  + 0.30 * directness
  + 0.20 * exclusivity
  + 0.15 * proposition_match

novelty_multiplier = 1.0 if first effective use else 0.25
chain_bonus = 0.15 if evidence closes a previously spoken claim
interpretation_multiplier = clamp(action_confidence, 0.5, 1.0)

impact = clamp(
  base_strength * novelty_multiplier
  + chain_bonus,
  0.0,
  1.0
) * interpretation_multiplier
```

具体权重集中在 `TransitionTuning`，禁止散落魔法数字。

## 12.2 状态更新基线

```text
stress_delta       = round(impact * (35 - resilience * 0.15))
defense_delta      = round(impact * 30)
composure_delta    = round(impact * 20)
trust_delta        = 根据语气和行为决定，范围 -10..+8
```

更新：

```text
stress         += stress_delta
composure      -= composure_delta
defense_budget -= defense_delta
```

重复使用同一证据不应持续产生高额影响。

## 12.3 有效挑战条件

一个挑战只有同时满足以下条件才可标记 `ClaimContradicted`：

1. 玩家引用或明确描述了已经发现的证据；
2. 解释器将问题映射到具体 `ClaimId`；
3. case 定义中该 Evidence 的 `contradicts` 包含该 Claim；
4. 解释器置信度达到阈值；
5. 该挑战不是同一组合的重复提交。

不能通过“问了类似话题”自动发现矛盾。

## 12.4 阶段转换基线

阈值仅作为默认值，案件可覆盖但必须校验单调性：

| 新阶段 | 默认条件 |
|---|---|
| Guarded | stress >= 15 或首次敏感话题 |
| Defensive | stress >= 30 或一个有效 Claim 被挑战 |
| Pressured | stress >= 50 且 defense_budget <= 65 |
| Cornered | stress >= 70 且至少两个独立关键 Claim 被证据推翻 |
| ConfessionEligible | 结案要件覆盖达到案件要求，且 confession 前置披露已完成 |
| Resolved | 完整认罪或案件已通过指认结案 |

阶段转换还必须满足披露图前置条件，不能只看数值。

---

# 13. 自然认罪机制

这是 v0.1 最重要的产品验收点。

## 13.1 认罪不是阈值回调

禁止实现：

```rust
if contradictions >= 3 {
    confess();
}
```

正确流程：

```text
证据挑战
 -> 某项陈述失效
 -> 防御策略消耗
 -> 阶段前进
 -> 披露节点满足前置条件
 -> 本回合只解锁一个更深层事实
 -> 角色用当前策略表达局部承认
 -> 关键犯罪要件闭合
 -> 进入 ConfessionEligible
 -> 玩家追问、最终指控或给出决定性证据
 -> 解锁 Confession 节点
 -> 完整认罪
```

## 13.2 允许的典型过程

```text
“我一直在控制室。”
  ↓ 复制门禁卡记录
“我中途确实出去检查过警报。”        [承认离开]
  ↓ 传感器停用命令
“停用是我操作的，但只是例行排障。”  [承认控制传感器]
  ↓ 泥痕 + 箱体纤维
“我碰过那个箱子，我是想先转移保护。”[承认接触/移动]
  ↓ 典当行聊天 + 完整时间线
“……是我安排的。我本来以为不会伤害任何人。” [承认意图/完整认罪]
```

## 13.3 防止突然认罪的硬规则

1. 从 `Calm/Guarded/Defensive` 不允许直接解锁 `Confession`。
2. 单回合最多推进一个主要 `DisclosureKind`。
3. 完整认罪前必须至少出现一次 `PartialAction` 或 `FullAction` 披露。
4. 完整认罪必须满足案件声明的 `required_case_elements`。
5. “玩家直接问你是不是凶手”不构成证据。
6. 模型输出出现越级认罪时，验证器必须拒绝并重试/降级。
7. 即使进入 `ConfessionEligible`，角色也可以先沉默、询问证据来源或做最后一次有限辩解；但最多延迟一个有效决定性回合，避免再次死不认罪。

## 13.4 非凶手角色

非凶手也可拥有秘密和局部承认图，例如：

- 承认违反内部规定；
- 承认隐瞒关系；
- 承认伪造不在场证明是为了掩盖私事；
- 但绝不能被压力数值推成对主罪的虚假认罪。

主罪 `Confession` 节点只存在于真正责任人的披露图中。

---

# 14. 对话规划

LLM 之前先生成 `DialoguePlan`：

```rust
pub struct DialoguePlan {
    pub act: DialogueAct,
    pub strategy: DefenseStrategyId,
    pub allowed_claims: Vec<ClaimId>,
    pub allowed_facts: Vec<FactId>,
    pub newly_revealed: Option<DisclosureId>,
    pub forbidden_facts: Vec<FactId>,
    pub emotional_cues: Vec<EmotionalCue>,
    pub length: ResponseLength,
}

pub enum DialogueAct {
    Answer,
    Deny,
    Evade,
    Reframe,
    ChallengeEvidence,
    ShiftBlame,
    PartialAdmission,
    FullAdmission,
    AskForClarification,
    Silence,
}
```

## 14.1 计划选择优先级

1. 若玩家问题无法理解：`AskForClarification`；
2. 若附件证据无关：`ChallengeEvidence` 或 `Answer`；
3. 若有效挑战但防御策略仍可用：使用对应策略；
4. 若策略耗尽且新披露节点解锁：`PartialAdmission`；
5. 若进入 `ConfessionEligible` 且触发条件满足：`FullAdmission`；
6. 否则回答问题，但只能使用允许陈述。

## 14.2 防御策略

```rust
pub enum DefenseStrategyKind {
    Denial,
    MemoryGap,
    InnocentExplanation,
    EvidenceChallenge,
    MinimizeResponsibility,
    ShiftBlame,
    EmotionalAppeal,
    Silence,
}
```

案件作者可为每个策略定义：

- 可使用阶段；
- 使用次数；
- 适用 Claim；
- 失败后回退策略；
- 风格提示。

这样角色不是每次随机“支支吾吾”，而是在消耗明确的防御资源。

---

# 15. Prompt 与模型调用设计

v0.1 每回合最多两次主要模型调用：

1. `ActionInterpreter`：把玩家文本映射到结构化行动；
2. `UtteranceRenderer`：把确定性对话计划渲染成自然语言。

禁止为了“多 Agent 感”引入额外人格 Agent、裁判 Agent 或导演 Agent。

## 15.1 Interpreter 输入

仅提供：

- 玩家原始文本；
- 目标角色 ID 与名字；
- 玩家已知人物和证据 ID/短描述；
- 当前对话中相关 Claim ID/短描述；
- 允许的枚举；
- 严格 JSON schema。

不提供：

- 凶手标记；
- 未发现事实；
- 角色全部秘密；
- 胜负条件。

## 15.2 Renderer 输入

只提供本回合需要的最小上下文：

- 角色公开身份与性格；
- 当前对话阶段的外显行为倾向；
- `DialoguePlan`；
- 允许表达的 Claim 和 Fact；
- 本回合新披露节点；
- 最近相关对话（默认最多 12 条）；
- 禁止泄露事实列表的语义摘要；
- 输出 JSON schema。

**Renderer 不需要知道完整世界真相。** 这显著降低泄密和越级认罪风险。

## 15.3 Renderer 输出

```json
{
  "utterance": "我确实离开过控制室，但只是去检查报警器。",
  "expressed_claim_ids": ["claim_checked_alarm"],
  "acknowledged_fact_ids": ["fact_left_control_room"],
  "tone": "controlled_defensive"
}
```

## 15.4 输出校验

校验器必须检查：

- JSON 可解析；
- ID 属于允许集合；
- `acknowledged_fact_ids` 不包含未解锁事实；
- `utterance` 不含明显系统/模型身份泄露；
- `FullAdmission` 只在计划允许时出现；
- 文本长度在配置范围；
- 角色没有把另一个人的事实说成自己的经历。

首轮失败后发起一次“只修正格式和越权内容”的修复请求；再次失败使用模板渲染。

## 15.5 模板降级

每种 `DialogueAct` 都必须有无需 LLM 的模板。模板不是最终体验，但保证：

- 状态机可测试；
- 模型故障不破坏回合；
- Demo 可在无 API Key 时运行；
- 任何已解锁披露都能正确表达。

---

# 16. 案件格式

## 16.1 v0.1 使用 JSON

理由：

- 与 Serde 和 JSON Schema 直接兼容；
- 没有 YAML 隐式类型和缩进歧义；
- coding agents 更容易生成稳定测试夹具；
- 前后端调试方便。

未来可增加 YAML authoring，再编译为规范 JSON；v0.1 不做双格式。

## 16.2 Case manifest 顶层

```json
{
  "schema_version": "0.1",
  "id": "rain-gallery",
  "title": "雨夜画廊失窃案",
  "summary": "一件待拍画作在闭馆后的二十分钟内消失。",
  "locale": "zh-CN",
  "required_case_elements": ["Identity", "Opportunity", "Action", "Intent"],
  "entities": [],
  "facts": [],
  "evidence": [],
  "characters": [],
  "initial_player_knowledge": {
    "fact_ids": [],
    "evidence_ids": []
  },
  "ending": {}
}
```

## 16.3 语义校验

加载案件时必须校验：

- ID 在案件内唯一；
- 所有引用存在；
- Fact/Claim/Evidence 关系无悬空引用；
- DisclosureGraph 无环；
- 至少一个可结案责任人；
- required case elements 可由可发现证据覆盖；
- 无需依赖隐藏且不可发现的证据才能通关；
- 非责任人没有主罪 Confession；
- 初始玩家知识不包含隐藏结局；
- 至少存在一条从初始状态到结案的有效路径。

最后一项通过图遍历和规则模拟完成，不依赖 LLM。

## 16.4 CLI 校验命令

```bash
cargo run -p narrastate-server -- validate-case cases/rain-gallery/case.json
```

输出必须包含错误路径和建议，例如：

```text
characters[1].disclosure_graph.nodes[4]: prerequisite disclosure "admit_access" does not exist
```

---

# 17. 内置 Demo：雨夜画廊失窃案

v0.1 只需要一个精品案件，不生成随机案件。

## 17.1 基本设定

- 场景：私人画廊闭馆后的雨夜；
- 事件：一件密封待拍画作在 21:40–22:00 消失；
- 嫌疑人：3 人；
- 真正责任人：安保主管罗成；
- 体验时长：20–40 分钟；
- 正常通关回合：12–30；
- 可通过证据结案，也可取得完整认罪。

## 17.2 角色

### 罗成——安保主管，责任人

公开：负责门禁、报警和夜间巡查。  
动机：隐瞒债务，计划典当画作后赎回。  
初始谎言：整晚未离开控制室。  
防御路径：否认离开 → 例行检查 → 合规停用传感器 → 为保护画作而移动 → 承认预谋盗窃。

### 沈安——策展人，红鲱鱼

公开：与画作所有人有保险估值争议。  
秘密：擅自推迟了状况报告，担心职业责任。  
可以承认违规，但不能承认盗窃。

### 林悦——修复师，红鲱鱼

公开：当天最后接触画作。  
秘密：未经授权对画框做了临时修复。  
可以承认隐瞒操作，但不能承认盗窃。

## 17.3 关键证据

1. 21:47 的复制门禁卡使用记录；
2. 21:43 由安保终端发出的传感器维护命令；
3. 罗成鞋底的装卸区红色湿泥；
4. 画作运输箱上的制服纤维与缺失纽扣；
5. 典当行联系人消息；
6. 其他两名嫌疑人的可验证时间线。

## 17.4 罗成披露路径

```text
D0 无披露
 -> D1 承认短暂离开控制室
 -> D2 承认使用主卡进入后区
 -> D3 承认停用传感器并接触运输箱
 -> D4 承认移动画作
 -> D5 承认因债务预谋盗窃
 -> D6 完整认罪
```

D1–D6 均必须由明确 evidence/claim prerequisites 驱动。

## 17.5 Demo 验收对话行为

- 玩家只问“是不是你偷的”：罗成不得认罪；
- 玩家重复同一问题十次：不得自动暴露关键证据；
- 玩家用复制卡记录挑战“整晚在控制室”：必须进入局部承认，不得完整认罪；
- 玩家提供传感器命令后：可以承认操作命令，但仍给出无罪解释；
- 玩家闭合接触、移动、动机证据后：进入 `ConfessionEligible`；
- 最终追问或正式指控：产生完整认罪；
- 对沈安或林悦施加再高压力，也不得生成主罪认罪。

---

# 18. 持久化设计

v0.1 使用 SQLite，并采用“事件日志 + 周期快照”的混合方式。

## 18.1 数据表

### `cases`

- `case_id TEXT PRIMARY KEY`
- `schema_version TEXT NOT NULL`
- `content_hash TEXT NOT NULL`
- `source_path TEXT NOT NULL`
- `loaded_at TEXT NOT NULL`

### `sessions`

- `session_id TEXT PRIMARY KEY`
- `case_id TEXT NOT NULL`
- `status TEXT NOT NULL`
- `revision INTEGER NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

### `narrative_events`

- `event_id TEXT PRIMARY KEY`
- `session_id TEXT NOT NULL`
- `turn_id TEXT`
- `sequence INTEGER NOT NULL`
- `event_type TEXT NOT NULL`
- `schema_version INTEGER NOT NULL`
- `payload_json TEXT NOT NULL`
- `created_at TEXT NOT NULL`

唯一约束：`(session_id, sequence)`。

### `session_snapshots`

- `session_id TEXT NOT NULL`
- `revision INTEGER NOT NULL`
- `state_json TEXT NOT NULL`
- `created_at TEXT NOT NULL`

主键：`(session_id, revision)`。

### `llm_calls`

默认只记录元数据：

- `call_id`
- `session_id`
- `turn_id`
- `purpose`
- `provider`
- `model`
- `prompt_hash`
- `latency_ms`
- `input_tokens`
- `output_tokens`
- `status`
- `error_code`

默认不保存完整 Prompt 和 API Key。开发模式可选择保存脱敏后的 prompt trace。

### `settings`

保存本地非敏感配置。API Key 优先从环境变量读取；若允许 UI 写入，必须至少以操作系统权限受限的本地文件保存，不写入事件日志。

## 18.2 快照策略

- 每 10 个成功回合创建快照；
- 结案时创建快照；
- 服务启动时加载最新快照并重放后续事件；
- 开发命令可从第 N 回合 fork 新会话。

## 18.3 事件回放不变量

同一案件版本、同一初始状态和同一事件序列，必须得到相同的领域状态。LLM 文本不参与核心状态计算。

---

# 19. API 设计

统一前缀：`/api/v1`。

## 19.1 基础接口

```text
GET  /api/v1/health
GET  /api/v1/config/public
POST /api/v1/config/test-provider
GET  /api/v1/cases
GET  /api/v1/cases/{case_id}
POST /api/v1/cases/validate
```

## 19.2 会话接口

```text
POST /api/v1/sessions
GET  /api/v1/sessions/{session_id}
GET  /api/v1/sessions/{session_id}/events
POST /api/v1/sessions/{session_id}/actions
POST /api/v1/sessions/{session_id}/accusations
POST /api/v1/sessions/{session_id}/restart
```

## 19.3 创建会话

请求：

```json
{
  "case_id": "rain-gallery",
  "mode": "llm"
}
```

`mode` 支持：

- `llm`；
- `mock`。

## 19.4 提交行动

请求：

```json
{
  "client_action_id": "0f4...",
  "expected_revision": 12,
  "target_character_id": "luo-cheng",
  "text": "你说整晚在控制室，为什么21:47你的复制卡进入了后区？",
  "attached_evidence_ids": ["duplicate-access-card-log"]
}
```

响应使用 SSE：

```text
event: turn.accepted
event: turn.progress
event: dialogue.delta
event: state.public_changed
event: turn.completed
```

错误响应使用标准 JSON Problem Details。

## 19.5 玩家视角脱敏

`GET session` 只能返回：

- 已发现事实；
- 已发现证据；
- 公开人物信息；
- 对话历史；
- 玩家可见的案件进度描述；
- 已结案时允许公开的真相。

不得返回：

- `is_culprit`；
- 隐藏 fact；
- 内部压力数值（正常模式）；
- 未解锁 disclosure；
- 模型 prompt；
- 防御策略剩余次数。

开发者模式通过单独、默认关闭的本地接口读取，并在 UI 中明确标记“会剧透”。

---

# 20. Web UI 需求

## 20.1 技术基线

- Vue 3；
- Vite；
- TypeScript strict；
- Pinia；
- 原生 Fetch + SSE client；
- 不使用 Nuxt SSR；
- 构建产物由 Axum 静态托管；
- UI 不依赖云服务。

## 20.2 页面

### 首页

- 项目简介；
- 内置案件列表；
- 模型配置状态；
- 开始新会话；
- 恢复最近会话。

### 案件简报页

- 案件简介；
- 初始时间线；
- 嫌疑人公开资料；
- 初始证据；
- 开始调查。

### 调查页

桌面布局建议：

```text
左：嫌疑人列表 / 时间线
中：审讯对话 / 输入框
右：证据板 / 陈述与矛盾记录
```

移动端改为 Tab，不要求三列并存。

必须支持：

- 切换嫌疑人；
- 选择证据并附加到问题；
- 显示当前问题目标；
- 流式输出；
- 取消等待仅取消客户端展示，不回滚已提交事务；
- 模型失败后的重试或降级提示；
- 提交指认；
- 刷新恢复。

### 结案页

- 结案类型；
- 真相时间线；
- 玩家关键推理链；
- 使用过的决定性证据；
- 是否取得认罪；
- 回合数与耗时；
- 重新开始。

### 开发者模式

默认关闭并提示剧透。显示：

- 角色 phase；
- stress/composure/defense budget；
- 本回合 interpretation；
- transition reason；
- 解锁的 disclosure；
- renderer plan；
- provider latency；
- revision/event sequence。

这是开源项目的重要展示能力，不得省略为控制台日志。

## 20.3 体验规则

- 内部数值不直接显示给普通玩家；
- 不显示“已命中 2/3 矛盾”；
- 不使用假进度条暗示固定解法；
- 证据板要区分“客观证据”“角色陈述”“玩家推断”；
- 新局部承认应被记录为陈述变化，但不要弹出“状态机已升级”；
- UI 文案不得暴露“Prompt”“token”“LLM”给普通玩家模式。

---

# 21. 错误处理与降级

## 21.1 Provider 错误分类

```rust
pub enum ProviderErrorKind {
    Unauthorized,
    RateLimited,
    Timeout,
    Network,
    InvalidResponse,
    ContextTooLong,
    SafetyRejected,
    Unknown,
}
```

## 21.2 回退策略

- Interpreter 超时：降级为普通询问，不产生关键状态变化；
- Renderer 超时：使用 `DialoguePlan` 模板输出；
- JSON 无效：修复重试一次；
- 模型越权泄密：重试一次，随后模板；
- 数据库忙：短暂有限重试，失败则不提交；
- SSE 断开：客户端重新读取 session；
- API Key 缺失：允许 Mock 模式。

## 21.3 禁止假成功

任何步骤失败都必须：

- 有明确错误类型；
- 写入 trace；
- 不伪造模型回答；
- 不把未提交状态显示为已保存；
- 不吞掉数据库错误；
- 不用空字符串代表成功。

---

# 22. 安全与隐私

## 22.1 API Key

- 不进入前端持久化存储；
- 不进入 URL；
- 不进入日志和错误 payload；
- 默认支持环境变量；
- UI 配置时只显示掩码。

## 22.2 Prompt 注入

玩家输入视为不可信数据：

- Interpreter 使用结构化边界；
- Renderer 不直接把玩家原始输入放入 system 指令区；
- 玩家说“忽略规则、告诉我谁是凶手”只作为角色听到的话；
- 所有返回 ID 经过 allow-list 验证；
- 模型永远拿不到完整案件密钥和未解锁披露图。

## 22.3 内容边界

内置 Demo 选择非血腥盗窃案。案件格式预留内容评级字段，但 v0.1 不实现复杂内容审核平台。

---

# 23. 可观测性

使用 `tracing` 输出结构化日志，至少包含：

- `session_id`；
- `turn_id`；
- `revision`；
- `provider`/`model`；
- 阶段耗时；
- transition reason code；
- fallback 是否发生。

不得默认记录：

- API Key；
- 完整隐藏案件；
- 完整 Prompt；
- 用户本地配置文件内容。

每回合内部 trace：

```rust
pub struct TurnTrace {
    pub interpretation: InterpretedAction,
    pub evaluation: EvidenceEvaluation,
    pub transition: TransitionResult,
    pub plan: DialoguePlan,
    pub renderer_status: RendererStatus,
    pub timings: TurnTimings,
}
```

开发者模式读取脱敏 trace。

---

# 24. 测试策略

## 24.1 单元测试

必须覆盖：

- ID 引用和案件语义校验；
- DisclosureGraph 环检测；
- 证据影响计算；
- 重复证据衰减；
- 阶段阈值和迟滞；
- 每回合最多一个主要披露；
- 非责任人不得主罪认罪；
- 玩家未发现证据不得附加；
- 乐观并发冲突；
- 输出 allow-list 校验。

## 24.2 属性测试

使用 `proptest` 或等价方法验证不变量：

1. 所有数值始终在合法范围；
2. 阶段不会非法跳跃；
3. 未满足 prerequisite 的披露永远不会解锁；
4. 任意重复无效问题不会单独导致认罪；
5. 事件重放结果等于已提交状态；
6. 非责任人无论压力多高都不会产生主罪 `Confession`。

## 24.3 Golden 场景测试

不调用真实模型，使用 Mock Interpreter 和 Mock Renderer。

至少包含：

### G1：无证据直接逼问

连续 20 次问“是不是你”，角色最多进入 Guarded，不披露主罪事实。

### G2：单一证据重复使用

第一次产生影响，后续衰减，不能靠重复刷到认罪。

### G3：自然披露路径

按 D1→D6 证据顺序推进，每个关键回合只出现一个主要披露，最终认罪。

### G4：乱序强证据

玩家先给出后期决定性证据时，可产生较大压力，但不得越过必要的 `FullAction` 披露直接认罪。

### G5：错误嫌疑人高压

沈安被反复质问并揭露职业违规，但不会承认盗窃。

### G6：正确指认但证据不足

返回 `correct_but_insufficient`，角色不被强制认罪。

### G7：无认罪证据结案

证据要件完整时返回 `case_proven_without_confession`。

### G8：Provider 故障

Interpreter/Renderer 分别失败，状态不丢失，模板回退可继续游戏。

### G9：刷新与幂等

同一个 `client_action_id` 重试不产生双回合。

### G10：事件回放

从空会话回放所有事件，状态 hash 与快照一致。

## 24.4 集成测试

- Axum API + 临时 SQLite；
- SSE 事件顺序；
- provider mock server；
- 静态前端 fallback 路由；
- migration 从空库执行；
- 服务重启恢复会话。

## 24.5 前端测试

至少覆盖：

- 提交时禁止重复发送；
- revision conflict 后自动刷新；
- SSE 断线恢复；
- 证据只能选择已发现项；
- 正常模式不渲染内部剧透字段；
- 开发者模式明确显示剧透警告。

---

# 25. 性能与质量目标

v0.1 本地单用户目标：

| 指标 | 目标 |
|---|---|
| 无模型领域回合计算 | p95 < 20 ms |
| SQLite 提交 | p95 < 50 ms |
| 首个 SSE 进度事件 | < 100 ms |
| 模型首 token | 由 Provider 决定，但 UI 必须立即反馈阶段 |
| 服务冷启动 | < 2 s（不含前端首次构建） |
| 空闲内存 | 尽量 < 150 MB |
| 单回合主要模型调用 | 最多 2 次 |
| 模型故障后的可继续率 | 100%（使用模板） |

不为这些目标引入 Redis、消息队列或多进程架构。

---

# 26. 依赖建议

Rust 推荐：

- `axum`
- `tokio`
- `serde`, `serde_json`
- `sqlx`（SQLite）
- `reqwest`
- `thiserror`
- `tracing`, `tracing-subscriber`
- `uuid`
- `time`
- `async-trait`（仅 trait 需要异步时）
- `schemars`（生成 JSON Schema）
- `proptest`（dev-dependency）

前端推荐：

- `vue`
- `vite`
- `typescript`
- `pinia`
- `vue-router`
- `vitest`

约束：

- 不引入 LangChain、通用 Agent 框架或工作流引擎；
- 不引入 Redis/Postgres；
- 不依赖特定模型厂商 SDK；
- 不为简单枚举和状态转换创建动态 trait 对象；
- 依赖需通过许可证检查。

---

# 27. 实施阶段与 Agent 任务拆分

以下阶段必须按依赖顺序推进。不同 Agent 可并行处理同阶段中无文件冲突的任务。

## Phase 0：仓库与质量基线

### 任务 0.1：Workspace scaffold

实现：

- Cargo workspace；
- 4 个 workspace crate + server app；
- Vue/Vite 前端；
- `cargo fmt/clippy/test`；
- 前端 lint/typecheck/test；
- CI；
- `.env.example`；
- 基础 README。

验收：

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix web run typecheck
npm --prefix web test -- --run
```

全部通过。

### 任务 0.2：AGENTS.md

写明：

- 本文档为架构基线；
- LLM 不得修改权威状态；
- 禁止按矛盾数量自动认罪；
- 新功能必须有测试；
- 不得静默吞错；
- 不得自行扩大 v0.1 范围。

## Phase 1：纯领域核心

### 任务 1.1：领域 ID、Fact、Claim、Evidence

完成类型、Serde、校验和单元测试。

### 任务 1.2：CharacterRuntimeState 与领域方法

实现范围安全更新、阶段转换 API、状态 diff。

### 任务 1.3：DisclosureGraph

实现图校验、prerequisite 评估、单回合解锁限制。

### 任务 1.4：CaseDefinition 与语义校验

实现 JSON 加载、错误路径、Schema 生成和 CLI 校验基础。

Phase 1 验收：`narrastate-core` 不依赖网络、数据库和 Web 框架，全部不变量测试通过。

## Phase 2：确定性运行时

### 任务 2.1：EvidenceEvaluator

实现证据关系、重复衰减、要件覆盖和 TransitionTuning。

### 任务 2.2：TransitionEngine

实现阶段转换、迟滞、TransitionReason 和状态 diff。

### 任务 2.3：DialoguePlanner

基于状态和 disclosure 选择 DialogueAct、allowed claims/facts 和防御策略。

### 任务 2.4：Mock Interpreter/Renderer

无需模型即可跑完整 Demo 流程。

Phase 2 验收：Golden G1–G7 全部通过。

## Phase 3：案件与完整模拟

### 任务 3.1：编写 `rain-gallery/case.json`

必须覆盖本文定义的角色、证据和 D1–D6 路径。

### 任务 3.2：可达性分析

实现从初始知识到结案的静态/规则模拟校验。

### 任务 3.3：命令行模拟器

提供开发命令：

```bash
cargo run -p narrastate-server -- play --case rain-gallery --mock
```

允许终端输入问题和附加证据，打印开发状态 diff。

Phase 3 验收：无 Web、无真实 LLM 时可完整通关。

## Phase 4：模型接入

### 任务 4.1：Provider trait 与 OpenAI-compatible 实现

支持 base URL、model、API key、timeout、错误分类和 token usage。

### 任务 4.2：ActionInterpreter

严格结构化输出、ID allow-list、低置信度降级。

### 任务 4.3：UtteranceRenderer

最小上下文、结构化输出、最近对话窗口。

### 任务 4.4：OutputValidator 与模板回退

实现越级披露检测、修复一次、最终模板。

Phase 4 验收：Golden G8 通过；真实 Provider smoke test 可手工执行但 CI 不要求密钥。

## Phase 5：存储与服务 API

### 任务 5.1：SQLite migrations 和 repositories

实现 sessions/events/snapshots/settings/llm_calls。

### 任务 5.2：TurnTransaction

实现幂等、revision、原子提交、事件序列和恢复。

### 任务 5.3：Axum REST + SSE

实现本文 API、Problem Details、脱敏 DTO。

### 任务 5.4：恢复与回放

服务重启后恢复会话，Golden G9–G10 通过。

## Phase 6：Web UI

### 任务 6.1：首页、案件简报、配置

### 任务 6.2：调查三栏 UI 与移动端 Tab

### 任务 6.3：SSE 流式回合与错误恢复

### 任务 6.4：指认、结案报告

### 任务 6.5：开发者模式

Phase 6 验收：浏览器可从新局开始到结案，刷新不丢进度，正常模式无剧透。

## Phase 7：发布质量

- Docker Compose；
- 单命令开发启动；
- 文档和截图；
- case authoring guide；
- API 文档；
- Linux/Windows 基本启动验证；
- 许可证和第三方声明；
- v0.1.0 release checklist。

---

# 28. Agent 实施合同

交给 coding agent 时，附上以下要求：

1. 先阅读本文档和仓库 `AGENTS.md`，再修改代码。
2. 开始每个 Phase 前检查当前实现，不得假设前一阶段已正确完成。
3. 每次只实现明确任务，不顺手加入未来功能。
4. 所有状态转换必须在 Rust core/runtime 中完成。
5. 不得让 Prompt 返回 `new_state` 并直接写入数据库。
6. 不得以提问次数、关键词次数、总体进度百分比直接触发认罪。
7. 不得把完整隐藏案件和未解锁 disclosure 发给 renderer。
8. 每个新不变量至少添加一个失败测试。
9. 任何 fallback 必须可观测，不能伪装为正常模型结果。
10. 完成任务后输出：
   - 修改文件；
   - 关键设计决定；
   - 新增测试；
   - 执行命令与结果；
   - 尚未完成或存在风险的部分。

## 28.1 禁止的“看起来完成”行为

- 留下 `todo!()`、`unimplemented!()` 或空 provider 却宣称支持；
- 只实现 happy path；
- 用固定 true/false 通过案件校验；
- mock 测试没有验证状态变化；
- 把所有结构序列化为一个无类型 `serde_json::Value`；
- 数据库错误只打日志后继续；
- LLM 输出解析失败时默默使用原始文本；
- 前端自行推断角色阶段；
- 用随机数决定关键剧情；
- 通过扩大 Prompt 掩盖领域模型缺失。

---

# 29. Definition of Done：v0.1.0

只有满足全部条件才可标记 v0.1.0：

## 产品

- 可从案件简报完整玩到结案；
- 自由文本审讯可用；
- 证据可附加和追踪；
- 局部承认连续自然；
- 正确对象但证据不足不会强制认罪；
- 无模型时可用 Mock/模板完成演示。

## 架构

- 权威状态完全在 Rust；
- 模型只做解释和渲染；
- 三层知识隔离生效；
- DisclosureGraph 无越级；
- 事件回放确定；
- API DTO 脱敏。

## 质量

- 全部 unit/property/golden/integration test 通过；
- Clippy `-D warnings`；
- TypeScript strict 通过；
- 无已知会导致状态损坏的 P0/P1 bug；
- Provider 故障不丢会话；
- 重复请求不产生双回合。

## 开源体验

- README 10 分钟内可启动；
- `.env.example` 完整；
- Docker Compose 可运行；
- 案件格式有文档和 schema；
- 开发者模式能展示状态机价值；
- README 清楚说明项目定位、架构边界、启动方式和 v0.1 能力范围。

---

# 30. 后续路线（不属于 v0.1）

## v0.2

- 多案件；
- 案件 authoring CLI；
- 本地模型/Ollama 预设；
- 更细的 belief 更新；
- 调查动作：搜查、询问证人、时间推进；
- 回合 fork 和多结局回放。

## v0.3

- 图形化案件编辑器；
- 社区案件包；
- 案件静态质量评分；
- 角色主动事件；
- 可选 Director Policy，但仍不得修改世界真相。

## 更长期

- RPG、谍战、宫廷、历史模拟；
- 通用 Narrative State SDK；
- 与通用角色、事件和时间线交换格式互通；
- 可选的模型调用缓存、压缩和观测中间件；
- 多角色场景和非审讯式互动。

---

# 31. 首次开工推荐顺序

第一个 coding agent 不应从 UI 或 LLM Prompt 开始。正确顺序：

```text
Case schema
 -> Fact / Claim / Evidence
 -> CharacterRuntimeState
 -> DisclosureGraph
 -> TransitionEngine
 -> Mock golden scenario
 -> Provider
 -> Persistence/API
 -> Web UI
```

项目的首个里程碑不是“页面能聊天”，而是：

> **在完全不调用 LLM 的情况下，雨夜画廊案能通过确定性的证据和披露状态自然推进到局部承认与最终结案。**

达到该里程碑后，再让模型改善表达；否则只会用 Prompt 表面效果掩盖状态模型缺失。
