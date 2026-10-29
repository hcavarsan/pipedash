import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { cleanup, renderHook } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { useAppShortcuts } = await import('./useAppShortcuts')

beforeEach(() => {
  cleanup()
  document.body.replaceChildren()
})

let user = userEvent.setup({ document })

beforeEach(() => {
  user = userEvent.setup({ document })
})

async function press(keys: string) {
  let prevented = false
  const watch = (event: KeyboardEvent) => {
    prevented = event.defaultPrevented
  }

  document.addEventListener('keydown', watch)
  try {
    await user.keyboard(keys)
  } finally {
    document.removeEventListener('keydown', watch)
  }

  return prevented
}

const mod = (key: string) => `{Control>}${key}{/Control}`

test('web commands share settings and sidebar actions without taking browser refresh or address shortcuts', async () => {
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

  await press(mod(','))
  await press(mod('\\'))
  assert.equal(settings, 1)
  assert.equal(sidebar, 1)
  assert.equal(await press(mod('r')), false)
  assert.equal(await press(mod('l')), false)
  assert.equal(refresh, 0)
  assert.equal(lock, 0)
})

test('slash and the search command focus the current page search', async () => {
  const main = document.createElement('main')
  const search = document.createElement('input')
  search.dataset.searchInput = ''
  main.append(search)
  document.body.append(main)
  renderHook(() => useAppShortcuts({ onRefresh: () => {} }))

  await press('/')
  assert.equal(document.activeElement, search)
  search.blur()
  await press(mod('k'))
  assert.equal(document.activeElement, search)
})

test('typing a slash is left alone, but the search command works from another input', async () => {
  const main = document.createElement('main')
  const search = document.createElement('input')
  search.dataset.searchInput = ''
  const other = document.createElement('input')
  main.append(search, other)
  document.body.append(main)
  renderHook(() => useAppShortcuts({ onRefresh: () => {} }))
  other.focus()

  assert.equal(await press('/'), false)
  assert.equal(document.activeElement, other)
  await press(mod('k'))
  assert.equal(document.activeElement, search)
})

test('the settings command waits while a dialog is open, so unsaved edits are not dropped', async () => {
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

  await press(mod(','))
  assert.equal(settings, 0)
  dialog.remove()
  await press(mod(','))
  assert.equal(settings, 1)
})
