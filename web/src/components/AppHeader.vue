<script setup lang="ts">
import AppIcon from './AppIcon.vue'

withDefaults(
  defineProps<{
    caseTitle?: string
    saved?: boolean
    showConclusion?: boolean
    backLabel?: string
  }>(),
  { saved: false, showConclusion: false },
)

defineEmits<{
  settings: []
  conclusion: []
  back: []
}>()
</script>

<template>
  <header class="app-header">
    <button v-if="backLabel" class="header-back" type="button" @click="$emit('back')">
      <AppIcon name="arrow-left" :size="18" />
      {{ backLabel }}
    </button>
    <RouterLink v-else class="wordmark" to="/">NarraState</RouterLink>
    <div v-if="caseTitle" class="header-case-title">{{ caseTitle }}</div>
    <div class="header-spacer" />
    <div v-if="saved" class="saved-state"><AppIcon name="check" :size="17" />已保存</div>
    <button v-if="showConclusion" class="primary-button header-primary" type="button" @click="$emit('conclusion')">
      提交判断
    </button>
    <button class="icon-button settings-button" type="button" aria-label="设置" @click="$emit('settings')">
      <AppIcon name="gear" :size="21" />
      <span>设置</span>
    </button>
  </header>
</template>
