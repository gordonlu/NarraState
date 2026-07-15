<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { useSessionStore } from '../stores/session'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ open: boolean }>()
defineEmits<{ close: [] }>()

const store = useSessionStore()
const form = reactive({ base_url: '', model: '', api_key: '', persist_api_key: true })
const imageForm = reactive({ enabled: false, base_url: '', model: '', api_key: '', persist_api_key: true })
const testing = ref(false)
const saving = ref(false)
const feedback = ref('')

watch(
  () => [props.open, store.config] as const,
  () => {
    if (!props.open) return
    form.base_url = store.config?.base_url ?? 'https://api.openai.com/v1'
    form.model = store.config?.model ?? 'gpt-4o-mini'
    form.api_key = ''
    form.persist_api_key = true
    feedback.value = ''
    Object.assign(imageForm, { ...store.config?.image_provider, api_key: '', persist_api_key: true })
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
      persist_api_key: form.persist_api_key,
    })
    feedback.value = '连接成功，配置已保存'
    form.api_key = ''
  } catch (error) {
    feedback.value = error instanceof Error ? error.message : '连接失败'
  } finally {
    testing.value = false
  }
}

function payload() {
  return {
    base_url: form.base_url,
    model: form.model,
    ...(form.api_key ? { api_key: form.api_key } : {}),
    persist_api_key: form.persist_api_key,
  }
}

async function saveConfig() {
  saving.value = true
  feedback.value = ''
  try {
    await store.saveProvider(payload())
    feedback.value = '配置已保存，未发起外部请求'
    form.api_key = ''
  } catch (error) {
    feedback.value = error instanceof Error ? error.message : '保存失败'
  } finally {
    saving.value = false
  }
}

async function saveImageConfig() {
  saving.value = true; feedback.value = ''
  try {
    await store.saveImageProvider({ enabled: imageForm.enabled, base_url: imageForm.base_url,
      model: imageForm.model, ...(imageForm.api_key ? { api_key: imageForm.api_key } : {}),
      persist_api_key: imageForm.persist_api_key })
    feedback.value = '图片 Provider 配置已保存'; imageForm.api_key = ''
  } catch (error) { feedback.value = error instanceof Error ? error.message : '保存失败' }
  finally { saving.value = false }
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
            <label>OpenAI-compatible Base URL<input v-model="form.base_url" required autocomplete="url" placeholder="https://api.deepseek.com/v1" /></label>
            <label>模型<input v-model="form.model" required autocomplete="off" /></label>
            <label>API Key<input v-model="form.api_key" type="password" :placeholder="store.config?.configured ? '已由本机服务保存' : '输入密钥'" autocomplete="off" /></label>
            <label class="settings-checkbox"><input v-model="form.persist_api_key" type="checkbox" />重启服务后保留 API Key</label>
            <p class="privacy-copy">支持 DeepSeek 等 OpenAI-compatible 服务。勾选后密钥写入仅服务端可读的 provider.env，不进入浏览器或 SQLite。</p>
            <button class="primary-button wide-button" type="button" :disabled="saving || testing" @click="saveConfig">
              {{ saving ? '正在保存…' : '保存配置' }}
            </button>
            <button class="secondary-button wide-button" type="submit" :disabled="testing || saving">
              {{ testing ? '正在测试…' : '测试连接' }}
            </button>
            <p v-if="feedback" class="form-feedback" role="status">{{ feedback }}</p>
          </form>
          <div class="mock-callout">
            <strong>Mock 演示模式</strong>
            <p>无需模型配置即可体验完整状态机和结案流程。</p>
          </div>
          <form class="image-provider-form" @submit.prevent="saveImageConfig">
            <h3>图片生成 Provider</h3>
            <label class="settings-checkbox"><input v-model="imageForm.enabled" type="checkbox" />启用可选视觉资产</label>
            <label>独立 Base URL<input v-model="imageForm.base_url" :required="imageForm.enabled" /></label>
            <label>独立模型<input v-model="imageForm.model" :required="imageForm.enabled" /></label>
            <label>独立 API Key<input v-model="imageForm.api_key" type="password" :placeholder="store.config?.image_provider.configured ? '已配置' : '输入图片 Provider Key'" /></label>
            <label class="settings-checkbox"><input v-model="imageForm.persist_api_key" type="checkbox" />重启后保留图片 Key</label>
            <p class="privacy-copy">与主对话模型完全独立；缺失或失败时使用默认视觉，不影响案件逻辑。</p>
            <button class="secondary-button wide-button" :disabled="saving">保存图片配置</button>
          </form>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>
