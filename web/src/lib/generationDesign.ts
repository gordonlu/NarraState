import type { GenerationRequest } from '../types/api'

export type GenerationDifficulty = 'easy' | 'medium' | 'hard'

export interface GenerationScope {
  maxCharacters: number
  maxVariants: number
  allowedDifficulties: GenerationDifficulty[]
  description: string
}

export function generationScope(duration: number): GenerationScope {
  if (duration < 25) return {
    maxCharacters: 3,
    maxVariants: 1,
    allowedDifficulties: ['easy'],
    description: '短篇体验：聚焦单一真相和少量人物。',
  }
  if (duration < 45) return {
    maxCharacters: 4,
    maxVariants: 2,
    allowedDifficulties: ['easy', 'medium'],
    description: '标准短案：可以容纳一条支线和两种真相。',
  }
  if (duration < 75) return {
    maxCharacters: 4,
    maxVariants: 3,
    allowedDifficulties: ['easy', 'medium', 'hard'],
    description: '完整案件：适合多人物、证据链和多种真相。',
  }
  return {
    maxCharacters: 4,
    maxVariants: 5,
    allowedDifficulties: ['easy', 'medium', 'hard'],
    description: '长篇调查：允许更多人物、支线和真相变化。',
  }
}

export function normalizeGenerationScope(request: GenerationRequest) {
  const scope = generationScope(request.target_duration_minutes)
  if (!scope.allowedDifficulties.includes(request.difficulty as GenerationDifficulty)) {
    request.difficulty = scope.allowedDifficulties.at(-1) ?? 'easy'
  }
  request.character_count = Math.min(request.character_count, scope.maxCharacters)
  request.variant_count = Math.min(request.variant_count, scope.maxVariants)
  return scope
}
