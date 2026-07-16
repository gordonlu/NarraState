import { describe, expect, it } from 'vitest'
import { generationScope, normalizeGenerationScope } from '../lib/generationDesign'
import type { GenerationRequest } from '../types/api'

function request(): GenerationRequest {
  return { theme: '测试', setting: '列车', tone: 'realistic', target_duration_minutes: 45,
    difficulty: 'hard', character_count: 4, variant_count: 3, realism: 'grounded',
    confession_policy: 'partial_then_full', content_constraints: [], language: 'zh-CN' }
}

describe('generation experience scope', () => {
  it('does not offer hard difficulty for a ten-minute case', () => {
    expect(generationScope(10).allowedDifficulties).toEqual(['easy'])
  })

  it('reduces dependent choices when duration is shortened', () => {
    const value = request()
    value.target_duration_minutes = 10
    normalizeGenerationScope(value)
    expect(value.difficulty).toBe('easy')
    expect(value.character_count).toBe(3)
    expect(value.variant_count).toBe(1)
  })
})
