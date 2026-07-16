<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { useSessionStore } from '../stores/session'

const store = useSessionStore()
const settingsOpen = ref(false)
const resumableCaseId = computed(() => store.resumableSession?.case_id)

onMounted(() => store.bootstrap())
</script>

<template>
  <div class="page-shell cases-page">
    <AppHeader @settings="settingsOpen = true" />
    <NoticeBar v-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />
    <main class="home-main cases-main">
      <header class="cases-heading">
        <div><span>案件档案</span><h1>选择一个谜局</h1><p>进入案件简报，了解背景与相关人物，再决定从哪一种真相开始调查。</p></div>
        <RouterLink class="primary-button" to="/generate"><AppIcon name="plus" :size="17" />生成案件</RouterLink>
      </header>

      <div v-if="store.loading && !store.cases.length" class="cases-loading" role="status">正在整理案件档案…</div>
      <div v-else-if="!store.cases.length" class="cases-empty">
        <strong>还没有可用案件</strong>
        <p>生成一个新的谜局，它通过完整检查后会出现在这里。</p>
        <RouterLink class="primary-button" to="/generate">生成第一个案件</RouterLink>
      </div>
      <section v-else class="case-list" aria-label="可用案件">
        <RouterLink v-for="item in store.cases" :key="item.id" class="case-row" :to="`/cases/${item.id}`">
          <div class="case-media">
            <img v-if="item.cover_url" :src="item.cover_url" :alt="`${item.title}封面`" />
            <span v-else aria-hidden="true">{{ item.title.slice(0, 1) }}</span>
          </div>
          <div class="case-copy">
            <div class="case-title-line"><h2>{{ item.title }}</h2><span v-if="resumableCaseId === item.id">调查进行中</span></div>
            <p>{{ item.summary }}</p>
            <dl><div><dt>相关人物</dt><dd>{{ item.character_count }} 人</dd></div><div><dt>初始线索</dt><dd>{{ item.evidence_count }} 条</dd></div></dl>
          </div>
          <AppIcon name="arrow-right" :size="22" />
        </RouterLink>
      </section>
    </main>
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
