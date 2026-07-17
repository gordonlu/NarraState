import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'
import SiteHeader from '../components/home/SiteHeader.vue'
import CaseBriefPage from '../pages/CaseBriefPage.vue'
import CaseGenerationPage from '../pages/CaseGenerationPage.vue'
import { homePageContent } from '../content/home'

describe('player-facing UI contract', () => {
  it('shows save and conclusion controls without internal state vocabulary', async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: AppHeader }] })
    const wrapper = mount(AppHeader, {
      props: { caseTitle: '测试案件', saved: true, showConclusion: true },
      global: { plugins: [router] },
    })
    await router.isReady()
    expect(wrapper.text()).toContain('已保存')
    expect(wrapper.text()).toContain('提交判断')
    for (const forbidden of ['stress', 'phase', 'token', 'Prompt', 'LLM']) {
      expect(wrapper.text()).not.toContain(forbidden)
    }
  })

  it('exposes working mechanism and case generation navigation targets', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/', component: SiteHeader },
        { path: '/generate', component: SiteHeader },
      ],
    })
    const wrapper = mount(SiteHeader, {
      props: {
        brand: homePageContent.brand,
        navigation: homePageContent.navigation,
        casesHref: '/cases',
        playHref: '/cases/rain-gallery',
      },
      global: { plugins: [router] },
    })
    await router.isReady()

    const mechanism = wrapper.get('a[href="#mechanism"]')
    await mechanism.trigger('click')
    expect(wrapper.emitted('mechanism')).toHaveLength(1)
    expect(wrapper.get('a[href="/cases"]').text()).toBe('案件')
    expect(wrapper.get('a[href="/generate"]').text()).toBe('生成案件')
  })

  it('uses in-product validation instead of browser-default required prompts', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      configured: false,
      image_provider: { enabled: false, configured: false },
    }), { headers: { 'Content-Type': 'application/json' } })))
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/generate', component: CaseGenerationPage }],
    })
    await router.push('/generate')
    await router.isReady()
    const wrapper = mount(CaseGenerationPage, { global: { plugins: [createPinia(), router] } })

    expect(wrapper.get('form').attributes('novalidate')).toBeDefined()
    expect(wrapper.find('input[required]').exists()).toBe(false)
    await wrapper.get('.generation-form').trigger('submit')
    expect(wrapper.get('.form-error-summary').text()).toContain('还需要补充一些内容')
    expect(wrapper.findAll('.field-error')).toHaveLength(1)
    expect(wrapper.get('input[placeholder*="海港仓库"]').attributes('aria-invalid')).toBeUndefined()
    expect(wrapper.get('input[placeholder*="海港仓库"]').element.closest('label')?.classList.contains('wide')).toBe(true)
    expect(wrapper.text()).toContain('留空时会根据主题自动构思')
    expect(wrapper.text()).not.toContain('未成年人受害')
    expect(wrapper.text()).not.toContain('露骨暴力')
    const selectablePreferences = wrapper.findAll('.generation-boundaries div label')
    expect(selectablePreferences).toHaveLength(2)
    expect(selectablePreferences.map(label => label.text())).toEqual([
      '不使用超自然解释',
      '降低情绪压迫感',
    ])
    vi.unstubAllGlobals()
  })

  it('starts with an empty setting and moves attention to animated progress immediately', async () => {
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: false }))
    const scrollIntoView = vi.fn()
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
      configurable: true,
      value: scrollIntoView,
    })
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request) => {
      const url = String(input)
      if (url.includes('/config/public')) {
        return Promise.resolve(new Response(JSON.stringify({
          configured: true,
          image_provider: { enabled: false, configured: false },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      return Promise.resolve(new Response(JSON.stringify({
        job_id: 'job-1', status: 'failed', attempt_count: 1, repair_count: 0,
        error_code: 'GENERATION_PROVIDER_OUTPUT_TRUNCATED', error_message: 'Model output was truncated',
        events: [{ sequence: 0, to: 'failed' }], updated_at: '',
      }), { headers: { 'Content-Type': 'application/json' } }))
    }))
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/generate', component: CaseGenerationPage }],
    })
    await router.push('/generate')
    await router.isReady()
    const wrapper = mount(CaseGenerationPage, { global: { plugins: [createPinia(), router] } })
    await flushPromises()

    await wrapper.get('input[aria-invalid]').setValue('雨夜列车失踪案')
    await wrapper.get('.generation-form').trigger('submit')
    await flushPromises()

    expect(scrollIntoView).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' })
    expect(wrapper.text()).toContain('模型返回的案件内容没有完整结束')
    expect(wrapper.get('input[placeholder*="海港仓库"]').element).toHaveProperty('value', '')
    vi.unstubAllGlobals()
  })

  it('opens the generated case brief when a completed job returns its case id', async () => {
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true }))
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request) => {
      if (String(input).includes('/config/public')) {
        return Promise.resolve(new Response(JSON.stringify({
          configured: true,
          image_provider: { enabled: false, configured: false },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      return Promise.resolve(new Response(JSON.stringify({
        job_id: 'job-complete', status: 'completed', attempt_count: 1, repair_count: 0,
        case_id: 'generated-harbor-case', case_version: '1.0.0', result_path: 'data/installed-cases/generated-harbor-case/1.0.0',
        events: [{ sequence: 0, to: 'completed' }], updated_at: '',
      }), { headers: { 'Content-Type': 'application/json' } }))
    }))
    const destination = { template: '<div>新案件简报</div>' }
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/generate', component: CaseGenerationPage },
        { path: '/cases', name: 'cases', component: destination },
        { path: '/cases/:caseId', name: 'case-brief', component: destination },
      ],
    })
    await router.push('/generate')
    await router.isReady()
    const wrapper = mount(CaseGenerationPage, { global: { plugins: [createPinia(), router] } })
    await flushPromises()

    await wrapper.get('input[aria-invalid]').setValue('港口失踪案')
    await wrapper.get('.generation-form').trigger('submit')
    await flushPromises()

    expect(router.currentRoute.value.name).toBe('case-brief')
    expect(router.currentRoute.value.params.caseId).toBe('generated-harbor-case')
    wrapper.unmount()
    vi.unstubAllGlobals()
  })

  it('offers append and regenerate actions for an installed generated case', async () => {
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true }))
    const modes: string[] = []
    let visualsReady = false
    const detail = {
      id: 'generated-case', title: '生成案件', summary: '测试案件', locale: 'zh-CN',
      character_count: 1, evidence_count: 0, facts: [], evidence: [],
      characters: [{ id: 'char-1', name: '林川', role: '相关人物', public_profile: '公开简介' }],
      visual_assets: [],
      visual_status: { requested: true, generated: 0, state: 'unavailable', failure_code: 'provider_failed' },
    }
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/api/v1/config/public')) {
        return Promise.resolve(new Response(JSON.stringify({
          configured: true,
          image_provider: { enabled: true, configured: true },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      if (url.endsWith('/api/v1/cases')) {
        return Promise.resolve(new Response(JSON.stringify([]), { headers: { 'Content-Type': 'application/json' } }))
      }
      if (url.includes('/visuals/generate')) {
        const mode = JSON.parse(String(init?.body)).mode as string
        modes.push(mode)
        visualsReady = true
        const updated = mode === 'append_missing' ? 2 : 3
        return Promise.resolve(new Response(JSON.stringify({
          mode, attempted: updated, updated, failed: 0, total: 3,
          visual_status: { requested: true, generated: 3, state: 'ready' },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      return Promise.resolve(new Response(JSON.stringify({
        ...detail,
        visual_status: visualsReady
          ? { requested: true, generated: 3, state: 'ready' }
          : detail.visual_status,
      }), { headers: { 'Content-Type': 'application/json' } }))
    }))
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/cases/:caseId', component: CaseBriefPage }],
    })
    await router.push('/cases/generated-case')
    await router.isReady()
    const wrapper = mount(CaseBriefPage, { global: { plugins: [createPinia(), router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('开局锁定一个已经验证的真相版本')
    expect(wrapper.text()).not.toContain('使用作者推荐版本')
    expect(wrapper.text()).not.toContain('开发者真相设置')
    await wrapper.get('.brief-visual-actions button:first-child').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('已更新 2 张配图')
    expect(wrapper.get('.brief-visual-actions button:first-child').attributes('disabled')).toBeDefined()
    expect(wrapper.get('.brief-visual-actions button:first-child').text()).toContain('已完成')

    await wrapper.findAll('.brief-visual-actions button')[1].trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('确认重新生成全部配图')
    expect(wrapper.text()).toContain('可能产生费用')
    expect(modes).toEqual(['append_missing'])
    await wrapper.get('.brief-visual-confirmation .secondary-button').trigger('click')
    expect(wrapper.find('.brief-visual-confirmation').exists()).toBe(false)
    expect(modes).toEqual(['append_missing'])

    await wrapper.findAll('.brief-visual-actions button')[1].trigger('click')
    await wrapper.get('.brief-visual-confirmation .primary-button').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('已更新 3 张配图')
    expect(modes).toEqual(['append_missing', 'regenerate_all'])

    wrapper.unmount()
    vi.unstubAllGlobals()
  })

  it('shows a live elapsed time while case generation is waiting', async () => {
    vi.useFakeTimers()
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true }))
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request) => {
      if (String(input).includes('/config/public')) {
        return Promise.resolve(new Response(JSON.stringify({
          configured: true,
          image_provider: { enabled: false, configured: false },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      return new Promise<Response>(() => {})
    }))
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/generate', component: CaseGenerationPage }],
    })
    await router.push('/generate')
    await router.isReady()
    const wrapper = mount(CaseGenerationPage, { global: { plugins: [createPinia(), router] } })
    await Promise.resolve()

    await wrapper.get('input[aria-invalid]').setValue('雾港失踪案')
    void wrapper.get('.generation-form').trigger('submit')
    await Promise.resolve()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('已等待 0 秒')

    await vi.advanceTimersByTimeAsync(2_100)
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('已等待 2 秒')

    wrapper.unmount()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('renders persisted staged generation progress without developer terminology', async () => {
    vi.useFakeTimers()
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({ matches: true }))
    vi.stubGlobal('fetch', vi.fn((input: string | URL | Request, init?: RequestInit) => {
      const url = String(input)
      if (url.includes('/config/public')) {
        return Promise.resolve(new Response(JSON.stringify({
          configured: true,
          image_provider: { enabled: false, configured: false },
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      if (init?.method === 'POST') {
        return Promise.resolve(new Response(JSON.stringify({
          job_id: 'job-staged', status: 'drafting', attempt_count: 0, repair_count: 0,
          events: [{ sequence: 0, to: 'drafting', stage: 'blueprint' }], updated_at: '',
        }), { headers: { 'Content-Type': 'application/json' } }))
      }
      return Promise.resolve(new Response(JSON.stringify({
        job_id: 'job-staged', status: 'drafting', attempt_count: 0, repair_count: 0,
        events: [
          { sequence: 0, to: 'drafting', stage: 'blueprint' },
          { sequence: 1, to: 'drafting', stage: 'shared_content' },
          { sequence: 2, to: 'drafting', stage: 'variants', completed: 2, total: 3 },
        ],
        updated_at: '',
      }), { headers: { 'Content-Type': 'application/json' } }))
    }))
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/generate', component: CaseGenerationPage }],
    })
    await router.push('/generate')
    await router.isReady()
    const wrapper = mount(CaseGenerationPage, { global: { plugins: [createPinia(), router] } })
    await Promise.resolve()

    await wrapper.get('input[aria-invalid]').setValue('群岛灯塔失踪案')
    void wrapper.get('.generation-form').trigger('submit')
    await Promise.resolve()
    await wrapper.vm.$nextTick()
    await vi.advanceTimersByTimeAsync(710)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('规划故事与真相框架')
    expect(wrapper.text()).toContain('构建人物与公共线索')
    expect(wrapper.text()).toContain('生成不同的真相 · 2/3')
    expect(wrapper.text()).not.toContain('blueprint')
    expect(wrapper.text()).not.toContain('shared_content')

    wrapper.unmount()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })
})
