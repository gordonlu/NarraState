<script setup lang="ts">
import type { HomePageContent } from '../../content/home'
import type { HomeAssetMap } from '../../content/homeAssets'

defineProps<{
  content: HomePageContent['replayScene']
  assets: HomeAssetMap
}>()
</script>

<template>
  <section id="replay" class="replay-scene" aria-labelledby="replay-title">
    <div class="archive-stage" aria-hidden="true">
      <div v-for="index in 3" :key="index" class="case-archive" :data-archive="index">
        <div class="archive-image" :style="{ backgroundImage: `url(${assets.archiveCover})` }" />
        <div class="archive-path"><i /><i /><i /></div>
        <span>NARRASTATE<br />CASE ARCHIVE</span>
      </div>
    </div>
    <div class="replay-copy">
      <span class="replay-index">REPLAY / 03</span>
      <h2 id="replay-title"><span>{{ content.headingLine1 }}</span><strong>{{ content.headingLine2 }}</strong></h2>
      <p>{{ content.description }}</p>
      <div class="truth-flow" aria-label="开局选择变体，然后冻结真相，最后进入游玩">
        <span>开局选择变体</span><i /><span>真相冻结</span><i /><span>角色状态演化</span>
      </div>
    </div>
  </section>
</template>
