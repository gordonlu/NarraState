<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { describeFact, formatStoryTime } from '../lib/present'
import { api } from '../lib/api'
import { createBriefEntrance, createCaseEntryTransition } from '../lib/appMotion'
import { useSessionStore } from '../stores/session'
import type { CaseDetail, SessionMode, VisualGenerationMode } from '../types/api'

const route = useRoute()
const router = useRouter()
const store = useSessionStore()
const settingsOpen = ref(false)
const selectedMode = ref<SessionMode>('mock')
const truthMode = ref<'default' | 'random' | 'specific'>('default')
const specificVariant = ref('')
const seed = ref<number>()
const briefMain = ref<HTMLElement>()
const transitionCurtain = ref<HTMLElement>()
const beginning = ref(false)
const visualOperation = ref<VisualGenerationMode>()
const visualFeedback = ref<{ tone: 'success' | 'warning' | 'error'; message: string }>()
const confirmingRegeneration = ref(false)
const regenerateConfirmButton = ref<HTMLButtonElement>()
let entrance: ReturnType<typeof createBriefEntrance>

const caseId = computed(() => String(route.params.caseId))
const resumableHere = computed(() =>
  store.resumableSession?.case_id === caseId.value ? store.resumableSession : undefined,
)
const generatedCase = computed(() => store.activeCase?.visual_status !== undefined)
const timeline = computed(() =>
  [...(store.activeCase?.facts ?? [])]
    .filter(fact => fact.happened_at)
    .sort((a, b) => formatStoryTime(a.happened_at).localeCompare(formatStoryTime(b.happened_at))),
)
const backgroundFacts = computed(() =>
  (store.activeCase?.facts ?? []).filter(fact => !fact.happened_at && !fact.tags.includes('身份')),
)
const atmosphereVisuals = computed(() =>
  (store.activeCase?.visual_assets ?? []).filter((asset) =>
    asset.visual_type === 'scene_background' || asset.visual_type === 'location_atmosphere' || asset.visual_type === 'chapter_illustration'),
)
function visualFailureMessage(status: NonNullable<CaseDetail['visual_status']>) {
  const details: Record<string, string> = {
    bad_request: '图片服务拒绝了请求（HTTP 400），通常是模型、图片尺寸或响应格式不兼容。',
    endpoint_not_found: '图片服务地址不存在对应的生图接口（HTTP 404）。请检查独立的图片 Base URL 与模型。',
    forbidden: '图片服务拒绝访问（HTTP 403）。请检查模型权限和账户权限。',
    incompatible_response: '图片服务响应中没有返回兼容的图片数据。',
    unauthorized: '图片服务没有通过身份验证（HTTP 401）。请检查图片 API Key。',
    rate_limited: '图片服务请求过多（HTTP 429），请稍后重试。',
    timeout: '图片服务等待超时，可以稍后重试或更换响应更快的图片模型。',
    not_configured: '图片服务尚未完整配置。请在设置中检查图片服务。',
    report_unavailable: '配图结果暂时无法读取。',
    provider_failed: '图片服务没有返回可用图片。请检查图片服务配置。',
  }
  const summary = details[status.failure_code ?? 'provider_failed']
  return status.failure_detail ? `${summary} 服务返回：${status.failure_detail}` : summary
}
const visualNotice = computed(() => {
  const status = store.activeCase?.visual_status
  if (!status?.requested || status.state === 'ready') return undefined
  if (status.state === 'partial') {
    return { title: `已生成 ${status.generated} 张配图`, detail: '部分配图服务请求没有完成，当前可用图片仍会正常显示，不影响案件内容。' }
  }
  return { title: '本案配图未能生成', detail: `${visualFailureMessage(status)} 案件仍可完整游玩。` }
})
const imageProviderAvailable = computed(() =>
  Boolean(store.config?.image_provider.enabled && store.config.image_provider.configured),
)
const visualsComplete = computed(() => store.activeCase?.visual_status?.state === 'ready')

onMounted(async () => {
  await Promise.all([store.loadCase(caseId.value), store.config ? Promise.resolve() : store.bootstrap()])
  if (store.config?.configured) selectedMode.value = 'llm'
  if (generatedCase.value) truthMode.value = 'random'
  await nextTick()
  if (briefMain.value) entrance = createBriefEntrance(briefMain.value)
})

onBeforeUnmount(() => entrance?.kill())

async function begin() {
  if (beginning.value) return
  beginning.value = true
  try {
    const created = await api.createGame({ case_id: caseId.value,
      variant_selection: truthMode.value === 'specific' ? { mode: 'specific', variant_id: specificVariant.value } : { mode: truthMode.value },
      ...(seed.value == null ? {} : { seed: seed.value }), mode: selectedMode.value })
    const session = await store.restoreSession(created.session_id, selectedMode.value)
    if (briefMain.value && transitionCurtain.value) {
      await createCaseEntryTransition(briefMain.value, transitionCurtain.value)
    }
    await router.push(`/sessions/${session.session_id}`)
  } catch {
    // Store exposes the actionable error.
    beginning.value = false
  }
}

async function continueInvestigation() {
  if (!resumableHere.value) return
  await router.push(`/sessions/${resumableHere.value.session_id}`)
}

async function generateVisuals(mode: VisualGenerationMode) {
  if (visualOperation.value) return
  confirmingRegeneration.value = false
  visualOperation.value = mode
  visualFeedback.value = undefined
  try {
    const result = await api.generateCaseVisuals(caseId.value, mode)
    await store.loadCase(caseId.value)
    if (result.attempted === 0) {
      visualFeedback.value = { tone: 'success', message: '本案配图已经完整，无需补充。' }
    } else if (result.updated === 0) {
      visualFeedback.value = { tone: 'error', message: `${visualFailureMessage(result.visual_status)} 原有配图已保留。` }
    } else if (result.failed > 0) {
      visualFeedback.value = { tone: 'warning', message: `已更新 ${result.updated} 张，另有 ${result.failed} 张未完成；原有图片不会被清空。` }
    } else {
      visualFeedback.value = { tone: 'success', message: `已更新 ${result.updated} 张配图。` }
    }
  } catch {
    visualFeedback.value = { tone: 'error', message: '无法开始配图生成，请检查独立图片 Provider 的地址、模型和 API Key。' }
  } finally {
    visualOperation.value = undefined
  }
}

async function requestRegenerateVisuals() {
  if (visualOperation.value) return
  confirmingRegeneration.value = true
  await nextTick()
  regenerateConfirmButton.value?.focus()
}

function cancelRegenerateVisuals() {
  confirmingRegeneration.value = false
}
</script>

<template>
  <div class="page-shell brief-shell">
    <AppHeader back-label="返回案件列表" @back="router.push('/cases')" @settings="settingsOpen = true" />
    <NoticeBar v-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />
    <main v-if="store.activeCase" ref="briefMain" class="brief-main">
      <section class="brief-lead">
        <span class="brief-index">案件简报</span>
        <h1>{{ store.activeCase.title }}</h1>
        <p>{{ store.activeCase.summary }}</p>
      </section>
      <section v-if="atmosphereVisuals.length" class="brief-visuals" aria-labelledby="visuals-title">
        <div class="section-heading"><h2 id="visuals-title">场景氛围</h2><span>视觉示意，不作为案件证据</span></div>
        <div class="brief-visual-grid">
          <figure v-for="asset in atmosphereVisuals" :key="asset.id">
            <img :src="asset.url" :alt="asset.alt_text" />
            <figcaption>{{ asset.alt_text }}</figcaption>
          </figure>
        </div>
      </section>
      <aside v-else-if="visualNotice" class="brief-visual-notice" role="status">
        <span><AppIcon name="warning" :size="20" /></span>
        <div><strong>{{ visualNotice.title }}</strong><p>{{ visualNotice.detail }}</p></div>
      </aside>
      <section v-if="store.activeCase.visual_status" class="brief-visual-manager" aria-labelledby="visual-manager-title">
        <div>
          <strong id="visual-manager-title">配图管理</strong>
          <p>补齐缺失图片，或重新生成全部配图。已有图片只会在新图片成功后替换，案件事实和存档不会改变。</p>
        </div>
        <div v-if="imageProviderAvailable" class="brief-visual-actions">
          <button class="secondary-button" type="button" :disabled="Boolean(visualOperation) || visualsComplete" :title="visualsComplete ? '本案计划配图已经全部生成' : undefined" @click="generateVisuals('append_missing')">
            {{ visualOperation === 'append_missing' ? '正在补充配图…' : visualsComplete ? '补充缺失配图 · 已完成' : '补充缺失配图' }}
          </button>
          <button class="secondary-button" type="button" :disabled="Boolean(visualOperation)" aria-controls="regenerate-visual-confirmation" :aria-expanded="confirmingRegeneration" @click="requestRegenerateVisuals">
            {{ visualOperation === 'regenerate_all' ? '正在重新生成…' : '重新生成全部' }}
          </button>
        </div>
        <button v-else class="secondary-button" type="button" @click="settingsOpen = true">配置图片服务</button>
        <div v-if="confirmingRegeneration" id="regenerate-visual-confirmation" class="brief-visual-confirmation" role="alert" @keydown.esc="cancelRegenerateVisuals">
          <div>
            <strong>确认重新生成全部配图？</strong>
            <p>这会再次调用图片服务，可能产生费用。只有成功生成的新图片会替换旧图，案件内容和存档不会改变。</p>
          </div>
          <div class="brief-visual-confirmation-actions">
            <button class="secondary-button" type="button" @click="cancelRegenerateVisuals">取消</button>
            <button ref="regenerateConfirmButton" class="primary-button" type="button" @click="generateVisuals('regenerate_all')">确认并重新生成</button>
          </div>
        </div>
        <p v-if="visualOperation" class="brief-visual-progress" role="status">图片服务正在处理，请保持当前页面打开。这可能需要几十秒。</p>
        <p v-if="visualFeedback" class="brief-visual-feedback" :class="`is-${visualFeedback.tone}`" :role="visualFeedback.tone === 'error' ? 'alert' : 'status'">{{ visualFeedback.message }}</p>
      </section>
      <section v-if="backgroundFacts.length" class="brief-background">
        <div class="section-heading"><h2>调查前已知</h2><span>进入案件前已经公开的背景</span></div>
        <ul><li v-for="fact in backgroundFacts" :key="fact.id">{{ describeFact(fact) }}</li></ul>
      </section>
      <section v-if="timeline.length" class="brief-timeline">
        <div class="section-heading"><h2>公开事件线</h2><span>仅包含调查开始时可见的信息</span></div>
        <ol>
          <li v-for="fact in timeline" :key="fact.id">
            <time>{{ formatStoryTime(fact.happened_at) }}</time>
            <span>{{ describeFact(fact) }}</span>
          </li>
        </ol>
      </section>
      <section class="brief-people">
        <div class="section-heading"><h2>相关人物</h2></div>
        <article v-for="character in store.activeCase.characters" :key="character.id" class="profile-row">
          <div class="profile-mark"><img v-if="character.portrait_url" :src="character.portrait_url" :alt="`${character.name}的角色示意头像`" /><template v-else>{{ character.name.slice(0, 1) }}</template></div>
          <div><h3>{{ character.name }} <span>{{ character.role }}</span></h3><p>{{ character.public_profile }}</p></div>
        </article>
      </section>
      <section class="brief-evidence">
        <div class="section-heading"><h2>初始线索</h2><span>{{ store.activeCase.evidence.length }} 条</span></div>
        <div v-if="store.activeCase.evidence.length === 0" class="empty-state">暂无可出示线索</div>
        <div v-for="item in store.activeCase.evidence" :key="item.id" class="brief-evidence-row">
          <AppIcon name="document" /><div><strong>{{ item.title }}</strong><p>{{ item.description }}</p></div>
        </div>
      </section>
      <section class="brief-actions">
        <div v-if="resumableHere" class="brief-resume">
          <span>调查仍在进行</span>
          <strong>已进行 {{ resumableHere.current_turn }} 回合</strong>
          <p>继续会读取服务端保存的最新进度，不会重新选择或改变本局真相。</p>
          <button class="primary-button begin-button" type="button" @click="continueInvestigation">继续调查<AppIcon name="chevron-right" /></button>
          <div class="brief-new-game-divider"><span>或开始一个新游戏</span></div>
        </div>
        <div v-if="generatedCase" class="mode-choice single" role="radiogroup" aria-label="真相模式">
          <button type="button" class="selected" @click="truthMode = 'random'"><strong>随机真相</strong><span>开局锁定一个已经验证的真相版本</span></button>
        </div>
        <div v-else class="mode-choice" role="radiogroup" aria-label="真相模式">
          <button type="button" :class="{ selected: truthMode === 'default' }" @click="truthMode = 'default'"><strong>经典真相</strong><span>使用作者推荐版本</span></button>
          <button type="button" :class="{ selected: truthMode === 'random' }" @click="truthMode = 'random'"><strong>随机真相</strong><span>按 seed 稳定选择</span></button>
        </div>
        <details v-if="store.config?.developer_mode" class="developer-variant">
          <summary><span>开发者真相设置</span><small>用于内容验证与稳定复现</small></summary>
          <div class="developer-variant-fields">
            <label class="developer-radio">
              <input v-model="truthMode" type="radio" value="specific" />
              <span><strong>指定真相变体</strong><small>普通玩家模式不会展示变体信息</small></span>
            </label>
            <label class="developer-field">
              <span>真相变体 ID</span>
              <input v-model="specificVariant" :disabled="truthMode !== 'specific'" placeholder="variant-id" autocomplete="off" />
            </label>
            <label class="developer-field">
              <span>固定 Seed</span>
              <input v-model.number="seed" type="number" min="0" placeholder="自动生成" inputmode="numeric" />
            </label>
          </div>
        </details>
        <div class="mode-choice" role="radiogroup" aria-label="会话模式">
          <button type="button" :class="{ selected: selectedMode === 'mock' }" @click="selectedMode = 'mock'"><strong>Mock 模式</strong><span>无需配置，体验完整状态机</span></button>
          <button type="button" :disabled="!store.config?.configured" :class="{ selected: selectedMode === 'llm' }" @click="selectedMode = 'llm'"><strong>模型模式</strong><span>{{ store.config?.configured ? store.config.model : '请先配置模型' }}</span></button>
        </div>
        <button class="primary-button begin-button" type="button" :disabled="beginning" @click="begin">{{ beginning ? '正在进入…' : '开始调查' }}<AppIcon name="chevron-right" /></button>
      </section>
    </main>
    <div ref="transitionCurtain" class="case-entry-curtain" aria-hidden="true"><span>进入案件</span></div>
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
