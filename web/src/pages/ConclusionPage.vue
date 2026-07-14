<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { api } from '../lib/api'
import { describeFact, formatStoryTime, resultLabel } from '../lib/present'
import { useSessionStore } from '../stores/session'
import type { ConclusionReport } from '../types/api'

const route = useRoute()
const router = useRouter()
const store = useSessionStore()
const report = ref<ConclusionReport>()
const settingsOpen = ref(false)
const loading = ref(true)
const sessionId = computed(() => String(route.params.sessionId))

onMounted(async () => {
  try {
    const session = await store.restoreSession(sessionId.value)
    if (session.status !== 'Resolved') {
      await router.replace(`/sessions/${session.session_id}`)
      return
    }
    report.value = await api.conclusion(sessionId.value)
  } finally {
    loading.value = false
  }
})

async function restart() {
  const session = await store.restartCurrent()
  await router.push(`/sessions/${session.session_id}`)
}
</script>

<template>
  <div class="page-shell conclusion-shell">
    <AppHeader :case-title="store.activeCase?.title" @settings="settingsOpen = true" />
    <main v-if="report" class="conclusion-main">
      <section class="conclusion-lead">
        <span>案件已结案</span>
        <h1>{{ resultLabel(report.result) }}</h1>
        <p>{{ report.epilogue }}</p>
        <dl><div><dt>调查回合</dt><dd>{{ report.turn_count }}</dd></div><div><dt>完整陈述</dt><dd>{{ report.confessed ? '已取得' : '未取得' }}</dd></div><div><dt>决定性线索</dt><dd>{{ report.decisive_evidence.length }}</dd></div></dl>
      </section>
      <section class="conclusion-timeline">
        <div class="section-heading"><h2>真相事件线</h2><span>结案后公开</span></div>
        <ol><li v-for="fact in report.truth_timeline" :key="fact.id"><time>{{ formatStoryTime(fact.happened_at) }}</time><span>{{ describeFact(fact) }}</span></li></ol>
      </section>
      <section class="conclusion-evidence">
        <div class="section-heading"><h2>决定性线索</h2></div>
        <article v-for="item in report.decisive_evidence" :key="item.id"><AppIcon name="document" /><div><h3>{{ item.title }}</h3><p>{{ item.description }}</p></div></article>
      </section>
      <section class="conclusion-reasoning"><div class="section-heading"><h2>你的关键推理</h2></div><blockquote>{{ report.reasoning || '本次判断未填写推理说明。' }}</blockquote></section>
      <footer class="conclusion-actions"><RouterLink class="secondary-button link-button" to="/">返回案件列表</RouterLink><button class="primary-button" type="button" @click="restart">重新开始<AppIcon name="chevron-right" /></button></footer>
    </main>
    <div v-else-if="loading" class="full-page-loading">正在生成结案报告…</div>
    <div v-else class="full-page-loading">无法读取结案报告。</div>
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
