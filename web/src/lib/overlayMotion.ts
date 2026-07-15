import { gsap } from 'gsap'
import { appMotion, prefersReducedMotion } from './motionCore'

export function animateOverlayEnter(element: Element, done: () => void) {
  const layer = element as HTMLElement
  if (prefersReducedMotion()) {
    done()
    return
  }

  const panel = layer.querySelector<HTMLElement>('aside, [role="dialog"]')
  const isDialog = panel?.matches('[role="dialog"]') ?? false
  gsap.timeline({ defaults: { ease: appMotion.ease } })
    .fromTo(layer, { autoAlpha: 0 }, { autoAlpha: 1, duration: appMotion.interface })
    .fromTo(
      panel ?? [],
      isDialog ? { autoAlpha: 0, y: 22, scale: 0.985 } : { autoAlpha: 0, x: 42 },
      { autoAlpha: 1, x: 0, y: 0, scale: 1, duration: appMotion.interface, clearProps: 'opacity,visibility,transform' },
      '-=.28',
    )
    .call(done)
}

export function animateOverlayLeave(element: Element, done: () => void) {
  const layer = element as HTMLElement
  if (prefersReducedMotion()) {
    done()
    return
  }

  const panel = layer.querySelector<HTMLElement>('aside, [role="dialog"]')
  const isDialog = panel?.matches('[role="dialog"]') ?? false
  gsap.timeline({ defaults: { ease: 'power2.in' } })
    .to(panel ?? [], {
      autoAlpha: 0,
      x: isDialog ? 0 : 28,
      y: isDialog ? 14 : 0,
      scale: isDialog ? 0.99 : 1,
      duration: appMotion.micro,
    })
    .to(layer, { autoAlpha: 0, duration: appMotion.micro }, '-=.06')
    .call(done)
}

export function animateResolutionExit(panel: HTMLElement) {
  if (prefersReducedMotion()) return Promise.resolve()
  return gsap.timeline({ defaults: { ease: appMotion.decisiveEase } })
    .to(panel.querySelector('.accusation-result'), { scale: 1.015, duration: appMotion.interface })
    .to(panel, { autoAlpha: 0, y: -14, duration: appMotion.interface }, '-=.12')
}

export function createConclusionEntrance(root: HTMLElement) {
  if (prefersReducedMotion()) return undefined

  const lead = root.querySelector('.conclusion-lead')
  const leadParts = root.querySelectorAll('.conclusion-lead > span, .conclusion-lead h1, .conclusion-lead > p')
  const stats = root.querySelectorAll('.conclusion-lead dl > div')
  const sections = root.querySelectorAll('.conclusion-timeline, .conclusion-evidence, .conclusion-reasoning')
  const evidence = root.querySelectorAll('.conclusion-evidence article')
  const actions = root.querySelector('.conclusion-actions')

  return gsap.timeline({ defaults: { ease: appMotion.ease } })
    .from(lead, { autoAlpha: 0, duration: appMotion.micro })
    .from(leadParts, { autoAlpha: 0, y: 22, stagger: 0.1, duration: appMotion.narrative }, '-=.08')
    .from(stats, { autoAlpha: 0, y: 12, stagger: 0.08, duration: appMotion.interface }, '-=.44')
    .from(sections, { autoAlpha: 0, y: 24, stagger: 0.12, duration: appMotion.narrative }, '-=.22')
    .from(evidence, { autoAlpha: 0, x: 14, stagger: 0.06, duration: appMotion.interface }, '-=.56')
    .from(actions, { autoAlpha: 0, y: 12, duration: appMotion.interface }, '-=.16')
}

export function animateConclusionExit(root: HTMLElement) {
  if (prefersReducedMotion()) return Promise.resolve()
  return gsap.to(root, {
    autoAlpha: 0,
    y: -18,
    scale: 0.99,
    duration: appMotion.interface,
    ease: appMotion.decisiveEase,
  })
}

export function animateGenerationEvents(elements: HTMLElement[]) {
  if (prefersReducedMotion() || elements.length === 0) return
  gsap.fromTo(
    elements,
    { autoAlpha: 0, y: 14 },
    { autoAlpha: 1, y: 0, stagger: 0.06, duration: appMotion.interface, ease: appMotion.ease, clearProps: 'opacity,visibility,transform' },
  )
  gsap.fromTo(
    elements.map((element) => element.querySelector('.generation-event-dot')).filter(Boolean),
    { scale: 0 },
    { scale: 1, stagger: 0.06, duration: appMotion.interface, ease: 'back.out(1.7)', clearProps: 'transform' },
  )
}
