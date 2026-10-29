import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import type { ReactNode } from 'react'

GlobalRegistrator.register({ url: 'http://tauri.localhost/', width: 390, height: 844 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'android' } })

let releases = 0

mockIPC(
  (command) => {
    switch (command) {
      case 'get_vault_status':
        return { is_initialized: true, is_unlocked: false, requires_password: false, app_lock: { enabled: true } }
      case 'native_release_privacy_cover':
        releases += 1
        return null
      default:
        return null
    }
  },
  { shouldMockEvents: true }
)

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { AppRoot } = await import('./AppRoot')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

const originalConsoleError = console.error

function CrashedApp(): ReactNode {
  throw new Error('App exploded')
}

const setVisibility = (state: DocumentVisibilityState) => {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state })
  document.dispatchEvent(new Event('visibilitychange'))
}

test('a crashed app still releases the native privacy cover when the app returns', async () => {
  console.error = () => undefined
  try {
    render(
      <AppRoot>
        <CrashedApp />
      </AppRoot>
    )
    await waitFor(() => assert.equal(releases, 1))

    await act(async () => setVisibility('hidden'))
    await act(async () => setVisibility('visible'))
    await waitFor(() => assert.equal(releases, 2))
  } finally {
    console.error = originalConsoleError
  }
})
