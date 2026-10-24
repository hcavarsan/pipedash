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
handle.setPointerCapture = () => {}
nav.append(handle)
document.body.append(nav)

const pointer = (clientX: number) =>
  ({
    button: 0,
    pointerId: 1,
    clientX,
    currentTarget: handle,
    preventDefault: () => {},
  }) as unknown as PointerEvent<HTMLElement>

const renderSidebar = () => renderHook((resizable: boolean) => useSidebarResize(resizable), { initialProps: true })

beforeEach(() => {
  cleanup()
  localStorage.clear()
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
