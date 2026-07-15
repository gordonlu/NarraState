import { gsap } from 'gsap'
import { Draggable } from 'gsap/Draggable'
import { Flip } from 'gsap/Flip'
import { appMotion, prefersReducedMotion } from './motionCore'

gsap.registerPlugin(Flip, Draggable)

export { appMotion, prefersReducedMotion }

export function createBriefEntrance(root: HTMLElement) {
  if (prefersReducedMotion()) return undefined

  return gsap.timeline({ defaults: { ease: appMotion.ease } })
    .from(root.querySelector('.brief-lead'), { autoAlpha: 0, y: 24, duration: appMotion.narrative })
    .from(
      root.querySelectorAll('.brief-visuals, .brief-timeline, .brief-people, .brief-evidence, .brief-actions'),
      { autoAlpha: 0, y: 18, stagger: 0.07, duration: appMotion.interface },
      '-=.46',
    )
}

export function createCaseEntryTransition(root: HTMLElement, curtain: HTMLElement) {
  if (prefersReducedMotion()) return Promise.resolve()

  return new Promise<void>((resolve) => {
    gsap.timeline({ defaults: { ease: appMotion.decisiveEase } })
      .to(root, { scale: 0.985, autoAlpha: 0.35, duration: appMotion.interface })
      .fromTo(
        curtain,
        { autoAlpha: 1, clipPath: 'inset(100% 0 0 0)' },
        { clipPath: 'inset(0% 0 0 0)', duration: appMotion.narrative },
        '-=.2',
      )
      .fromTo(curtain.querySelector('span'), { autoAlpha: 0, y: 12 }, { autoAlpha: 1, y: 0, duration: appMotion.interface }, '-=.32')
      .call(resolve)
  })
}

export function createInvestigationEntrance(root: HTMLElement) {
  if (prefersReducedMotion()) return undefined

  const panels = root.querySelectorAll('.workspace-people, .workspace-dialogue, .workspace-research')
  const dialogueParts = root.querySelectorAll('.dialogue-heading, .transcript, .composer-region')

  return gsap.timeline({ defaults: { ease: appMotion.ease } })
    .from(panels, { autoAlpha: 0, y: 18, stagger: 0.08, duration: appMotion.narrative })
    .from(dialogueParts, { autoAlpha: 0, y: 12, stagger: 0.06, duration: appMotion.interface }, '-=.5')
}

export async function animateLayoutChange(
  panel: HTMLElement,
  update: () => Promise<void> | void,
  revealSelector: string,
) {
  if (prefersReducedMotion()) {
    await update()
    return
  }

  const state = Flip.getState(panel)
  await update()
  Flip.from(state, { targets: panel, duration: appMotion.interface, ease: appMotion.decisiveEase })
  gsap.fromTo(
    panel.querySelectorAll(revealSelector),
    { autoAlpha: 0, y: 12 },
    { autoAlpha: 1, y: 0, stagger: 0.05, duration: appMotion.interface, ease: appMotion.ease, clearProps: 'opacity,visibility,transform' },
  )
}

export async function animateSharedElementFlip(
  source: HTMLElement,
  update: () => Promise<void> | void,
  resolveTarget: () => HTMLElement | null,
) {
  if (prefersReducedMotion()) {
    await update()
    return
  }

  const state = Flip.getState(source)
  await update()
  const target = resolveTarget()
  if (!target) return

  Flip.from(state, {
    targets: target,
    absolute: true,
    nested: true,
    duration: appMotion.interface,
    ease: appMotion.decisiveEase,
    clearProps: true,
  })
  const details = target.querySelectorAll('[data-detail-reveal]')
  if (details.length > 0) {
    gsap.fromTo(
      details,
      { autoAlpha: 0, y: 10 },
      { autoAlpha: 1, y: 0, stagger: 0.05, duration: appMotion.interface, ease: appMotion.ease, clearProps: 'opacity,visibility,transform' },
    )
  }
}

export function animateNewTurns(turns: HTMLElement[]) {
  if (prefersReducedMotion() || turns.length === 0) return
  gsap.fromTo(
    turns,
    { autoAlpha: 0, y: 18 },
    { autoAlpha: 1, y: 0, stagger: 0.08, duration: appMotion.interface, ease: appMotion.ease, clearProps: 'opacity,visibility,transform' },
  )
}

export async function animateCharacterSwap(panel: HTMLElement, update: () => Promise<void> | void) {
  if (prefersReducedMotion()) {
    await update()
    return
  }

  await gsap.to(panel, { autoAlpha: 0.45, x: -8, duration: appMotion.micro, ease: 'power1.in' })
  await update()
  gsap.fromTo(
    panel,
    { autoAlpha: 0.45, x: 10 },
    { autoAlpha: 1, x: 0, duration: appMotion.interface, ease: appMotion.ease, clearProps: 'opacity,visibility,transform' },
  )
}

export function animateEvidenceFlip(start: DOMRect, target: HTMLElement, label: string) {
  if (prefersReducedMotion()) return

  const end = target.getBoundingClientRect()
  const flight = document.createElement('span')
  flight.className = 'evidence-flight'
  flight.textContent = label
  Object.assign(flight.style, {
    left: `${start.left}px`,
    top: `${start.top + start.height / 2 - 17}px`,
    width: `${Math.min(Math.max(start.width * 0.72, 150), 260)}px`,
  })
  document.body.append(flight)

  const state = Flip.getState(flight)
  gsap.set(flight, {
    left: end.left,
    top: end.top + end.height / 2 - 17,
    width: Math.min(Math.max(end.width * 0.72, 150), 260),
  })
  Flip.from(state, {
    duration: appMotion.interface,
    ease: appMotion.decisiveEase,
    absolute: true,
    onComplete: () => flight.remove(),
  })
}

export interface EvidenceDragController {
  refresh: () => void
  destroy: () => void
}

export function createEvidenceDragController(
  root: HTMLElement,
  onDrop: (evidenceId: string, source: HTMLElement) => void,
): EvidenceDragController {
  let instances: Draggable[] = []

  const destroyInstances = () => {
    instances.forEach((instance) => instance.kill())
    instances = []
  }

  const refresh = () => {
    destroyInstances()
    if (prefersReducedMotion() || !window.matchMedia('(min-width: 821px) and (pointer: fine)').matches) return

    const dropzone = root.querySelector<HTMLElement>('[data-evidence-dropzone]')
    if (!dropzone) return

    root.querySelectorAll<HTMLElement>('[data-draggable-evidence]').forEach((row) => {
      const handle = row.querySelector<HTMLElement>('[data-evidence-drag-handle]')
      const evidenceId = row.dataset.draggableEvidence
      if (!handle || !evidenceId) return

      const created = Draggable.create(row, {
        type: 'x,y',
        trigger: handle,
        zIndexBoost: true,
        onPress() {
          row.classList.add('is-dragging')
        },
        onDrag() {
          dropzone.classList.toggle('is-drag-over', Draggable.hitTest(row, dropzone, '32%'))
        },
        onRelease() {
          const accepted = Draggable.hitTest(row, dropzone, '32%') && !row.classList.contains('attached')
          dropzone.classList.remove('is-drag-over')
          row.classList.remove('is-dragging')
          if (accepted) onDrop(evidenceId, row)
          gsap.to(row, {
            x: 0,
            y: 0,
            duration: accepted ? appMotion.micro : appMotion.interface,
            ease: accepted ? 'power1.out' : 'back.out(1.4)',
            clearProps: 'transform,zIndex',
          })
        },
      })
      instances.push(...created)
    })
  }

  refresh()
  return { refresh, destroy: destroyInstances }
}
