import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useAppShortcuts } = await import('./useAppShortcuts')

beforeEach(() => {
  cleanup()
  document.body.replaceChildren()
})

function press(key: string, mod = false) {
  const event = new KeyboardEvent('keydown', { key, ctrlKey: mod, cancelable: true, bubbles: true })
  act(() => {
    document.documentElement.dispatchEvent(event)
  })
  return event
}

test('web commands share settings and sidebar actions without taking browser refresh or address shortcuts', () => {
  let settings = 0
  let sidebar = 0
  let refresh = 0
  let lock = 0
  renderHook(() =>
    useAppShortcuts({
      onSettings: () => {
        settings += 1
      },
      onSidebar: () => {
        sidebar += 1
      },
      onRefresh: () => {
        refresh += 1
      },
      onLock: () => {
        lock += 1
      },
    })
  )

  press(',', true)
  press('\\', true)
  assert.equal(settings, 1)
  assert.equal(sidebar, 1)
  assert.equal(press('r', true).defaultPrevented, false)
  assert.equal(press('l', true).defaultPrevented, false)
  assert.equal(refresh, 0)
  assert.equal(lock, 0)
})

test('slash and the search command focus the current page search', () => {
  const main = document.createElement('main')
  const search = document.createElement('input')
  search.dataset.searchInput = ''
  main.append(search)
  document.body.append(main)
  renderHook(() => useAppShortcuts({ onRefresh: () => {} }))

  press('/')
  assert.equal(document.activeElement, search)
  search.blur()
  press('k', true)
  assert.equal(document.activeElement, search)
})

test('typing a slash is left alone, but the search command works from another input', () => {
  const main = document.createElement('main')
  const search = document.createElement('input')
  search.dataset.searchInput = ''
  const other = document.createElement('input')
  main.append(search, other)
  document.body.append(main)
  renderHook(() => useAppShortcuts({ onRefresh: () => {} }))
  other.focus()

  const slash = new KeyboardEvent('keydown', { key: '/', cancelable: true, bubbles: true })
  act(() => {
    other.dispatchEvent(slash)
  })
  assert.equal(slash.defaultPrevented, false)
  assert.equal(document.activeElement, other)
  const searchCommand = new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, cancelable: true, bubbles: true })
  act(() => {
    other.dispatchEvent(searchCommand)
  })
  assert.equal(document.activeElement, search)
})

test('the settings command waits while a dialog is open, so unsaved edits are not dropped', () => {
  let settings = 0
  const dialog = document.createElement('section')
  dialog.className = 'mantine-Modal-content'
  document.body.append(dialog)
  renderHook(() =>
    useAppShortcuts({
      onSettings: () => {
        settings += 1
      },
      onRefresh: () => {},
    })
  )

  press(',', true)
  assert.equal(settings, 0)
  dialog.remove()
  press(',', true)
  assert.equal(settings, 1)
})
