import type { SpringConfig } from '../../lib/spring'
import { createSpringValue, type SpringValue } from './springValue'

export interface Reveal {
  element: HTMLElement
  bar: HTMLElement
  full: number
  spring: SpringValue
}

const isOpen = (element: HTMLElement) =>
  element.hasAttribute('data-open') || element.hasAttribute('data-pinned') || element.matches(':focus-within')

export interface SearchReveal {
  closed: () => Reveal | null
  settle: (target: Reveal, open: boolean, velocity: number, config: SpringConfig) => Promise<void>
  stop: () => void
}

export function createSearchReveal(root: () => HTMLElement | null): SearchReveal {
  let reveal: Reveal | null = null

  const closed = (): Reveal | null => {
    const element = Array.from(root()?.querySelectorAll<HTMLElement>('[data-search-reveal]') ?? []).find(
      (candidate) => candidate.firstElementChild instanceof HTMLElement && candidate.firstElementChild.offsetHeight > 0
    )
    const bar = element?.firstElementChild

    if (!element || !(bar instanceof HTMLElement) || isOpen(element)) {
      return null
    }

    const full = bar.offsetHeight

    if (reveal?.element !== element || reveal.full !== full) {
      reveal?.spring.grab()

      reveal = {
        element,
        bar,
        full,
        spring: createSpringValue(0, (value) => {
          element.style.height = `${value}px`
          bar.style.opacity = String(full > 0 ? Math.min(1, value / full) : 1)
        }),
      }
    }

    return reveal
  }

  const settle = (target: Reveal, open: boolean, velocity: number, config: SpringConfig) =>
    target.spring.to(open ? target.full : 0, velocity, config).then((live) => {
      if (!live) {
        return
      }

      target.element.toggleAttribute('data-open', open)
      target.element.style.height = ''
      target.bar.style.opacity = ''
      if (reveal === target) {
        reveal = null
      }
    })

  const stop = () => {
    reveal?.spring.grab()
  }

  return { closed, settle, stop }
}
