import { type Dispatch, type SetStateAction, useEffect, useRef, useState } from 'react'

import { createLongPress, guardLongPressClick } from '../components/ios/longPress'
import { haptic } from '../lib/haptics'
import { openRowMenu } from '../utils/rowMenu'

const EDITABLE = 'input, textarea, select, [contenteditable]:not([contenteditable="false"])'

export function useLongPress(
  onLongPress: (row: HTMLElement) => boolean,
  enabled = true
): Dispatch<SetStateAction<HTMLElement | null>> {
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

    const clear = () => {
      element.removeAttribute('data-pressing')
      activePointer = null
      document.removeEventListener('scroll', onScroll, { capture: true })
      document.removeEventListener('pointerup', onUp, { capture: true })
      document.removeEventListener('pointercancel', onCancel, { capture: true })
    }

    const abort = () => {
      press.cancel()
      clear()
    }

    const release = () => {
      press.end()
      clear()
    }

    const onScroll = (event: Event) => {
      if (event.target === document || (event.target instanceof Node && event.target.contains(element))) {
        abort()
      }
    }

    const onUp = (event: PointerEvent) => {
      if (event.pointerId === activePointer) {
        release()
      }
    }

    const onCancel = (event: PointerEvent) => {
      if (event.pointerId === activePointer) {
        abort()
      }
    }

    const onDown = (event: PointerEvent) => {
      if (event.button !== 0 || (event.target instanceof Element && event.target.closest('[data-row-menu]'))) {
        return
      }

      activePointer = event.pointerId
      element.setAttribute('data-pressing', '')
      press.start(event.clientX, event.clientY)
      document.addEventListener('scroll', onScroll, { capture: true, passive: true })
      document.addEventListener('pointerup', onUp, { capture: true })
      document.addEventListener('pointercancel', onCancel, { capture: true })
    }

    const onMove = (event: PointerEvent) => {
      if (event.pointerId === activePointer && !press.move(event.clientX, event.clientY)) {
        element.removeAttribute('data-pressing')
      }
    }

    const onKey = (event: KeyboardEvent) => {
      const nested = event.target instanceof Element && event.target.closest(EDITABLE)

      if (!nested && (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) && open()) {
        event.preventDefault()
      }
    }

    element.addEventListener('pointerdown', onDown, { passive: true })
    element.addEventListener('pointermove', onMove, { passive: true })
    element.addEventListener('keydown', onKey)

    return () => {
      press.cancel()
      removeClickGuard?.()
      element.removeEventListener('pointerdown', onDown)
      element.removeEventListener('pointermove', onMove)
      element.removeEventListener('keydown', onKey)
      clear()
    }
  }, [element, enabled])

  return ref
}

export const useRowMenuLongPress = (enabled = true): Dispatch<SetStateAction<HTMLElement | null>> =>
  useLongPress(openRowMenu, enabled)
