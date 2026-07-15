export type SessionMode = 'mock' | 'llm'
export type SessionStatus = 'Active' | 'Resolved' | 'Abandoned'
export type AccusationResult =
  | 'WrongSuspect'
  | 'CorrectButInsufficient'
  | 'CaseProvenWithoutConfession'
  | 'CaseProvenWithConfession'

export interface StoryTime {
  year?: number
  month?: number
  day?: number
  hour?: number
  minute?: number
  label?: string
}

export interface Fact {
  id: string
  display_text?: string
  subject: string
  predicate: string
  object: string | number | boolean
  happened_at?: StoryTime
  location?: string
  truth: 'True' | 'False' | 'Uncertain'
  tags: string[]
  visibility: 'PublicAtStart' | 'Discoverable' | 'Hidden'
}

export interface CharacterSummary {
  id: string
  name: string
  role: string
  public_profile: string
  portrait_url?: string
}

export interface Evidence {
  id: string
  title: string
  description: string
}

export interface CaseSummary {
  id: string
  title: string
  summary: string
  locale: string
  character_count: number
  evidence_count: number
  cover_url?: string
}

export interface CaseDetail extends CaseSummary {
  facts: Fact[]
  evidence: Evidence[]
  characters: CharacterSummary[]
  visual_assets: VisualAsset[]
}

export type GeneratedVisualType =
  | 'case_cover'
  | 'chapter_illustration'
  | 'scene_background'
  | 'location_atmosphere'
  | 'character_portrait'
  | 'transition_illustration'
  | 'ending_illustration'

export interface VisualAsset {
  id: string
  visual_type: GeneratedVisualType
  url: string
  alt_text: string
}

export type DialogueSpeaker = 'Player' | 'System' | { Character: string }

export interface DialogueEntry {
  turn_id: string
  speaker: DialogueSpeaker
  text: string
  attached_evidence: string[]
}

export interface Accusation {
  turn_id: string
  target: string
  evidence_ids: string[]
  reasoning: string
  result: AccusationResult
}

export interface PublicSession {
  session_id: string
  case_id: string
  mode?: SessionMode
  status: SessionStatus
  current_turn: number
  active_character?: string
  discovered_facts: Fact[]
  discovered_evidence: Evidence[]
  conversation: DialogueEntry[]
  accusations: Accusation[]
  revision: number
}

export interface PublicConfig {
  configured: boolean
  key_persisted: boolean
  base_url: string
  model: string
  api_key: string
  image_provider: { enabled: boolean; configured: boolean; key_persisted: boolean; base_url: string; model: string }
}

export interface GenerationRequest {
  theme: string; setting: string; tone: string; target_duration_minutes: number
  difficulty: string; character_count: number; variant_count: number; realism: string
  confession_policy: string; content_constraints: string[]; language: string
}

export interface GenerationJob {
  job_id: string; status: string; attempt_count: number; repair_count: number
  error_code?: string; error_message?: string; result_path?: string
  events: Array<{ sequence: number; to: string; error_code?: string }>; updated_at: string
}
export interface CreateGameResponse { session_id: string; instance_id: string; case_id: string; case_version: string; seed: number }

export interface ProblemDetails {
  type?: string
  title: string
  status: number
  detail: string
}

export interface TurnResult {
  session_id: string
  turn_id: string
  revision: number
  utterance: string
  degraded: boolean
}

export interface PublicEvent {
  sequence: number
  turn_id?: string
  event_type: string
  schema_version: number
}

export interface CharacterDebugState {
  phase: string
  stress: number
  composure: number
  trust: number
  defense_budget: number
  confronted_evidence: string[]
  revealed_disclosures: string[]
  invalidated_claims: string[]
}

export interface DebugEvent extends PublicEvent {
  payload: Record<string, unknown>
}

export interface LlmCallDebug {
  call_id: string
  turn_id?: string
  purpose: string
  provider: string
  model: string
  latency_ms: number
  input_tokens?: number
  output_tokens?: number
  status: string
  error_code?: string
}

export interface DebugSession {
  character_states: Record<string, CharacterDebugState>
  events: DebugEvent[]
  llm_calls: LlmCallDebug[]
}

export interface ConclusionReport {
  result: AccusationResult
  epilogue: string
  truth_timeline: Fact[]
  decisive_evidence: Evidence[]
  reasoning: string
  confessed: boolean
  turn_count: number
}

export interface SseMessage<T = unknown> {
  event: string
  data: T
}
