import { useEffect, useRef, useState } from 'react'

import { haptic } from '../../lib/haptics'

import { createLongPress, guardLongPressClick } from './longPress'

export function openRowMenu(row: HTMLElement): boolean {
  const target = row.querySelector<HTMLElement>('[data-row-menu]')

  if (!target || target.matches(':disabled, [aria-disabled="true"]')) {
    return false
  }

  target.click()

  return true
}

export function useLongPress(onLongPress: (row: HTMLElement) => boolean, enabled = true) {
  const [element, ref] = useState<HTMLElement | null>(null)
  const latest = useRef(onLongPress)

  latest.current = onLongPress

  useEffect(() => {
    if (!element || !enabled) {
      return
    }

    let activePointer: number | null = null
    let removeClickGuard: (() => void) | undefined

    const open = () => {
      if (latest.current(element)) {
        haptic('medium')

        return true
      }

      return false
    }

    const press = createLongPress(() => {
      const pointerId = activePointer

      if (!open()) {
        element.removeAttribute('data-pressing')

        return
      }

      removeClickGuard?.()
      removeClickGuard = guardLongPressClick(document, pointerId)
    })

    const abort = () => {
      press.cancel()
      element.removeAttribute('data-pressing')
      activePointer = null
      document.removeEventListener('scroll', onScroll, { capture: true })
    }

    const onScroll = (event: Event) => {
      if (event.target === document || (event.target as Node).contains(element)) {
        abort()
      }
    }

    const release = () => {
      press.end()
      element.removeAttribute('data-pressing')
      activePointer = null
      document.removeEventListener('scroll', onScroll, { capture: true })
    }

    const onDown = (event: PointerEvent) => {
      if (event.button !== 0 || (event.target instanceof Element && event.target.closest('[data-row-menu]'))) {
        return
      }

      activePointer = event.pointerId
      element.setAttribute('data-pressing', '')
      press.start(event.clientX, event.clientY)
      document.addEventListener('scroll', onScroll, { capture: true, passive: true })
    }

    const onMove = (event: PointerEvent) => {
      if (event.pointerId === activePointer && !press.move(event.clientX, event.clientY)) {
        element.removeAttribute('data-pressing')
      }
    }

    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
        event.preventDefault()
        open()
      }
    }

    element.addEventListener('pointerdown', onDown, { passive: true })
    element.addEventListener('pointermove', onMove, { passive: true })
    element.addEventListener('pointerup', release, { passive: true })
    element.addEventListener('pointercancel', abort, { passive: true })
    element.addEventListener('keydown', onKey)

    return () => {
      press.cancel()
      removeClickGuard?.()
      element.removeAttribute('data-pressing')
      element.removeEventListener('pointerdown', onDown)
      element.removeEventListener('pointermove', onMove)
      element.removeEventListener('pointerup', release)
      element.removeEventListener('pointercancel', abort)
      element.removeEventListener('keydown', onKey)
      document.removeEventListener('scroll', onScroll, { capture: true })
    }
  }, [element, enabled])

  return ref
}

export const useRowMenuLongPress = (enabled = true) => useLongPress(openRowMenu, enabled)
