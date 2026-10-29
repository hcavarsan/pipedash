import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

// react-dom checks for a DOM when it loads, so these load after happy-dom registers it.
const { act, cleanup, render, renderHook, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { createElement } = await import('react')
const { useSidebarResize } = await import('./useSidebarResize')

const nav = document.createElement('nav')
const handle = document.createElement('div')
let renderedWidth = 248
nav.getBoundingClientRect = () => new DOMRect(0, 0, renderedWidth, 0)
handle.setPointerCapture = () => {}
nav.append(handle)
document.body.append(nav)

const pointer = (clientX: number) => ({
  button: 0,
  pointerId: 1,
  clientX,
  currentTarget: handle,
  timeStamp: performance.now(),
  preventDefault: () => {},
})

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

afterEach(() => {
  cleanup()
})

function SidebarHarness() {
  const sidebar = useSidebarResize(true)

  return createElement(
    'aside',
    { 'aria-label': 'Sidebar', 'data-width': String(sidebar.navbarWidth) },
    createElement('div', sidebar.handleProps)
  )
}

const mountSidebar = () => {
  const user = userEvent.setup({ document })

  render(createElement(SidebarHarness))

  const separator = within(document.body).getByRole('separator', { name: 'Resize sidebar' })
  const sidebar = within(document.body).getByRole('complementary', { name: 'Sidebar' })

  separator.setPointerCapture = () => undefined
  separator.releasePointerCapture = () => undefined

  return { user, separator, sidebar }
}

const widthOf = (separator: HTMLElement) => separator.getAttribute('aria-valuenow')

test('arrow keys resize the sidebar and persist the width', async () => {
  const { user, separator, sidebar } = mountSidebar()

  assert.equal(widthOf(separator), '248')
  separator.focus()
  await user.keyboard('{ArrowRight}')
  assert.equal(widthOf(separator), '264')
  assert.equal(sidebar.getAttribute('data-width'), 'clamp(200px, 264px, 40vw)')
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '264')

  await user.keyboard('{ArrowLeft}{ArrowLeft}')
  assert.equal(widthOf(separator), '232')
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '232')
})

test('arrow keys past the minimum width collapse the sidebar to the rail', async () => {
  const { user, separator, sidebar } = mountSidebar()

  separator.focus()
  await user.keyboard('{ArrowLeft}{ArrowLeft}{ArrowLeft}{ArrowLeft}')

  assert.equal(widthOf(separator), '68')
  assert.equal(sidebar.getAttribute('data-width'), '68')
  assert.equal(localStorage.getItem('pipedash-sidebar-compact'), 'true')

  await user.keyboard('{ArrowRight}')
  assert.equal(localStorage.getItem('pipedash-sidebar-compact'), 'false')
})

test('dragging the separator resizes the sidebar and persists the released width', async () => {
  const { user, separator } = mountSidebar()

  await user.pointer([
    { keys: '[MouseLeft>]', target: separator, coords: { clientX: 248 } },
    { coords: { clientX: 300 } },
  ])
  assert.equal(widthOf(separator), '300')

  await user.pointer({ keys: '[/MouseLeft]' })
  act(() => {
    separator.dispatchEvent(new Event('lostpointercapture', { bubbles: true }))
  })

  assert.equal(widthOf(separator), '300')
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '300')
})

test('double-clicking the separator restores the default width', async () => {
  const { user, separator, sidebar } = mountSidebar()

  separator.focus()
  await user.keyboard('{ArrowRight}{ArrowRight}')
  assert.equal(widthOf(separator), '280')

  await user.dblClick(separator)

  assert.equal(widthOf(separator), '248')
  assert.equal(sidebar.getAttribute('data-width'), 'clamp(200px, 248px, 40vw)')
  assert.equal(localStorage.getItem('pipedash-sidebar-width'), '248')
  assert.equal(localStorage.getItem('pipedash-sidebar-compact'), 'false')
})
