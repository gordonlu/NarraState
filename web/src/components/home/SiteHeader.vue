<script setup lang="ts">
import { ref } from 'vue'
import type { HomePageContent } from '../../content/home'
import AppIcon from '../AppIcon.vue'

defineProps<{
  brand: HomePageContent['brand']
  navigation: HomePageContent['navigation']
  playHref: string
}>()

defineEmits<{ settings: [] }>()

const menuOpen = ref(false)
</script>

<template>
  <header class="home-site-header" :class="{ 'menu-open': menuOpen }">
    <a class="home-wordmark" href="#top" :aria-label="`${brand.chineseName}，${brand.englishName} 首页`">
      <strong>{{ brand.chineseName }}</strong><small>{{ brand.englishName }}</small><span aria-hidden="true" />
    </a>
    <button
      class="home-menu-button"
      type="button"
      :aria-expanded="menuOpen"
      aria-controls="home-navigation"
      aria-label="打开导航菜单"
      @click="menuOpen = !menuOpen"
    >
      <AppIcon :name="menuOpen ? 'close' : 'menu'" :size="22" />
    </button>
    <nav id="home-navigation" class="home-navigation" aria-label="首页导航">
      <a href="#mechanism" @click="menuOpen = false">{{ navigation.mechanism }}</a>
      <RouterLink :to="playHref" @click="menuOpen = false">{{ navigation.cases }}</RouterLink>
      <a href="https://github.com/gordonlu/NarraState" target="_blank" rel="noreferrer">{{ navigation.openSource }}</a>
      <a href="https://github.com/gordonlu/NarraState/tree/main/docs" target="_blank" rel="noreferrer">{{ navigation.docs }}</a>
      <button class="home-settings-button" type="button" @click="$emit('settings'); menuOpen = false">
        <AppIcon name="gear" :size="17" />
        <span>{{ navigation.settings }}</span>
      </button>
      <RouterLink class="home-nav-cta" :to="playHref" @click="menuOpen = false">
        {{ navigation.play }}
        <AppIcon name="arrow-right" :size="16" />
      </RouterLink>
    </nav>
  </header>
</template>
