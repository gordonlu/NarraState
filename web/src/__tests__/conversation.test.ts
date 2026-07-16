import { describe, expect, it } from 'vitest'
import { buildTurnNumberIndex, conversationForCharacter } from '../lib/conversation'
import type { DialogueEntry } from '../types/api'

function entry(
  turnId: string,
  speaker: DialogueEntry['speaker'],
  text: string,
  targetCharacterId?: string,
): DialogueEntry {
  return {
    turn_id: turnId,
    target_character_id: targetCharacterId,
    speaker,
    text,
    attached_evidence: [],
  }
}

describe('character conversation projection', () => {
  it('keeps each character dialogue isolated', () => {
    const conversation = [
      entry('turn-a', 'Player', '问 A', 'a'),
      entry('turn-a', { Character: 'a' }, 'A 的回答', 'a'),
      entry('turn-b', 'Player', '问 B', 'b'),
      entry('turn-b', { Character: 'b' }, 'B 的回答', 'b'),
    ]

    expect(conversationForCharacter(conversation, 'a').map((item) => item.text))
      .toEqual(['问 A', 'A 的回答'])
    expect(conversationForCharacter(conversation, 'b').map((item) => item.text))
      .toEqual(['问 B', 'B 的回答'])
  })

  it('recovers targets in older saves from the stable turn id', () => {
    const legacyConversation = [
      entry('turn-a', 'Player', '旧存档问题'),
      entry('turn-a', { Character: 'a' }, '旧存档回答'),
      entry('turn-b', 'Player', '另一个问题'),
      entry('turn-b', { Character: 'b' }, '另一个回答'),
    ]

    expect(conversationForCharacter(legacyConversation, 'a').map((item) => item.text))
      .toEqual(['旧存档问题', '旧存档回答'])
  })

  it('assigns one number to both sides of the same turn', () => {
    const conversation = [
      entry('turn-a', 'Player', '第一问', 'a'),
      entry('turn-a', { Character: 'a' }, '第一答', 'a'),
      entry('turn-b', 'Player', '第二问', 'a'),
      entry('turn-b', { Character: 'a' }, '第二答', 'a'),
    ]

    expect([...buildTurnNumberIndex(conversation).entries()]).toEqual([
      ['turn-a', 1],
      ['turn-b', 2],
    ])
  })
})
