<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { api } from '../lib/api'
import { animateGenerationEvents } from '../lib/overlayMotion'
import type { GenerationJob, GenerationRequest } from '../types/api'

const router = useRouter()
const settingsOpen = ref(false)
const running = ref(false)
const job = ref<GenerationJob>()
const generationResult = ref<HTMLElement>()
let stopped = false
const constraints = ref('不得依赖超自然因素\n所有关键证据必须可发现')
const form = reactive<GenerationRequest>({
  theme: '', setting: '', tone: 'realistic', target_duration_minutes: 45,
  difficulty: 'medium', character_count: 4, variant_count: 3, realism: 'grounded',
  confession_policy: 'partial_then_full', content_constraints: [], language: 'zh-CN',
})

const statusLabels: Record<string, string> = {
  pending: '任务已创建', drafting: '生成结构化草案', parsing: '严格解析草案',
  normalizing: '规范化内容', compiling: '编译真相变体', validating: '执行确定性校验',
  simulating: '模拟合法通关路径', repairing: '按诊断修复草案', completed: '案件已验证并安装',
  failed: '生成任务失败',
}
const statusLabel = computed(() => statusLabels[job.value?.status ?? 'pending'] ?? job.value?.status ?? '等待任务')
const visibleEvents = computed(() => {
  if (!job.value) return []
  if (job.value.events.length > 0) return job.value.events
  return [{ sequence: 0, to: job.value.status, ...(job.value.error_code ? { error_code: job.value.error_code } : {}) }]
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

onBeforeUnmount(() => { stopped = true })

async function generate() {
  running.value = true; job.value = undefined
  try {
    job.value = await api.generateCase({ ...form, content_constraints: constraints.value.split('\n').map(v => v.trim()).filter(Boolean) })
    while (!stopped && job.value.status !== 'completed' && job.value.status !== 'failed') {
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
      <section v-if="job" ref="generationResult" class="generation-result" role="status" aria-live="polite">
        <header class="generation-result-heading">
          <span>{{ job.status === 'completed' ? 'READY' : job.status === 'failed' ? 'FAILED' : 'IN PROGRESS' }}</span>
          <h2>{{ statusLabel }}</h2>
          <p>这里只显示服务端已经记录的确定性阶段，不预测后续进度。</p>
        </header>
        <div class="generation-event-list">
          <article v-for="(event, index) in visibleEvents" :key="`${event.sequence}-${event.to}`" :class="{ current: index === visibleEvents.length - 1, failed: event.to === 'failed' }" :data-generation-event="index">
            <i class="generation-event-dot" aria-hidden="true" />
            <span>{{ String(event.sequence).padStart(2, '0') }}</span>
            <div><strong>{{ statusLabels[event.to] ?? event.to }}</strong><small v-if="event.error_code">{{ event.error_code }}</small></div>
          </article>
        </div>
        <p v-if="job.error_code" class="generation-error"><code>{{ job.error_code }}</code> · {{ job.error_message }}</p>
        <footer><span>生成尝试 {{ job.attempt_count }}</span><span>修复 {{ job.repair_count }}</span><span v-if="job.result_path">已输出案件包</span></footer>
      </section>
    </main><SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
