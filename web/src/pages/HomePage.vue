<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { formatSavedAt } from '../lib/present'
import { useSessionStore } from '../stores/session'

const store = useSessionStore()
const router = useRouter()
const settingsOpen = ref(false)

onMounted(() => store.bootstrap())

async function resume() {
  if (!store.lastSession) return
  try {
    const session = await store.restoreSession(store.lastSession.sessionId, store.lastSession.mode)
    await router.push(session.status === 'Resolved' ? `/sessions/${session.session_id}/conclusion` : `/sessions/${session.session_id}`)
  } catch {
    // Store exposes the actionable error.
  }
}
</script>

<template>
  <div class="page-shell home-shell">
    <AppHeader @settings="settingsOpen = true" />
    <NoticeBar v-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />
    <main class="home-main">
      <section class="home-intro">
        <h1>选择一个故事，开始调查</h1>
        <p>你的问题推动对话，证据和陈述决定结果。案件状态由本机服务保存。</p>
      </section>

      <section class="case-rail" aria-labelledby="cases-title">
        <header class="section-heading">
          <h2 id="cases-title">内置案件</h2>
          <span>{{ store.cases.length }} 个可用案件</span>
        </header>
        <div v-if="store.loading && store.cases.length === 0" class="empty-state">正在读取案件…</div>
        <RouterLink v-for="item in store.cases" :key="item.id" class="case-row" :to="`/cases/${item.id}`">
          <div class="case-media" aria-hidden="true"><span>{{ item.title.slice(0, 1) }}</span></div>
          <div class="case-copy">
            <h3>{{ item.title }}</h3>
            <p>{{ item.summary }}</p>
            <dl>
              <div><dt>相关人物</dt><dd>{{ item.character_count }}</dd></div>
              <div><dt>公开线索</dt><dd>{{ item.evidence_count }}</dd></div>
            </dl>
          </div>
          <AppIcon name="chevron-right" :size="24" />
        </RouterLink>
      </section>

      <section class="home-lower">
        <div class="recent-session">
          <div class="section-heading"><h2>最近会话</h2></div>
          <button v-if="store.lastSession" class="session-row" type="button" @click="resume">
            <div>
              <strong>{{ store.cases.find((item) => item.id === store.lastSession?.caseId)?.title ?? '上次调查' }}</strong>
              <span>{{ formatSavedAt(store.lastSession.savedAt) }} · {{ store.lastSession.mode === 'mock' ? 'Mock 模式' : '模型模式' }}</span>
            </div>
            <span>恢复调查</span>
            <AppIcon name="chevron-right" />
          </button>
          <p v-else class="empty-state">开始案件后，可从这里恢复最近进度。</p>
        </div>
        <div class="provider-summary">
          <div>
            <h2>模型配置</h2>
            <p>{{ store.config?.configured ? `${store.config.model} 已连接` : '尚未配置，仍可使用 Mock 模式。' }}</p>
          </div>
          <button class="secondary-button" type="button" @click="settingsOpen = true">{{ store.config?.configured ? '管理设置' : '配置模型' }}</button>
        </div>
      </section>
    </main>
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
