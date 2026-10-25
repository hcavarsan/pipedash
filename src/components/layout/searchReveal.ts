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

export function createSearchReveal(root: () => HTMLElement | null) {
  let reveal: Reveal | null = null

  const closed = (): Reveal | null => {
    const element = root()?.querySelector<HTMLElement>('[data-search-reveal]')
    const bar = element?.firstElementChild

    if (!element || !(bar instanceof HTMLElement) || isOpen(element)) {
      return null
    }

    if (reveal?.element !== element) {
      const full = bar.offsetHeight

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
