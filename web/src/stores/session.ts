import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useSessionStore = defineStore('session', () => {
  const revision = ref(0)
  const activeCaseId = ref<string | null>(null)

  return { revision, activeCaseId }
})
