import { gsap } from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import type { NarrativeStageContent } from '../content/home'

gsap.registerPlugin(ScrollTrigger)

export interface HomeScrollElements {
  root: HTMLElement
  header?: HTMLElement | null
}

export interface HomeScrollOptions {
  compact?: boolean
  stages: NarrativeStageContent[]
}

export interface HomeScrollController {
  timeline: gsap.core.Timeline
  scrollToLabel: (label: string) => boolean
  destroy: () => void
}

function required<T extends Element>(root: ParentNode, selector: string): T {
  const element = root.querySelector<T>(selector)
  if (!element) throw new Error(`Home scroll element missing: ${selector}`)
  return element
}

export function createHomeScrollTimeline(
  elements: HomeScrollElements,
  options: HomeScrollOptions,
): HomeScrollController {
  const { root, header } = elements
  const hero = required<HTMLElement>(root, '.hero-scene')
  const heroCopy = required<HTMLElement>(root, '.hero-copy')
  const heroImage = required<HTMLElement>(root, '.hero-world-image')
  const heroCue = required<HTMLElement>(root, '.hero-scroll-cue')
  const narrative = required<HTMLElement>(root, '.narrative-scene')
  const narrativeContent = required<HTMLElement>(root, '.narrative-content')
  const characterImage = required<HTMLElement>(root, '.character-room-image')
  const rule = required<HTMLElement>(root, '.rule-statement')
  const replay = required<HTMLElement>(root, '.replay-scene')
  const replayCopy = required<HTMLElement>(root, '.replay-copy')
  const archiveStage = required<HTMLElement>(root, '.archive-stage')
  const stageCopies = Array.from(root.querySelectorAll<HTMLElement>('.narrative-stage-copy'))
  const markers = Array.from(root.querySelectorAll<HTMLElement>('.state-trajectory li'))
  const trajectoryFill = required<HTMLElement>(root, '.trajectory-line b')
  const pressure = required<HTMLElement>(root, '[data-pressure]')
  const phase = required<HTMLElement>(root, '[data-phase]')
  const archives = Array.from(root.querySelectorAll<HTMLElement>('.case-archive'))
  const pressureState = { value: options.stages[0]?.pressure ?? 24 }

  gsap.set([narrative, replay, rule], { autoAlpha: 0 })
  gsap.set(stageCopies.slice(1), { autoAlpha: 0, y: 28 })
  gsap.set(trajectoryFill, { scaleX: 0.03, transformOrigin: 'left center' })
  gsap.set(markers.slice(1), { color: 'rgba(244, 240, 232, .42)' })
  gsap.set(archives, { yPercent: 34, rotationY: -16, autoAlpha: 0 })

  const timeline = gsap.timeline({
    defaults: { ease: 'none' },
    scrollTrigger: {
      trigger: root,
      start: 'top top',
      end: options.compact ? '+=400%' : '+=520%',
      pin: true,
      scrub: 1,
      anticipatePin: 1,
      invalidateOnRefresh: true,
    },
  })

  timeline
    .addLabel('enter-world')
    .to(heroCopy, { autoAlpha: 0, xPercent: -12, duration: 0.7 }, 'enter-world')
    .to(heroCue, { autoAlpha: 0, duration: 0.35 }, 'enter-world')
    .to(
      heroImage,
      {
        scale: options.compact ? 1.65 : 2.75,
        xPercent: options.compact ? -13 : -27,
        transformOrigin: '76% 55%',
        duration: 1.05,
      },
      'enter-world',
    )
    .to(hero, { backgroundColor: '#101112', duration: 0.75 }, 'enter-world+=.3')
    .to(header ?? [], {
      color: '#f4f0e8',
      borderColor: 'rgba(255,255,255,.16)',
      backgroundColor: 'rgba(16,17,18,.82)',
      duration: 0.45,
    }, 'enter-world+=.45')
    .to(narrative, { autoAlpha: 1, duration: 0.55 }, 'enter-world+=.62')
    .to(hero, { autoAlpha: 0, duration: 0.28 }, 'enter-world+=.82')
    .addLabel('denial')
    .fromTo(narrativeContent, { yPercent: 4 }, { yPercent: 0, duration: 0.65 }, 'denial')
    .fromTo(characterImage, { scale: 0.94 }, { scale: 1, duration: 0.65 }, 'denial')
    .addLabel('explanation')
    .to(stageCopies[0] ?? [], { autoAlpha: 0, y: -24, duration: 0.25 }, 'explanation')
    .to(stageCopies[1] ?? [], { autoAlpha: 1, y: 0, duration: 0.3 }, 'explanation+=.12')
    .to(trajectoryFill, { scaleX: 0.5, duration: 0.5 }, 'explanation')
    .to(markers[0] ?? [], { color: 'rgba(244,240,232,.42)', duration: 0.2 }, 'explanation')
    .to(markers[1] ?? [], { color: '#e45136', duration: 0.2 }, 'explanation')
    .to(characterImage, { scale: options.compact ? 1.04 : 1.09, xPercent: -1.5, duration: 0.7 }, 'explanation')
    .to(pressureState, {
      value: options.stages[1]?.pressure ?? 53,
      duration: 0.55,
      onUpdate: () => { pressure.textContent = String(Math.round(pressureState.value)) },
      onStart: () => { phase.textContent = options.stages[1]?.label ?? '解释' },
    }, 'explanation')
    .addLabel('partial-admission')
    .to(stageCopies[1] ?? [], { autoAlpha: 0, y: -24, duration: 0.25 }, 'partial-admission')
    .to(stageCopies[2] ?? [], { autoAlpha: 1, y: 0, duration: 0.3 }, 'partial-admission+=.12')
    .to(trajectoryFill, { scaleX: 1, duration: 0.55 }, 'partial-admission')
    .to(markers[1] ?? [], { color: 'rgba(244,240,232,.42)', duration: 0.2 }, 'partial-admission')
    .to(markers[2] ?? [], { color: '#e45136', duration: 0.2 }, 'partial-admission')
    .to(characterImage, { scale: options.compact ? 1.08 : 1.2, xPercent: -3, duration: 0.72 }, 'partial-admission')
    .to(pressureState, {
      value: options.stages[2]?.pressure ?? 78,
      duration: 0.55,
      onUpdate: () => { pressure.textContent = String(Math.round(pressureState.value)) },
      onStart: () => { phase.textContent = options.stages[2]?.label ?? '部分承认' },
    }, 'partial-admission')
    .addLabel('rule')
    .to(narrativeContent, { autoAlpha: 0, xPercent: -8, duration: 0.4 }, 'rule')
    .to(characterImage, { autoAlpha: 0.24, scale: options.compact ? 1.1 : 1.24, duration: 0.5 }, 'rule')
    .to(rule, { autoAlpha: 1, yPercent: -4, duration: 0.55 }, 'rule+=.12')
    .addLabel('replay')
    .to(rule, { autoAlpha: 0, yPercent: -12, duration: 0.35 }, 'replay')
    .to(narrative, { autoAlpha: 0, duration: 0.45 }, 'replay')
    .to(replay, { autoAlpha: 1, duration: 0.5 }, 'replay+=.12')
    .to(archives, { yPercent: 0, rotationY: 0, autoAlpha: 1, stagger: 0.1, duration: 0.65 }, 'replay+=.1')
    .fromTo(archiveStage, { scale: 1.16 }, { scale: 1, duration: 0.8 }, 'replay')
    .fromTo(replayCopy, { xPercent: 8, autoAlpha: 0 }, { xPercent: 0, autoAlpha: 1, duration: 0.65 }, 'replay+=.2')
    .addLabel('footer')
    .to(replay, { yPercent: -3, duration: 0.5 }, 'footer')

  return {
    timeline,
    scrollToLabel: (label) => {
      const labelTime = timeline.labels[label]
      const scrollTrigger = timeline.scrollTrigger
      const duration = timeline.duration()
      if (labelTime === undefined || !scrollTrigger || duration <= 0) return false
      scrollTrigger.refresh()
      const progress = labelTime / duration
      const target = scrollTrigger.start + (scrollTrigger.end - scrollTrigger.start) * progress
      window.scrollTo({ top: target, behavior: 'smooth' })
      return true
    },
    destroy: () => {
      timeline.scrollTrigger?.kill()
      timeline.kill()
      gsap.set(header ?? [], { clearProps: 'color,borderColor,backgroundColor' })
    },
  }
}
