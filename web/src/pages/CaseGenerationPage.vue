<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { api } from '../lib/api'
import { generationScope, normalizeGenerationScope } from '../lib/generationDesign'
import { animateGenerationEvents, animateGenerationStart } from '../lib/overlayMotion'
import type { GenerationJob, GenerationRequest } from '../types/api'

const router = useRouter()
const settingsOpen = ref(false)
const providerWarningDismissed = ref(false)
const configLoading = ref(true)
const publicConfig = ref<Awaited<ReturnType<typeof api.config>>>()
const running = ref(false)
const elapsedSeconds = ref(0)
const job = ref<GenerationJob>()
const generationResult = ref<HTMLElement>()
const progressSection = ref<HTMLElement>()
let stopped = false
let elapsedTimer: ReturnType<typeof setInterval> | undefined
const customPreferences = ref('')
const selectedBoundaries = ref<string[]>([])

const riddles = [
  { q: '什么东西早上四条腿，中午两条腿，晚上三条腿？', a: '人', hint: '与年龄有关' },
  { q: '什么东西越洗越脏？', a: '水', hint: '你用它洗东西' },
  { q: '什么东西打破才能用？', a: '鸡蛋', hint: '厨房里' },
  { q: '什么门永远关不上？', a: '球门', hint: '运动场' },
  { q: '什么东西你只能给别人，自己不能留？', a: '名字', hint: '身份标识' },
  { q: '什么东西丢了就再也找不回来？', a: '时间', hint: '一直在流逝' },
  { q: '什么东西有牙齿却不会咬人？', a: '梳子', hint: '日用品' },
  { q: '什么东西有头无脚？', a: '钉子', hint: '金属制品' },
  { q: '什么东西越黑越干净？', a: '黑板', hint: '教室里' },
  { q: '什么东西你左手能拿，右手不能拿？', a: '右手', hint: '自己的身体' },
  { q: '什么东西你越分享就越多？', a: '知识', hint: '不是物质的' },
  { q: '什么东西每个人都能看见，但自己看不见？', a: '自己的脸', hint: '需要镜子' },
  { q: '什么东西有脚却不能走路？', a: '桌子', hint: '家具' },
  { q: '什么东西越擦越湿？', a: '毛巾', hint: '浴室里' },
  { q: '什么东西白天跑到天上，晚上跑到地下？', a: '太阳', hint: '天文现象' },
  { q: '什么东西你有，别人也用你的，但你自己却很少用？', a: '名字', hint: '身份标识' },
  { q: '什么东西一旦说出口就消失了？', a: '秘密', hint: '不能告诉别人' },
  { q: '什么东西你越想抓住它，它跑得越快？', a: '沙子', hint: '海边有' },
  { q: '什么东西既不能吃也不能喝，但人人都需要？', a: '睡眠', hint: '每天都要' },
  { q: '什么东西你借了不用还？', a: '建议', hint: '别人给你的' },
  { q: '什么东西越老越值钱？', a: '古董', hint: '收藏品' },
  { q: '什么东西你越给出去自己就越多？', a: '爱', hint: '情感' },
  { q: '什么东西世界上只有一个，但每个人都拥有？', a: '名字', hint: '独一无二' },
  { q: '什么东西你能制造但看不见？', a: '声音', hint: '听觉的' },
  { q: '什么东西越热越小，越冷越大？', a: '冰', hint: '与温度有关' },
  { q: '什么东西你有一份，别人也有，但全世界没有两份相同的？', a: '指纹', hint: '唯一的' },
  { q: '什么东西越用越聪明？', a: '脑子', hint: '身体器官' },
  { q: '什么东西不是门却有钥匙？', a: '键盘', hint: '电脑上' },
  { q: '什么东西有手没有脚，有头没有眼？', a: '钟', hint: '看时间' },
  { q: '什么东西千言万语却无声？', a: '书', hint: '阅读' },
  { q: '什么东西你一转身它就跟着你？', a: '影子', hint: '光线下' },
  { q: '什么东西最公平？', a: '时间', hint: '每个人每天都一样多' },
  { q: '什么东西你越藏它越明显？', a: '秘密', hint: '不想让人知道的事' },
]
const riddleIndex = ref(Math.floor(Math.random() * riddles.length))
const riddleGuess = ref('')
const riddleFeedback = ref<'idle' | 'correct' | 'wrong'>('idle')
const riddleRevealed = ref(false)

function nextRiddle() {
  riddleIndex.value = (riddleIndex.value + 1) % riddles.length
  riddleGuess.value = ''
  riddleFeedback.value = 'idle'
  riddleRevealed.value = false
}

function checkRiddle() {
  const guess = riddleGuess.value.trim()
  if (!guess) return
  if (guess.includes(riddles[riddleIndex.value].a) || riddles[riddleIndex.value].a.includes(guess)) {
    riddleFeedback.value = 'correct'
  } else {
    riddleFeedback.value = 'wrong'
  }
}
const boundaryOptions = [
  { id: 'no_supernatural', label: '不使用超自然解释', value: '所有核心事件必须有现实可验证的解释' },
  { id: 'low_emotional_intensity', label: '降低情绪压迫感', value: '避免持续的高压、羞辱或强烈心理恐惧描写' },
]
const form = reactive<GenerationRequest>({
  theme: '', setting: '', tone: 'realistic', target_duration_minutes: 45,
  difficulty: 'medium', character_count: 4, variant_count: 3, realism: 'grounded',
  confession_policy: 'partial_then_full', content_constraints: [], language: 'zh-CN',
})
const generateVisuals = ref(false)
const formErrors = reactive<Record<string, string>>({})
const formErrorMessages = computed(() => Object.values(formErrors))
const imageGenerationAvailable = computed(() =>
  Boolean(publicConfig.value?.image_provider.enabled && publicConfig.value.image_provider.configured),
)
const scope = computed(() => generationScope(form.target_duration_minutes))
const scopeAdjustment = ref('')
const characterOptions = computed(() => Array.from({ length: scope.value.maxCharacters - 1 }, (_, index) => index + 2))
const variantOptions = computed(() => Array.from({ length: scope.value.maxVariants }, (_, index) => index + 1))

watch(
  () => form.target_duration_minutes,
  () => {
    const before = `${form.difficulty}/${form.character_count}/${form.variant_count}`
    normalizeGenerationScope(form)
    const after = `${form.difficulty}/${form.character_count}/${form.variant_count}`
    scopeAdjustment.value = before === after ? '' : '已根据新的游玩时长调整难度和案件规模。'
    clearFormError('duration')
  },
)

api.config()
  .then((config) => { publicConfig.value = config })
  .catch(() => { publicConfig.value = undefined })
  .finally(() => { configLoading.value = false })

const statusLabels: Record<string, string> = {
  pending: '准备开始', drafting: '构思人物与故事', parsing: '整理案件内容',
  normalizing: '完善细节与线索', compiling: '组合不同真相', validating: '检查故事是否自洽',
  simulating: '确认每种真相都能完成', repairing: '自动调整不合理之处', completed: '案件已经准备好',
  failed: '这次没有生成成功',
}
const stageLabels: Record<string, string> = {
  blueprint: '规划故事与真相框架',
  shared_content: '构建人物与公共线索',
  shared_characters: '塑造案件人物',
  variants: '生成不同的真相',
  assembling: '组装完整案件',
  repairing_shared: '调整人物与公共线索',
  repairing_variants: '修正未通过检查的真相',
  repairing_full: '重新整理案件结构',
  generating_visuals: '绘制案件氛围图',
}
function eventLabel(event: GenerationJob['events'][number]) {
  const base = event.stage ? stageLabels[event.stage] : undefined
  if ((event.stage === 'shared_characters' || event.stage === 'variants' || event.stage === 'repairing_variants') && event.total !== undefined) {
    return `${base} · ${event.completed ?? 0}/${event.total}`
  }
  return base ?? statusLabels[event.to] ?? event.to
}
const statusDescription = computed(() => {
  if (job.value?.status === 'failed') return '你可以检查设置、调整偏好后重新尝试。'
  if (job.value?.status === 'completed') return '案件已经加入列表，可以开始一局新的调查。'
  return '生成进度会自动更新。复杂案件通常需要几十秒到几分钟。'
})
const visibleErrorMessage = computed(() => {
  if (job.value?.error_code === 'GENERATION_PROVIDER_TIMEOUT') {
    return '模型在等待时间内没有完成案件。你的填写内容仍在，可以稍后重试，或换用响应更快的模型。'
  }
  if (job.value?.error_code === 'GENERATION_PROVIDER_OUTPUT_TRUNCATED') {
    return '模型返回的案件内容没有完整结束。你的填写内容仍在，可以重新生成；如果反复出现，请换用支持更长输出的模型。'
  }
  if (job.value?.error_code === 'GENERATION_PROVIDER_INVALID_RESPONSE') {
    return '模型返回的内容无法整理成完整案件。你的填写内容仍在，可以重试或更换兼容性更好的模型。'
  }
  return job.value?.error_message
})
const visibleEvents = computed(() => {
  if (!job.value) return []
  const events = job.value.events.length > 0
    ? job.value.events
    : [{ sequence: 0, to: job.value.status, ...(job.value.error_code ? { error_code: job.value.error_code } : {}) }]
  return events.reduce<typeof events>((visible, event) => {
    const previous = visible.at(-1)
    if (event.stage && previous?.stage === event.stage) visible[visible.length - 1] = event
    else visible.push(event)
    return visible
  }, [])
})
const statusLabel = computed(() => {
  if (job.value?.status === 'completed' || job.value?.status === 'failed') {
    return statusLabels[job.value.status]
  }
  const latest = visibleEvents.value.at(-1)
  return latest ? eventLabel(latest) : statusLabels[job.value?.status ?? 'pending'] ?? '等待任务'
})

watch(
  () => visibleEvents.value.length,
  async (length, previousLength) => {
    if (length <= previousLength) return
    await nextTick()
    const items = Array.from(generationResult.value?.querySelectorAll<HTMLElement>('[data-generation-event]') ?? [])
      .filter((item) => Number(item.dataset.generationEvent) >= previousLength)
    animateGenerationEvents(items)
  },
)

onBeforeUnmount(() => {
  stopped = true
  stopElapsedTimer()
})

function startElapsedTimer() {
  stopElapsedTimer()
  const startedAt = Date.now()
  elapsedSeconds.value = 0
  elapsedTimer = setInterval(() => {
    elapsedSeconds.value = Math.floor((Date.now() - startedAt) / 1000)
  }, 1000)
}

function stopElapsedTimer() {
  if (elapsedTimer !== undefined) clearInterval(elapsedTimer)
  elapsedTimer = undefined
}

async function generate() {
  if (!validateForm()) return
  running.value = true; job.value = undefined
  startElapsedTimer()
  await nextTick()
  if (progressSection.value) {
    animateGenerationStart(progressSection.value)
    progressSection.value.scrollIntoView({
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth',
      block: 'start',
    })
  }
  try {
    job.value = await api.generateCase({ ...form,
      content_constraints: [
        ...boundaryOptions.filter(option => selectedBoundaries.value.includes(option.id)).map(option => option.value),
        ...customPreferences.value.split('\n').map(value => value.trim()).filter(Boolean),
      ],
      generate_visuals: generateVisuals.value && imageGenerationAvailable.value,
    })
    while (!stopped && job.value.status !== 'completed' && job.value.status !== 'failed') {
      await new Promise(resolve => setTimeout(resolve, 700))
      job.value = await api.generationJob(job.value.job_id)
    }
    if (!stopped && job.value.status === 'completed') {
      await router.replace(job.value.case_id
        ? { name: 'case-brief', params: { caseId: job.value.case_id } }
        : { name: 'cases' })
    }
  } catch (error) {
    job.value = { job_id: '', status: 'failed', attempt_count: 0, repair_count: 0,
      error_message: error instanceof Error ? error.message : '生成失败', events: [], updated_at: '' }
  } finally {
    running.value = false
    stopElapsedTimer()
  }
}

function validateForm() {
  for (const key of Object.keys(formErrors)) delete formErrors[key]
  if (!form.theme.trim()) formErrors.theme = '请先写下你想体验的案件主题。'
  if (form.target_duration_minutes < 10 || form.target_duration_minutes > 120) formErrors.duration = '游玩时长需要在 10 到 120 分钟之间。'
  if (form.character_count < 2 || form.character_count > 4) formErrors.characters = '角色数量需要在 2 到 4 人之间。'
  if (form.variant_count < 1 || form.variant_count > 5) formErrors.variants = '真相数量需要在 1 到 5 个之间。'
  if (formErrorMessages.value.length) {
    void nextTick(() => document.querySelector<HTMLElement>('.generation-form [aria-invalid="true"]')?.focus())
    return false
  }
  return true
}

function clearFormError(key: string) {
  delete formErrors[key]
}

async function resume() {
  if (!job.value?.job_id || running.value) return
  running.value = true
  startElapsedTimer()
  try {
    job.value = await api.resumeGenerationJob(job.value.job_id)
    while (!stopped && job.value.status !== 'completed' && job.value.status !== 'failed') {
      await new Promise(resolve => setTimeout(resolve, 700))
      job.value = await api.generationJob(job.value.job_id)
    }
    if (!stopped && job.value.status === 'completed') {
      await router.replace(job.value.case_id
        ? { name: 'case-brief', params: { caseId: job.value.case_id } }
        : { name: 'cases' })
    }
  } catch {
    // keep the current job state on error
  } finally {
    running.value = false
    stopElapsedTimer()
  }
}
</script>

<template>
  <div class="page-shell generation-page"><AppHeader back-label="返回案件列表" @back="router.push('/cases')" @settings="settingsOpen = true" />
    <NoticeBar v-if="!configLoading && !publicConfig?.configured && !providerWarningDismissed" message="尚未配置文本生成服务。你可以先填写偏好，生成前请到设置中完成配置。" tone="warning" @close="providerWarningDismissed = true" />
    <main class="generation-main"><header><p class="eyebrow">创建你的谜局</p><h1>生成新案件</h1><p>描述你想体验的故事。谜局AI会构思人物、线索和多种可能的真相，并在准备完成后加入案件列表。</p></header>
      <form class="generation-form" novalidate @submit.prevent="generate">
        <div v-if="formErrorMessages.length" class="form-error-summary wide" role="alert">
          <span><AppIcon name="warning" :size="20" /></span><div><strong>还需要补充一些内容</strong><p>{{ formErrorMessages[0] }}</p></div>
        </div>
        <label class="wide" :class="{ 'field-invalid': formErrors.theme }">主题<input v-model="form.theme" maxlength="4000" :aria-invalid="Boolean(formErrors.theme)" @input="clearFormError('theme')" /><small v-if="formErrors.theme" class="field-error">{{ formErrors.theme }}</small></label>
        <label class="wide">场景偏好（可选）<input v-model="form.setting" maxlength="4000" placeholder="例如：海港仓库；或画廊、办公室和雨夜街道" /><small>可以写一个或多个地点，用逗号、顿号或一句话描述都可以。留空时会根据主题自动构思。</small></label>
        <label :class="{ 'field-invalid': formErrors.duration }">预计游玩时长<select v-model.number="form.target_duration_minutes" :aria-invalid="Boolean(formErrors.duration)"><option :value="10">10 分钟 · 微型案件</option><option :value="20">20 分钟 · 短篇</option><option :value="30">30 分钟 · 标准短案</option><option :value="45">45 分钟 · 完整案件</option><option :value="60">60 分钟 · 深度调查</option><option :value="90">90 分钟 · 长篇</option><option :value="120">120 分钟 · 大型案件</option></select><small>{{ scope.description }}</small><small v-if="scopeAdjustment" class="scope-adjustment">{{ scopeAdjustment }}</small><small v-if="formErrors.duration" class="field-error">{{ formErrors.duration }}</small></label>
        <label>推理难度<select v-model="form.difficulty"><option value="easy">简单 · 线索指向清晰</option><option value="medium" :disabled="!scope.allowedDifficulties.includes('medium')">中等 · 需要核对矛盾</option><option value="hard" :disabled="!scope.allowedDifficulties.includes('hard')">困难 · 多条证据链交叉</option></select><small>时长不足时不会开放无法公平展开的难度。</small></label>
        <label>叙事类型<select v-model="form.tone"><option value="realistic">现实克制</option><option value="noir">黑色叙事</option><option value="suspenseful">悬疑紧张</option><option value="light">轻松明快</option></select></label>
        <label>现实程度<select v-model="form.realism"><option value="grounded">写实可信</option><option value="dramatic">戏剧化</option></select></label>
        <label>结案偏好<select v-model="form.confession_policy"><option value="partial_then_full">逐步披露，可完整承认</option><option value="evidence_only_allowed">允许仅凭证据结案</option><option value="never_required">不要求认罪</option></select></label>
        <label>生成语言<select v-model="form.language"><option value="zh-CN">简体中文</option><option value="zh-TW">繁体中文</option><option value="en-US">English</option></select></label>
        <label :class="{ 'field-invalid': formErrors.characters }">主要角色<select v-model.number="form.character_count" :aria-invalid="Boolean(formErrors.characters)" @change="clearFormError('characters')"><option v-for="count in characterOptions" :key="count" :value="count">{{ count }} 人</option></select><small>当前时长最多容纳 {{ scope.maxCharacters }} 名主要角色。</small><small v-if="formErrors.characters" class="field-error">{{ formErrors.characters }}</small></label>
        <label :class="{ 'field-invalid': formErrors.variants }">可能的真相<select v-model.number="form.variant_count" :aria-invalid="Boolean(formErrors.variants)" @change="clearFormError('variants')"><option v-for="count in variantOptions" :key="count" :value="count">{{ count }} 种</option></select><small>每一种真相都会单独检查是否能够完成。</small><small v-if="formErrors.variants" class="field-error">{{ formErrors.variants }}</small></label>
        <fieldset class="generation-boundaries wide"><legend>故事偏好</legend><p>选择你希望采用的叙事方向；未选择时会根据主题自然处理。</p><div><label v-for="option in boundaryOptions" :key="option.id"><input v-model="selectedBoundaries" type="checkbox" :value="option.id" /><span>{{ option.label }}</span></label></div></fieldset>
        <label class="wide">额外偏好（可选）<textarea v-model="customPreferences" rows="3" maxlength="1000" placeholder="例如：希望故事更侧重人物关系，不出现浪漫情节。每行一条。" /><small>这里只影响题材和表达风格，不能降低线索公平性或改变游戏规则。</small></label>
        <label class="wide generation-visual-option" :class="{ unavailable: !imageGenerationAvailable }">
          <input v-model="generateVisuals" type="checkbox" :disabled="!imageGenerationAvailable" />
          <span><strong>为案件生成配图</strong><small>{{ imageGenerationAvailable ? '生成封面、人物与地点氛围图；配图不会作为案件线索。' : '请先在设置中单独启用并配置图片生成服务。' }}</small></span>
        </label>
        <button class="primary-button wide" :disabled="running">{{ running ? '正在准备案件…' : '生成案件' }}</button>
      </form>
      <section v-if="running && !job" ref="progressSection" class="generation-result generation-starting" role="status" aria-live="polite" tabindex="-1">
        <div class="generation-progress-visual" aria-hidden="true"><span class="generation-progress-orbit"><i /></span><span class="generation-progress-dots"><i /><i /><i /></span></div>
        <header class="generation-result-heading">
          <span>准备中</span>
          <h2>正在创建生成任务</h2>
          <p>请求已提交。模型响应可能需要一些时间，请勿重复点击。</p>
          <div class="generation-wait-time" aria-hidden="true">已等待 {{ elapsedSeconds }} 秒</div>
        </header>
      </section>
      <section v-if="job" ref="generationResult" class="generation-result" role="status" aria-live="polite">
        <header class="generation-result-heading">
          <span>{{ job.status === 'completed' ? '已完成' : job.status === 'failed' ? '未完成' : '生成中' }}</span>
          <h2>{{ statusLabel }}</h2>
          <p>{{ statusDescription }}</p>
          <div v-if="running" class="generation-live-state"><span aria-hidden="true"><i /><i /><i /></span>任务正在后台继续 · 已等待 {{ elapsedSeconds }} 秒</div>
        </header>
        <div class="generation-event-list">
          <article v-for="(event, index) in visibleEvents" :key="`${event.sequence}-${event.to}`" :class="{ current: index === visibleEvents.length - 1, failed: event.to === 'failed' }" :data-generation-event="index">
            <i class="generation-event-dot" aria-hidden="true" />
            <span>步骤 {{ index + 1 }}</span>
            <div><strong>{{ eventLabel(event) }}</strong></div>
          </article>
        </div>
        <div v-if="job.error_code" class="generation-error">
          <span class="generation-error-icon"><AppIcon name="warning" :size="22" /></span>
          <div><strong>这次没有生成成功</strong><p>{{ visibleErrorMessage }}</p>
            <div class="generation-error-actions">
              <button v-if="job.can_resume" class="primary-button" type="button" :disabled="running" @click="resume">{{ running ? '正在继续…' : '从断点继续' }}</button>
              <button class="secondary-button" type="button" @click="job = undefined">重新填写</button>
            </div>
            <details><summary>查看技术详情</summary><code>{{ job.error_code }}</code><p class="generation-error-detail">{{ job.error_message }}</p></details>
          </div>
        </div>
        <div v-if="running && job.status !== 'completed' && job.status !== 'failed'" class="generation-minigame">
          <div class="minigame-header"><span>等案件生成的时候，猜个谜吧</span><button type="button" class="minigame-skip" @click="nextRiddle">换一题</button></div>
          <p class="minigame-question">{{ riddles[riddleIndex].q }}</p>
          <div class="minigame-input-row">
            <input v-model="riddleGuess" class="minigame-input" placeholder="输入你的答案" @keydown.enter="checkRiddle" @input="riddleFeedback = 'idle'" />
            <button class="secondary-button" type="button" @click="checkRiddle">猜</button>
          </div>
          <p v-if="riddleFeedback === 'correct'" class="minigame-feedback correct">猜对了！再来一题？</p>
          <p v-else-if="riddleFeedback === 'wrong'" class="minigame-feedback wrong">不对，再想想？</p>
          <p v-if="riddleRevealed" class="minigame-hint">答案：{{ riddles[riddleIndex].a }}</p>
          <button v-if="riddleFeedback === 'wrong' && !riddleRevealed" type="button" class="minigame-reveal" @click="riddleRevealed = true">看答案</button>
        </div>
        <footer v-if="!running"><span>已尝试 {{ job.attempt_count }} 次</span><span>自动调整 {{ job.repair_count }} 次</span><span v-if="job.result_path">已加入案件列表</span></footer>
      </section>
    </main><SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
