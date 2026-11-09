import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockWindows } from '@tauri-apps/api/mocks'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
mockWindows('main')

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, Route, Routes, useLocation } = await import('react-router-dom')
const { UnlockRoute } = await import('./UnlockRoute')
const { service } = await import('../../../services/service')
const { wsClient } = await import('../../../services/websocket')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const mocks: { mock: { restore: () => void } }[] = []

beforeEach(() => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  mocks.push(
    mock.method(service, 'getVaultStatus', async () => ({
      is_unlocked: false,
      password_source: 'none',
      backend: 'sqlite',
      is_first_time: false,
      requires_password: false,
      app_lock: {
        supported: false,
        biometry: 'none',
        biometry_available: false,
        enabled: false,
        lock_on_background: false,
        locked: true,
      },
    })),
    mock.method(service, 'unlockVault', async () => ({ success: true, message: '' })),
    mock.method(wsClient, 'manualReconnect', () => undefined)
  )
})

afterEach(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  for (const installed of mocks.splice(0)) {
    installed.mock.restore()
  }
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function Landing() {
  const location = useLocation()

  return <output data-testid="landed">{location.pathname + location.search}</output>
}

async function unlockFrom(entry: string): Promise<string> {
  render(
    <MantineProvider>
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[entry]}>
          <Routes>
            <Route path="/unlock" element={<UnlockRoute />} />
            <Route path="*" element={<Landing />} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  const view = within(document.body)
  await userEvent.setup({ document }).click(await view.findByRole('button', { name: 'Unlock' }))
  const landed = await view.findByTestId('landed')

  return landed.textContent ?? ''
}

for (const target of ['//evil.example/phish', 'https://evil.example/phish', '/\\evil.example', 'pipelines']) {
  test(`unlock ignores a return target that is not an app path: ${target}`, async () => {
    assert.equal(await unlockFrom(`/unlock?return=${encodeURIComponent(target)}`), '/pipelines')
  })
}

test('unlock returns to an app path from the query', async () => {
  assert.equal(await unlockFrom(`/unlock?return=${encodeURIComponent('/runs/42?tab=logs')}`), '/runs/42?tab=logs')
})
