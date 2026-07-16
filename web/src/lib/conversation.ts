import type { DialogueEntry } from '../types/api'

function characterSpeakerId(entry: DialogueEntry) {
  return typeof entry.speaker === 'object' ? entry.speaker.Character : undefined
}

/**
 * Returns only the turns addressed to one character. Older saved sessions did
 * not persist target_character_id, so their target is recovered from the
 * character response carrying the same stable turn_id.
 */
export function conversationForCharacter(
  conversation: DialogueEntry[],
  characterId: string | undefined,
) {
  if (!characterId) return []

  const targetByTurn = new Map<string, string>()
  for (const entry of conversation) {
    const target = entry.target_character_id ?? characterSpeakerId(entry)
    if (target) targetByTurn.set(entry.turn_id, target)
  }

  return conversation.filter((entry) => {
    const target = entry.target_character_id ?? targetByTurn.get(entry.turn_id)
    return target === characterId
  })
}

/** One player question and its response share the same domain turn id. */
export function buildTurnNumberIndex(conversation: DialogueEntry[]) {
  const numbers = new Map<string, number>()
  for (const entry of conversation) {
    if (!numbers.has(entry.turn_id)) numbers.set(entry.turn_id, numbers.size + 1)
  }
  return numbers
}
