import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { PublicSession, TurnResult } from '../types/api'

const mocks = vi.hoisted(() => ({
  cases: vi.fn(),
  config: vi.fn(),
  session: vi.fn(),
  streamAction: vi.fn(),
}))

vi.mock('../lib/api', () => ({
  ApiError: class ApiError extends Error {
    problem = { title: '请求失败', status: 500, detail: this.message }
  },
  api: {
    cases: mocks.cases,
    config: mocks.config,
    session: mocks.session,
  },
  streamAction: mocks.streamAction,
}))

import { useSessionStore } from '../stores/session'

function publicSession(conversation: PublicSession['conversation'] = []): PublicSession {
  return {
    session_id: 'session-1',
    case_id: 'case-1',
    mode: 'mock',
    status: 'Active',
    current_turn: conversation.length / 2,
    active_character: 'a',
    discovered_facts: [],
    discovered_evidence: [],
    conversation,
    accusations: [],
    revision: conversation.length / 2,
  }
}

describe('session dialogue submission', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.clearAllMocks()
    mocks.cases.mockResolvedValue([])
    mocks.config.mockResolvedValue({ configured: false })
  })

  it('exposes the player question before the model response completes', async () => {
    let finishTurn!: (result: TurnResult) => void
    mocks.streamAction.mockImplementation(() => new Promise<TurnResult>((resolve) => {
      finishTurn = resolve
    }))
    mocks.session.mockResolvedValue(publicSession([
      {
        turn_id: 'turn-1',
        target_character_id: 'a',
        speaker: 'Player',
        text: '你去了哪里？',
        attached_evidence: [],
      },
      {
        turn_id: 'turn-1',
        target_character_id: 'a',
        speaker: { Character: 'a' },
        text: '我去了仓库。',
        attached_evidence: [],
      },
    ]))

    const store = useSessionStore()
    store.session = publicSession()
    store.activeCharacterId = 'a'

    const request = store.sendQuestion('你去了哪里？')
    expect(store.pendingQuestion).toEqual({
      targetCharacterId: 'a',
      text: '你去了哪里？',
      attachedEvidenceIds: [],
    })
    expect(store.streaming).toBe(true)

    finishTurn({
      session_id: 'session-1',
      turn_id: 'turn-1',
      revision: 1,
      utterance: '我去了仓库。',
      degraded: false,
    })
    await request

    expect(store.pendingQuestion).toBeUndefined()
    expect(store.session?.conversation.map((entry) => entry.text))
      .toEqual(['你去了哪里？', '我去了仓库。'])
  })

  it('validates and exposes an active saved session as the continue target', async () => {
    localStorage.setItem('narrastate:last-session', JSON.stringify({
      sessionId: 'session-1',
      caseId: 'case-1',
      mode: 'mock',
      savedAt: '2026-07-16T00:00:00.000Z',
    }))
    mocks.session.mockResolvedValue(publicSession())

    const store = useSessionStore()
    await store.bootstrap()

    expect(store.resumableSession?.session_id).toBe('session-1')
  })
})
