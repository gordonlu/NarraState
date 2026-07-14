import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, streamAction } from '../lib/api'

afterEach(() => vi.unstubAllGlobals())

describe('POST SSE client', () => {
  it('parses the ordered turn contract and returns completion', async () => {
    const body = [
      'event: turn.accepted\ndata: {"client_action_id":"a"}\n\n',
      'event: turn.progress\ndata: {"stage":"processing"}\n\n',
      'event: dialogue.delta\ndata: {"text":"回答"}\n\n',
      'event: state.public_changed\ndata: {"revision":1}\n\n',
      'event: turn.completed\ndata: {"session_id":"s","turn_id":"t","revision":1,"utterance":"回答","degraded":false}\n\n',
    ].join('')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body, {
      headers: { 'Content-Type': 'text/event-stream' },
    })))
    const events: string[] = []
    const result = await streamAction(
      's',
      {
        client_action_id: 'a',
        expected_revision: 0,
        target_character_id: 'c',
        text: '问题',
        attached_evidence_ids: [],
      },
      (message) => events.push(message.event),
    )
    expect(events).toEqual([
      'turn.accepted',
      'turn.progress',
      'dialogue.delta',
      'state.public_changed',
      'turn.completed',
    ])
    expect(result.utterance).toBe('回答')
  })

  it('surfaces turn.failed as Problem Details', async () => {
    const body = 'event: turn.failed\ndata: {"title":"失败","status":409,"detail":"revision conflict"}\n\n'
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body)))
    await expect(
      streamAction(
        's',
        {
          client_action_id: 'a',
          expected_revision: 0,
          target_character_id: 'c',
          text: '问题',
          attached_evidence_ids: [],
        },
        () => undefined,
      ),
    ).rejects.toBeInstanceOf(ApiError)
  })
})
