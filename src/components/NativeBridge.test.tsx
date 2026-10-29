import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { useState } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })

type Intent = { type: 'link'; path: string }

let release: (intent: Intent | null) => void = () => {}
let requested = Promise.withResolvers<void>()

mockIPC(
  (command) => {
    switch (command) {
      case 'native_take_pending_intent': {
        const pending = Promise.withResolvers<Intent | null>()

        release = pending.resolve
        requested.resolve()

        return pending.promise
      }
      default:
        return
    }
  },
  { shouldMockEvents: true }
)

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { NativeBridge } = await import('./NativeBridge')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  await act(async () => cleanup())
})

const view = () => within(document.body)

function Harness() {
  const [mounted, setMounted] = useState(true)
  const { pathname } = useLocation()

  return (
    <>
      {mounted && <NativeBridge />}
      <output aria-label="Location">{pathname}</output>
      <button type="button" onClick={() => setMounted(false)}>
        Unmount bridge
      </button>
    </>
  )
}

const mount = async () => {
  requested = Promise.withResolvers<void>()
  render(
    <MemoryRouter initialEntries={['/pipelines']}>
      <Harness />
    </MemoryRouter>
  )
  await requested.promise
}

test('a pending intent that resolves while mounted is handled', async () => {
  await mount()
  await act(async () => release({ type: 'link', path: '/settings' }))
  await waitFor(() => assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/settings'))
})

test('a pending intent that resolves after unmount is ignored', async () => {
  const user = userEvent.setup({ document, delay: null })

  await mount()
  await user.click(view().getByRole('button', { name: 'Unmount bridge' }))
  await act(async () => release({ type: 'link', path: '/settings' }))
  await act(async () => {})

  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/pipelines')
})
