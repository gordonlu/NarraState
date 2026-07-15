<script setup lang="ts">
import type { HomePageContent } from '../../content/home'
import type { HomeAssetMap } from '../../content/homeAssets'
import AppIcon from '../AppIcon.vue'

defineProps<{
  content: HomePageContent['hero']
  assets: HomeAssetMap
  playHref: string
}>()
</script>

<template>
  <section id="top" class="hero-scene" aria-labelledby="home-title">
    <img
      class="hero-world-image"
      :src="assets.heroWorld"
      alt=""
      width="1672"
      height="941"
      fetchpriority="high"
    />
    <div class="hero-paper-edge" aria-hidden="true" />
    <div class="hero-copy">
      <h1 id="home-title">
        <span>{{ content.titleLine1 }}</span>
        <strong>{{ content.highlightedText }}</strong>
      </h1>
      <p>{{ content.description }}</p>
      <RouterLink class="home-primary-action" :to="playHref">
        {{ content.primaryAction }}
        <AppIcon name="arrow-right" :size="18" />
      </RouterLink>
    </div>
    <div class="hero-scroll-cue" aria-hidden="true"><span>SCROLL</span><i /></div>
  </section>
</template>
