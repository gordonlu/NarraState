<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import AccusationDialog from '../components/AccusationDialog.vue'
import AppHeader from '../components/AppHeader.vue'
import AppIcon from '../components/AppIcon.vue'
import DeveloperDrawer from '../components/DeveloperDrawer.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import {
  animateCharacterSwap,
  animateEvidenceFlip,
  animateNewTurns,
  createEvidenceDragController,
  createInvestigationEntrance,
  type EvidenceDragController,
} from '../lib/appMotion'
import { describeFact, formatStoryTime, speakerId } from '../lib/present'
import { useSessionStore } from '../stores/session'
import type { DialogueEntry } from '../types/api'

type ResearchTab = 'evidence' | 'statements' | 'inferences'
type MobileTab = 'people' | 'dialogue' | 'research'

const route = useRoute()
const router = useRouter()
const store = useSessionStore()
const question = ref('')
const researchTab = ref<ResearchTab>('evidence')
const mobileTab = ref<MobileTab>('dialogue')
const settingsOpen = ref(false)
const accusationOpen = ref(false)
const developerOpen = ref(false)
const transcript = ref<HTMLElement>()
const inference = ref('')
const investigationGrid = ref<HTMLElement>()
const dialoguePanel = ref<HTMLElement>()
let entrance: ReturnType<typeof createInvestigationEntrance>
let evidenceDrag: EvidenceDragController | undefined
let sceneReady = false

const sessionId = computed(() => String(route.params.sessionId))
const statements = computed(() =>
  (store.session?.conversation ?? []).filter((entry) => typeof entry.speaker !== 'string'),
)
const timeline = computed(() =>
  [...(store.activeCase?.facts ?? [])].sort((a, b) =>
    formatStoryTime(b.happened_at).localeCompare(formatStoryTime(a.happened_at)),
  ),
)

onMounted(async () => {
  try {
    const session = await store.restoreSession(sessionId.value)
    if (session.status === 'Resolved') await router.replace(`/sessions/${session.session_id}/conclusion`)
    inference.value = localStorage.getItem(`narrastate:inference:${sessionId.value}`) ?? ''
    await nextTick()
    if (investigationGrid.value) {
      entrance = createInvestigationEntrance(investigationGrid.value)
      evidenceDrag = createEvidenceDragController(investigationGrid.value, (evidenceId) => {
        void toggleEvidenceWithMotion(evidenceId)
      })
    }
    sceneReady = true
  } catch {
    // Store exposes the actionable error.
  }
})

onBeforeUnmount(() => {
  entrance?.kill()
  evidenceDrag?.destroy()
})

watch(
  () => [store.session?.conversation.length, store.streamText],
  async () => {
    await nextTick()
    transcript.value?.scrollTo({ top: transcript.value.scrollHeight, behavior: 'smooth' })
  },
)

watch(
  () => store.session?.conversation.length ?? 0,
  async (length, previousLength) => {
    if (!sceneReady || length <= previousLength) return
    await nextTick()
    if (!transcript.value) return
    const turns = Array.from(
      transcript.value.querySelectorAll<HTMLElement>(`[data-turn-index]`),
    ).filter((turn) => Number(turn.dataset.turnIndex) >= previousLength)
    animateNewTurns(turns)
  },
)

watch(inference, (value) => localStorage.setItem(`narrastate:inference:${sessionId.value}`, value))

function characterName(id: string) {
  return store.activeCase?.characters.find((character) => character.id === id)?.name ?? id
}

function entrySpeaker(entry: DialogueEntry) {
  const id = speakerId(entry.speaker)
  if (id === 'Player') return '你'
  if (id === 'System') return '系统'
  return characterName(id)
}

async function send() {
  const text = question.value.trim()
  if (!text || store.streaming) return
  question.value = ''
  try {
    await store.sendQuestion(text)
  } catch {
    if (!store.notice) question.value = text
  }
}

async function selectCharacter(characterId: string) {
  mobileTab.value = 'dialogue'
  if (store.activeCharacterId === characterId || !dialoguePanel.value) {
    store.activeCharacterId = characterId
    return
  }
  await animateCharacterSwap(dialoguePanel.value, async () => {
    store.activeCharacterId = characterId
    await nextTick()
  })
}

async function toggleEvidenceWithMotion(evidenceId: string) {
  const root = investigationGrid.value
  if (!root) {
    store.toggleEvidence(evidenceId)
    return
  }

  const wasAttached = store.attachedEvidenceIds.includes(evidenceId)
  const start = root.querySelector<HTMLElement>(
    wasAttached ? `[data-evidence-attachment="${CSS.escape(evidenceId)}"]` : `[data-evidence-source="${CSS.escape(evidenceId)}"]`,
  )
  const startRect = start?.getBoundingClientRect()
  const label = store.session?.discovered_evidence.find((item) => item.id === evidenceId)?.title ?? '线索'

  store.toggleEvidence(evidenceId)
  await nextTick()

  const target = root.querySelector<HTMLElement>(
    wasAttached ? `[data-evidence-source="${CSS.escape(evidenceId)}"]` : `[data-evidence-attachment="${CSS.escape(evidenceId)}"]`,
  )
  if (startRect && target) animateEvidenceFlip(startRect, target, label)
}

function onComposerKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    void send()
  }
}
</script>

<template>
  <div class="investigation-shell">
    <AppHeader
      :case-title="store.activeCase?.title"
      :saved="Boolean(store.session) && !store.streaming"
      show-conclusion
      @settings="settingsOpen = true"
      @conclusion="accusationOpen = true"
    />
    <NoticeBar v-if="store.notice" :message="store.notice" :tone="store.degraded ? 'warning' : 'info'" @close="store.clearNotice" />
    <NoticeBar v-else-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />

    <main v-if="store.session && store.activeCase" ref="investigationGrid" class="investigation-grid" :data-mobile-tab="mobileTab">
      <aside class="people-panel workspace-people">
        <section>
          <h2><AppIcon name="people" />相关人物</h2>
          <button
            v-for="character in store.activeCase.characters"
            :key="character.id"
            class="person-row"
            :class="{ selected: store.activeCharacterId === character.id }"
            type="button"
            @click="selectCharacter(character.id)"
          >
            <span class="person-mark">{{ character.name.slice(0, 1) }}</span>
            <span><strong>{{ character.name }}</strong><small>{{ character.role }}</small></span>
          </button>
        </section>
        <section class="timeline-section">
          <h2><AppIcon name="timeline" />事件线</h2>
          <ol class="event-list">
            <li class="current"><span class="event-dot" /><time>现在</time><strong>正在与{{ store.activeCharacter?.name }}交谈</strong></li>
            <li v-for="fact in timeline" :key="fact.id"><span class="event-dot" /><time>{{ formatStoryTime(fact.happened_at) }}</time><span>{{ describeFact(fact) }}</span></li>
          </ol>
        </section>
      </aside>

      <section ref="dialoguePanel" class="dialogue-panel workspace-dialogue">
        <header class="dialogue-heading">
          <div><span>正在与{{ store.activeCharacter?.name }}交谈</span><small>{{ store.activeCharacter?.role }}</small></div>
          <button class="mobile-context-button" type="button" @click="mobileTab = 'people'">切换人物</button>
        </header>
        <div ref="transcript" class="transcript" aria-live="polite">
          <div v-if="store.session.conversation.length === 0" class="transcript-empty">
            <h2>从一个具体问题开始</h2>
            <p>询问公开事件，或附加线索来核对角色陈述。</p>
          </div>
          <article v-for="(entry, index) in store.session.conversation" :key="`${entry.turn_id}-${entrySpeaker(entry)}`" class="transcript-turn" :class="{ player: speakerId(entry.speaker) === 'Player' }" :data-turn-index="index">
            <header><span class="speaker-mark">{{ entrySpeaker(entry).slice(0, 1) }}</span><strong>{{ entrySpeaker(entry) }}</strong><time>回合 {{ store.session.conversation.indexOf(entry) + 1 }}</time></header>
            <p>{{ entry.text }}</p>
            <div v-if="entry.attached_evidence.length" class="turn-attachments"><AppIcon name="paperclip" :size="16" />{{ entry.attached_evidence.map((id) => store.session?.discovered_evidence.find((item) => item.id === id)?.title ?? id).join('、') }}</div>
          </article>
          <article v-if="store.streaming" class="transcript-turn streaming-turn">
            <header><span class="streaming-mark"><i /><i /><i /></span><strong>{{ store.activeCharacter?.name }}</strong><time>{{ store.streamStage }}</time></header>
            <p>{{ store.streamText }}<span class="stream-caret" /></p>
          </article>
        </div>
        <div class="composer-region" data-evidence-dropzone>
          <div class="attachment-summary"><span><AppIcon name="paperclip" :size="18" />已附加 {{ store.selectedEvidence.length }} 条线索<small>也可拖入此处</small></span><button type="button" @click="mobileTab = 'research'; researchTab = 'evidence'">选择线索</button></div>
          <div v-if="store.selectedEvidence.length" class="attachment-list">
            <div v-for="item in store.selectedEvidence" :key="item.id" class="attachment-row" :data-evidence-attachment="item.id"><AppIcon name="document" /><span>{{ item.title }}</span><button type="button" :aria-label="`移除 ${item.title}`" @click="toggleEvidenceWithMotion(item.id)"><AppIcon name="close" :size="18" /></button></div>
          </div>
          <div class="composer-box">
            <textarea v-model="question" maxlength="2000" :disabled="store.streaming" placeholder="输入你的问题" @keydown="onComposerKeydown" />
            <footer><span>{{ question.length }}/2000 · Ctrl Enter 发送</span><button v-if="store.streaming" class="secondary-button" type="button" @click="store.cancelDisplay">停止显示</button><button v-else class="primary-button send-button" type="button" :disabled="!question.trim()" @click="send">发送<AppIcon name="send" :size="17" /></button></footer>
          </div>
        </div>
      </section>

      <aside class="research-panel workspace-research">
        <nav class="research-tabs" aria-label="调查记录">
          <button type="button" :class="{ selected: researchTab === 'evidence' }" @click="researchTab = 'evidence'">线索</button>
          <button type="button" :class="{ selected: researchTab === 'statements' }" @click="researchTab = 'statements'">陈述</button>
          <button type="button" :class="{ selected: researchTab === 'inferences' }" @click="researchTab = 'inferences'">推断</button>
        </nav>
        <div v-if="researchTab === 'evidence'" class="research-content evidence-content">
          <label class="research-search"><AppIcon name="search" :size="18" /><input placeholder="搜索线索" /></label>
          <label v-for="item in store.session.discovered_evidence" :key="item.id" class="evidence-row" :class="{ attached: store.attachedEvidenceIds.includes(item.id) }" :data-draggable-evidence="item.id" :data-evidence-source="item.id">
            <input type="checkbox" :checked="store.attachedEvidenceIds.includes(item.id)" @change="toggleEvidenceWithMotion(item.id)" />
            <AppIcon class="evidence-drag-handle" name="document" :size="23" data-evidence-drag-handle />
            <span><strong>{{ item.title }}</strong><small>{{ item.description }}</small></span>
            <AppIcon name="chevron-right" />
          </label>
        </div>
        <div v-else-if="researchTab === 'statements'" class="research-content">
          <div v-if="statements.length === 0" class="empty-state">角色陈述会随调查记录在这里。</div>
          <article v-for="entry in statements" :key="entry.turn_id" class="statement-row"><strong>{{ entrySpeaker(entry) }}</strong><p>{{ entry.text }}</p><small>回合 {{ store.session.conversation.indexOf(entry) + 1 }}</small></article>
        </div>
        <div v-else class="research-content inference-content">
          <label>玩家推断<textarea v-model="inference" placeholder="记录你对人物、时间和线索关系的判断。仅保存在当前浏览器。" /></label>
          <small>自动保存在此设备，不会影响案件状态。</small>
        </div>
        <footer class="research-footer"><button type="button" @click="developerOpen = true">开发者模式</button><span>revision {{ store.session.revision }}</span></footer>
      </aside>
    </main>
    <div v-else class="full-page-loading">正在恢复调查…</div>

    <nav class="mobile-workspace-tabs" aria-label="调查区域">
      <button type="button" :class="{ selected: mobileTab === 'people' }" @click="mobileTab = 'people'">人物</button>
      <button type="button" :class="{ selected: mobileTab === 'dialogue' }" @click="mobileTab = 'dialogue'">对话</button>
      <button type="button" :class="{ selected: mobileTab === 'research' }" @click="mobileTab = 'research'">调查记录</button>
    </nav>

    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
    <AccusationDialog :open="accusationOpen" @close="accusationOpen = false" />
    <DeveloperDrawer :open="developerOpen" @close="developerOpen = false" />
  </div>
</template>
