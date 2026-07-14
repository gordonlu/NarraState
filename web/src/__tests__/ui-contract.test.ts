import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import AppHeader from '../components/AppHeader.vue'

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
})
