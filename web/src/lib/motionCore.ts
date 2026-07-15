export const appMotion = {
  micro: 0.16,
  interface: 0.42,
  narrative: 0.8,
  ease: 'power2.out',
  decisiveEase: 'power3.inOut',
} as const

export function prefersReducedMotion() {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}
