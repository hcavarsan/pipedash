import { IconCopy } from '@tabler/icons-react'
import { createElement, type KeyboardEvent, type MouseEvent, type RefObject, type TouchEvent, useRef } from 'react'
import { type ContextMenuItem, useContextMenu } from '../../../components/common/ContextMenu'
import { PRESENT_EASING, prefersReducedMotion } from '../../../lib/motion'
import type { LogDocumentMeta } from '../../../types/logs'
import type { LineState } from '../lib/lineCacheTypes'
import { sectionAtLine } from '../lib/logRows'
import type { LogSelection } from './useLogNavigation'

const SEARCH_SLIDE_MS = 200
const SWIPE_HIDE_PX = 12

const copyIcon = createElement(IconCopy, { size: 14, 'aria-hidden': true })

const lineAt = (target: EventTarget | null): number | null => {
  const value = target instanceof HTMLElement ? target.closest<HTMLElement>('[data-line]')?.dataset.line : undefined

  return value === undefined ? null : Number(value)
}

export function slideSearch(input: HTMLInputElement | null, open: boolean): void {
  const reveal = input?.closest<HTMLElement>('[data-search-reveal]')

  if (!input || !reveal) {
    return
  }

  const from = reveal.offsetHeight

  if (open) {
    reveal.setAttribute('data-open', '')
    input.focus({ preventScroll: true })
  } else if (from > 0 && !reveal.hasAttribute('data-pinned')) {
    input.blur()
    reveal.removeAttribute('data-open')
  }

  const to = reveal.offsetHeight

  if (from !== to && !prefersReducedMotion()) {
    reveal.animate([{ height: `${from}px` }, { height: `${to}px` }], {
      duration: SEARCH_SLIDE_MS,
      easing: PRESENT_EASING,
    })
  }
}

interface TouchEvents {
  onTouchStart: (event: TouchEvent<HTMLDivElement>) => void
  onTouchMove: (event: TouchEvent<HTMLDivElement>) => void
  onTouchEnd: () => void
  onTouchCancel: () => void
}

type BodyKeyEvent = Pick<globalThis.KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'preventDefault'>

interface LogViewerInteractionsOptions {
  meta: LogDocumentMeta
  phone: boolean
  selection: LogSelection | null
  searchRef: RefObject<HTMLInputElement | null>
  getLine: (line: number) => LineState
  copyRange: (from: number, to: number, title: string) => Promise<void>
  copySelection: () => void
  clearSelection: () => void
  onFullLine: (line: number) => void
}

interface LogViewerInteractions {
  rowEvents: {
    onContextMenu: (event: MouseEvent<HTMLDivElement>) => void
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void
  }
  onBodyKeyDown: (event: BodyKeyEvent) => void
  touchEvents: TouchEvents | false
}

export function useLogViewerInteractions(options: LogViewerInteractionsOptions): LogViewerInteractions {
  const { meta, phone, selection, searchRef, getLine, copyRange, copySelection, clearSelection, onFullLine } = options
  const contextMenu = useContextMenu()
  const swipeStartY = useRef<number | null>(null)
  const contextLine = useRef<number | null>(null)

  const menuHandlers = contextMenu(() => {
    const line = contextLine.current

    if (line === null) {
      return []
    }

    const data = getLine(line)
    const step = sectionAtLine(meta, line)
    const items: Array<ContextMenuItem | false> = [
      {
        label: 'Copy line',
        icon: copyIcon,
        onClick: () => void copyRange(line, line + 1, "Couldn't copy the line"),
      },
      !!step && {
        label: 'Copy step output',
        icon: copyIcon,
        onClick: () => void copyRange(step.start_line, step.end_line, "Couldn't copy the step"),
      },
      typeof data === 'object' && data.omitted > 0 && { label: 'Show full line', onClick: () => onFullLine(line) },
    ]

    return items.filter((item) => item !== false)
  })

  const rowEvents = {
    onContextMenu: (event: MouseEvent<HTMLDivElement>) => {
      contextLine.current = lineAt(event.target)
      menuHandlers.onContextMenu(event)
    },
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
      contextLine.current = lineAt(event.target)
      menuHandlers.onKeyDown(event)
    },
  }

  const onBodyKeyDown = (event: BodyKeyEvent) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c' && selection) {
      if (window.getSelection()?.isCollapsed) {
        event.preventDefault()
        copySelection()
      }
    } else if (event.key === 'Escape' && selection) {
      clearSelection()
    }
  }

  const touchEvents = phone && {
    onTouchStart: (event: TouchEvent<HTMLDivElement>) => {
      swipeStartY.current = event.touches.length === 1 ? (event.touches[0]?.clientY ?? null) : null
    },
    onTouchMove: (event: TouchEvent<HTMLDivElement>) => {
      const start = swipeStartY.current
      const y = event.touches[0]?.clientY

      if (start !== null && y !== undefined && start - y > SWIPE_HIDE_PX) {
        swipeStartY.current = null
        slideSearch(searchRef.current, false)
      }
    },
    onTouchEnd: () => {
      swipeStartY.current = null
    },
    onTouchCancel: () => {
      swipeStartY.current = null
    },
  }

  return { rowEvents, onBodyKeyDown, touchEvents }
}
