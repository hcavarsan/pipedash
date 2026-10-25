import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { PointerEvent } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

// react-dom checks for a DOM when it loads, so these load after happy-dom registers it.
const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useSidebarResize } = await import('./useSidebarResize')

const nav = document.createElement('nav')
const handle = document.createElement('div')
let renderedWidth = 248
nav.getBoundingClientRect = () => ({ width: renderedWidth, left: 0 }) as DOMRect
handle.setPointerCapture = () => {}
nav.append(handle)
document.body.append(nav)

const pointer = (clientX: number) =>
  ({
    button: 0,
    pointerId: 1,
    clientX,
    currentTarget: handle,
    timeStamp: performance.now(),
    preventDefault: () => {},
  }) as unknown as PointerEvent<HTMLElement>

const renderSidebar = () => renderHook((resizable: boolean) => useSidebarResize(resizable), { initialProps: true })

beforeEach(() => {
  cleanup()
  localStorage.clear()
  renderedWidth = 248
})

test('releasing a drag saves the new width', () => {
  const { result } = renderSidebar()

  act(() => result.current.handleProps.onPointerDown(pointer(248)))
  act(() => result.current.handleProps.onPointerMove(pointer(300)))
  act(() => result.current.handleProps.onLostPointerCapture())

  assert.equal(result.current.resizing, false)
  assert.equal(result.current.handleProps['aria-valuenow'], 300)
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '300')
})

test('a drag ends when the sidebar stops being resizable, and the saved size and toggle take over', () => {
  const { result, rerender } = renderSidebar()

  act(() => result.current.handleProps.onPointerDown(pointer(248)))
  act(() => result.current.handleProps.onPointerMove(pointer(350)))
  assert.equal(result.current.resizing, true)
  assert.equal(result.current.handleProps['aria-valuenow'], 350)

  rerender(false)
  assert.equal(result.current.resizing, false)
  assert.equal(result.current.handleProps['aria-valuenow'], 248)

  act(() => result.current.toggle())
  rerender(true)
  assert.equal(result.current.compact, true)
  assert.equal(result.current.resizing, false)
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '248')
})

test('keeps the grab offset and starts from the rendered width', () => {
  renderedWidth = 176
  const { result } = renderSidebar()

  act(() => result.current.handleProps.onPointerDown(pointer(180)))
  assert.equal(result.current.navbarWidth, 176)
  act(() => result.current.handleProps.onPointerMove(pointer(190)))
  assert.equal(result.current.navbarWidth, 186)
})

test('crossing the compact threshold tracks the pointer without snapping during the drag', () => {
  const { result } = renderSidebar()

  act(() => result.current.handleProps.onPointerDown(pointer(248)))
  act(() => result.current.handleProps.onPointerMove(pointer(120)))
  assert.equal(result.current.navbarWidth, 120)
  assert.equal(result.current.compact, false)
  act(() => result.current.handleProps.onLostPointerCapture())
  assert.equal(result.current.resizing, false)
  assert.equal(result.current.compact, true)
  assert.equal(localStorage.getItem('pipedash-sidebar-compact'), 'true')
})

test('a new drag can grab a snap at its current rendered position', () => {
  const { result } = renderSidebar()

  act(() => result.current.handleProps.onPointerDown(pointer(248)))
  act(() => result.current.handleProps.onPointerMove(pointer(120)))
  act(() => result.current.handleProps.onLostPointerCapture())
  renderedWidth = 105
  act(() => result.current.handleProps.onPointerDown(pointer(109)))
  assert.equal(result.current.navbarWidth, 105)
  act(() => result.current.handleProps.onPointerMove(pointer(119)))
  assert.equal(result.current.navbarWidth, 115)
})

test('reduced motion settles the compact snap immediately', async () => {
  const matchMedia = window.matchMedia.bind(window)
  window.matchMedia = (query) => {
    const result = matchMedia(query)
    Object.defineProperty(result, 'matches', { value: query === '(prefers-reduced-motion: reduce)' })
    return result
  }
  try {
    const { result } = renderSidebar()
    act(() => result.current.handleProps.onPointerDown(pointer(248)))
    act(() => result.current.handleProps.onPointerMove(pointer(120)))
    await act(async () => result.current.handleProps.onLostPointerCapture())
    assert.equal(result.current.navbarWidth, 68)
    assert.equal(result.current.compact, true)
    assert.equal(result.current.resizing, false)
  } finally {
    window.matchMedia = matchMedia
  }
})
