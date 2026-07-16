import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('./pages/HomePage.vue'),
    },
    {
      path: '/cases',
      name: 'cases',
      component: () => import('./pages/CaseListPage.vue'),
    },
    {
      path: '/cases/:caseId',
      name: 'case-brief',
      component: () => import('./pages/CaseBriefPage.vue'),
    },
    {
      path: '/generate',
      name: 'case-generation',
      component: () => import('./pages/CaseGenerationPage.vue'),
    },
    {
      path: '/sessions/:sessionId',
      name: 'investigation',
      component: () => import('./pages/InvestigationPage.vue'),
    },
    {
      path: '/sessions/:sessionId/conclusion',
      name: 'conclusion',
      component: () => import('./pages/ConclusionPage.vue'),
    },
  ],
})

export default router
