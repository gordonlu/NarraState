<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { animateLayoutChange } from '../lib/appMotion'
import { animateOverlayEnter, animateOverlayLeave, animateResolutionExit } from '../lib/overlayMotion'
import { resultLabel } from '../lib/present'
import { useSessionStore } from '../stores/session'
import type { AccusationResult } from '../types/api'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ close: [] }>()
const store = useSessionStore()
const router = useRouter()
const target = ref('')
const selected = ref<string[]>([])
const reasoning = ref('')
const result = ref<AccusationResult>()
const submitting = ref(false)
const dialog = ref<HTMLElement>()

const proven = computed(() =>
  result.value === 'CaseProvenWithoutConfession' || result.value === 'CaseProvenWithConfession',
)

watch(
  () => props.open,
  (open) => {
    if (!open) return
    target.value = store.activeCharacterId ?? store.activeCase?.characters[0]?.id ?? ''
    selected.value = [...store.attachedEvidenceIds]
    reasoning.value = ''
    result.value = undefined
  },
)

function toggle(id: string) {
  const index = selected.value.indexOf(id)
  if (index >= 0) selected.value.splice(index, 1)
  else selected.value.push(id)
}

async function submit() {
  if (!target.value || !reasoning.value.trim()) return
  submitting.value = true
  try {
    const nextResult = await store.submitAccusation({
      targetCharacterId: target.value,
      evidenceIds: selected.value,
      reasoning: reasoning.value.trim(),
    })
    if (dialog.value) {
      await animateLayoutChange(dialog.value, async () => {
        result.value = nextResult
        await nextTick()
      }, '.accusation-result, .result-footer')
    } else {
      result.value = nextResult
    }
  } finally {
    submitting.value = false
  }
}

async function openConclusion() {
  if (!store.session) return
  if (dialog.value) await animateResolutionExit(dialog.value)
  emit('close')
  await router.push(`/sessions/${store.session.session_id}/conclusion`)
}
</script>

<template>
  <Teleport to="body">
    <Transition :css="false" @enter="animateOverlayEnter" @leave="animateOverlayLeave">
      <div v-if="open" class="dialog-layer" @mousedown.self="$emit('close')">
        <section ref="dialog" class="accusation-dialog" role="dialog" aria-modal="true" aria-labelledby="accusation-title">
          <header><div><h2 id="accusation-title">提交判断</h2><p>选择对象、支撑线索，并说明你的推理。</p></div><button class="icon-button" type="button" aria-label="关闭" @click="$emit('close')"><AppIcon name="close" /></button></header>
          <div v-if="result" class="accusation-result" :class="{ proven }">
            <AppIcon :name="proven ? 'check' : 'warning'" :size="24" />
            <div><strong>{{ resultLabel(result) }}</strong><p v-if="result === 'WrongSuspect'">现有事实无法支持这个对象，调查仍可继续。</p><p v-else-if="result === 'CorrectButInsufficient'">对象方向正确，但所选线索尚未覆盖完整证据链。</p><p v-else>案件已经结案，可以查看完整报告。</p></div>
          </div>
          <form v-if="!proven" @submit.prevent="submit">
            <fieldset><legend>判断对象</legend><div class="choice-list"><label v-for="character in store.activeCase?.characters" :key="character.id"><input v-model="target" type="radio" :value="character.id" /><span><strong>{{ character.name }}</strong>{{ character.role }}</span></label></div></fieldset>
            <fieldset><legend>支撑线索 <span>{{ selected.length }} 条已选</span></legend><div class="evidence-choice-list"><label v-for="item in store.session?.discovered_evidence" :key="item.id"><input type="checkbox" :checked="selected.includes(item.id)" @change="toggle(item.id)" /><span><strong>{{ item.title }}</strong><small>{{ item.description }}</small></span></label></div></fieldset>
            <label class="reasoning-field">推理说明<textarea v-model="reasoning" maxlength="4000" placeholder="说明这些线索如何支持你的判断" required /><span>{{ reasoning.length }}/4000</span></label>
            <footer><button class="secondary-button" type="button" @click="$emit('close')">继续调查</button><button class="primary-button" type="submit" :disabled="submitting || !target || !reasoning.trim()">{{ submitting ? '正在提交…' : '确认判断' }}</button></footer>
          </form>
          <footer v-else class="result-footer"><button class="primary-button" type="button" @click="openConclusion">查看结案报告<AppIcon name="chevron-right" /></button></footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
