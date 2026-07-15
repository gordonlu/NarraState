<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { animateLayoutChange } from '../lib/appMotion'
import { animateOverlayEnter, animateOverlayLeave } from '../lib/overlayMotion'
import { useSessionStore } from '../stores/session'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ open: boolean }>()
defineEmits<{ close: [] }>()
const store = useSessionStore()
const confirmed = ref(false)
const loading = ref(false)
const drawer = ref<HTMLElement>()
const selectedCharacter = computed(() =>
  store.activeCharacterId ? store.debug?.character_states[store.activeCharacterId] : undefined,
)
const recentEvents = computed(() => [...(store.debug?.events ?? [])].reverse().slice(0, 12))

watch(
  () => props.open,
  (open) => {
    if (!open) confirmed.value = false
  },
)

async function reveal() {
  const update = async () => {
    confirmed.value = true
    loading.value = true
    await nextTick()
    try {
      await store.loadDebug()
    } finally {
      loading.value = false
      await nextTick()
    }
  }
  if (drawer.value) await animateLayoutChange(drawer.value, update, '.debug-content > section, .empty-state')
  else await update()
}
</script>

<template>
  <Teleport to="body">
    <Transition :css="false" @enter="animateOverlayEnter" @leave="animateOverlayLeave">
      <div v-if="open" class="drawer-layer" @mousedown.self="$emit('close')">
        <aside ref="drawer" class="developer-drawer" aria-labelledby="developer-title">
          <header><div><h2 id="developer-title">开发者模式</h2><p>查看确定性状态与回合 trace。</p></div><button class="icon-button" type="button" aria-label="关闭" @click="$emit('close')"><AppIcon name="close" /></button></header>
          <div v-if="!confirmed" class="spoiler-gate"><AppIcon name="warning" :size="32" /><h3>这里包含案件剧透</h3><p>将显示角色阶段、内部数值、解锁节点、Provider 调用与事件 payload。普通玩家无需打开。</p><button class="danger-button" type="button" @click="reveal">确认显示内部状态</button></div>
          <div v-else-if="loading" class="empty-state">正在读取 trace…</div>
          <div v-else class="debug-content">
            <section v-if="selectedCharacter"><h3>当前角色状态</h3><dl class="debug-grid"><div><dt>phase</dt><dd>{{ selectedCharacter.phase }}</dd></div><div><dt>stress</dt><dd>{{ selectedCharacter.stress }}</dd></div><div><dt>composure</dt><dd>{{ selectedCharacter.composure }}</dd></div><div><dt>trust</dt><dd>{{ selectedCharacter.trust }}</dd></div><div><dt>defense</dt><dd>{{ selectedCharacter.defense_budget }}</dd></div><div><dt>revision</dt><dd>{{ store.session?.revision }}</dd></div></dl></section>
            <section><h3>Provider 调用</h3><div v-if="store.debug?.llm_calls.length === 0" class="empty-state compact">Mock 模式没有 Provider 调用。</div><div v-for="call in store.debug?.llm_calls" :key="call.call_id" class="debug-row"><strong>{{ call.purpose }}</strong><span>{{ call.status }} · {{ call.latency_ms }}ms</span></div></section>
            <section><h3>最近事件</h3><div v-for="event in recentEvents" :key="event.sequence" class="debug-event"><span>#{{ event.sequence }}</span><strong>{{ event.event_type }}</strong><code>{{ JSON.stringify(event.payload) }}</code></div></section>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>
