# 非证据性视觉资产

生成图片只具有 `AssetSemanticRole::Decorative`。视觉清单位于 `manifest.json.visual_assets`，不进入 `CaseTemplate`、`CompiledCase`、Evidence、DisclosureGraph 或结案条件，因此删除全部视觉资产不会改变编译结果和模拟路径。

图片 Provider 与主文本 Provider 完全独立，分别配置 Base URL、模型和 API Key。图片 Key 使用 `NARRASTATE_IMAGE_API_KEY` 或服务端 `data/image-provider.env`；不复用文本 Provider Key。

允许封面、人物头像、地点氛围、章节、转场与结局插画。所有视觉必须跨真相变体共享，不得存放在 `assets/evidence/`。UI 在容易误解的位置应标记“场景示意图，不作为案件证据”。

默认生成计划包含一张封面、一张共享场景图、章节开场、调查转场、通用结局图、最多六个共享地点以及全部角色头像。任务使用固定上限的有界并发，并按生成计划顺序写入清单；任何单张失败只记录 warning。章节、转场和结局 Prompt 只使用案件标题、公开简介与用户公开设定，不读取真相变体或隐藏结局。

未配置 Provider、超时、限流、无效响应或单张生成失败只产生 warning。案件草案仍继续编译、校验、模拟和安装，运行时使用渐变背景与首字母头像。

玩家询问背景图、头像、封面或插画中的具体细节时，运行时会直接返回固定的非证据提示。这类消息不会调用文本 LLM，也不会进入状态机：即使同时附带证据，角色压力、信任、审讯阶段、已发现事实和披露节点都不会改变。

稳定错误码包括：

- `GENERATED_IMAGE_USED_AS_EVIDENCE`
- `GENERATED_IMAGE_LEAKS_VARIANT`
- `VISUAL_PROVIDER_NOT_CONFIGURED`
- `VISUAL_PROVIDER_FAILED`
