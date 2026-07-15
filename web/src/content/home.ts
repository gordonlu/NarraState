// Homepage copy is centralized here so product language can evolve without editing components.
export interface NarrativeStageContent {
  id: 'denial' | 'explanation' | 'partial-admission'
  index: string
  label: string
  pressure: number
  playerPrompt?: string
  response: string
}

export interface HomePageContent {
  brand: {
    chineseName: string
    englishName: string
  }
  navigation: {
    mechanism: string
    cases: string
    openSource: string
    docs: string
    settings: string
    play: string
  }
  hero: {
    titleLine1: string
    highlightedText: string
    description: string
    primaryAction: string
  }
  stateScene: {
    lockStatement: string
    heading: string
    description: string
    stages: NarrativeStageContent[]
    ruleStatementLine1: string
    ruleStatementLine2: string
  }
  replayScene: {
    headingLine1: string
    headingLine2: string
    description: string
  }
  footer: {
    tagline: string
    copyright: string
  }
}

export const homePageContent: HomePageContent = {
  brand: {
    chineseName: '谜局AI',
    englishName: 'NarraState',
  },
  navigation: {
    mechanism: '核心机制',
    cases: '案件',
    openSource: '开源',
    docs: '文档',
    settings: '设置',
    play: '开始游戏',
  },
  hero: {
    titleLine1: '真相不会改变。',
    highlightedText: '人会。',
    description: '谜局AI会在每局开始时，从已经验证的真相中锁定一种。\n你的追问、证据与选择不会改写事实，只会改变人物如何回应，以及真相如何浮现。',
    primaryAction: '进入谜局',
  },
  stateScene: {
    lockStatement: '本局真相已锁定',
    heading: '一句追问，改变人物的下一句话',
    description: '同一个人。同一个地点。同一局已经冻结的真相。事实不动，人物在变化。',
    stages: [
      {
        id: 'denial',
        index: '01',
        label: '否认',
        pressure: 24,
        response: '“我从来没有进入过那里。”',
      },
      {
        id: 'explanation',
        index: '02',
        label: '解释',
        pressure: 53,
        playerPrompt: '“但记录显示，你后来又回去了。”',
        response: '“我确实回来过，但只是为了拿一份文件。”',
      },
      {
        id: 'partial-admission',
        index: '03',
        label: '部分承认',
        pressure: 78,
        response: '“好吧，我碰过那个箱子。\n但事情不是你想的那样。”',
      },
    ],
    ruleStatementLine1: 'AI 负责人物如何说，',
    ruleStatementLine2: '规则决定什么是真的。',
  },
  replayScene: {
    headingLine1: '同一宗案件。下一局，',
    headingLine2: '可能是另一种真相。',
    description: '每次开局只会从经过验证的真相变体中选择一个。\n选择完成，世界立即冻结；本局事实不再改变。',
  },
  footer: {
    tagline: '每局一种真相，每次追问都留下变化。',
    copyright: `© ${new Date().getFullYear()} NarraState`,
  },
}
