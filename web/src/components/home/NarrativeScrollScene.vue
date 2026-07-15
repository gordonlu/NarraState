<script setup lang="ts">
import type { HomePageContent } from '../../content/home'
import type { HomeAssetMap } from '../../content/homeAssets'

defineProps<{
  content: HomePageContent['stateScene']
  assets: HomeAssetMap
}>()
</script>

<template>
  <section id="mechanism" class="narrative-scene" aria-labelledby="state-scene-title">
    <img class="character-room-image" :src="assets.characterRoom" alt="同一角色在同一场景中的中性示意图" width="1672" height="941" loading="lazy" />
    <div class="narrative-vignette" aria-hidden="true" />
    <div class="narrative-content">
      <p class="truth-lock"><span aria-hidden="true" />{{ content.lockStatement }}</p>
      <h2 id="state-scene-title">{{ content.heading }}</h2>
      <p class="narrative-description">{{ content.description }}</p>
      <div class="narrative-stage-stack" aria-live="polite">
        <article v-for="stage in content.stages" :key="stage.id" class="narrative-stage-copy" :data-stage="stage.id">
          <p v-if="stage.playerPrompt" class="player-prompt"><span>你问</span>{{ stage.playerPrompt }}</p>
          <blockquote>{{ stage.response }}</blockquote>
        </article>
      </div>
      <div class="state-trajectory" aria-label="角色披露阶段：否认、解释、部分承认">
        <i class="trajectory-line" aria-hidden="true"><b /></i>
        <ol>
          <li v-for="stage in content.stages" :key="stage.id" :data-marker="stage.id">
            <span>{{ stage.index }}</span><strong>{{ stage.label }}</strong>
          </li>
        </ol>
      </div>
      <div class="state-readout">
        <span>压力 <strong data-pressure>24</strong></span>
        <span>披露阶段 <strong data-phase>否认</strong></span>
      </div>
    </div>
    <div class="rule-statement" aria-label="核心规则">
      <p>{{ content.ruleStatementLine1 }}</p>
      <p><strong>{{ content.ruleStatementLine2 }}</strong></p>
    </div>
  </section>
</template>
