import type {
  AccusationResult,
  CaseDetail,
  CaseSummary,
  CreateGameResponse,
  ConclusionReport,
  DebugSession,
  GenerationJob,
  CreateGenerationJobRequest,
  ProblemDetails,
  PublicConfig,
  PublicEvent,
  PublicSession,
  SessionMode,
  SseMessage,
  TurnResult,
  VisualGenerationMode,
  VisualGenerationResult,
} from '../types/api'

const API_BASE = (import.meta.env.VITE_API_BASE ?? '').replace(/\/$/, '')

export class ApiError extends Error {
  readonly problem: ProblemDetails

  constructor(problem: ProblemDetails) {
    super(problem.detail || problem.title)
    this.name = 'ApiError'
    this.problem = problem
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...init?.headers,
    },
  })
  if (!response.ok) throw await responseError(response)
  return response.json() as Promise<T>
}

async function responseError(response: Response): Promise<ApiError> {
  try {
    const problem = (await response.json()) as ProblemDetails
    return new ApiError(problem)
  } catch {
    return new ApiError({
      title: '请求失败',
      status: response.status,
      detail: response.statusText || '服务暂时不可用',
    })
  }
}

export const api = {
  health: () => request<{ status: string; version: string }>('/api/v1/health'),
  config: () => request<PublicConfig>('/api/v1/config/public'),
  saveProvider: (payload: { base_url: string; model: string; api_key?: string; persist_api_key?: boolean }) =>
    request<{ ok: boolean }>('/api/v1/config/provider', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  saveImageProvider: (payload: { enabled: boolean; base_url: string; model: string; api_key?: string; persist_api_key?: boolean }) =>
    request<{ ok: boolean }>('/api/v1/config/image-provider', { method: 'POST', body: JSON.stringify(payload) }),
  testProvider: (payload: { base_url: string; model: string; api_key?: string; persist_api_key?: boolean }) =>
    request<{ ok: boolean }>('/api/v1/config/test-provider', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  generateCase: (payload: CreateGenerationJobRequest) => request<GenerationJob>('/api/v1/case-generation/jobs', {
    method: 'POST', body: JSON.stringify(payload),
  }),
  generationJob: (jobId: string) => request<GenerationJob>(`/api/v1/case-generation/jobs/${encodeURIComponent(jobId)}`),
  cases: () => request<CaseSummary[]>('/api/v1/cases'),
  case: (caseId: string) => request<CaseDetail>(`/api/v1/cases/${encodeURIComponent(caseId)}`),
  generateCaseVisuals: (caseId: string, mode: VisualGenerationMode) =>
    request<VisualGenerationResult>(`/api/v1/cases/${encodeURIComponent(caseId)}/visuals/generate`, {
      method: 'POST', body: JSON.stringify({ mode }),
    }),
  createGame: (payload: { case_id: string; variant_selection: { mode: 'default' | 'random' } | { mode: 'specific'; variant_id: string }; seed?: number; mode: SessionMode }) =>
    request<CreateGameResponse>('/api/v1/games', { method: 'POST', body: JSON.stringify(payload) }),
  createSession: (caseId: string, mode: SessionMode, targetCharacterId?: string) =>
    request<PublicSession>('/api/v1/sessions', {
      method: 'POST',
      body: JSON.stringify({
        case_id: caseId,
        mode,
        target_character_id: targetCharacterId,
      }),
    }),
  session: (sessionId: string) =>
    request<PublicSession>(`/api/v1/sessions/${encodeURIComponent(sessionId)}`),
  events: (sessionId: string) =>
    request<PublicEvent[]>(`/api/v1/sessions/${encodeURIComponent(sessionId)}/events`),
  debug: (sessionId: string) =>
    request<DebugSession>(`/api/v1/sessions/${encodeURIComponent(sessionId)}/debug`),
  conclusion: (sessionId: string) =>
    request<ConclusionReport>(`/api/v1/sessions/${encodeURIComponent(sessionId)}/conclusion`),
  restart: (sessionId: string) =>
    request<PublicSession>(`/api/v1/sessions/${encodeURIComponent(sessionId)}/restart`, {
      method: 'POST',
    }),
  accuse: (
    sessionId: string,
    payload: {
      expected_revision: number
      target_character_id: string
      evidence_ids: string[]
      reasoning: string
    },
  ) =>
    request<{ result: AccusationResult; session: PublicSession }>(
      `/api/v1/sessions/${encodeURIComponent(sessionId)}/accusations`,
      { method: 'POST', body: JSON.stringify(payload) },
    ),
}

export async function streamAction(
  sessionId: string,
  payload: {
    client_action_id: string
    expected_revision: number
    target_character_id: string
    text: string
    attached_evidence_ids: string[]
  },
  onMessage: (message: SseMessage) => void,
  signal?: AbortSignal,
): Promise<TurnResult> {
  const response = await fetch(
    `${API_BASE}/api/v1/sessions/${encodeURIComponent(sessionId)}/actions`,
    {
      method: 'POST',
      headers: { Accept: 'text/event-stream', 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal,
    },
  )
  if (!response.ok) throw await responseError(response)
  if (!response.body) {
    throw new ApiError({ title: '流式响应失败', status: 502, detail: '服务未返回响应流' })
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  let completed: TurnResult | undefined

  while (true) {
    const { value, done } = await reader.read()
    buffer += decoder.decode(value, { stream: !done })
    const blocks = buffer.split(/\r?\n\r?\n/)
    buffer = blocks.pop() ?? ''
    for (const block of blocks) {
      const message = parseSseBlock(block)
      if (!message) continue
      onMessage(message)
      if (message.event === 'turn.completed') completed = message.data as TurnResult
      if (message.event === 'turn.failed') throw new ApiError(message.data as ProblemDetails)
    }
    if (done) break
  }

  if (!completed) {
    throw new ApiError({
      title: '回合未完成',
      status: 502,
      detail: '连接提前中断，正在重新读取已保存进度',
    })
  }
  return completed
}

function parseSseBlock(block: string): SseMessage | undefined {
  let event = 'message'
  const data: string[] = []
  for (const line of block.split(/\r?\n/)) {
    if (line.startsWith('event:')) event = line.slice(6).trim()
    if (line.startsWith('data:')) data.push(line.slice(5).trimStart())
  }
  if (data.length === 0) return undefined
  const raw = data.join('\n')
  try {
    return { event, data: JSON.parse(raw) as unknown }
  } catch {
    return { event, data: raw }
  }
}
