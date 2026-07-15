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
import type { SessionMode } from '../types/api'

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
let entrance: ReturnType<typeof createBriefEntrance>

const caseId = computed(() => String(route.params.caseId))
const timeline = computed(() =>
  [...(store.activeCase?.facts ?? [])].sort((a, b) => formatStoryTime(a.happened_at).localeCompare(formatStoryTime(b.happened_at))),
)
const atmosphereVisuals = computed(() =>
  (store.activeCase?.visual_assets ?? []).filter((asset) =>
    asset.visual_type === 'scene_background' || asset.visual_type === 'location_atmosphere' || asset.visual_type === 'chapter_illustration'),
)

onMounted(async () => {
  await Promise.all([store.loadCase(caseId.value), store.config ? Promise.resolve() : store.bootstrap()])
  if (store.config?.configured) selectedMode.value = 'llm'
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
</script>

<template>
  <div class="page-shell brief-shell">
    <AppHeader back-label="返回案件" @back="router.push('/')" @settings="settingsOpen = true" />
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
      <section class="brief-timeline">
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
        <div class="mode-choice" role="radiogroup" aria-label="真相模式">
          <button type="button" :class="{ selected: truthMode === 'default' }" @click="truthMode = 'default'"><strong>经典真相</strong><span>使用作者推荐版本</span></button>
          <button type="button" :class="{ selected: truthMode === 'random' }" @click="truthMode = 'random'"><strong>随机真相</strong><span>按 seed 稳定选择</span></button>
        </div>
        <details class="developer-variant">
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
