<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ message: string; tone?: 'info' | 'warning' | 'error' }>()
defineEmits<{ close: [] }>()

const title = computed(() => ({
  info: '进度已更新',
  warning: '需要留意',
  error: '暂时无法完成',
})[props.tone ?? 'info'])
</script>

<template>
  <div class="notice-bar" :class="`notice-${tone ?? 'info'}`" :role="tone === 'error' ? 'alert' : 'status'">
    <span class="notice-icon"><AppIcon :name="tone === 'error' || tone === 'warning' ? 'warning' : 'check'" :size="19" /></span>
    <span class="notice-copy"><strong>{{ title }}</strong><span>{{ message }}</span></span>
    <button type="button" aria-label="关闭提示" @click="$emit('close')"><AppIcon name="close" :size="17" /></button>
  </div>
</template>
