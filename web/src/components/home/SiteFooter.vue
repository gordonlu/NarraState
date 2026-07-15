<script setup lang="ts">
import type { HomePageContent } from '../../content/home'

defineProps<{ content: HomePageContent['footer']; brand: HomePageContent['brand'] }>()
</script>

<template>
  <footer class="home-site-footer">
    <div><strong>{{ brand.chineseName }}</strong><span>{{ brand.englishName }}</span><p>{{ content.tagline }}</p></div>
    <nav aria-label="页脚导航">
      <a href="https://github.com/gordonlu/NarraState" target="_blank" rel="noreferrer">GitHub</a>
      <a href="https://github.com/gordonlu/NarraState/tree/main/docs" target="_blank" rel="noreferrer">文档</a>
      <a href="https://github.com/gordonlu/NarraState/blob/main/LICENSE" target="_blank" rel="noreferrer">MIT License</a>
    </nav>
    <small>{{ content.copyright }}</small>
  </footer>
</template>
