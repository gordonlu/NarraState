<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { useSessionStore } from '../stores/session'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ open: boolean }>()
defineEmits<{ close: [] }>()

const store = useSessionStore()
const form = reactive({ base_url: '', model: '', api_key: '' })
const testing = ref(false)
const feedback = ref('')

watch(
  () => [props.open, store.config] as const,
  () => {
    if (!props.open) return
    form.base_url = store.config?.base_url ?? 'https://api.openai.com/v1'
    form.model = store.config?.model ?? 'gpt-4o-mini'
    form.api_key = ''
    feedback.value = ''
  },
  { immediate: true },
)

async function testConnection() {
  testing.value = true
  feedback.value = ''
  try {
    await store.testProvider({
      base_url: form.base_url,
      model: form.model,
      ...(form.api_key ? { api_key: form.api_key } : {}),
    })
    feedback.value = '连接成功，配置已保存'
    form.api_key = ''
  } catch (error) {
    feedback.value = error instanceof Error ? error.message : '连接失败'
  } finally {
    testing.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="drawer">
      <div v-if="open" class="drawer-layer" @mousedown.self="$emit('close')">
        <aside class="settings-drawer" aria-labelledby="settings-title">
          <header>
            <div>
              <h2 id="settings-title">模型配置</h2>
              <p>普通玩家界面不会显示模型内部信息。</p>
            </div>
            <button class="icon-button" type="button" aria-label="关闭设置" @click="$emit('close')"><AppIcon name="close" /></button>
          </header>
          <div class="connection-state">
            <span>连接状态</span>
            <strong>{{ store.config?.configured ? '已配置' : '尚未配置' }}</strong>
          </div>
          <form @submit.prevent="testConnection">
            <label>Base URL<input v-model="form.base_url" required autocomplete="url" /></label>
            <label>模型<input v-model="form.model" required autocomplete="off" /></label>
            <label>API Key<input v-model="form.api_key" type="password" :placeholder="store.config?.configured ? '已由本机服务保存' : '输入密钥'" autocomplete="off" /></label>
            <p class="privacy-copy">密钥只提交给本机 Rust 服务，不写入浏览器存储。</p>
            <button class="primary-button wide-button" type="submit" :disabled="testing">
              {{ testing ? '正在测试…' : '测试连接并保存' }}
            </button>
            <p v-if="feedback" class="form-feedback" role="status">{{ feedback }}</p>
          </form>
          <div class="mock-callout">
            <strong>Mock 演示模式</strong>
            <p>无需模型配置即可体验完整状态机和结案流程。</p>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>
