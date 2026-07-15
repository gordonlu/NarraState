<script setup lang="ts">
import { reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { api } from '../lib/api'
import type { GenerationJob, GenerationRequest } from '../types/api'

const router = useRouter()
const settingsOpen = ref(false)
const running = ref(false)
const job = ref<GenerationJob>()
const constraints = ref('不得依赖超自然因素\n所有关键证据必须可发现')
const form = reactive<GenerationRequest>({
  theme: '', setting: '', tone: 'realistic', target_duration_minutes: 45,
  difficulty: 'medium', character_count: 4, variant_count: 3, realism: 'grounded',
  confession_policy: 'partial_then_full', content_constraints: [], language: 'zh-CN',
})

async function generate() {
  running.value = true; job.value = undefined
  try {
    job.value = await api.generateCase({ ...form, content_constraints: constraints.value.split('\n').map(v => v.trim()).filter(Boolean) })
    while (job.value.status !== 'completed' && job.value.status !== 'failed') {
      await new Promise(resolve => setTimeout(resolve, 700))
      job.value = await api.generationJob(job.value.job_id)
    }
  } catch (error) {
    job.value = { job_id: '', status: 'failed', attempt_count: 0, repair_count: 0,
      error_message: error instanceof Error ? error.message : '生成失败', events: [], updated_at: '' }
  } finally { running.value = false }
}
</script>

<template>
  <div class="page-shell"><AppHeader back-label="返回案件" @back="router.push('/')" @settings="settingsOpen = true" />
    <main class="generation-main"><header><p class="eyebrow">非权威草案 → Rust 校验</p><h1>生成新案件</h1><p>模型输出必须通过编译、校验和每个真相变体的确定性模拟后才会安装。</p></header>
      <form class="generation-form" @submit.prevent="generate">
        <label>主题<input v-model="form.theme" required maxlength="4000" /></label>
        <label>场景<input v-model="form.setting" required maxlength="4000" /></label>
        <label>时长（分钟）<input v-model.number="form.target_duration_minutes" type="number" min="10" max="120" /></label>
        <label>难度<select v-model="form.difficulty"><option value="easy">简单</option><option value="medium">中等</option><option value="hard">困难</option></select></label>
        <label>角色数量<input v-model.number="form.character_count" type="number" min="2" max="6" /></label>
        <label>真相变体<input v-model.number="form.variant_count" type="number" min="1" max="5" /></label>
        <label class="wide">内容限制<textarea v-model="constraints" rows="4" /></label>
        <button class="primary-button wide" :disabled="running">{{ running ? '正在生成并验证…' : '生成案件' }}</button>
      </form>
      <section v-if="job" class="generation-result" role="status"><h2>{{ job.status === 'completed' ? '案件已安装' : '生成未完成' }}</h2><p v-if="job.error_code"><code>{{ job.error_code }}</code> · {{ job.error_message }}</p><p>尝试 {{ job.attempt_count }} 次，Repair {{ job.repair_count }} 次</p><ol><li v-for="event in job.events" :key="event.sequence">{{ event.to }}</li></ol></section>
    </main><SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
