<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { describeFact, formatStoryTime } from '../lib/present'
import { useSessionStore } from '../stores/session'
import type { SessionMode } from '../types/api'

const route = useRoute()
const router = useRouter()
const store = useSessionStore()
const settingsOpen = ref(false)
const selectedMode = ref<SessionMode>('mock')

const caseId = computed(() => String(route.params.caseId))
const timeline = computed(() =>
  [...(store.activeCase?.facts ?? [])].sort((a, b) => formatStoryTime(a.happened_at).localeCompare(formatStoryTime(b.happened_at))),
)

onMounted(async () => {
  await Promise.all([store.loadCase(caseId.value), store.config ? Promise.resolve() : store.bootstrap()])
  if (store.config?.configured) selectedMode.value = 'llm'
})

async function begin() {
  try {
    const session = await store.createSession(caseId.value, selectedMode.value)
    await router.push(`/sessions/${session.session_id}`)
  } catch {
    // Store exposes the actionable error.
  }
}
</script>

<template>
  <div class="page-shell brief-shell">
    <AppHeader back-label="返回案件" @back="router.push('/')" @settings="settingsOpen = true" />
    <NoticeBar v-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />
    <main v-if="store.activeCase" class="brief-main">
      <section class="brief-lead">
        <span class="brief-index">案件简报</span>
        <h1>{{ store.activeCase.title }}</h1>
        <p>{{ store.activeCase.summary }}</p>
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
          <div class="profile-mark">{{ character.name.slice(0, 1) }}</div>
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
        <div class="mode-choice" role="radiogroup" aria-label="会话模式">
          <button type="button" :class="{ selected: selectedMode === 'mock' }" @click="selectedMode = 'mock'"><strong>Mock 模式</strong><span>无需配置，体验完整状态机</span></button>
          <button type="button" :disabled="!store.config?.configured" :class="{ selected: selectedMode === 'llm' }" @click="selectedMode = 'llm'"><strong>模型模式</strong><span>{{ store.config?.configured ? store.config.model : '请先配置模型' }}</span></button>
        </div>
        <button class="primary-button begin-button" type="button" :disabled="store.loading" @click="begin">{{ store.loading ? '正在创建…' : '开始调查' }}<AppIcon name="chevron-right" /></button>
      </section>
    </main>
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
