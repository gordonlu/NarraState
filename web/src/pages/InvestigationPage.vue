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
  animateSharedElementFlip,
  createEvidenceDragController,
  createInvestigationEntrance,
  type EvidenceDragController,
} from '../lib/appMotion'
import { buildTurnNumberIndex, conversationForCharacter } from '../lib/conversation'
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
const prologueOpen = ref(false)
const transcript = ref<HTMLElement>()
const inference = ref('')
const evidenceQuery = ref('')
const focusedEvidenceId = ref<string>()
const focusedCharacterId = ref<string>()
const investigationGrid = ref<HTMLElement>()
const dialoguePanel = ref<HTMLElement>()
let entrance: ReturnType<typeof createInvestigationEntrance>
let evidenceDrag: EvidenceDragController | undefined
let sceneReady = false

const sessionId = computed(() => String(route.params.sessionId))
const visibleConversation = computed(() =>
  conversationForCharacter(store.session?.conversation ?? [], store.activeCharacterId),
)
const turnNumberById = computed(() =>
  buildTurnNumberIndex(store.session?.conversation ?? []),
)
const pendingTurnNumber = computed(() => (store.session?.current_turn ?? 0) + 1)
const statements = computed(() =>
  visibleConversation.value.filter((entry) => typeof entry.speaker !== 'string'),
)
const visiblePendingQuestion = computed(() =>
  store.pendingQuestion?.targetCharacterId === store.activeCharacterId
    ? store.pendingQuestion
    : undefined,
)
const timeline = computed(() =>
  [...(store.activeCase?.facts ?? [])].sort((a, b) =>
    formatStoryTime(b.happened_at).localeCompare(formatStoryTime(a.happened_at)),
  ),
)
const focusedEvidence = computed(() =>
  store.session?.discovered_evidence.find((item) => item.id === focusedEvidenceId.value),
)
const focusedCharacter = computed(() =>
  store.activeCase?.characters.find((character) => character.id === focusedCharacterId.value),
)
const sceneVisual = computed(() => {
  const visuals = store.activeCase?.visual_assets ?? []
  return visuals.find((asset) => asset.visual_type === 'scene_background')
    ?? visuals.find((asset) => asset.visual_type === 'location_atmosphere')
    ?? visuals.find((asset) => asset.visual_type === 'chapter_illustration')
})
const prologueVisual = computed(() =>
  (store.activeCase?.visual_assets ?? []).find((asset) => asset.visual_type === 'case_cover')
    ?? sceneVisual.value,
)
const filteredEvidence = computed(() => {
  const query = evidenceQuery.value.trim().toLocaleLowerCase()
  if (!query) return store.session?.discovered_evidence ?? []
  return (store.session?.discovered_evidence ?? []).filter((item) =>
    `${item.title} ${item.description}`.toLocaleLowerCase().includes(query),
  )
})

onMounted(async () => {
  try {
    const session = await store.restoreSession(sessionId.value)
    if (session.status === 'Resolved') await router.replace(`/sessions/${session.session_id}/conclusion`)
    inference.value = localStorage.getItem(`narrastate:inference:${sessionId.value}`) ?? ''
    const prologueKey = `narrastate:prologue:${sessionId.value}`
    prologueOpen.value = session.current_turn === 0
      && session.conversation.length === 0
      && localStorage.getItem(prologueKey) !== 'seen'
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

async function scrollTranscriptToLatest(behavior: ScrollBehavior) {
  await nextTick()
  const element = transcript.value
  if (!element) return
  element.scrollTo({ top: element.scrollHeight, behavior })
}

watch(
  () => [
    store.session?.conversation.length ?? 0,
    store.pendingQuestion?.text ?? '',
    store.streaming,
    store.activeCharacterId ?? '',
  ],
  () => scrollTranscriptToLatest('smooth'),
  { flush: 'post' },
)

watch(
  () => store.streamText,
  () => scrollTranscriptToLatest('auto'),
  { flush: 'post' },
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

function characterPortrait(id: string) {
  return store.activeCase?.characters.find((character) => character.id === id)?.portrait_url
}

function entryPortrait(entry: DialogueEntry) {
  const id = speakerId(entry.speaker)
  return id === 'Player' || id === 'System' ? undefined : characterPortrait(id)
}

function entrySpeaker(entry: DialogueEntry) {
  const id = speakerId(entry.speaker)
  if (id === 'Player') return '你'
  if (id === 'System') return '系统'
  return characterName(id)
}

function entryTurnNumber(entry: DialogueEntry) {
  return turnNumberById.value.get(entry.turn_id) ?? 1
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
  if (store.streaming) return
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

async function openCharacterDetail(characterId: string) {
  const root = investigationGrid.value
  const source = root?.querySelector<HTMLElement>(`[data-character-source="${CSS.escape(characterId)}"]`)
  if (!root || !source) {
    focusedCharacterId.value = characterId
    return
  }
  await animateSharedElementFlip(source, async () => {
    focusedCharacterId.value = characterId
    await nextTick()
  }, () => root.querySelector<HTMLElement>(`[data-character-detail="${CSS.escape(characterId)}"]`))
}

async function closeCharacterDetail() {
  const characterId = focusedCharacterId.value
  const root = investigationGrid.value
  const source = characterId ? root?.querySelector<HTMLElement>(`[data-character-detail="${CSS.escape(characterId)}"]`) : undefined
  if (!characterId || !root || !source) {
    focusedCharacterId.value = undefined
    return
  }
  await animateSharedElementFlip(source, async () => {
    focusedCharacterId.value = undefined
    await nextTick()
  }, () => root.querySelector<HTMLElement>(`[data-character-source="${CSS.escape(characterId)}"]`))
}

async function talkToFocusedCharacter() {
  const characterId = focusedCharacterId.value
  if (!characterId) return
  await closeCharacterDetail()
  await selectCharacter(characterId)
}

async function openEvidenceDetail(evidenceId: string) {
  const root = investigationGrid.value
  const source = root?.querySelector<HTMLElement>(`[data-evidence-source="${CSS.escape(evidenceId)}"]`)
  if (!root || !source) {
    focusedEvidenceId.value = evidenceId
    return
  }
  evidenceDrag?.destroy()
  await animateSharedElementFlip(source, async () => {
    focusedEvidenceId.value = evidenceId
    await nextTick()
  }, () => root.querySelector<HTMLElement>(`[data-evidence-detail="${CSS.escape(evidenceId)}"]`))
}

async function closeEvidenceDetail() {
  const evidenceId = focusedEvidenceId.value
  const root = investigationGrid.value
  const source = evidenceId ? root?.querySelector<HTMLElement>(`[data-evidence-detail="${CSS.escape(evidenceId)}"]`) : undefined
  if (!evidenceId || !root || !source) {
    focusedEvidenceId.value = undefined
    evidenceDrag?.refresh()
    return
  }
  await animateSharedElementFlip(source, async () => {
    focusedEvidenceId.value = undefined
    await nextTick()
  }, () => root.querySelector<HTMLElement>(`[data-evidence-source="${CSS.escape(evidenceId)}"]`))
  evidenceDrag?.refresh()
}

function selectResearchTab(tab: ResearchTab) {
  focusedEvidenceId.value = undefined
  researchTab.value = tab
  void nextTick(() => evidenceDrag?.refresh())
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

function enterInvestigation() {
  localStorage.setItem(`narrastate:prologue:${sessionId.value}`, 'seen')
  prologueOpen.value = false
}
</script>

<template>
  <div class="investigation-shell">
    <AppHeader
      :case-title="store.activeCase?.title"
      :saved="Boolean(store.session) && !store.streaming"
      show-conclusion
      back-label="返回案件"
      @back="router.push(`/cases/${store.activeCase?.id ?? store.session?.case_id}`)"
      @settings="settingsOpen = true"
      @conclusion="accusationOpen = true"
    />
    <NoticeBar v-if="store.notice" :message="store.notice" :tone="store.degraded ? 'warning' : 'info'" @close="store.clearNotice" />
    <NoticeBar v-else-if="store.error" :message="store.error" tone="error" @close="store.error = undefined" />

    <section v-if="prologueOpen && store.activeCase" class="investigation-prologue" role="dialog" aria-modal="true" aria-labelledby="prologue-title">
      <img v-if="prologueVisual" class="prologue-background" :src="prologueVisual.url" alt="" />
      <div class="prologue-card">
        <span>调查开始 · CASE OPEN</span>
        <h1 id="prologue-title">{{ store.activeCase.title }}</h1>
        <p class="prologue-summary">{{ store.activeCase.summary }}</p>
        <ol>
          <li><strong>先听说法</strong><span>选择一名人物，从事情经过开始询问。</span></li>
          <li><strong>核对记录</strong><span>用公开事件线和已发现线索检查说法。</span></li>
          <li><strong>追问矛盾</strong><span>附加相关线索继续提问，再提交你的判断。</span></li>
        </ol>
        <small>本局真相已经锁定；你的行动会改变人物如何回应，不会改写事实。</small>
        <button class="primary-button" type="button" @click="enterInvestigation">进入调查<AppIcon name="arrow-right" :size="17" /></button>
      </div>
    </section>

    <main v-if="store.session && store.activeCase" ref="investigationGrid" class="investigation-grid" :data-mobile-tab="mobileTab">
      <aside class="people-panel workspace-people">
        <section>
          <h2><AppIcon name="people" />相关人物</h2>
          <article v-if="focusedCharacter" class="character-detail-view" :data-character-detail="focusedCharacter.id" :data-flip-id="`character-${focusedCharacter.id}`">
            <button class="detail-back" type="button" @click="closeCharacterDetail"><AppIcon name="arrow-left" :size="17" />返回人物</button>
            <div class="character-detail-identity" data-detail-reveal>
              <div class="character-detail-mark"><img v-if="focusedCharacter.portrait_url" :src="focusedCharacter.portrait_url" alt="" /><template v-else>{{ focusedCharacter.name.slice(0, 1) }}</template></div>
              <div><h3>{{ focusedCharacter.name }}</h3><span>{{ focusedCharacter.role }}</span></div>
            </div>
            <p data-detail-reveal>{{ focusedCharacter.public_profile }}</p>
            <small data-detail-reveal>公开人物档案，不包含隐藏事实或内部状态。</small>
            <button class="secondary-button character-talk-button" type="button" data-detail-reveal @click="talkToFocusedCharacter">与{{ focusedCharacter.name }}交谈<AppIcon name="arrow-right" :size="16" /></button>
          </article>
          <div v-else class="people-list">
            <div v-for="character in store.activeCase.characters" :key="character.id" class="person-row-wrap" :class="{ selected: store.activeCharacterId === character.id }" :data-character-source="character.id" :data-flip-id="`character-${character.id}`">
              <button class="person-row" type="button" :disabled="store.streaming" @click="selectCharacter(character.id)">
                <span class="person-mark"><img v-if="character.portrait_url" :src="character.portrait_url" alt="" /><template v-else>{{ character.name.slice(0, 1) }}</template></span>
                <span><strong>{{ character.name }}</strong><small>{{ character.role }}</small></span>
              </button>
              <button class="person-detail-button" type="button" :aria-label="`查看 ${character.name} 的公开档案`" @click="openCharacterDetail(character.id)"><AppIcon name="chevron-right" :size="18" /></button>
            </div>
          </div>
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
        <img v-if="sceneVisual" class="dialogue-scene-backdrop" :src="sceneVisual.url" alt="" />
        <header class="dialogue-heading">
          <div><span>正在与{{ store.activeCharacter?.name }}交谈</span><small>{{ store.activeCharacter?.role }}</small></div>
          <button class="mobile-context-button" type="button" @click="mobileTab = 'people'">切换人物</button>
        </header>
        <div ref="transcript" class="transcript" aria-live="polite">
          <div v-if="visibleConversation.length === 0 && !visiblePendingQuestion" class="transcript-empty">
            <span>第一步 · 听取说法</span>
            <h2>先听{{ store.activeCharacter?.name }}从头说明</h2>
            <p>第一轮不必急着出示线索。先建立完整说法，再从时间、地点和行为中寻找需要核对之处。</p>
            <button type="button" @click="question = '请从头说说，事情发生前后你做了什么？'">使用建议问题</button>
          </div>
          <article v-for="(entry, index) in visibleConversation" :key="`${entry.turn_id}-${entrySpeaker(entry)}`" class="transcript-turn" :class="{ player: speakerId(entry.speaker) === 'Player' }" :data-turn-index="index">
            <header><span class="speaker-mark"><img v-if="entryPortrait(entry)" :src="entryPortrait(entry)" alt="" /><template v-else>{{ entrySpeaker(entry).slice(0, 1) }}</template></span><strong>{{ entrySpeaker(entry) }}</strong><time>回合 {{ entryTurnNumber(entry) }}</time></header>
            <p>{{ entry.text }}</p>
            <div v-if="entry.attached_evidence.length" class="turn-attachments"><AppIcon name="paperclip" :size="16" />{{ entry.attached_evidence.map((id) => store.session?.discovered_evidence.find((item) => item.id === id)?.title ?? id).join('、') }}</div>
          </article>
          <article v-if="visiblePendingQuestion" class="transcript-turn player pending-player-turn">
            <header><span class="speaker-mark">你</span><strong>你</strong><time>回合 {{ pendingTurnNumber }}</time></header>
            <p>{{ visiblePendingQuestion.text }}</p>
            <div v-if="visiblePendingQuestion.attachedEvidenceIds.length" class="turn-attachments"><AppIcon name="paperclip" :size="16" />{{ visiblePendingQuestion.attachedEvidenceIds.map((id) => store.session?.discovered_evidence.find((item) => item.id === id)?.title ?? id).join('、') }}</div>
          </article>
          <article v-if="store.streaming" class="transcript-turn streaming-turn">
            <header><span class="speaker-mark streaming-mark"><img v-if="store.activeCharacter?.portrait_url" :src="store.activeCharacter.portrait_url" alt="" /><template v-else><i /><i /><i /></template></span><strong>{{ store.activeCharacter?.name }}</strong><time>{{ store.streamStage }}</time></header>
            <p>{{ store.streamText }}<span class="stream-caret" /></p>
          </article>
        </div>
        <div class="composer-region" data-evidence-dropzone>
          <div class="attachment-summary"><span><AppIcon name="paperclip" :size="18" />已附加 {{ store.selectedEvidence.length }} 条线索<small>也可拖入此处</small></span><button type="button" @click="mobileTab = 'research'; selectResearchTab('evidence')">选择线索</button></div>
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
          <button type="button" :class="{ selected: researchTab === 'evidence' }" @click="selectResearchTab('evidence')">线索</button>
          <button type="button" :class="{ selected: researchTab === 'statements' }" @click="selectResearchTab('statements')">陈述</button>
          <button type="button" :class="{ selected: researchTab === 'inferences' }" @click="selectResearchTab('inferences')">推断</button>
        </nav>
        <div v-if="researchTab === 'evidence'" class="research-content evidence-content">
          <article v-if="focusedEvidence" class="evidence-detail-view" :data-evidence-detail="focusedEvidence.id" :data-evidence-source="focusedEvidence.id" :data-flip-id="`evidence-${focusedEvidence.id}`">
            <button class="detail-back" type="button" @click="closeEvidenceDetail"><AppIcon name="arrow-left" :size="17" />返回线索</button>
            <div class="evidence-detail-icon" data-detail-reveal><AppIcon name="document" :size="30" /></div>
            <span class="detail-index" data-detail-reveal>已发现线索</span>
            <h2 data-detail-reveal>{{ focusedEvidence.title }}</h2>
            <p data-detail-reveal>{{ focusedEvidence.description }}</p>
            <small data-detail-reveal>这里仅显示结构化案件记录中的公开内容。</small>
            <button class="secondary-button evidence-attach-button" type="button" data-detail-reveal @click="toggleEvidenceWithMotion(focusedEvidence.id)">
              <AppIcon name="paperclip" :size="17" />{{ store.attachedEvidenceIds.includes(focusedEvidence.id) ? '从问题中移除' : '附加到问题' }}
            </button>
          </article>
          <template v-else>
            <label class="research-search"><AppIcon name="search" :size="18" /><input v-model="evidenceQuery" placeholder="搜索线索" /></label>
            <div v-if="filteredEvidence.length === 0" class="empty-state compact">没有匹配的线索。</div>
            <div v-for="item in filteredEvidence" :key="item.id" class="evidence-row" :class="{ attached: store.attachedEvidenceIds.includes(item.id) }" :data-draggable-evidence="item.id" :data-evidence-source="item.id" :data-flip-id="`evidence-${item.id}`">
              <label class="evidence-select">
                <input type="checkbox" :checked="store.attachedEvidenceIds.includes(item.id)" @change="toggleEvidenceWithMotion(item.id)" />
                <AppIcon class="evidence-drag-handle" name="document" :size="23" data-evidence-drag-handle />
                <span><strong>{{ item.title }}</strong><small>{{ item.description }}</small></span>
              </label>
              <button class="evidence-detail-button" type="button" :aria-label="`查看线索 ${item.title}`" @click="openEvidenceDetail(item.id)"><AppIcon name="chevron-right" :size="18" /></button>
            </div>
          </template>
        </div>
        <div v-else-if="researchTab === 'statements'" class="research-content">
          <div v-if="statements.length === 0" class="empty-state">角色陈述会随调查记录在这里。</div>
          <article v-for="entry in statements" :key="entry.turn_id" class="statement-row"><strong>{{ entrySpeaker(entry) }}</strong><p>{{ entry.text }}</p><small>回合 {{ entryTurnNumber(entry) }}</small></article>
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
