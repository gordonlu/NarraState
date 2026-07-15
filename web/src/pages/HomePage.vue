<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import HeroScene from '../components/home/HeroScene.vue'
import NarrativeScrollScene from '../components/home/NarrativeScrollScene.vue'
import ReplayScene from '../components/home/ReplayScene.vue'
import SiteFooter from '../components/home/SiteFooter.vue'
import SiteHeader from '../components/home/SiteHeader.vue'
import NoticeBar from '../components/NoticeBar.vue'
import SettingsDrawer from '../components/SettingsDrawer.vue'
import { homePageContent } from '../content/home'
import { homeAssets } from '../content/homeAssets'
import { createHomeScrollTimeline, type HomeScrollController } from '../lib/homeScroll'
import { useSessionStore } from '../stores/session'

const store = useSessionStore()
const settingsOpen = ref(false)
const cinematicRoot = ref<HTMLElement>()
let scrollController: HomeScrollController | undefined

const playHref = computed(() => store.cases[0] ? `/cases/${store.cases[0].id}` : '/generate')

onMounted(async () => {
  await store.bootstrap()
  await nextTick()
  if (!cinematicRoot.value || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
  scrollController = createHomeScrollTimeline(
    {
      root: cinematicRoot.value,
      header: document.querySelector<HTMLElement>('.home-site-header'),
    },
    {
      compact: window.matchMedia('(max-width: 760px)').matches,
      stages: homePageContent.stateScene.stages,
    },
  )
})

onBeforeUnmount(() => scrollController?.destroy())
</script>

<template>
  <div class="home-experience">
    <SiteHeader :brand="homePageContent.brand" :navigation="homePageContent.navigation" :play-href="playHref" @settings="settingsOpen = true" />
    <NoticeBar v-if="store.error" class="home-notice" :message="store.error" tone="error" @close="store.error = undefined" />
    <main>
      <div ref="cinematicRoot" class="home-cinematic">
        <div class="home-cinematic-frame">
          <HeroScene :content="homePageContent.hero" :assets="homeAssets" :play-href="playHref" />
          <NarrativeScrollScene :content="homePageContent.stateScene" :assets="homeAssets" />
          <ReplayScene :content="homePageContent.replayScene" :assets="homeAssets" />
        </div>
      </div>
    </main>
    <SiteFooter :content="homePageContent.footer" :brand="homePageContent.brand" />
    <SettingsDrawer :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>
