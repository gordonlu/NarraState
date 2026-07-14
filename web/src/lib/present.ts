import type { AccusationResult, DialogueSpeaker, Fact, StoryTime } from '../types/api'

export function formatStoryTime(time?: StoryTime) {
  if (!time) return '—'
  if (time.label) return time.label
  if (time.hour === undefined) return '—'
  return `${String(time.hour).padStart(2, '0')}:${String(time.minute ?? 0).padStart(2, '0')}`
}

export function describeFact(fact: Fact) {
  if (fact.display_text) return fact.display_text
  const predicate = fact.predicate.replaceAll('_', ' ')
  return `${fact.subject} ${predicate} ${String(fact.object)}`
}

export function speakerId(speaker: DialogueSpeaker) {
  if (typeof speaker === 'string') return speaker
  return speaker.Character
}

export function resultLabel(result: AccusationResult) {
  switch (result) {
    case 'WrongSuspect':
      return '指认对象不成立'
    case 'CorrectButInsufficient':
      return '方向正确，证据不足'
    case 'CaseProvenWithoutConfession':
      return '证据链成立'
    case 'CaseProvenWithConfession':
      return '证据链成立并取得完整陈述'
  }
}

export function formatSavedAt(value: string) {
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value))
}
