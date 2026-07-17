# 非证据性视觉资产

生成图片只具有 `AssetSemanticRole::Decorative`。视觉清单位于 `manifest.json.visual_assets`，不进入 `CaseTemplate`、`CompiledCase`、Evidence、DisclosureGraph 或结案条件，因此删除全部视觉资产不会改变编译结果和模拟路径。

图片 Provider 与主文本 Provider 完全独立，分别配置 Base URL、模型和 API Key。图片 Key 使用 `NARRASTATE_IMAGE_API_KEY` 或服务端 `data/image-provider.env`；不复用文本 Provider Key。

允许封面、人物头像、地点氛围、章节、转场与结局插画。所有视觉必须跨真相变体共享，不得存放在 `assets/evidence/`。UI 在容易误解的位置应标记“场景示意图，不作为案件证据”。

默认生成计划严格按当前玩家界面的实际消费位置生成：一张案件封面、一张共享主场景图，以及每名角色一张头像。三人案件因此默认生成五张图片。章节、地点、转场和结局仍是支持的非证据视觉类型，但在对应运行时界面尚未消费它们之前不会默认请求，避免无用途的图片调用。任务使用固定上限的有界并发，并按生成计划顺序写入清单；任何单张失败只记录 warning。所有 Prompt 只使用案件标题、公开简介与用户公开设定，不读取真相变体或隐藏结局。

未配置 Provider、超时、限流、无效响应或单张生成失败只产生 warning。案件草案仍继续编译、校验、模拟和安装，运行时使用渐变背景与首字母头像。

图片服务错误会保留稳定分类、HTTP 状态以及最多 280 个字符的结构化服务商错误信息，供案件页诊断。服务端只读取 JSON `error.message`，不会把 HTML 等非结构化响应复制到报告中，并会在写入前移除当前 API Key。Seedream 模型自动使用火山方舟兼容的 `2K` 单图参数；其他 OpenAI-compatible 服务继续使用案件视觉规格中的精确尺寸。

已安装的 AI 生成案件可以在案件简报中执行“补充缺失配图”或“重新生成全部”。补充模式只请求当前默认计划中缺少的视觉；重新生成模式重新请求当前默认计划。旧案件已经生成的章节、地点、转场或结局图片会保留，不会因为计划收敛而被删除。更新先写入临时案件包并完成哈希和安全校验，再原子替换原目录并同步安装索引。单张重新生成失败时保留原图，成功后使用内容哈希查询参数刷新浏览器缓存。视觉更新不会改写 `CaseTemplate`、冻结实例、Session 或模拟结果。人工创作的非 generated 案件不允许通过该接口原地修改。

玩家询问背景图、头像、封面或插画中的具体细节时，运行时会直接返回固定的非证据提示。这类消息不会调用文本 LLM，也不会进入状态机：即使同时附带证据，角色压力、信任、审讯阶段、已发现事实和披露节点都不会改变。

稳定错误码包括：

- `GENERATED_IMAGE_USED_AS_EVIDENCE`
- `GENERATED_IMAGE_LEAKS_VARIANT`
- `VISUAL_PROVIDER_NOT_CONFIGURED`
- `VISUAL_PROVIDER_FAILED`
- `PACKAGE_VISUAL_UPDATE_UNSUPPORTED`
