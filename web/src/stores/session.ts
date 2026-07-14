import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { api, ApiError, streamAction } from '../lib/api'
import type {
  AccusationResult,
  CaseDetail,
  CaseSummary,
  DebugSession,
  PublicConfig,
  PublicSession,
  SessionMode,
} from '../types/api'

const LAST_SESSION_KEY = 'narrastate:last-session'

interface SavedSession {
  sessionId: string
  caseId: string
  mode: SessionMode
  savedAt: string
}

export const useSessionStore = defineStore('session', () => {
  const cases = ref<CaseSummary[]>([])
  const activeCase = ref<CaseDetail>()
  const session = ref<PublicSession>()
  const config = ref<PublicConfig>()
  const mode = ref<SessionMode>('mock')
  const activeCharacterId = ref<string>()
  const attachedEvidenceIds = ref<string[]>([])
  const loading = ref(false)
  const streaming = ref(false)
  const streamText = ref('')
  const streamStage = ref('')
  const degraded = ref(false)
  const notice = ref<string>()
  const error = ref<string>()
  const lastSession = ref<SavedSession | null>(readSavedSession())
  const debug = ref<DebugSession>()
  let actionAbort: AbortController | undefined

  const activeCharacter = computed(() =>
    activeCase.value?.characters.find((character) => character.id === activeCharacterId.value),
  )
  const selectedEvidence = computed(() =>
    session.value?.discovered_evidence.filter((item) => attachedEvidenceIds.value.includes(item.id)) ?? [],
  )

  async function bootstrap() {
    loading.value = true
    error.value = undefined
    try {
      ;[cases.value, config.value] = await Promise.all([api.cases(), api.config()])
    } catch (reason) {
      setError(reason)
    } finally {
      loading.value = false
    }
  }

  async function loadCase(caseId: string) {
    loading.value = true
    error.value = undefined
    try {
      activeCase.value = await api.case(caseId)
      activeCharacterId.value ??= activeCase.value.characters[0]?.id
    } catch (reason) {
      setError(reason)
      throw reason
    } finally {
      loading.value = false
    }
  }

  async function createSession(caseId: string, selectedMode: SessionMode) {
    if (!activeCase.value || activeCase.value.id !== caseId) await loadCase(caseId)
    const target = activeCharacterId.value ?? activeCase.value?.characters[0]?.id
    loading.value = true
    error.value = undefined
    try {
      mode.value = selectedMode
      session.value = await api.createSession(caseId, selectedMode, target)
      activeCharacterId.value = session.value.active_character ?? target
      saveSession()
      return session.value
    } catch (reason) {
      setError(reason)
      throw reason
    } finally {
      loading.value = false
    }
  }

  async function restoreSession(sessionId: string, savedMode?: SessionMode) {
    loading.value = true
    error.value = undefined
    try {
      session.value = await api.session(sessionId)
      mode.value = session.value.mode ?? savedMode ?? lastSession.value?.mode ?? 'mock'
      await loadCase(session.value.case_id)
      activeCharacterId.value = session.value.active_character ?? activeCase.value?.characters[0]?.id
      saveSession()
      notice.value = '已从本地服务恢复最新进度'
      return session.value
    } catch (reason) {
      setError(reason)
      throw reason
    } finally {
      loading.value = false
    }
  }

  async function sendQuestion(text: string) {
    if (!session.value || !activeCharacterId.value) throw new Error('没有活动会话')
    streaming.value = true
    streamText.value = ''
    streamStage.value = '正在提交问题'
    degraded.value = false
    error.value = undefined
    notice.value = undefined
    actionAbort = new AbortController()
    const sessionId = session.value.session_id
    try {
      const result = await streamAction(
        sessionId,
        {
          client_action_id: crypto.randomUUID(),
          expected_revision: session.value.revision,
          target_character_id: activeCharacterId.value,
          text,
          attached_evidence_ids: [...attachedEvidenceIds.value],
        },
        ({ event, data }) => {
          if (event === 'turn.accepted') streamStage.value = '问题已接收'
          if (event === 'turn.progress') streamStage.value = '正在整理回应'
          if (event === 'dialogue.delta') streamText.value = (data as { text: string }).text
          if (event === 'state.public_changed') streamStage.value = '正在保存进度'
        },
        actionAbort.signal,
      )
      degraded.value = result.degraded
      session.value = await api.session(sessionId)
      attachedEvidenceIds.value = []
      streamText.value = ''
      streamStage.value = ''
      if (result.degraded) notice.value = '本回合已使用安全降级回应，状态仍已保存'
      saveSession()
      return result
    } catch (reason) {
      if (reason instanceof DOMException && reason.name === 'AbortError') {
        notice.value = '已停止显示，正在同步后台已提交的回合'
      } else {
        setError(reason)
      }
      try {
        session.value = await api.session(sessionId)
        notice.value = '连接已恢复，进度已同步'
        saveSession()
      } catch {
        // Preserve the actionable original error when recovery is unavailable.
      }
      throw reason
    } finally {
      streaming.value = false
      actionAbort = undefined
    }
  }

  function cancelDisplay() {
    actionAbort?.abort()
  }

  function toggleEvidence(evidenceId: string) {
    const index = attachedEvidenceIds.value.indexOf(evidenceId)
    if (index >= 0) attachedEvidenceIds.value.splice(index, 1)
    else attachedEvidenceIds.value.push(evidenceId)
  }

  async function submitAccusation(payload: {
    targetCharacterId: string
    evidenceIds: string[]
    reasoning: string
  }): Promise<AccusationResult> {
    if (!session.value) throw new Error('没有活动会话')
    loading.value = true
    error.value = undefined
    try {
      const response = await api.accuse(session.value.session_id, {
        expected_revision: session.value.revision,
        target_character_id: payload.targetCharacterId,
        evidence_ids: payload.evidenceIds,
        reasoning: payload.reasoning,
      })
      session.value = response.session
      saveSession()
      return response.result
    } catch (reason) {
      setError(reason)
      throw reason
    } finally {
      loading.value = false
    }
  }

  async function loadDebug() {
    if (!session.value) return
    debug.value = await api.debug(session.value.session_id)
  }

  async function restartCurrent() {
    if (!session.value) throw new Error('没有可重开的会话')
    session.value = await api.restart(session.value.session_id)
    activeCharacterId.value = session.value.active_character ?? activeCase.value?.characters[0]?.id
    attachedEvidenceIds.value = []
    debug.value = undefined
    saveSession()
    return session.value
  }

  async function testProvider(payload: { base_url: string; model: string; api_key?: string }) {
    await api.testProvider(payload)
    config.value = await api.config()
  }

  function clearNotice() {
    notice.value = undefined
  }

  function saveSession() {
    if (!session.value) return
    const saved: SavedSession = {
      sessionId: session.value.session_id,
      caseId: session.value.case_id,
      mode: mode.value,
      savedAt: new Date().toISOString(),
    }
    localStorage.setItem(LAST_SESSION_KEY, JSON.stringify(saved))
    lastSession.value = saved
  }

  function setError(reason: unknown) {
    error.value = reason instanceof ApiError ? reason.problem.detail : toMessage(reason)
  }

  return {
    cases,
    activeCase,
    session,
    config,
    mode,
    activeCharacterId,
    activeCharacter,
    attachedEvidenceIds,
    selectedEvidence,
    loading,
    streaming,
    streamText,
    streamStage,
    degraded,
    notice,
    error,
    lastSession,
    debug,
    bootstrap,
    loadCase,
    createSession,
    restoreSession,
    sendQuestion,
    cancelDisplay,
    toggleEvidence,
    submitAccusation,
    loadDebug,
    restartCurrent,
    testProvider,
    clearNotice,
  }
})

function readSavedSession(): SavedSession | null {
  try {
    const raw = localStorage.getItem(LAST_SESSION_KEY)
    return raw ? (JSON.parse(raw) as SavedSession) : null
  } catch {
    return null
  }
}

function toMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : '发生未知错误'
}
