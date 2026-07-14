<script setup lang="ts">
import AppIcon from './AppIcon.vue'

defineProps<{ message: string; tone?: 'info' | 'warning' | 'error' }>()
defineEmits<{ close: [] }>()
</script>

<template>
  <div class="notice-bar" :class="`notice-${tone ?? 'info'}`" role="status">
    <AppIcon :name="tone === 'error' || tone === 'warning' ? 'warning' : 'check'" :size="18" />
    <span>{{ message }}</span>
    <button type="button" aria-label="关闭提示" @click="$emit('close')"><AppIcon name="close" :size="17" /></button>
  </div>
</template>
